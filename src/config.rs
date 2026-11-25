//! catalog_config.json рядом с программой (формат совместим с Python-версией).
use crate::ctx::{keys_from, which, Ctx};
use crate::jv::{put, truthy, Obj};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn config_path(app_dir: &Path) -> PathBuf {
    app_dir.join("catalog_config.json")
}

pub fn load_raw(path: &Path) -> Obj {
    let txt = match std::fs::read(path) {
        Ok(b) => String::from_utf8_lossy(&b).trim_start_matches('\u{feff}').to_string(),
        Err(_) => return Obj::new(),
    };
    match serde_json::from_str::<Value>(&txt) {
        Ok(Value::Object(o)) => o,
        _ => Obj::new(),
    }
}

pub fn save_raw(path: &Path, o: &Obj) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(&Value::Object(o.clone())).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, s)
}

fn find_exe(path: &str, names: &[&str]) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    let p = Path::new(path);
    if p.is_dir() {
        for n in names {
            let c = p.join(n);
            if c.is_file() {
                return Some(c);
            }
        }
        return None;
    }
    if p.is_file() {
        Some(p.to_path_buf())
    } else {
        None
    }
}

impl Ctx {
    /// Настройки: catalog_config.json рядом с программой, затем в корне диска; переменные окружения приоритетнее.
    pub fn load_config(&mut self, root: &Path) {
        let mut data = Obj::new();
        for base in [self.app_dir.clone(), root.to_path_buf()] {
            let p = config_path(&base);
            if p.is_file() {
                let d = load_raw(&p);
                if d.is_empty() {
                    self.say(&format!("  ! не удалось прочитать {}", p.display()));
                }
                for (k, v) in d {
                    if truthy(&v) {
                        data.insert(k, v);
                    }
                }
            }
        }
        if let Some(Value::Object(kw)) = data.get("category_keywords") {
            for (k, v) in kw {
                if let Value::Array(a) = v {
                    let list: Vec<String> = a.iter().map(|x| crate::jv::text_of(x)).collect();
                    if let Some(e) = self.cfg.category_keywords.iter_mut().find(|e| &e.0 == k) {
                        e.1 = list;
                    } else {
                        self.cfg.category_keywords.push((k.clone(), list));
                    }
                }
            }
        }
        let gs = |k: &str| -> String {
            match data.get(k) {
                Some(Value::String(s)) => s.trim().to_string(),
                _ => String::new(),
            }
        };
        let env = |k: &str| std::env::var(k).unwrap_or_default().trim().to_string();

        self.cfg.tmdb_key = if !env("TMDB_API_KEY").is_empty() { env("TMDB_API_KEY") } else { gs("tmdb_key") };
        let mut vals: Vec<Value> = vec![Value::String(env("OMDB_API_KEY"))];
        if let Some(v) = data.get("omdb_key") {
            vals.push(v.clone());
        }
        if let Some(v) = data.get("omdb_keys") {
            vals.push(v.clone());
        }
        self.cfg.omdb_keys = keys_from(&vals);
        let lang = gs("language");
        if !lang.is_empty() {
            self.cfg.language = lang;
        }
        if let Some(Value::Array(a)) = data.get("ignore_folders") {
            self.cfg.ignore_folders = a.iter().map(|x| crate::jv::text_of(x)).collect();
        }
        self.rebuild_keywords();

        // ffprobe: путь из настроек → рядом с программой → PATH
        let mut fp = if !env("FFPROBE_PATH").is_empty() { env("FFPROBE_PATH") } else { gs("ffprobe_path") };
        fp = fp.trim_matches('"').trim().to_string();
        self.cfg.ffprobe_path = fp.clone();
        self.ffprobe = None;
        if !fp.is_empty() {
            match find_exe(&fp, &["ffprobe.exe", "ffprobe"]) {
                Some(p) => self.ffprobe = Some(p),
                None => self.say(&format!("  ! ffprobe не найден по пути: {}", fp)),
            }
        }
        if self.ffprobe.is_none() {
            let bases = [self.app_dir.clone(), self.app_dir.join("ffmpeg"), self.app_dir.join("bin")];
            for b in bases.iter() {
                if let Some(c) = find_exe(&b.to_string_lossy(), &["ffprobe.exe", "ffprobe"]) {
                    self.say(&format!("  ffprobe найден рядом с программой: {}", c.display()));
                    self.ffprobe = Some(c);
                    break;
                }
            }
        }
        if self.ffprobe.is_none() {
            self.ffprobe = which(&["ffprobe.exe", "ffprobe"]);
        }

        // ffmpeg (для обложек, вшитых в MKV/MP4): явный путь → рядом с ffprobe → PATH
        let mp = if !env("FFMPEG_PATH").is_empty() { env("FFMPEG_PATH") } else { gs("ffmpeg_path") };
        let mp = mp.trim_matches('"').trim().to_string();
        self.cfg.ffmpeg_path = mp.clone();
        let mut found = find_exe(&mp, &["ffmpeg.exe", "ffmpeg"]);
        if found.is_none() {
            if let Some(fpb) = &self.ffprobe {
                if let Some(dir) = fpb.parent() {
                    found = find_exe(&dir.to_string_lossy(), &["ffmpeg.exe", "ffmpeg"]);
                }
            }
        }
        if found.is_none() {
            found = which(&["ffmpeg.exe", "ffmpeg"]);
        }
        if !mp.is_empty() && find_exe(&mp, &["ffmpeg.exe", "ffmpeg"]).is_none() {
            self.say(&format!("  ! ffmpeg не найден по пути: {}", mp));
        }
        self.ffmpeg = found;
    }
}

/// Настройки для окна: (tmdb, omdb через запятую, ffprobe, ignore через запятую, last_target, language, настроено ли уже)
pub struct UiSettings {
    pub tmdb_key: String,
    pub omdb_key: String,
    pub ffprobe_path: String,
    pub ignore_folders: String,
    pub last_target: String,
    pub configured: bool,
}

pub fn ui_settings(app_dir: &Path) -> UiSettings {
    let c = load_raw(&config_path(app_dir));
    let ok = match c.get("omdb_keys") {
        Some(v) if truthy(v) => v.clone(),
        _ => c.get("omdb_key").cloned().unwrap_or(Value::Null),
    };
    let omdb = keys_from(&[ok]).join(", ");
    let ign = match c.get("ignore_folders") {
        Some(Value::Array(a)) if !a.is_empty() => {
            a.iter().map(|x| crate::jv::text_of(x)).collect::<Vec<_>>().join(", ")
        }
        _ => String::new(), // пусто = значения по умолчанию (X, XX, XXX)
    };
    let st = |k: &str| match c.get(k) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    let omdb_nonempty = omdb.clone();
    UiSettings {
        tmdb_key: st("tmdb_key"),
        omdb_key: omdb,
        ffprobe_path: st("ffprobe_path"),
        ignore_folders: ign,
        last_target: st("last_target"),
        configured: !st("tmdb_key").is_empty() || !omdb_nonempty.is_empty(),
    }
}

pub fn save_ui_settings(app_dir: &Path, tmdb: &str, omdb: &str, ffprobe: &str, ignore: &str) -> std::io::Result<()> {
    let p = config_path(app_dir);
    let mut c = load_raw(&p);
    let keys = keys_from(&[Value::String(omdb.to_string())]);
    put(&mut c, "tmdb_key", tmdb.trim());
    let omdb_val = if keys.len() > 1 {
        Value::from(keys)
    } else if keys.len() == 1 {
        Value::String(keys[0].clone())
    } else {
        Value::String(String::new())
    };
    c.insert("omdb_key".to_string(), omdb_val);
    c.remove("omdb_keys");
    put(&mut c, "ffprobe_path", ffprobe.trim().trim_matches('"').replace('\\', "/"));
    let ig: Vec<String> =
        ignore.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
    if !ig.is_empty() {
        c.insert("ignore_folders".to_string(), Value::from(ig));
    }
    save_raw(&p, &c)
}

pub fn save_last_target(app_dir: &Path, target: &str) {
    let p = config_path(app_dir);
    let mut c = load_raw(&p);
    put(&mut c, "last_target", target);
    let _ = save_raw(&p, &c);
}

/// Язык и тема интерфейса: (0 ru / 1 en / 2 uk, тёмная ли тема). По умолчанию — русский и тёмная.
pub fn load_prefs(app_dir: &Path) -> (i32, bool) {
    let c = load_raw(&config_path(app_dir));
    // по умолчанию интерфейс запускается на английском
    let lang = match c.get("ui_language") {
        Some(Value::String(s)) => crate::i18n::lang_from_code(s),
        _ => 1,
    };
    let dark = match c.get("ui_theme") {
        Some(Value::String(s)) => s != "light",
        _ => true,
    };
    (lang, dark)
}

pub fn save_prefs(app_dir: &Path, lang: i32, dark: bool) {
    let p = config_path(app_dir);
    let mut c = load_raw(&p);
    put(&mut c, "ui_language", crate::i18n::lang_code(lang));
    put(&mut c, "ui_theme", if dark { "dark" } else { "light" });
    let _ = save_raw(&p, &c);
}
