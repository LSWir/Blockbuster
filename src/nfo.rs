//! Чтение .nfo (Kodi/Jellyfin) и поиск файлов рядом с видео.
use crate::jv::{put, Obj};
use crate::rx::Rx;
use crate::titles::IMAGE_EXTS;
use crate::util::ext_lower;
use roxmltree::{Document, Node};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn children<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Vec<Node<'a, 'i>> {
    n.children().filter(|c| c.is_element() && c.tag_name().name() == name).collect()
}

fn text_of(n: Node, tag: &str) -> String {
    match child(n, tag) {
        Some(c) => c.text().map(|t| t.trim().to_string()).unwrap_or_default(),
        None => String::new(),
    }
}

fn texts(n: Node, tag: &str) -> Vec<String> {
    children(n, tag)
        .iter()
        .filter_map(|c| c.text().map(|t| t.trim().to_string()))
        .filter(|t| !t.is_empty())
        .collect()
}

fn unique_id(n: Node, kind: &str) -> String {
    for u in children(n, "uniqueid") {
        if u.attribute("type").unwrap_or("").to_lowercase() == kind {
            if let Some(t) = u.text() {
                let t = t.trim();
                if !t.is_empty() {
                    return t.to_string();
                }
            }
        }
    }
    String::new()
}

pub fn parse_nfo(path: &Path) -> Obj {
    let mut info = Obj::new();
    let raw = match std::fs::read(path) {
        Ok(r) => r,
        Err(_) => return info,
    };
    let mut txt = String::from_utf8_lossy(&raw).into_owned();
    if txt.starts_with('\u{feff}') {
        txt = txt.trim_start_matches('\u{feff}').to_string();
    }
    let rx = Rx::new(r"(?is)<(movie|tvshow)[\s>].*?</\1>");
    let xml = match rx.find(&txt) {
        Some((a, b)) => txt[a..b].to_string(),
        None => return info,
    };
    let doc = match Document::parse(&xml) {
        Ok(d) => d,
        Err(_) => return info,
    };
    let root = doc.root_element();

    let mut imdb = text_of(root, "imdbid");
    if imdb.is_empty() {
        imdb = unique_id(root, "imdb");
    }
    let mut tmdb = text_of(root, "tmdbid");
    if tmdb.is_empty() {
        tmdb = unique_id(root, "tmdb");
    }
    let mut plot = text_of(root, "plot");
    if plot.is_empty() {
        plot = text_of(root, "outline");
    }
    let mut year = text_of(root, "year");
    if year.is_empty() {
        let mut prem = text_of(root, "premiered");
        if prem.is_empty() {
            prem = text_of(root, "releasedate");
        }
        let head: String = prem.chars().take(4).collect();
        if head.len() == 4 && head.chars().all(|c| c.is_ascii_digit()) {
            year = head;
        }
    }
    put(&mut info, "title", text_of(root, "title"));
    put(&mut info, "original", text_of(root, "originaltitle"));
    put(&mut info, "year", year);
    put(&mut info, "plot", plot);
    put(&mut info, "tagline", text_of(root, "tagline"));
    put(&mut info, "runtime", text_of(root, "runtime"));
    put(&mut info, "genres", Value::from(texts(root, "genre")));
    put(&mut info, "directors", Value::from(texts(root, "director")));
    put(&mut info, "countries", Value::from(texts(root, "country")));
    put(&mut info, "studio", text_of(root, "studio"));
    put(&mut info, "mpaa", text_of(root, "mpaa"));
    put(&mut info, "imdb", imdb);
    put(&mut info, "tmdb", tmdb);

    let mut actors: Vec<String> = Vec::new();
    for a in children(root, "actor") {
        let n = text_of(a, "name");
        if !n.is_empty() {
            actors.push(n);
        }
    }
    actors.truncate(6);
    put(&mut info, "actors", Value::from(actors));

    let mut r = text_of(root, "rating");
    if r.is_empty() {
        if let Some(rs) = child(root, "ratings") {
            if let Some(rt) = child(rs, "rating") {
                r = text_of(rt, "value");
            }
        }
    }
    let rating = match r.replace(',', ".").trim().parse::<f64>() {
        Ok(f) if !r.is_empty() => format!("{:.1}", f),
        _ => String::new(),
    };
    put(&mut info, "rating", rating);
    info
}

/// Файл .nfo для видео: «имя.nfo» → movie.nfo → единственный .nfo в папке (кроме tvshow.nfo).
pub fn find_nfo(dir: &Path, files: &[String], video_stem: &str, single_video: bool) -> Option<PathBuf> {
    let lower: HashMap<String, &String> = files.iter().map(|f| (f.to_lowercase(), f)).collect();
    let mut cand = vec![format!("{}.nfo", video_stem.to_lowercase())];
    if single_video {
        cand.push("movie.nfo".to_string());
    }
    for c in cand {
        if let Some(f) = lower.get(&c) {
            return Some(dir.join(f.as_str()));
        }
    }
    if single_video {
        let mut nfos: Vec<&String> = files
            .iter()
            .filter(|f| f.to_lowercase().ends_with(".nfo") && f.to_lowercase() != "tvshow.nfo")
            .collect();
        nfos.sort();
        if let Some(f) = nfos.first() {
            return Some(dir.join(f.as_str()));
        }
    }
    None
}

const POSTER_NAMES: [&str; 5] = ["poster", "cover", "folder", "front", "movie"];

/// Постер-файл рядом с видео: «имя-poster.jpg» / «имя.jpg» → poster/cover/folder… → любая картинка.
pub fn find_poster(dir: &Path, files: &[String], video_stem: &str, single_video: bool) -> Option<PathBuf> {
    let lower: HashMap<String, &String> = files.iter().map(|f| (f.to_lowercase(), f)).collect();
    if !video_stem.is_empty() {
        for ext in IMAGE_EXTS.iter() {
            for suffix in ["-poster", ""] {
                let k = format!("{}{}{}", video_stem, suffix, ext).to_lowercase();
                if let Some(f) = lower.get(&k) {
                    return Some(dir.join(f.as_str()));
                }
            }
        }
    }
    if !single_video {
        return None;
    }
    for name in POSTER_NAMES.iter() {
        for ext in IMAGE_EXTS.iter() {
            let k = format!("{}{}", name, ext);
            if let Some(f) = lower.get(&k) {
                return Some(dir.join(f.as_str()));
            }
        }
    }
    let mut sorted: Vec<&String> = files.iter().collect();
    sorted.sort();
    for f in sorted {
        let ext = ext_lower(f);
        let fl = f.to_lowercase();
        let bad = ["fanart", "banner", "backdrop", "thumb", "logo", "clearart", "landscape"];
        if IMAGE_EXTS.contains(&ext.as_str()) && !bad.iter().any(|x| fl.contains(x)) {
            return Some(dir.join(f.as_str()));
        }
    }
    None
}
