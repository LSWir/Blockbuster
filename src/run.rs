//! Полный запуск: то, что делал main() в Python-версии (сканирование диска → кэш → log.txt → index.html).
use crate::config::load_raw;
use crate::ctx::{Ctx, Fail};
use crate::jv::Obj;
use crate::protocol::{protocol_status, register_protocol, remember_library};
use crate::rx::Rx;
use crate::titles::CATEGORY_LABELS;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct RunOutcome {
    pub ok: bool,
    pub stopped: bool,
    pub out_dir: PathBuf,
}

/// "G", "G:" или "G:\" → корень диска G:\ ; иначе путь как есть.
pub fn normalize_target(t: &str) -> String {
    let t = t.trim().trim_matches('"').trim().to_string();
    if Rx::new(r"^[A-Za-z]:?[\\/]*$").is_match(&t) {
        let c = t.chars().next().unwrap_or('C').to_ascii_uppercase();
        return format!("{}:\\", c);
    }
    t
}

pub fn run_scan(
    app_dir: &Path,
    target: &str,
    offline: bool,
    refresh: bool,
    embed_logos: bool,
    lang: i32,
    cancel: Arc<AtomicBool>,
    log: Box<dyn Fn(&str) + Send>,
) -> RunOutcome {
    let mut ctx = Ctx::new(app_dir, cancel, log);
    let root = PathBuf::from(normalize_target(target));
    let fail = |ctx: &Ctx, root: &Path| RunOutcome { ok: false, stopped: false, out_dir: if ctx.out_dir.as_os_str().is_empty() { root.to_path_buf() } else { ctx.out_dir.clone() } };
    if !root.is_dir() {
        ctx.say(&format!("Папка или диск не найдены: {}", root.display()));
        return fail(&ctx, &root);
    }
    if let Err(e) = remember_library(app_dir, &root.to_string_lossy()) {
        ctx.say(&format!("  ! не удалось записать libraries.json: {}", e));
    }
    if matches!(protocol_status(), "missing" | "stale") {
        let _ = register_protocol(false);
    }

    ctx.refresh = refresh;
    ctx.embed_logos = embed_logos;
    ctx.load_config(&root);
    // язык каталога = язык интерфейса: описания, жанры, названия и постеры TMDB берутся на нём
    ctx.html_lang = crate::i18n::lang_code(lang).to_string();
    ctx.cfg.language = match lang {
        1 => "en-US",
        2 => "uk-UA",
        _ => "ru-RU",
    }
    .to_string();
    ctx.say(&format!("Язык каталога: {} (описания, жанры и постеры TMDB)", ctx.cfg.language));
    ctx.online = !offline && (!ctx.cfg.tmdb_key.is_empty() || !ctx.cfg.omdb_keys.is_empty());
    ctx.cache_dir = root.join(".catalog_cache");
    ctx.prepare_output(&root);
    let online_file = ctx.cache_dir.join("online.json");
    let probe_file = ctx.cache_dir.join("probe.json");

    if ctx.online {
        ctx.cache = load_raw(&online_file);
        let mut names: Vec<String> = Vec::new();
        if !ctx.cfg.tmdb_key.is_empty() {
            names.push("TMDB".to_string());
        }
        if !ctx.cfg.omdb_keys.is_empty() {
            names.push(format!("OMDb/IMDb (ключей: {})", ctx.cfg.omdb_keys.len()));
        }
        ctx.say(&format!(
            "Онлайн-данные: {} (первый запуск долгий, дальше результаты берутся из кэша)",
            names.join(", ")
        ));
    } else if !offline {
        ctx.say("  (API-ключи не заданы — онлайн-поиск постеров и описаний выключен, см. «Ключи и настройки»)");
    }

    ctx.say(&format!("Сканирование: {}", root.display()));
    if ctx.ffprobe.is_some() {
        ctx.probe_cache = load_raw(&probe_file);
        if ctx.ffmpeg.is_none() {
            ctx.say("  (ffmpeg не найден — обложки, вшитые в MKV/MP4, не извлекаются; положите ffmpeg.exe рядом с ffprobe)");
        }
    } else {
        ctx.say("  (ffprobe не найден — длительность/разрешение берутся только из .nfo; обложки из MKV не читаются)");
    }

    let scanned = ctx.scan_library(&root);

    // кэш сохраняется всегда — в том числе после «Стоп», чтобы следующий запуск продолжил с этого места
    let mut saved: Vec<String> = Vec::new();
    {
        let mut save_json = |ctx: &Ctx, path: &Path, obj: &Obj| {
            let _ = std::fs::create_dir_all(&ctx.cache_dir); // папка создаётся только когда есть что сохранить
            match serde_json::to_string(&Value::Object(obj.clone())) {
                Ok(s) => match std::fs::write(path, s) {
                    Ok(_) => saved.push(format!(
                        "{}: {} записей",
                        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                        obj.len()
                    )),
                    Err(e) => ctx.say(&format!("  ! не удалось сохранить кэш: {}", e)),
                },
                Err(e) => ctx.say(&format!("  ! не удалось сохранить кэш: {}", e)),
            }
        };
        if ctx.online && !ctx.cache.is_empty() {
            save_json(&ctx, &online_file, &ctx.cache);
        }
        if ctx.ffprobe.is_some() && !ctx.probe_cache.is_empty() {
            save_json(&ctx, &probe_file, &ctx.probe_cache);
        }
    }

    let mut data = match scanned {
        Ok(d) => d,
        Err(Fail::Stop) => {
            ctx.say("■ Остановлено пользователем (найденное сохранено в кэш — следующий запуск продолжит с этого места)");
            return RunOutcome { ok: false, stopped: true, out_dir: ctx.out_dir.clone() };
        }
        Err(Fail::Auth) => {
            ctx.say("[ОШИБКА] неверный ключ API");
            return fail(&ctx, &root);
        }
    };

    // итоговый отчёт — чтобы было видно, что реально происходило
    ctx.say("");
    if ctx.online {
        let s = ctx.stats.clone();
        ctx.say(&format!(
            "Онлайн-запросы: найдено новых {}, взято из кэша {}, не найдено {}",
            s.online_ok, s.online_cached, s.online_miss
        ));
        if s.net_fail > 0 {
            let e = ctx.last_net_err.clone();
            ctx.say(&format!(
                "  ! Ошибок сети: {}, последняя: {}\n    Проверьте интернет и доступ к api.themoviedb.org и omdbapi.com (VPN, брандмауэр, антивирус).",
                s.net_fail, e
            ));
        }
    }
    if ctx.ffprobe.is_some() {
        let s = ctx.stats.clone();
        let tail = if s.probe_fail > 0 { format!(" (последняя: {})", ctx.last_probe_err) } else { String::new() };
        ctx.say(&format!("ffprobe: новых {}, из кэша {}, ошибок {}{}", s.probe_ok, s.probe_cached, s.probe_fail, tail));
    }
    if !saved.is_empty() {
        ctx.say(&format!("Кэш ({}): {}", ctx.cache_dir.display(), saved.join("; ")));
    } else {
        let mut why: Vec<&str> = Vec::new();
        if !ctx.online {
            why.push("онлайн-поиск выключен (нет ключей или галочка «без интернета»)");
        } else if ctx.cache.is_empty() {
            why.push("ни один онлайн-запрос не вернул данных");
        }
        if ctx.ffprobe.is_none() {
            why.push("ffprobe не найден");
        } else if ctx.probe_cache.is_empty() {
            why.push("ffprobe не обработал ни одного файла");
        }
        if why.is_empty() {
            why.push("нет данных");
        }
        ctx.say(&format!("Кэш не создан — сохранять нечего: {}", why.join("; ")));
    }

    ctx.write_log(&root, &data, ctx.online);
    if !offline {
        ctx.apply_imdb_ratings(&mut data);
    }
    let logos = ctx.sync_logos(&root);
    ctx.hide_cache();
    if !logos.links.is_empty() || !logos.unknown.is_empty() {
        let tail = if logos.unknown.is_empty() {
            String::new()
        } else {
            format!("; НЕ распознаны (проверьте имена): {}", logos.unknown.join(", "))
        };
        ctx.say(&format!(
            "Логотипы: распознано {}, скопировано в .catalog_cache\\logos: {}{}",
            logos.links.len(),
            logos.copied,
            tail
        ));
    }

    let html = ctx.render_html(&root, &data, &logos.links);
    let out_base = if ctx.out_dir.as_os_str().is_empty() { root.clone() } else { ctx.out_dir.clone() };
    let out_file = out_base.join("index.html");
    if let Err(e) = std::fs::write(&out_file, html.as_bytes()) {
        ctx.say(&format!("[ОШИБКА] не удалось записать {}: {}", out_file.display(), e));
        return fail(&ctx, &root);
    }
    let size_mb = html.len() as f64 / 1024.0 / 1024.0;
    let summary: Vec<String> = CATEGORY_LABELS
        .iter()
        .map(|(k, label)| {
            let n = data.iter().filter(|x| x.get("category").and_then(|c| c.as_str()) == Some(*k)).count();
            format!("{}: {}", label, n)
        })
        .collect();
    ctx.say(&format!(
        "\nГотово! Всего карточек: {} ({}). Файл: {} ({:.1} МБ)",
        data.len(),
        summary.join(", "),
        out_file.display(),
        size_mb
    ));
    RunOutcome { ok: true, stopped: false, out_dir: out_base }
}
