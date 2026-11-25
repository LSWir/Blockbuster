//! Состояние одного запуска: настройки, кэши, счётчики, лог, флаг остановки.
use crate::jv::Obj;
use crate::rx::Rx;
use crate::titles::{default_keywords, Keywords};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Причина прерывания: пользователь нажал «Стоп» либо ключ API неверен/исчерпан.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fail {
    Stop,
    Auth,
}
pub type R<T> = Result<T, Fail>;

#[derive(Default, Clone, Debug)]
pub struct Stats {
    pub online_ok: u32,
    pub online_cached: u32,
    pub online_miss: u32,
    pub net_fail: u32,
    pub probe_ok: u32,
    pub probe_cached: u32,
    pub probe_fail: u32,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub tmdb_key: String,
    pub omdb_keys: Vec<String>,
    pub language: String,
    pub ffprobe_path: String,
    pub ffmpeg_path: String,
    pub ignore_folders: Vec<String>,
    pub category_keywords: Vec<(String, Vec<String>)>,
}

impl Config {
    pub fn new() -> Config {
        Config {
            tmdb_key: String::new(),
            omdb_keys: Vec::new(),
            language: "ru-RU".to_string(),
            ffprobe_path: String::new(),
            ffmpeg_path: String::new(),
            ignore_folders: vec!["X".to_string(), "XX".to_string(), "XXX".to_string()],
            category_keywords: default_keywords(),
        }
    }
}

pub struct Ctx {
    pub cfg: Config,
    pub app_dir: PathBuf,
    pub cancel: Arc<AtomicBool>,
    pub log: Box<dyn Fn(&str) + Send>,
    pub stats: Stats,
    pub last_net_err: String,
    pub last_probe_err: String,
    pub online: bool,
    pub refresh: bool,
    pub embed_logos: bool,
    pub html_lang: String, // язык страницы каталога: ru / en / uk
    pub cache: Obj,
    pub cache_dir: PathBuf,
    pub probe_cache: Obj,
    pub ffprobe: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub out_dir: PathBuf,
    pub cache_in_root: bool,
    pub ignored: Vec<String>,
    pub dead: HashSet<String>,
    pub omdb_bad: HashSet<String>,
    pub poster_cache: HashMap<PathBuf, String>,
    pub keywords: Keywords,
    pub agent: ureq::Agent,
}

impl Ctx {
    pub fn new(app_dir: &Path, cancel: Arc<AtomicBool>, log: Box<dyn Fn(&str) + Send>) -> Ctx {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent("Blockbuster/1.0")
            .build();
        let cfg = Config::new();
        let keywords = Keywords::new(&cfg.category_keywords);
        Ctx {
            cfg,
            html_lang: "ru".to_string(),
            app_dir: app_dir.to_path_buf(),
            cancel,
            log,
            stats: Stats::default(),
            last_net_err: String::new(),
            last_probe_err: String::new(),
            online: false,
            refresh: false,
            embed_logos: false,
            cache: Obj::new(),
            cache_dir: PathBuf::new(),
            probe_cache: Obj::new(),
            ffprobe: None,
            ffmpeg: None,
            out_dir: PathBuf::new(),
            cache_in_root: true,
            ignored: Vec::new(),
            dead: HashSet::new(),
            omdb_bad: HashSet::new(),
            poster_cache: HashMap::new(),
            keywords,
            agent,
        }
    }

    /// Строка в лог окна.
    pub fn say(&self, s: &str) {
        (self.log)(s);
    }

    pub fn check(&self) -> R<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(Fail::Stop)
        } else {
            Ok(())
        }
    }

    /// Подпапка кэша; если в .catalog_cache писать нельзя — во временную папку.
    pub fn cache_sub(&self, name: &str) -> PathBuf {
        let d = self.cache_dir.join(name);
        if std::fs::create_dir_all(&d).is_ok() {
            return d;
        }
        let t = std::env::temp_dir().join("Blockbuster").join(name);
        let _ = std::fs::create_dir_all(&t);
        t
    }

    pub fn rebuild_keywords(&mut self) {
        self.keywords = Keywords::new(&self.cfg.category_keywords);
    }
}

pub fn which(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for n in names {
            let c = dir.join(n);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

/// Значение JSON → список ключей: строка («a», «a, b; c»), список или их смесь.
pub fn keys_from(vals: &[Value]) -> Vec<String> {
    let sp = Rx::new(r"[,\s;]+");
    let mut out: Vec<String> = Vec::new();
    for v in vals {
        let items: Vec<String> = match v {
            Value::Array(a) => a.iter().map(|x| crate::jv::text_of(x)).collect(),
            Value::String(s) => sp.split(s, 0),
            _ => Vec::new(),
        };
        for x in items {
            let x = x.trim().to_string();
            if !x.is_empty() && !out.contains(&x) {
                out.push(x);
            }
        }
    }
    out
}
