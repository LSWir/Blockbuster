// Без консольного окна в релизной сборке под Windows
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

use blockbuster::config::{load_prefs, save_last_target, save_prefs, save_ui_settings, ui_settings};
use blockbuster::i18n::{tr, tr_arg};
use blockbuster::logtr::translate;
use blockbuster::protocol::{handle_open, register_protocol, remember_library};
use blockbuster::run::{normalize_target, run_scan};
use blockbuster::util::{classify_line, open_path, open_url};
use slint::{CloseRequestResponse, ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

slint::include_modules!();

#[derive(Default)]
struct Shared {
    lines: Vec<(String, i32)>,
    done: Option<(bool, bool, PathBuf)>, // (успех, остановлено, папка с результатом)
}

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|x| x.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn drives() -> Vec<String> {
    let mut v = Vec::new();
    if cfg!(windows) {
        for c in 'A'..='Z' {
            let d = format!("{}:\\", c);
            if Path::new(&d).is_dir() {
                v.push(d);
            }
        }
    }
    v
}

/// Полный лог последнего сканирования: scan_log.txt рядом с программой, каждый запуск перезаписывает файл.
type LogFile = Option<Arc<Mutex<std::fs::File>>>;

fn open_log_file(dir: &Path, target: &str, lang: i32) -> LogFile {
    let mut f = std::fs::File::create(dir.join("scan_log.txt")).ok()?;
    let _ = f.write_all(&[0xEF, 0xBB, 0xBF]); // UTF-8 с BOM: Блокнот Windows покажет кириллицу
    let _ = write!(
        f,
        "Blockbuster v{} — {} — {}\r\n{}\r\n\r\n",
        env!("CARGO_PKG_VERSION"),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        lang_name(lang),
        target
    );
    Some(Arc::new(Mutex::new(f)))
}

fn lang_name(lang: i32) -> &'static str {
    match lang {
        1 => "English",
        2 => "Українська",
        _ => "Русский",
    }
}

fn write_log_file(lf: &LogFile, text: &str) {
    if let Some(f) = lf {
        if let Ok(mut f) = f.lock() {
            for part in text.split('\n') {
                let _ = write!(f, "{}\r\n", part.trim_end_matches('\r'));
            }
            let _ = f.flush();
        }
    }
}

fn push_lines(shared: &Arc<Mutex<Shared>>, text: &str) {
    if let Ok(mut s) = shared.lock() {
        for part in text.split('\n') {
            let part = part.trim_end_matches('\r');
            s.lines.push((part.to_string(), classify_line(part)));
        }
    }
}

fn apply_lang(ui: &MainWindow, lang: i32) {
    let g = ui.global::<Tr>();
    g.set_drive_or_folder(tr(lang, "drive-or-folder").into());
    g.set_paste(tr(lang, "paste").into());
    g.set_browse(tr(lang, "browse").into());
    g.set_keys_settings(tr(lang, "keys-settings").into());
    g.set_tmdb_key(tr(lang, "tmdb-key").into());
    g.set_omdb_keys(tr(lang, "omdb-keys").into());
    g.set_ffprobe_folder(tr(lang, "ffprobe-folder").into());
    g.set_skip_folders(tr(lang, "skip-folders").into());
    g.set_show_keys(tr(lang, "show-keys").into());
    g.set_save(tr(lang, "save").into());
    g.set_scan(tr(lang, "scan").into());
    g.set_stop(tr(lang, "stop").into());
    g.set_offline(tr(lang, "offline").into());
    g.set_embed_logos(tr(lang, "embed-logos").into());
    g.set_autoscroll(tr(lang, "autoscroll").into());
    g.set_open_catalog(tr(lang, "open-catalog").into());
    g.set_drive_folder(tr(lang, "drive-folder").into());
    g.set_about(tr(lang, "about").into());
    g.set_description(tr(lang, "description").into());
    g.set_links_note(tr(lang, "links-note").into());
    g.set_version(tr(lang, "version").into());
    g.set_previous(tr(lang, "previous").into());
    g.set_language(tr(lang, "language").into());
    g.set_theme(tr(lang, "theme").into());
    g.set_light(tr(lang, "light").into());
    g.set_dark(tr(lang, "dark").into());
    g.set_close(tr(lang, "close").into());
    g.set_project(tr(lang, "project").into());
    ui.set_lang(lang);
}

fn main() -> Result<(), slint::PlatformError> {
    let dir = app_dir();
    let args: Vec<String> = std::env::args().skip(1).collect();

    // служебные режимы без окна: ссылки из HTML и регистрация протокола
    if let Some(i) = args.iter().position(|a| a == "--open") {
        if let Some(url) = args.get(i + 1) {
            handle_open(&dir, url);
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--register" || a == "--unregister") {
        let remove = args.iter().any(|a| a == "--unregister");
        if !remove {
            if let Some(a0) = args.iter().find(|a| !a.starts_with("--")) {
                let r = normalize_target(a0);
                if Path::new(&r).is_dir() {
                    let _ = remember_library(&dir, &r);
                }
            }
        }
        let _ = register_protocol(remove);
        return Ok(());
    }

    let ui = MainWindow::new()?;
    let (lang0, dark0) = load_prefs(&dir);
    let lang = Rc::new(Cell::new(lang0));
    apply_lang(&ui, lang0);
    ui.invoke_apply_theme(dark0);
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    let st = ui_settings(&dir);
    ui.set_tmdb_key(st.tmdb_key.into());
    ui.set_omdb_key(st.omdb_key.into());
    ui.set_ffprobe_path(st.ffprobe_path.into());
    ui.set_ignore_folders(st.ignore_folders.into());
    ui.set_target_path(st.last_target.into());
    ui.set_keys_open(!st.configured);
    let drive_model: Vec<SharedString> = drives().into_iter().map(SharedString::from).collect();
    ui.set_drives(ModelRc::new(VecModel::from(drive_model)));
    let log_model: Rc<VecModel<LogLine>> = Rc::new(VecModel::default());
    ui.set_log_lines(ModelRc::from(log_model.clone()));

    let shared = Arc::new(Mutex::new(Shared::default()));
    let cancel = Arc::new(AtomicBool::new(false));
    let result_dir: Rc<RefCell<PathBuf>> = Rc::new(RefCell::new(PathBuf::new()));
    let closing = Rc::new(Cell::new(false));

    // ── «О программе»: ссылки, язык, тема ──
    {
        ui.on_open_url(|u| open_url(&u));
        let w = ui.as_weak();
        let d = dir.clone();
        let lang_l = lang.clone();
        ui.on_set_language(move |l| {
            lang_l.set(l);
            if let Some(ui) = w.upgrade() {
                apply_lang(&ui, l);
                save_prefs(&d, l, ui.global::<Pal>().get_dark());
            }
        });
        let w = ui.as_weak();
        let d = dir.clone();
        let lang = lang.clone();
        ui.on_set_theme(move |dark| {
            if let Some(ui) = w.upgrade() {
                ui.invoke_apply_theme(dark);
                save_prefs(&d, lang.get(), dark);
            }
        });
    }

    // ── выбор папок ──
    {
        let w = ui.as_weak();
        let lang_a = lang.clone();
        ui.on_browse_target(move || {
            let lang = &lang_a;
            if let Some(p) = rfd::FileDialog::new().set_title(tr(lang.get(), "dlg-target")).pick_folder() {
                if let Some(ui) = w.upgrade() {
                    ui.set_target_path(p.to_string_lossy().into_owned().into());
                    ui.set_selected_drive("".into());
                }
            }
        });
        let w = ui.as_weak();
        let lang_b = lang.clone();
        ui.on_browse_ffprobe(move || {
            let lang = &lang_b;
            if let Some(p) = rfd::FileDialog::new().set_title(tr(lang.get(), "dlg-ffprobe")).pick_folder() {
                if let Some(ui) = w.upgrade() {
                    ui.set_ffprobe_path(p.to_string_lossy().replace('\\', "/").into());
                }
            }
        });
    }

    // ── сохранение настроек ──
    {
        let w = ui.as_weak();
        let d = dir.clone();
        let lang = lang.clone();
        ui.on_save_settings(move |tmdb, omdb, ffp, ign| {
            if let Some(ui) = w.upgrade() {
                match save_ui_settings(&d, &tmdb, &omdb, &ffp, &ign) {
                    Ok(_) => ui.set_status(tr(lang.get(), "st-saved").into()),
                    Err(e) => ui.set_status(tr_arg(lang.get(), "st-save-fail", &e.to_string()).into()),
                }
            }
        });
    }

    // ── запуск сканирования ──
    {
        let w = ui.as_weak();
        let d = dir.clone();
        let shared = shared.clone();
        let cancel = cancel.clone();
        let lm = log_model.clone();
        let lang = lang.clone();
        ui.on_start_scan(move |path, offline, embed| {
            let ui = match w.upgrade() {
                Some(u) => u,
                None => return,
            };
            lang.set(ui.get_lang()); // язык берём прямо из окна — единственный источник истины
            if ui.get_running() {
                return;
            }
            let t = normalize_target(&path);
            if t.is_empty() {
                ui.set_status(tr(lang.get(), "st-choose").into());
                return;
            }
            if !Path::new(&t).is_dir() {
                ui.set_status(tr_arg(lang.get(), "st-notfound", &t).into());
                return;
            }
            let _ = save_ui_settings(
                &d,
                &ui.get_tmdb_key(),
                &ui.get_omdb_key(),
                &ui.get_ffprobe_path(),
                &ui.get_ignore_folders(),
            );
            save_last_target(&d, &t);
            cancel.store(false, Ordering::Relaxed);
            ui.set_running(true);
            ui.set_results_visible(false);
            ui.set_done_ok(false);
            ui.set_status(tr(lang.get(), "st-scanning").into());
            lm.set_vec(Vec::<LogLine>::new());
            if let Ok(mut s) = shared.lock() {
                s.lines.clear();
                s.done = None;
            }
            push_lines(&shared, &tr_arg(lang.get(), "log-start", &t));

            let sh = shared.clone();
            let cn = cancel.clone();
            let dd = d.clone();
            let off = offline;
            let lg = lang.get();
            let emb = embed;
            let lf = open_log_file(&dd, &t, lg);
            write_log_file(&lf, &tr_arg(lg, "log-start", &t));
            std::thread::spawn(move || {
                let sh_log = sh.clone();
                let lf_log = lf.clone();
                // сообщения сканера пишутся по-русски — переводим на язык интерфейса и дублируем в scan_log.txt
                let log: Box<dyn Fn(&str) + Send> = Box::new(move |s: &str| {
                    let s = translate(lg, s);
                    push_lines(&sh_log, &s);
                    write_log_file(&lf_log, &s);
                });
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_scan(&dd, &t, off, false, emb, lg, cn, log)));
                let done = match res {
                    Ok(o) => (o.ok, o.stopped, o.out_dir),
                    Err(_) => {
                        push_lines(&sh, tr(lg, "log-panic"));
                        write_log_file(&lf, tr(lg, "log-panic"));
                        (false, false, PathBuf::from(&t))
                    }
                };
                let final_msg = if done.1 { "st-stopped" } else if done.0 { "st-done" } else { "st-error" };
                write_log_file(&lf, &format!("\n== {} ==", tr(lg, final_msg)));
                if let Ok(mut s) = sh.lock() {
                    s.done = Some(done);
                }
            });
        });
    }

    // ── стоп ──
    {
        let w = ui.as_weak();
        let cancel = cancel.clone();
        let lang = lang.clone();
        ui.on_stop_scan(move || {
            cancel.store(true, Ordering::Relaxed);
            if let Some(ui) = w.upgrade() {
                ui.set_status(tr(lang.get(), "st-stopping").into());
            }
        });
    }

    // ── кнопки результата ──
    {
        let rd = result_dir.clone();
        ui.on_open_result(move |what| {
            let base = rd.borrow().clone();
            let p = match what {
                0 => base.join("index.html"),
                1 => base.join("log.txt"),
                _ => base,
            };
            if p.exists() {
                open_path(&p);
            }
        });
    }

    // ── закрытие окна во время сканирования: сначала останавливаем (найденное сохранится в кэш) ──
    {
        let w = ui.as_weak();
        let cancel = cancel.clone();
        let closing = closing.clone();
        let lang = lang.clone();
        ui.window().on_close_requested(move || {
            if let Some(ui) = w.upgrade() {
                if ui.get_running() {
                    cancel.store(true, Ordering::Relaxed);
                    closing.set(true);
                    ui.set_status(tr(lang.get(), "st-closing").into());
                    return CloseRequestResponse::KeepWindowShown;
                }
            }
            CloseRequestResponse::HideWindow
        });
    }

    // ── таймер: переносит строки лога из рабочего потока в окно ──
    let timer = Timer::default();
    {
        let w = ui.as_weak();
        let shared = shared.clone();
        let lm = log_model.clone();
        let rd = result_dir.clone();
        let closing = closing.clone();
        let lang = lang.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let ui = match w.upgrade() {
                Some(u) => u,
                None => return,
            };
            let (lines, done) = match shared.lock() {
                Ok(mut s) => (std::mem::take(&mut s.lines), s.done.take()),
                Err(_) => return,
            };
            if !lines.is_empty() {
                for (text, kind) in lines {
                    lm.push(LogLine { text: text.into(), kind });
                }
                if lm.row_count() > 6000 {
                    let keep: Vec<LogLine> = lm.iter().skip(lm.row_count() - 4000).collect();
                    lm.set_vec(keep);
                }
                if ui.get_autoscroll() {
                    ui.invoke_scroll_bottom();
                }
            }
            if let Some((ok, stopped, out)) = done {
                ui.set_running(false);
                *rd.borrow_mut() = out;
                let msg = if stopped {
                    tr(lang.get(), "st-stopped")
                } else if ok {
                    tr(lang.get(), "st-done")
                } else {
                    tr(lang.get(), "st-error")
                };
                ui.set_status(msg.into());
                ui.set_results_visible(ok && !stopped);
                ui.set_done_ok(ok && !stopped);
                if ui.get_autoscroll() {
                    ui.invoke_scroll_bottom();
                }
                if closing.get() {
                    let _ = ui.hide();
                }
            }
        });
    }

    ui.run()
}
