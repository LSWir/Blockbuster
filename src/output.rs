//! Результат работы: постеры, рейтинги IMDb из датасета, логотипы, log.txt, index.html, права и скрытая папка кэша.
use crate::ctx::Ctx;
use crate::jv::{has, put, text_of, Obj};
use crate::titles::CATEGORY_LABELS;
use crate::util::*;
use base64::Engine;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const POSTER_MAX_W: u32 = 400;
const POSTER_QUALITY: u8 = 80;
const RATINGS_URL: &str = "https://datasets.imdbws.com/title.ratings.tsv.gz";
const POPCORN_BUILTIN: &str = include_str!("../assets/Popcorn.json");
const TEMPLATE: &str = include_str!("../assets/template.html");

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn make_poster_uri(path: &Path) -> Result<String, String> {
    use image::codecs::jpeg::JpegEncoder;
    use image::imageops::FilterType;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    match image::load_from_memory(&bytes) {
        Ok(img) => {
            let img = if img.width() > POSTER_MAX_W {
                let h = ((img.height() as u64) * (POSTER_MAX_W as u64) / (img.width() as u64)) as u32;
                img.resize_exact(POSTER_MAX_W, h.max(1), FilterType::Lanczos3)
            } else {
                img
            };
            let rgb = img.to_rgb8();
            let mut buf: Vec<u8> = Vec::new();
            {
                let mut enc = JpegEncoder::new_with_quality(&mut buf, POSTER_QUALITY);
                enc.encode_image(&rgb).map_err(|e| e.to_string())?;
            }
            Ok(format!("data:image/jpeg;base64,{}", b64(&buf)))
        }
        Err(_) => {
            // формат не распознан — встраиваем файл как есть
            let ext = ext_lower(&file_name(path));
            let mime = match ext.as_str() {
                ".png" => "png",
                ".webp" => "webp",
                _ => "jpeg",
            };
            Ok(format!("data:image/{};base64,{}", mime, b64(&bytes)))
        }
    }
}

// ───────────── логотипы ─────────────
pub const LOGO_KEYS: [&str; 30] = [
    "imdb", "tmdb", "rt", "h264", "hevc", "av1", "vp9", "mpeg2", "mpeg4", "vc1", "dv", "hdr10", "hlg", "ac3", "eac3",
    "eac3_atmos", "truehd", "truehd_atmos", "atmos", "dts", "dtshd_ma", "dtshd_hra", "dtshd", "dtsx", "aac", "flac",
    "pcm", "opus", "vorbis", "mp3",
];
const LOGO_EXTS: [&str; 5] = [".png", ".svg", ".webp", ".jpg", ".jpeg"];

fn logo_mime(ext: &str) -> &'static str {
    match ext {
        ".png" => "image/png",
        ".svg" => "image/svg+xml",
        ".webp" => "image/webp",
        _ => "image/jpeg",
    }
}

/// Имя файла → ключ логотипа: 'DTS-HD Master Audio.png' → 'dtshd_ma', 'eac3+JOC Atmos.png' → 'eac3_atmos'.
pub fn logo_key(filename: &str) -> Option<String> {
    let stem = stem_of(filename).to_lowercase();
    if !stem.is_empty() && LOGO_KEYS.contains(&stem.as_str()) {
        return Some(stem);
    }
    let c: String = stem.chars().filter(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == '+').collect();
    if !c.is_empty() && LOGO_KEYS.contains(&c.as_str()) {
        return Some(c);
    }
    let has_ = |s: &str| c.contains(s);
    let atmos = has_("atmos");
    let k = |s: &str| Some(s.to_string());
    if has_("imdb") { return k("imdb"); }
    if has_("tmdb") || has_("themoviedb") { return k("tmdb"); }
    if has_("tomato") || c == "rt" { return k("rt"); }
    if has_("truehd") { return if atmos { k("truehd_atmos") } else { k("truehd") }; }
    if ["eac3", "ddp", "dd+", "digitalplus", "digital+"].iter().any(|x| has_(*x)) {
        return if atmos || has_("joc") { k("eac3_atmos") } else { k("eac3") };
    }
    if atmos { return k("atmos"); }
    if has_("dtsx") { return k("dtsx"); }
    if has_("dts") && has_("hd") {
        if has_("master") || c.ends_with("ma") { return k("dtshd_ma"); }
        if has_("high") || has_("hra") { return k("dtshd_hra"); }
        return k("dtshd");
    }
    if has_("dts") { return k("dts"); }
    if has_("ac3") || has_("dolbydigital") { return k("ac3"); }
    if has_("dolbyvision") || c == "dv" { return k("dv"); }
    if has_("hdr10") { return k("hdr10"); }
    if has_("hlg") { return k("hlg"); }
    let table: [(&str, &[&str]); 13] = [
        ("h264", &["h264", "avc", "x264"]),
        ("hevc", &["h265", "hevc", "x265"]),
        ("av1", &["av1"]),
        ("vp9", &["vp9"]),
        ("mpeg2", &["mpeg2"]),
        ("mpeg4", &["mpeg4"]),
        ("vc1", &["vc1"]),
        ("aac", &["aac"]),
        ("flac", &["flac"]),
        ("opus", &["opus"]),
        ("vorbis", &["vorbis"]),
        ("mp3", &["mp3"]),
        ("pcm", &["pcm"]),
    ];
    for (key, words) in table.iter() {
        let key: &str = key;
        if words.iter().any(|w| has_(*w)) {
            return k(key);
        }
    }
    None
}

/// Логотип → data-URI; крупные растровые уменьшаются до 128 px по большей стороне (файл остаётся небольшим).
fn logo_data_uri(bytes: &[u8], ext: &str) -> String {
    use image::imageops::FilterType;
    const MAXSIDE: u32 = 128;
    if ext != ".svg" {
        if let Ok(img) = image::load_from_memory(bytes) {
            if img.width() > MAXSIDE || img.height() > MAXSIDE {
                let small = img.resize(MAXSIDE, MAXSIDE, FilterType::Lanczos3);
                let mut buf: Vec<u8> = Vec::new();
                if small.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).is_ok() {
                    return format!("data:image/png;base64,{}", b64(&buf));
                }
            }
        }
    }
    format!("data:{};base64,{}", logo_mime(ext), b64(bytes))
}

/// Результат синхронизации логотипов.
pub struct Logos {
    pub links: Obj,
    pub unknown: Vec<String>,
    pub copied: usize,
}

impl Ctx {
    pub fn poster_data_uri(&mut self, path: &Path) -> String {
        if let Some(u) = self.poster_cache.get(path) {
            return u.clone();
        }
        let uri = match make_poster_uri(path) {
            Ok(u) => u,
            Err(e) => {
                self.say(&format!("  ! не удалось прочитать постер {}: {}", path.display(), e));
                String::new()
            }
        };
        self.poster_cache.insert(path.to_path_buf(), uri.clone());
        uri
    }

    // ───────────── рейтинг IMDb из официальных датасетов ─────────────
    fn imdb_ratings_file(&mut self) -> Option<PathBuf> {
        let path = self.cache_dir.join("title.ratings.tsv.gz");
        let fresh = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs() < 24 * 3600)
            .unwrap_or(false);
        if fresh {
            return Some(path);
        }
        let _ = std::fs::create_dir_all(&self.cache_dir);
        self.say("  Загружаю рейтинги IMDb (datasets.imdbws.com)...");
        let tmp = self.cache_dir.join("title.ratings.tsv.gz.part");
        let res: Result<(), String> = (|| {
            let resp = self.agent.get(RATINGS_URL).timeout(Duration::from_secs(180)).call().map_err(|e| e.to_string())?;
            let mut rd = resp.into_reader();
            let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            std::io::copy(&mut rd, &mut f).map_err(|e| e.to_string())?;
            drop(f);
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(e) = res {
            let tail = if path.is_file() { "; использую прошлую копию" } else { "" };
            self.say(&format!("  ! не удалось скачать датасет рейтингов IMDb: {}{}", e, tail));
        }
        if path.is_file() { Some(path) } else { None }
    }

    /// Подставляет актуальный рейтинг и число голосов IMDb по IMDb ID (кроме рейтингов, заданных в .nfo).
    pub fn apply_imdb_ratings(&mut self, items: &mut Vec<Value>) -> usize {
        let needs = |it: &Value| -> bool {
            let o = match it.as_object() { Some(o) => o, None => return false };
            has(o, "imdb") && (has(o, "rating_src") || !has(o, "rating"))
        };
        let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for it in items.iter() {
            if needs(it) {
                ids.insert(it.get("imdb").map(text_of).unwrap_or_default());
            }
        }
        if ids.is_empty() {
            return 0;
        }
        let path = match self.imdb_ratings_file() {
            Some(p) => p,
            None => return 0,
        };
        let mut found: HashMap<String, (String, String)> = HashMap::new();
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                self.say(&format!("  ! не удалось прочитать датасет рейтингов IMDb: {}", e));
                return 0;
            }
        };
        let rd = BufReader::new(flate2::read::GzDecoder::new(file));
        let mut first = true;
        for line in rd.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    self.say(&format!("  ! не удалось прочитать датасет рейтингов IMDb: {}", e));
                    return 0;
                }
            };
            if first {
                first = false; // заголовок
                continue;
            }
            let mut it = line.splitn(2, '\t');
            let tc = it.next().unwrap_or("");
            let rest = it.next().unwrap_or("");
            if ids.contains(tc) {
                let mut p = rest.trim().splitn(2, '\t');
                let r = p.next().unwrap_or("").to_string();
                let v = p.next().unwrap_or("").to_string();
                found.insert(tc.to_string(), (r, v));
                if found.len() == ids.len() {
                    break;
                }
            }
        }
        let mut n = 0usize;
        for it in items.iter_mut() {
            let ok = needs(it);
            let imdb = it.get("imdb").map(text_of).unwrap_or_default();
            if let (true, Some(hit)) = (ok, found.get(&imdb)) {
                if let (Ok(r), Ok(v)) = (hit.0.parse::<f64>(), hit.1.parse::<i64>()) {
                    if let Some(o) = it.as_object_mut() {
                        put(o, "rating", format!("{:.1}", r));
                        put(o, "rating_src", "IMDb");
                        put(o, "votes", v);
                        n += 1;
                    }
                }
            }
        }
        let total = items.len();
        self.say(&format!("  Рейтинг IMDb из датасета: {} из {} карточек", n, total));
        n
    }

    /// Копирует папку logos (рядом с программой, затем <корень>\logos) в <корень>\.catalog_cache\logos.
    pub fn sync_logos(&mut self, root: &Path) -> Logos {
        let mut sources: Vec<(String, PathBuf)> = Vec::new(); // имя файла → путь (первая папка приоритетнее)
        for base in [self.app_dir.join("logos"), root.join("logos")] {
            if base.is_dir() {
                let mut names = list_dir(&base);
                names.sort();
                for name in names {
                    let p = base.join(&name);
                    if p.is_file() && !sources.iter().any(|(n, _)| n.to_lowercase() == name.to_lowercase()) {
                        sources.push((name, p));
                    }
                }
            }
        }
        let mut logos = Logos { links: Obj::new(), unknown: Vec::new(), copied: 0 };
        if sources.is_empty() {
            return logos;
        }
        // 1) ключ → файл (имена, совпадающие с ключом, приоритетнее «человеческих»)
        let mut names: Vec<String> = sources.iter().map(|(n, _)| n.clone()).collect();
        names.sort_by_key(|n| {
            let st = stem_of(n).to_lowercase();
            (!LOGO_KEYS.contains(&st.as_str()), n.to_lowercase())
        });
        let mut chosen: Vec<(String, String)> = Vec::new();
        for name in names {
            if !LOGO_EXTS.contains(&ext_lower(&name).as_str()) {
                continue;
            }
            match logo_key(&name) {
                None => logos.unknown.push(name),
                Some(k) => {
                    if !chosen.iter().any(|(ck, _)| *ck == k) {
                        chosen.push((k, name));
                    }
                }
            }
        }
        // 2) копия в .catalog_cache\logos (зеркало: лишние файлы удаляются)
        let dest = self.cache_dir.join("logos");
        let mut copy_err: Option<String> = None;
        if !self.embed_logos {
            let copy_res: std::io::Result<usize> = (|| {
                std::fs::create_dir_all(&dest)?;
                let mut copied = 0usize;
                for (name, src) in &sources {
                    let dst = dest.join(name);
                    let (ms, md) = (std::fs::metadata(src)?, std::fs::metadata(&dst));
                    let need = match md {
                        Ok(d) => d.len() != ms.len() || mtime_secs(&d) < mtime_secs(&ms),
                        Err(_) => true,
                    };
                    if need {
                        std::fs::copy(src, &dst)?;
                        copied += 1;
                    }
                }
                for name in list_dir(&dest) {
                    if !sources.iter().any(|(n, _)| *n == name) && dest.join(&name).is_file() {
                        let _ = std::fs::remove_file(dest.join(&name));
                    }
                }
                Ok(copied)
            })();
            match copy_res {
                Ok(c) => logos.copied = c,
                Err(e) => copy_err = Some(e.to_string()),
            }
        }
        if !self.embed_logos && copy_err.is_none() && self.cache_in_root {
            for (k, n) in &chosen {
                put(&mut logos.links, k, format!(".catalog_cache/logos/{}", quote(n)));
            }
        } else {
            if !self.embed_logos {
                let why = copy_err.unwrap_or_else(|| "кэш вне корня диска".to_string());
                self.say(&format!(
                    "  ! не удалось скопировать логотипы в {}: {}\n    логотипы будут встроены в HTML",
                    dest.display(),
                    why
                ));
            }
            for (k, n) in &chosen {
                if let Some((_, src)) = sources.iter().find(|(sn, _)| sn == n) {
                    if let Ok(bytes) = std::fs::read(src) {
                        put(&mut logos.links, k, logo_data_uri(&bytes, &ext_lower(n)));
                    }
                }
            }
        }
        logos
    }

    /// Popcorn.json (Lottie): рядом с программой или встроенный; → компактный JSON для вставки в HTML.
    pub fn load_popcorn(&self) -> String {
        for base in [self.app_dir.clone(), self.app_dir.join("logos")] {
            let mut names = list_dir(&base);
            names.sort();
            for name in names {
                let l = name.to_lowercase();
                if l == "popcorn.json" || l == "logo.json" || l == "header.json" {
                    if let Some(s) = popcorn_from(&std::fs::read_to_string(base.join(&name)).unwrap_or_default()) {
                        return s;
                    }
                }
            }
        }
        popcorn_from(POPCORN_BUILTIN).unwrap_or_else(|| "null".to_string())
    }

    // ───────────── права на запись, скрытая папка ─────────────
    /// Проверяет доступ на запись к <диск>\.catalog_cache и к корню диска; при отказе пробует выдать права или уходит в запасную папку.
    pub fn prepare_output(&mut self, root: &Path) {
        self.out_dir = root.to_path_buf();
        self.cache_in_root = true;
        if can_write(&self.cache_dir) {
            return;
        }
        let cd = self.cache_dir.clone();
        self.say(&format!("  ! нет доступа на запись в {}", cd.display()));
        if cfg!(windows) && cd.is_dir() {
            // вернуть себе права на папку (если мы её владелец)
            let user = std::env::var("USERNAME").unwrap_or_default();
            let mut c1 = Command::new("icacls");
            c1.arg(&cd).args(["/grant", &format!("{}:(OI)(CI)F", user), "/T", "/C", "/Q"]);
            let _ = run_capture(c1, 120);
            let mut c2 = Command::new("attrib");
            c2.arg("-R").arg(&cd).args(["/S", "/D"]);
            let _ = run_capture(c2, 60);
            if can_write(&cd) {
                self.say("    права восстановлены");
                return;
            }
        }
        let base: PathBuf = match std::env::var_os("LOCALAPPDATA") {
            Some(p) => PathBuf::from(p),
            None => {
                let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
                home.join(".local").join("share")
            }
        };
        let rs = root.to_string_lossy().to_string();
        let tag_src = if rs.chars().nth(1) == Some(':') { rs.chars().take(2).collect::<String>() } else { file_name(root) };
        let mut tag: String = tag_src.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        if tag.is_empty() {
            tag = "root".to_string();
        }
        let alt = base.join("Blockbuster").join("cache").join(&tag);
        self.cache_dir = alt.join(".catalog_cache");
        self.cache_in_root = false;
        self.say(&format!(
            "    кэш будет храниться здесь: {}\n    (чтобы вернуть кэш на диск: удалите папку {} от имени администратора или выдайте себе «Полный доступ» в Свойства → Безопасность)",
            self.cache_dir.display(),
            root.join(".catalog_cache").display()
        ));
        if !can_write(&self.cache_dir) {
            self.cache_dir = std::env::temp_dir().join("Blockbuster").join(&tag).join(".catalog_cache");
            let _ = std::fs::create_dir_all(&self.cache_dir);
        }
        if !can_write(root) {
            self.out_dir = base.join("Blockbuster").join("out").join(&tag);
            let _ = std::fs::create_dir_all(&self.out_dir);
            self.say(&format!(
                "  ! корень {} закрыт для записи — index.html и log.txt будут сохранены в {}",
                root.display(),
                self.out_dir.display()
            ));
        }
    }

    /// Windows: делает .catalog_cache скрытой папкой. Ссылки из HTML продолжают работать.
    pub fn hide_cache(&self) {
        if !cfg!(windows) || !self.cache_dir.is_dir() {
            return;
        }
        let mut c = Command::new("attrib");
        c.arg("+H").arg(&self.cache_dir);
        let _ = run_capture(c, 30);
    }

    // ───────────── log.txt ─────────────
    pub fn write_log(&self, root: &Path, items: &[Value], online_on: bool) {
        let line = |it: &Value| -> String {
            let p = unquote(&it.get("video").map(text_of).unwrap_or_default()).replace('/', "\\");
            let title = it.get("title").map(text_of).unwrap_or_default();
            let year = it.get("year").map(text_of).unwrap_or_default();
            let want = if !year.is_empty() { format!("{} ({})", title, year) } else { title };
            let mut s = format!("  {}\n      искали как: «{}»", p, want);
            if it.get("kind").and_then(|k| k.as_str()) == Some("show") {
                let n = it.get("episodes_count").map(text_of).filter(|s| !s.is_empty()).unwrap_or_else(|| "?".to_string());
                s.push_str(&format!("   [сериал, серий: {}]", n));
            }
            s
        };
        let flag = |it: &Value, k: &str| it.as_object().map(|o| has(o, k)).unwrap_or(false);
        let title_of = |it: &Value| it.get("title").map(text_of).unwrap_or_default().to_lowercase();
        let mut nf: Vec<&Value> = items.iter().filter(|it| !flag(it, "imdb") && !flag(it, "tmdb")).collect();
        let mut np: Vec<&Value> =
            items.iter().filter(|it| !flag(it, "poster") && (flag(it, "imdb") || flag(it, "tmdb"))).collect();
        nf.sort_by_key(|x| title_of(x));
        np.sort_by_key(|x| title_of(x));
        let mut out: Vec<String> = vec![
            format!("Каталог {} — {}", root.display(), chrono::Local::now().format("%Y-%m-%d %H:%M")),
            format!(
                "Карточек: {}; не найдено: {}; найдено, но без постера: {}; пропущено папок: {}",
                items.len(),
                nf.len(),
                np.len(),
                self.ignored.len()
            ),
            String::new(),
        ];
        if !online_on {
            out.push("! Онлайн-поиск был выключен (нет ключей или --offline): список «не найдено» неполон/недостоверен.".to_string());
            out.push(String::new());
        }
        out.push("=== НЕ НАЙДЕНО В ИНТЕРНЕТЕ (нет IMDb/TMDB ID) — переименуйте в «Название (Год)» ===".to_string());
        if nf.is_empty() {
            out.push("  (нет)".to_string());
        }
        for it in &nf {
            out.push(line(it));
        }
        out.push(String::new());
        out.push("=== НАЙДЕНО, НО БЕЗ ПОСТЕРА ===".to_string());
        if np.is_empty() {
            out.push("  (нет)".to_string());
        }
        for it in &np {
            out.push(line(it));
        }
        out.push(String::new());
        out.push("=== ПРОПУЩЕННЫЕ ПАПКИ (по имени) ===".to_string());
        let mut ign = self.ignored.clone();
        ign.sort();
        if ign.is_empty() {
            out.push("  (нет)".to_string());
        }
        for p in ign {
            out.push(format!("  {}", p));
        }
        let base = if self.out_dir.as_os_str().is_empty() { root.to_path_buf() } else { self.out_dir.clone() };
        let path = base.join("log.txt");
        // utf-8-sig: Блокнот Windows корректно покажет кириллицу
        let mut data: Vec<u8> = vec![0xEF, 0xBB, 0xBF];
        let text = crate::logtr::translate(crate::i18n::lang_from_code(&self.html_lang), &(out.join("\n") + "\n"));
        data.extend(text.into_bytes());
        match std::fs::write(&path, data) {
            Ok(_) => self.say(&format!(
                "Лог не найденного: {}  (не найдено: {}, без постера: {})",
                path.display(),
                nf.len(),
                np.len()
            )),
            Err(e) => self.say(&format!("  ! не удалось записать log.txt: {}", e)),
        }
    }

    // ───────────── index.html ─────────────
    pub fn render_html(&self, root: &Path, items: &[Value], logos: &Obj) -> String {
        let esc = |s: String| s.replace("</", "<\\/");
        let cats: Vec<Value> = CATEGORY_LABELS
            .iter()
            .map(|(k, v)| {
                let mut o = Obj::new();
                put(&mut o, "id", *k);
                put(&mut o, "label", *v);
                Value::Object(o)
            })
            .collect();
        let payload = esc(serde_json::to_string(&Value::Array(items.to_vec())).unwrap_or_else(|_| "[]".to_string()));
        // данные для .bat, который страница предлагает скачать (регистрация кнопок «на диске»)
        let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let mut bat = Obj::new();
        put(&mut bat, "python", exe);
        put(&mut bat, "script", "");
        let mut lib = Obj::new();
        put(&mut lib, "root", root.to_string_lossy().into_owned());
        let js = |v: Value| esc(serde_json::to_string(&v).unwrap_or_else(|_| "null".to_string()));
        let html_lang = self.html_lang.clone();
        TEMPLATE
            .replace("__LANG__", &html_lang)
            .replace("__CATEGORIES__", &serde_json::to_string(&Value::Array(cats)).unwrap_or_default())
            .replace("__BAT_INFO__", &js(Value::Object(bat)))
            .replace("__LIB__", &js(Value::Object(lib)))
            .replace("__LOGOS__", &serde_json::to_string(&Value::Object(logos.clone())).unwrap_or_else(|_| "{}".to_string()))
            .replace("__POPCORN__", &self.load_popcorn())
            .replace("__MOVIES_DATA__", &payload)
    }
}

fn popcorn_from(txt: &str) -> Option<String> {
    let v: Value = serde_json::from_str(txt.trim_start_matches('\u{feff}')).ok()?;
    let ok = v.get("layers").map(crate::jv::truthy).unwrap_or(false);
    if !ok {
        return None;
    }
    serde_json::to_string(&v).ok().map(|s| s.replace("</", "<\\/"))
}
