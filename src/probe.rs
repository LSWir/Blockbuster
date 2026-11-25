//! ffprobe: длительность, разрешение, кодеки, HDR, аудиодорожки и субтитры; вложенные обложки (ffmpeg).
use crate::ctx::{Ctx, R};
use crate::jv::{put, so, truthy, Obj};
use crate::rx::Rx;
use crate::titles::IMAGE_EXTS;
use crate::util::{ext_lower, md5hex, run_capture};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const PROBE_VERSION: i64 = 6; // при смене версии кэш ffprobe пересоздаётся

fn lang_name(code: &str) -> String {
    let c = code.trim().to_lowercase();
    if c.is_empty() || c == "und" {
        return String::new();
    }
    let name = match c.as_str() {
        "rus" => "Русский", "eng" => "Английский", "ukr" => "Украинский", "bel" => "Белорусский",
        "ron" | "rum" => "Румынский", "fra" | "fre" => "Французский", "deu" | "ger" => "Немецкий",
        "spa" => "Испанский", "ita" => "Итальянский", "por" => "Португальский", "jpn" => "Японский",
        "kor" => "Корейский", "zho" | "chi" => "Китайский", "pol" => "Польский", "ces" | "cze" => "Чешский",
        "slk" | "slo" => "Словацкий", "hun" => "Венгерский", "bul" => "Болгарский", "srp" => "Сербский",
        "hrv" => "Хорватский", "tur" => "Турецкий", "ara" => "Арабский", "heb" => "Иврит", "hin" => "Хинди",
        "tha" => "Тайский", "nld" | "dut" => "Нидерландский", "swe" => "Шведский", "nor" => "Норвежский",
        "dan" => "Датский", "fin" => "Финский", "ell" | "gre" => "Греческий", "lav" => "Латышский",
        "lit" => "Литовский", "est" => "Эстонский", "kaz" => "Казахский", "aze" => "Азербайджанский",
        "hye" => "Армянский", "kat" | "geo" => "Грузинский", "vie" => "Вьетнамский", "ind" => "Индонезийский",
        "fas" | "per" => "Персидский",
        _ => "",
    };
    if name.is_empty() {
        c.to_uppercase()
    } else {
        name.to_string()
    }
}

fn sub_codec(cn: &str) -> String {
    match cn {
        "subrip" | "srt" => "SRT".to_string(),
        "ass" => "ASS".to_string(),
        "ssa" => "SSA".to_string(),
        "hdmv_pgs_subtitle" => "PGS".to_string(),
        "dvd_subtitle" => "VobSub".to_string(),
        "mov_text" => "TX3G".to_string(),
        "webvtt" => "VTT".to_string(),
        "dvb_subtitle" => "DVB".to_string(),
        "text" => "TXT".to_string(),
        other => other.to_uppercase(),
    }
}

fn gs(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn chan_label(s: &Value) -> String {
    let layout = gs(s, "channel_layout");
    if let Some(c) = Rx::new(r"^(\d)\.(\d)").caps(&layout) {
        return format!("{}.{}", c.get(1).unwrap_or(""), c.get(2).unwrap_or(""));
    }
    let ch = s.get("channels").and_then(|x| x.as_i64()).unwrap_or(0);
    match ch {
        0 => String::new(),
        1 => "1.0".to_string(),
        2 => "2.0".to_string(),
        3 => "2.1".to_string(),
        4 => "4.0".to_string(),
        5 => "5.0".to_string(),
        6 => "5.1".to_string(),
        7 => "6.1".to_string(),
        8 => "7.1".to_string(),
        n => format!("{} ch", n),
    }
}

/// → (ключ для значка/логотипа, подпись)
pub fn audio_codec(codec: &str, profile: &str, title: &str) -> (String, String) {
    let c = codec.to_lowercase();
    let p = profile.to_lowercase();
    let atmos = p.contains("atmos") || title.to_lowercase().contains("atmos");
    let r = |a: &str, b: &str| (a.to_string(), b.to_string());
    match c.as_str() {
        "truehd" => {
            if atmos { r("truehd_atmos", "TrueHD Atmos") } else { r("truehd", "TrueHD") }
        }
        "eac3" => {
            if atmos { r("eac3_atmos", "E-AC-3 Atmos") } else { r("eac3", "E-AC-3") }
        }
        "ac3" => r("ac3", "AC-3"),
        "dts" => {
            if p.contains("dts:x") || p.contains("dts-x") {
                r("dtsx", "DTS:X")
            } else if p.contains("dts-hd ma") {
                r("dtshd_ma", "DTS-HD MA")
            } else if p.contains("dts-hd") {
                r("dtshd_hra", "DTS-HD HRA")
            } else if p.contains("es") {
                r("dts", "DTS-ES")
            } else {
                r("dts", "DTS")
            }
        }
        "aac" => {
            if p.contains("he") { r("aac", "HE-AAC") } else { r("aac", "AAC") }
        }
        _ if c.starts_with("pcm") => r("pcm", "PCM"),
        _ => {
            let name = match c.as_str() {
                "flac" => "FLAC", "opus" => "Opus", "vorbis" => "Vorbis", "mp3" => "MP3", "mp2" => "MP2",
                "alac" => "ALAC",
                _ => "",
            };
            let label = if name.is_empty() { c.to_uppercase() } else { name.to_string() };
            (c.clone(), label)
        }
    }
}

fn video_info(s: &Value) -> Value {
    let cn = gs(s, "codec_name").to_lowercase();
    let (key, label) = match cn.as_str() {
        "h264" => ("h264", "H.264".to_string()),
        "hevc" => ("hevc", "H.265".to_string()),
        "av1" => ("av1", "AV1".to_string()),
        "vp9" => ("vp9", "VP9".to_string()),
        "mpeg2video" => ("mpeg2", "MPEG-2".to_string()),
        "mpeg4" => ("mpeg4", "MPEG-4".to_string()),
        "vc1" => ("vc1", "VC-1".to_string()),
        "wmv3" => ("vc1", "WMV".to_string()),
        _ => ("", cn.to_uppercase()),
    };
    let key = if key.is_empty() { cn.clone() } else { key.to_string() };
    let pix = gs(s, "pix_fmt");
    let bits = gs(s, "bits_per_raw_sample");
    let depth: i64 = match bits.parse::<i64>() {
        Ok(b) if !bits.is_empty() => b,
        _ => {
            if pix.contains("12") { 12 } else if pix.contains("10") { 10 } else { 8 }
        }
    };
    let mut hdr: Vec<String> = Vec::new();
    if let Some(Value::Array(list)) = s.get("side_data_list") {
        let dv = list.iter().any(|x| {
            let t = gs(x, "side_data_type").to_lowercase();
            t.contains("dovi") || t.contains("dolby vision")
        });
        if dv {
            hdr.push("dv".to_string());
        }
    }
    let ct = gs(s, "color_transfer");
    if ct == "smpte2084" {
        hdr.push("hdr10".to_string());
    } else if ct == "arib-std-b67" {
        hdr.push("hlg".to_string());
    }
    let mut o = Obj::new();
    put(&mut o, "key", key);
    put(&mut o, "codec", label);
    put(&mut o, "bits", depth);
    put(&mut o, "hdr", Value::from(hdr));
    Value::Object(o)
}

fn tag_map(s: &Value) -> Obj {
    let mut m = Obj::new();
    if let Some(Value::Object(t)) = s.get("tags") {
        for (k, v) in t {
            m.insert(k.to_lowercase(), v.clone());
        }
    }
    m
}

fn split_list(s: &str) -> Vec<String> {
    s.split(|c| c == ',' || c == ';' || c == '/').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// Теги Matroska → поля как у .nfo: title, year, imdb, plot, genres, directors, actors, mpaa.
pub fn tag_info(pr: &Obj) -> Obj {
    let mut o = Obj::new();
    let t = match pr.get("mkv_tags") {
        Some(Value::Object(t)) => t,
        _ => return o,
    };
    let g = |k: &str| -> String { t.get(k).map(crate::jv::text_of).unwrap_or_default() };
    let title = g("title");
    if !title.is_empty() {
        put(&mut o, "title", title);
    }
    let date = g("date_released");
    let date = if date.is_empty() { g("date_release") } else { date };
    let year: String = date.chars().take(4).collect();
    if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) {
        put(&mut o, "year", year);
    }
    let imdb = g("imdb");
    if imdb.starts_with("tt") {
        put(&mut o, "imdb", imdb);
    }
    let plot = {
        let s = g("summary");
        if s.is_empty() { g("description") } else { s }
    };
    if !plot.is_empty() {
        put(&mut o, "plot", plot);
    }
    let genres = split_list(&g("genre"));
    if !genres.is_empty() {
        put(&mut o, "genres", Value::from(genres));
    }
    let dirs = split_list(&g("director"));
    if !dirs.is_empty() {
        put(&mut o, "directors", Value::from(dirs));
    }
    let mut actors = split_list(&g("actor"));
    actors.truncate(6);
    if !actors.is_empty() {
        put(&mut o, "actors", Value::from(actors));
    }
    let r = g("law_rating");
    if !r.is_empty() {
        put(&mut o, "mpaa", r);
    }
    o
}

fn disp_flag(s: &Value, k: &str) -> bool {
    match s.get("disposition") {
        Some(d) => d.get(k).map(truthy).unwrap_or(false),
        None => false,
    }
}

fn audio_track(s: &Value, tags: &Obj) -> Value {
    let (key, label) = audio_codec(&gs(s, "codec_name"), &gs(s, "profile"), &so(tags, "title"));
    // битрейт — из тех же данных ffprobe: bit_rate или тег BPS (его пишет mkvmerge)
    let mut br: i64 = 0;
    let cands: Vec<String> = vec![gs(s, "bit_rate"), so(tags, "bps"), so(tags, "bps-eng"), so(tags, "bit_rate")];
    for v in cands {
        if let Ok(f) = v.trim().parse::<f64>() {
            br = f as i64;
            if br > 0 {
                break;
            }
        }
    }
    let title: String = so(tags, "title").chars().take(60).collect();
    let mut o = Obj::new();
    put(&mut o, "lang", lang_name(&so(tags, "language")));
    put(&mut o, "key", key);
    put(&mut o, "codec", label);
    put(&mut o, "ch", chan_label(s));
    put(&mut o, "kbps", if br > 0 { ((br as f64) / 1000.0).round() as i64 } else { 0 });
    put(&mut o, "title", title);
    put(&mut o, "default", disp_flag(s, "default"));
    Value::Object(o)
}

fn sub_track(s: &Value, tags: &Obj) -> Value {
    let cn = gs(s, "codec_name");
    let title: String = so(tags, "title").chars().take(60).collect();
    let mut o = Obj::new();
    put(&mut o, "lang", lang_name(&so(tags, "language")));
    put(&mut o, "codec", sub_codec(&cn));
    put(&mut o, "title", title);
    put(&mut o, "forced", disp_flag(s, "forced"));
    put(&mut o, "default", disp_flag(s, "default"));
    put(&mut o, "sdh", disp_flag(s, "hearing_impaired"));
    Value::Object(o)
}

/// Выбор постера среди вложений MKV: cover/poster > small_cover > любая другая картинка.
fn pick_cover(atts: &[(i64, String, String)], pics: &[(i64, String)]) -> Value {
    let bad = ["land", "fanart", "banner", "backdrop", "thumb", "logo", "clearart"];
    let mut best: Option<(i64, String)> = None;
    let mut best_score = 0;
    for (t, name, mime) in atts {
        let name = name.to_lowercase();
        let ext = ext_lower(&name);
        let stem = crate::util::stem_of(&name);
        if !(mime.to_lowercase().starts_with("image/") || IMAGE_EXTS.contains(&ext.as_str())) {
            continue;
        }
        if bad.iter().any(|b| stem.contains(b)) {
            continue;
        }
        let score = if ["cover", "poster", "folder", "front"].contains(&stem.as_str()) {
            3
        } else if stem.contains("cover") || stem.contains("poster") {
            2
        } else {
            1
        };
        if score > best_score {
            let e = if IMAGE_EXTS.contains(&ext.as_str()) { ext.clone() } else { ".jpg".to_string() };
            best = Some((*t, e));
            best_score = score;
        }
    }
    if let Some((t, ext)) = best {
        let mut o = Obj::new();
        put(&mut o, "kind", "att");
        put(&mut o, "t", t);
        put(&mut o, "ext", ext);
        return Value::Object(o);
    }
    if let Some((index, codec)) = pics.first() {
        let mut o = Obj::new();
        put(&mut o, "kind", "pic");
        put(&mut o, "index", *index);
        put(&mut o, "ext", if codec == "png" { ".png" } else { ".jpg" });
        return Value::Object(o);
    }
    Value::Null
}

/// Разбор JSON-ответа ffprobe (отдельная функция — чтобы её можно было тестировать без ffprobe).
pub fn parse_probe(data: &Value) -> Option<Obj> {
    let streams = match data.get("streams") {
        Some(Value::Array(a)) if !a.is_empty() => a,
        _ => return None,
    };
    let mut res = Obj::new();
    put(&mut res, "_v", PROBE_VERSION);
    // глобальные теги Matroska (MOVIE/TITLE, MOVIE/IMDB, MOVIE/SUMMARY …) — ключи без префикса цели, в нижнем регистре
    let mut mt = Obj::new();
    if let Some(Value::Object(t)) = data.get("format").and_then(|f| f.get("tags")) {
        for (k, v) in t {
            let key = k.rsplit('/').next().unwrap_or(k).to_lowercase();
            if let Value::String(sv) = v {
                if !sv.trim().is_empty() && !mt.contains_key(&key) {
                    mt.insert(key, Value::String(sv.trim().to_string()));
                }
            }
        }
    }
    put(&mut res, "mkv_tags", Value::Object(mt));
    let dur = data
        .get("format")
        .and_then(|f| f.get("duration"))
        .map(|d| match d {
            Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
            Value::Number(n) => n.as_f64().unwrap_or(0.0),
            _ => 0.0,
        })
        .unwrap_or(0.0);
    if dur > 0.0 {
        put(&mut res, "runtime_min", dur / 60.0);
    }
    let mut langs: Vec<String> = Vec::new();
    let mut atts: Vec<(i64, String, String)> = Vec::new();
    let mut pics: Vec<(i64, String)> = Vec::new();
    let mut atracks: Vec<Value> = Vec::new();
    let mut stracks: Vec<Value> = Vec::new();
    for s in streams {
        let ct = gs(s, "codec_type");
        let tags = tag_map(s);
        let attached_pic = s.get("disposition").and_then(|d| d.get("attached_pic")).and_then(|x| x.as_i64()) == Some(1);
        if ct == "attachment" {
            atts.push((atts.len() as i64, so(&tags, "filename"), so(&tags, "mimetype")));
        } else if ct == "video" && attached_pic {
            pics.push((s.get("index").and_then(|x| x.as_i64()).unwrap_or(0), gs(s, "codec_name")));
        } else if ct == "video" && !res.contains_key("resolution") {
            let w = s.get("width").and_then(|x| x.as_i64()).unwrap_or(0);
            let h = s.get("height").and_then(|x| x.as_i64()).unwrap_or(0);
            if w > 0 && h > 0 {
                put(&mut res, "resolution", format!("{}×{}", w, h));
                let q = if w >= 3000 { "4K" } else if w >= 1800 { "1080p" } else if w >= 1200 { "720p" } else { "SD" };
                put(&mut res, "quality", q);
            }
            put(&mut res, "vcodec", gs(s, "codec_name").to_uppercase());
            put(&mut res, "vinfo", video_info(s));
        } else if ct == "audio" {
            let lang = so(&tags, "language");
            if !lang.is_empty() && !langs.contains(&lang) && lang != "und" {
                langs.push(lang);
            }
            atracks.push(audio_track(s, &tags));
        } else if ct == "subtitle" {
            stracks.push(sub_track(s, &tags));
        }
    }
    put(&mut res, "audio", Value::from(langs));
    put(&mut res, "audio_tracks", Value::Array(atracks));
    put(&mut res, "sub_tracks", Value::Array(stracks));
    put(&mut res, "cover", pick_cover(&atts, &pics));
    Some(res)
}

impl Ctx {
    /// ffprobe одного файла; пустой объект при ошибке.
    fn probe_run(&mut self, path: &Path) -> Obj {
        let ffprobe = match &self.ffprobe {
            Some(p) => p.clone(),
            None => return Obj::new(),
        };
        let mut cmd = Command::new(&ffprobe);
        cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path);
        let (out, err) = match run_capture(cmd, 30) {
            Some(x) => x,
            None => {
                self.stats.probe_fail += 1;
                self.last_probe_err = "не удалось запустить ffprobe".to_string();
                return Obj::new();
            }
        };
        let data: Value = match serde_json::from_slice(&out) {
            Ok(v) => v,
            Err(e) => {
                self.stats.probe_fail += 1;
                let first = String::from_utf8_lossy(&err).lines().next().unwrap_or("").to_string();
                self.last_probe_err = if first.is_empty() { format!("{}", e) } else { first };
                return Obj::new();
            }
        };
        match parse_probe(&data) {
            Some(r) => {
                self.stats.probe_ok += 1;
                r
            }
            None => {
                self.stats.probe_fail += 1;
                let first = String::from_utf8_lossy(&err).lines().next().unwrap_or("").to_string();
                let first: String = first.chars().take(200).collect();
                self.last_probe_err = if first.is_empty() { "ffprobe вернул пустой ответ".to_string() } else { first };
                Obj::new()
            }
        }
    }

    /// ffprobe с кэшем (ключ: путь + размер + дата).
    pub fn probe_cached(&mut self, path: &Path, rel: &str, size: u64, mtime: i64) -> R<Obj> {
        self.check()?;
        if self.ffprobe.is_none() {
            return Ok(Obj::new());
        }
        let key = format!("{}|{}|{}", rel, size, mtime);
        if let Some(Value::Object(c)) = self.probe_cache.get(&key) {
            if c.get("_v").and_then(|x| x.as_i64()) == Some(PROBE_VERSION) {
                let c = c.clone();
                self.stats.probe_cached += 1;
                return Ok(c);
            }
        }
        let pr = self.probe_run(path);
        if !pr.is_empty() {
            self.probe_cache.insert(key, Value::Object(pr.clone()));
        }
        Ok(pr)
    }

    /// Достаёт обложку, вшитую в видеофайл (через ffmpeg), в .catalog_cache/covers.
    pub fn embedded_cover(&mut self, video: &Path, rel: &str, size: u64, mtime: i64, pr: &Obj) -> Option<PathBuf> {
        let desc = match pr.get("cover") {
            Some(Value::Object(d)) => d.clone(),
            _ => return None,
        };
        let ffmpeg = self.ffmpeg.clone()?;
        let cdir = self.cache_sub("covers");
        let ext = so(&desc, "ext");
        let out = cdir.join(format!("{}{}", md5hex(&format!("{}|{}|{}", rel, size, mtime)), ext));
        if std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false) {
            return Some(out);
        }
        let mut cmd = Command::new(&ffmpeg);
        cmd.args(["-y", "-v", "error"]);
        if so(&desc, "kind") == "att" {
            // для -dump_attachment ffmpeg ругается на отсутствие выходного файла, но вложение уже записано
            cmd.arg(format!("-dump_attachment:t:{}", desc.get("t").and_then(|x| x.as_i64()).unwrap_or(0)));
            cmd.arg(&out).arg("-i").arg(video);
        } else {
            cmd.arg("-i").arg(video);
            cmd.arg("-map").arg(format!("0:{}", desc.get("index").and_then(|x| x.as_i64()).unwrap_or(0)));
            cmd.args(["-frames:v", "1", "-c", "copy"]).arg(&out);
        }
        let _ = run_capture(cmd, 60);
        if std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false) {
            Some(out)
        } else {
            None
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mkv_tags_to_info() {
        let v: Value = serde_json::from_str(
            r#"{"mkv_tags":{"title":"The Bone Collector","date_released":"1999-11-05","imdb":"tt0145681","genre":"Crime, Drama","actor":"A, B","summary":"Plot","law_rating":"R"}}"#,
        )
        .unwrap();
        let pr = v.as_object().unwrap().clone();
        let t = tag_info(&pr);
        assert_eq!(so(&t, "title"), "The Bone Collector");
        assert_eq!(so(&t, "year"), "1999");
        assert_eq!(so(&t, "imdb"), "tt0145681");
        assert_eq!(so(&t, "mpaa"), "R");
        assert_eq!(t.get("genres").and_then(|x| x.as_array()).map(|a| a.len()), Some(2));
    }
}
