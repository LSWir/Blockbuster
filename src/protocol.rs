//! Ссылки catalogopen://… из HTML («Открыть фильм», «Показать на диске», «Открыть папку»).
//! Один раз регистрируется протокол в HKCU (права администратора не нужны). Обработчик работает только
//! с дисками из libraries.json (их пополняет каждый запуск) и открывает только пути внутри них.
use crate::rx::Rx;
use crate::titles::is_video;
use crate::util::{run_capture, unquote};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const PROTO: &str = "catalogopen";

fn libs_file(app_dir: &Path) -> PathBuf {
    app_dir.join("libraries.json")
}

fn norm_root(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches(|c| c == '\\' || c == '/').to_lowercase()
}

pub fn known_libraries(app_dir: &Path) -> Vec<String> {
    let txt = match std::fs::read(libs_file(app_dir)) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(_) => return Vec::new(),
    };
    match serde_json::from_str::<Value>(&txt) {
        Ok(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(),
        _ => Vec::new(),
    }
}

/// Запоминает отсканированный диск/папку: только такие корни обработчик ссылок считает «своими».
pub fn remember_library(app_dir: &Path, root: &str) -> Result<(), String> {
    let mut libs = known_libraries(app_dir);
    let n = norm_root(root);
    if !libs.iter().any(|x| norm_root(x) == n) {
        libs.push(root.to_string());
        let v = Value::from(libs);
        let s = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        std::fs::write(libs_file(app_dir), s).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// catalogopen://<действие>/<корень библиотеки>/<путь> → (действие, абсолютный путь)
pub fn resolve_open(app_dir: &Path, url: &str) -> Option<(String, PathBuf)> {
    let prefix = format!("{}://", PROTO);
    if !url.to_lowercase().starts_with(&prefix) {
        return None;
    }
    let tail = &url[prefix.len()..];
    let (action, rest) = match tail.find('/') {
        Some(i) => (&tail[..i], &tail[i + 1..]),
        None => (tail, ""),
    };
    let action = action.trim().to_lowercase();
    let (root_enc, rel) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    let root = unquote(root_enc);
    let rel_raw = Rx::new(r"[?#]").split(rel, 1).into_iter().next().unwrap_or_default();
    let rel = unquote(&rel_raw);
    let rel = rel.trim_matches('/').to_string();
    if !["show", "dir", "play"].contains(&action.as_str()) || root.is_empty() {
        return None;
    }
    let nr = norm_root(&root);
    if !known_libraries(app_dir).iter().any(|x| norm_root(x) == nr) {
        return None;
    }
    let mut target = PathBuf::from(&root);
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        // защита от выхода за пределы библиотеки
        if part == ".." || part == "." || part.contains(':') || part.contains('\\') {
            return None;
        }
        target.push(part);
    }
    if action == "show" && target.is_file() {
        return Some((action, target));
    }
    if action == "play" && target.is_file() && is_video(&target.to_string_lossy()) {
        return Some((action, target)); // запускать можно только видеофайлы из библиотеки
    }
    if action == "dir" && target.is_dir() {
        return Some((action, target));
    }
    None
}

pub fn handle_open(app_dir: &Path, url: &str) {
    let (action, target) = match resolve_open(app_dir, url) {
        Some(x) => x,
        None => return,
    };
    let t = target.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if action == "show" {
            let _ = Command::new("explorer").raw_arg(format!("/select,\"{}\"", t)).spawn();
        } else if action == "play" {
            // системное окно «Открыть с помощью…» — Windows спросит, каким плеером открыть
            let _ = Command::new("rundll32.exe").arg("shell32.dll,OpenAs_RunDLL").arg(&t).spawn();
        } else {
            let _ = Command::new("explorer").arg(&t).spawn();
        }
    }
    #[cfg(not(windows))]
    {
        let arg = if action == "show" {
            target.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or(t.clone())
        } else {
            t.clone()
        };
        let _ = Command::new("xdg-open").arg(arg).spawn();
    }
}

fn proto_command() -> String {
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    format!("\"{}\" --open \"%1\"", exe)
}

fn reg(args: &[&str]) -> Option<(bool, String)> {
    let mut c = Command::new("reg");
    c.args(args);
    let (out, _err) = run_capture(c, 15)?;
    Some((true, String::from_utf8_lossy(&out).into_owned()))
}

/// 'ok' | 'stale' | 'missing' | 'n/a'
pub fn protocol_status() -> &'static str {
    if !cfg!(windows) {
        return "n/a";
    }
    let key = format!("HKCU\\Software\\Classes\\{}\\shell\\open\\command", PROTO);
    match reg(&["query", &key, "/ve"]) {
        Some((_, out)) => {
            if out.contains("REG_SZ") {
                if out.contains(&proto_command()) { "ok" } else { "stale" }
            } else {
                "missing"
            }
        }
        None => "missing",
    }
}

/// Регистрация (или удаление) протокола в HKCU. Возвращает текст для пользователя.
pub fn register_protocol(remove: bool) -> String {
    if !cfg!(windows) {
        return "Регистрация протокола нужна только в Windows.".to_string();
    }
    let base = format!("HKCU\\Software\\Classes\\{}", PROTO);
    if remove {
        let _ = reg(&["delete", &base, "/f"]);
        return "Протокол catalogopen:// удалён.".to_string();
    }
    let cmd_key = format!("{}\\shell\\open\\command", base);
    let _ = reg(&["add", &base, "/ve", "/d", "URL:Catalog Open", "/f"]);
    let _ = reg(&["add", &base, "/v", "URL Protocol", "/d", "", "/f"]);
    let _ = reg(&["add", &cmd_key, "/ve", "/d", &proto_command(), "/f"]);
    "Готово: кнопки «Открыть фильм», «Показать на диске» и «Открыть папку» включены для всех дисков, которые вы сканировали. \
При первом нажатии браузер спросит разрешение — отметьте «Всегда разрешать»."
        .to_string()
}
