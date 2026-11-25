//! Общие мелочи: строки, URL-кодирование, запуск внешних программ без окна, пути.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Python: s.strip(chars)
pub fn strip_chars<'a>(s: &'a str, chars: &str) -> &'a str {
    s.trim_matches(|c: char| chars.contains(c))
}

pub fn human_size(n: u64) -> String {
    let mut x = n as f64;
    let units = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
    for (i, u) in units.iter().enumerate() {
        if x < 1024.0 || i == units.len() - 1 {
            return if i == 0 { format!("{} {}", n, u) } else { format!("{:.1} {}", x, u) };
        }
        x /= 1024.0;
    }
    String::new()
}

pub fn human_duration(minutes: f64) -> String {
    let m = minutes.round() as i64;
    if m <= 0 {
        return String::new();
    }
    let (h, mm) = (m / 60, m % 60);
    if h > 0 {
        format!("{} ч {:02} мин", h, mm)
    } else {
        format!("{} мин", mm)
    }
}

fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-' || b == b'~'
}

/// urllib.parse.quote (safe="/")
pub fn quote(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        if unreserved(b) || b == b'/' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn quote_plus(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        if unreserved(b) {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// urllib.parse.urlencode
pub fn urlencode(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", quote_plus(k), quote_plus(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// urllib.parse.unquote
pub fn unquote(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|x| u8::from_str_radix(x, 16).ok());
            if let Some(v) = h {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn md5hex(s: &str) -> String {
    format!("{:x}", md5::compute(s.as_bytes()))
}

#[cfg(windows)]
pub fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: чёрное окно ffprobe не мигает
}

#[cfg(not(windows))]
pub fn hide_window(_cmd: &mut Command) {}

/// Запускает программу, ждёт не дольше `secs` секунд, возвращает (stdout, stderr).
pub fn run_capture(mut cmd: Command, secs: u64) -> Option<(Vec<u8>, Vec<u8>)> {
    hide_window(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut so = child.stdout.take()?;
    let mut se = child.stderr.take()?;
    let h1 = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = so.read_to_end(&mut b);
        b
    });
    let h2 = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = se.read_to_end(&mut b);
        b
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed().as_secs() >= secs {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => break,
        }
    }
    let out = h1.join().unwrap_or_default();
    let err = h2.join().unwrap_or_default();
    Some((out, err))
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn ext_lower(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_lowercase(),
        _ => String::new(),
    }
}

pub fn stem_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].to_string(),
        _ => name.to_string(),
    }
}

/// Путь относительно корня, с «/» в качестве разделителя; "" — это сам корень.
pub fn rel_str(root: &Path, p: &Path) -> String {
    match p.strip_prefix(root) {
        Ok(r) => r
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => p.to_string_lossy().replace('\\', "/"),
    }
}

/// Родительская «папка» для строки вида a/b/c.mkv → a/b
pub fn dirname_rel(rel: &str) -> String {
    match rel.rfind('/') {
        Some(i) => rel[..i].to_string(),
        None => String::new(),
    }
}

/// Общий родитель набора путей (os.path.commonpath).
pub fn common_path(dirs: &[PathBuf]) -> PathBuf {
    if dirs.is_empty() {
        return PathBuf::new();
    }
    let mut base: Vec<std::ffi::OsString> = dirs[0].components().map(|c| c.as_os_str().to_os_string()).collect();
    for d in &dirs[1..] {
        let comps: Vec<std::ffi::OsString> = d.components().map(|c| c.as_os_str().to_os_string()).collect();
        let mut n = 0;
        while n < base.len() && n < comps.len() && base[n] == comps[n] {
            n += 1;
        }
        base.truncate(n);
    }
    let mut out = PathBuf::new();
    for c in base {
        out.push(c);
    }
    out
}

pub fn mtime_secs(md: &std::fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Проверка записи реальным созданием файла (права Windows бывают хитрыми).
pub fn can_write(d: &Path) -> bool {
    if std::fs::create_dir_all(d).is_err() {
        return false;
    }
    let t = d.join(".bb_write_test");
    match std::fs::write(&t, b"x") {
        Ok(_) => {
            let _ = std::fs::remove_file(&t);
            true
        }
        Err(_) => false,
    }
}

pub fn list_dir(path: &Path) -> Vec<String> {
    match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Открывает файл или папку программой по умолчанию (Проводник / браузер / Блокнот).
pub fn open_path(p: &Path) {
    #[cfg(windows)]
    {
        let _ = Command::new("explorer").arg(p).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(p).spawn();
    }
}

/// Класс строки лога для раскраски: 0 обычная, 1 ошибка, 2 успех, 3 приглушённая.
pub fn classify_line(line: &str) -> i32 {
    let l = line.to_lowercase();
    let t = line.trim_start();
    if l.contains("ошибк") || l.contains("traceback") || t.starts_with('!') || l.contains("не удалось") || l.contains("нет связи") {
        1
    } else if line.contains("Готово") || line.contains("[OK]") || t.starts_with("+ ") {
        2
    } else if t.starts_with('(') {
        3
    } else {
        0
    }
}

/// Открывает ссылку в браузере по умолчанию.
pub fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    #[cfg(windows)]
    {
        let _ = Command::new("explorer").arg(url).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}
