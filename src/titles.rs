//! Разбор имён папок и файлов: название, год, серии, сезоны, разделы.
use crate::rx::{escape, Rx};
use crate::util::strip_chars;

pub const VIDEO_EXTS: [&str; 11] =
    [".mkv", ".mp4", ".avi", ".mov", ".wmv", ".m4v", ".ts", ".m2ts", ".webm", ".mpg", ".mpeg"];
pub const IMAGE_EXTS: [&str; 4] = [".jpg", ".jpeg", ".png", ".webp"];
pub const SKIP_DIRS: [&str; 17] = [
    "extras", "extra", "bonus", "bonuses", "special features", "featurettes", "trailers", "sample", "samples",
    "behind the scenes", "bd menu", "cm&pv", "@eadir", "$recycle.bin", ".git", ".catalog_cache", "logos",
];
pub const SKIP_DIRS_EXTRA: [&str; 2] = ["system volume information", "recycler"];

pub const CATEGORY_LABELS: [(&str, &str); 4] = [
    ("movies", "Фильмы"),
    ("series", "Сериалы"),
    ("cartoons", "Мультфильмы"),
    ("docs", "Документальное"),
];

pub fn category_label(id: &str) -> &'static str {
    for (k, v) in CATEGORY_LABELS.iter() {
        if *k == id {
            return v;
        }
    }
    "Фильмы"
}

pub fn is_skip_dir(lower: &str) -> bool {
    SKIP_DIRS.contains(&lower) || SKIP_DIRS_EXTRA.contains(&lower)
}

pub fn is_video(name: &str) -> bool {
    let e = crate::util::ext_lower(name);
    VIDEO_EXTS.contains(&e.as_str())
}

pub fn default_keywords() -> Vec<(String, Vec<String>)> {
    let mk = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    vec![
        ("docs".to_string(), mk(&["documentary", "documentaries", "docu", "nature", "документ", "природ"])),
        ("cartoons".to_string(), mk(&["cartoon", "animation", "animated", "мульт"])),
        ("series".to_string(), mk(&["tvshow", "tv show", "tv_show", "tv-show", "series", "сериал"])),
    ]
}

// ───────────── регулярные выражения ─────────────
lazy_rx!(year_rx, r"(?<!\d)(?:19|20)\d{2}(?!\d)");
lazy_rx!(
    tech_rx,
    r"[\s._\-\(\[]+(?:2160p|1080p|720p|480p|bluray|bdrip|brrip|web[-.]?dl|webrip|hdrip|hddvd|hdtv|uhd|remux|dvdrip|x264|x265|hevc|h\.?264|h\.?265)",
    i
);
lazy_rx!(ep_s_e, r"(?<![A-Za-z0-9])s(\d{1,2})[\s._-]*e(\d{1,3})(?:[\s._-]?e\d{1,3})*", i);
lazy_rx!(ep_nxm, r"(?<![A-Za-z0-9])(\d{1,2})x(\d{2,3})(?!\d)", i);
lazy_rx!(ep_season_ep, r"season[\s._-]*(\d{1,2})[\s._-]*(?:episode|ep)[\s._-]*(\d{1,3})", i);
lazy_rx!(ep_weak1, r"(?:^|[\s._\-\[\(])(?:episode|ep|e|эпизод|эп|серия|серії)[\s._-]*(\d{1,3})(?!\d)", i);
lazy_rx!(ep_weak2, r"(?<!\d)(\d{1,3})[\s._-]*(?:серия|серії|эпизод|episode)", i);
lazy_rx!(ep_lead, r"^\s*(\d{1,3})(?!\d)");
lazy_rx!(
    season_range_rx,
    r"(?:^|[\s._\-\[\(])(?:seasons?|сезоны?|series|s)[\s._-]*\d{1,2}(?:[\s._]*(?:-|–|—|to|и|,|&)[\s._]*(?:seasons?|сезон\w*|s)?[\s._-]*\d{1,2})*(?=$|[\s._\-\]\)])",
    i
);
lazy_rx!(season_dir_rx, r"(?:^|[\s._\-\[\(])(?:season|сезон|series|s)[\s._-]*(\d{1,2})(?=$|[\s._\-\]\)])", i);
lazy_rx!(specials_rx, r"^\s*(?:specials?|спецвыпуски?)\s*$", i);
lazy_rx!(tags_rx, r"\s*\[[^\]]*\]");
lazy_rx!(two_letters_rx, r"[^\W\d_]{2}");
lazy_rx!(lead_num_rx, r"^0\d[\s._-]+(?=\D)");
lazy_rx!(year_tail_rx, r"[\s._\(\[\-]+((?:19|20)\d{2})(?=[\s._\)\]\-–—,]|$)");
lazy_rx!(
    edition_rx,
    r"(?:[\s._\-\(]+(?:open[\s._]?matte|unrated(?:[\s._]version)?|remastered|imax|hybrid|(?:director'?s|extended|ext|theatrical|final)[\s._]cut|ext))+[\s)\]]*$",
    i
);
lazy_rx!(glue_rx, r"(?<=[a-zа-я])(?:BDRip|BRRip|BluRay|WEB-?DL(?:Rip)?|WebRip|HDRip|DVDRip|HDTV)\w*$", i);
lazy_rx!(sep_rx, r"[._]+");
lazy_rx!(camel_rx, r"(?<=[a-zа-я])(?=[A-ZА-Я])");
lazy_rx!(non_word_rx, r"[\W_]+");
lazy_rx!(letter_rx, r"[^\W\d_]");

pub fn has_year(s: &str) -> bool {
    year_rx().is_match(s)
}

/// Эпизод: (сезон, серия, диапазон совпадения, «сильное» ли совпадение)
pub type EpInfo = (Option<i64>, Option<i64>, Option<(usize, usize)>, bool);

pub fn episode_info(stem: &str) -> EpInfo {
    for rx in [ep_s_e(), ep_nxm(), ep_season_ep()] {
        if let Some(c) = rx.caps(stem) {
            return (c.num(1), c.num(2), Some((c.start(0), c.end(0))), true);
        }
    }
    for rx in [ep_weak1(), ep_weak2()] {
        if let Some(c) = rx.caps(stem) {
            return (None, c.num(1), Some((c.start(0), c.end(0))), false);
        }
    }
    if !has_year(stem) {
        if let Some(c) = ep_lead().caps(stem) {
            return (None, c.num(1), Some((c.start(0), c.end(0))), false);
        }
    }
    (None, None, None, false)
}

pub fn lead_matches(stem: &str) -> bool {
    ep_lead().is_match(stem)
}

pub fn episode_title(stem: &str, m: Option<(usize, usize)>) -> String {
    let s = match m {
        Some((_, e)) => &stem[e..],
        None => stem,
    };
    let s = tech_rx().head(s);
    let s = sep_rx().replace(s, " ");
    strip_chars(&s, " -–—[]()").to_string()
}

pub fn norm_title(s: &str) -> String {
    non_word_rx().replace(&s.to_lowercase(), "")
}

/// Вырезает из строки «S01», «Season 2», «Сезоны 1-3» (заменяя пробелом).
pub fn strip_season(name: &str) -> String {
    season_range_rx().replace(name, " ")
}

/// Название сериала из имени папки сезона: 'Gen V (Season 1) WEB-DL 1080p' → ('Gen V',''); 'Season 1' → ('','').
pub fn season_dir_title(name: &str) -> (String, String) {
    let m = match season_range_rx().find(name) {
        Some(m) => m,
        None => return (String::new(), String::new()),
    };
    let before = strip_chars(&name[..m.0], " ._-[]()").to_string();
    if !letter_rx().is_match(&before) {
        return (String::new(), String::new());
    }
    clean_title(&before)
}

pub fn season_of_dir(name: &str) -> Option<i64> {
    if let Some(c) = season_dir_rx().caps(name) {
        return c.num(1);
    }
    if specials_rx().is_match(name) {
        return Some(0);
    }
    None
}

// ───────────── разделы ─────────────
pub struct Keywords {
    list: Vec<(String, Rx)>,
}

impl Keywords {
    pub fn new(src: &[(String, Vec<String>)]) -> Keywords {
        let mut list = Vec::new();
        for (cat, kws) in src {
            let parts: Vec<String> = kws.iter().filter(|k| !k.is_empty()).map(|k| escape(&k.to_lowercase())).collect();
            if !parts.is_empty() {
                list.push((cat.clone(), Rx::ci(&format!("(?<![a-zа-яё0-9])(?:{})", parts.join("|")))));
            }
        }
        Keywords { list }
    }

    /// Какие разделы упоминаются в именах папок (папка с годом — это фильм/сериал, а не раздел).
    pub fn flags(&self, names: &[String]) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for n in names {
            if has_year(n) {
                continue;
            }
            for (cat, rx) in &self.list {
                if rx.is_match(n) && !found.contains(cat) {
                    found.push(cat.clone());
                }
            }
        }
        found
    }
}

pub fn pick_category(flags: &[String]) -> String {
    for c in ["docs", "cartoons", "series"] {
        if flags.iter().any(|f| f == c) {
            return c.to_string();
        }
    }
    "movies".to_string()
}

// ───────────── название и год ─────────────
pub fn clean_title(raw: &str) -> (String, String) {
    let mut s = raw.to_string();
    // теги в [] — не часть названия; если после удаления букв не остаётся («[REC] (2007)») — оставляем как есть
    let no_tags = tags_rx().replace(&s, "");
    if two_letters_rx().is_match(&no_tags) {
        s = no_tags;
    }
    let t = lead_num_rx().replace(&s, "");
    if !t.is_empty() {
        s = t;
    }
    // год в скобках важнее: «Blade Runner 2049 (2017)» → 2017; без скобок — последний
    let ms = year_tail_rx().all(&s);
    let mut paren: Option<usize> = None;
    for (i, c) in ms.iter().enumerate() {
        let before = s[..c.start(1)].chars().last().unwrap_or(' ');
        if before == '(' || before == '[' {
            paren = Some(i);
            break;
        }
    }
    let pick = match paren {
        Some(i) => Some(i),
        None => {
            if ms.is_empty() {
                None
            } else {
                Some(ms.len() - 1)
            }
        }
    };
    let mut year = String::new();
    match pick {
        Some(i) => {
            year = ms[i].get(1).unwrap_or("").to_string();
            let cut = ms[i].start(0);
            s.truncate(cut);
        }
        None => {
            let h = tech_rx().head(&s).to_string();
            s = h;
        }
    }
    s = edition_rx().replace(&s, "");
    s = glue_rx().replace(&s, "");
    s = sep_rx().replace(&s, " ");
    s = strip_chars(&s, " -[]()").to_string();
    if !s.contains(' ') && camel_rx().all(&s).len() >= 2 {
        s = camel_rx().replace(&s, " ");
    }
    if s.is_empty() {
        (raw.to_string(), year)
    } else {
        (s, year)
    }
}

// ───────────── варианты названия для поиска ─────────────
const TR_MULTI: [(&str, &str); 24] = [
    ("iya", "ия"), ("iyu", "ию"), ("shch", "щ"), ("sch", "щ"), ("yo", "ё"), ("jo", "ё"), ("yu", "ю"), ("ju", "ю"),
    ("ya", "я"), ("ja", "я"), ("ye", "е"), ("zh", "ж"), ("kh", "х"), ("ts", "ц"), ("tc", "ц"), ("ch", "ч"),
    ("sh", "ш"), ("yj", "ый"), ("yi", "ый"), ("iy", "ий"), ("ij", "ий"), ("ey", "ей"), ("ay", "ай"), ("oy", "ой"),
];

/// Python: dict(zip("abvwgdezijklmnoprstufhcyxq", "абввгдезийклмнопрстуфхкыкк")), "x" → "кс"
fn tr_one(c: char) -> String {
    let lat: Vec<char> = "abvwgdezijklmnoprstufhcyxq".chars().collect();
    let cyr: Vec<char> = "абввгдезийклмнопрстуфхкыкк".chars().collect();
    if c == 'x' {
        return "кс".to_string();
    }
    for (i, l) in lat.iter().enumerate() {
        if *l == c {
            if let Some(x) = cyr.get(i) {
                return x.to_string();
            }
        }
    }
    c.to_string()
}

pub fn translit_ru(s: &str) -> String {
    let src: Vec<char> = s.to_lowercase().replace('\'', "ь").chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < src.len() {
        let mut hit = false;
        for (lat, cyr) in TR_MULTI.iter() {
            let l: Vec<char> = lat.chars().collect();
            if i + l.len() <= src.len() && src[i..i + l.len()] == l[..] {
                out.push_str(cyr);
                i += l.len();
                hit = true;
                break;
            }
        }
        if !hit {
            out.push_str(&tr_one(src[i]));
            i += 1;
        }
    }
    let mut cs = out.chars();
    match cs.next() {
        Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}

lazy_rx!(ws_rx, r"\s+");
lazy_rx!(
    part_rx,
    r"[,\s]*\b(?:part|pt|часть|cd|disc|диск)[\s.]*(?:\d+|one|two|three|i{1,3})\b",
    i
);
lazy_rx!(dc_rx, r"\s+(?:DC|TC|SE|CE|UE)$");
lazy_rx!(lead2_rx, r"^\d{1,2}[\s.]+(?=\D)");
lazy_rx!(aka_rx, r"\s+(?:aka|a\.k\.a\.|/|\|)\s+", i);
lazy_rx!(cyr_rx, r"[а-яёА-ЯЁ]");
lazy_rx!(lat_rx, r"[A-Za-z]");
lazy_rx!(trail_num_rx, r"\s+\d+(?:\s+\d+)*$");
lazy_rx!(head_rx, r"\s*(?:[.:–—]\s|\s-\s)");
lazy_rx!(mnum_rx, r"^(.+?\s\d{1,2})\s+[А-ЯЁA-Z]");
lazy_rx!(any_alnum_rx, r"[^\W_]");

pub fn title_variants(title: &str, parent_title: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    fn add(out: &mut Vec<String>, t: &str) {
        let t = ws_rx().replace(t, " ");
        let t = strip_chars(&t, " -.,:;!?()[]").to_string();
        if t.chars().count() >= 2 && any_alnum_rx().is_match(&t) && !out.iter().any(|x| x.to_lowercase() == t.to_lowercase()) {
            out.push(t);
        }
    }
    add(&mut out, title);
    let mut t = part_rx().replace(title, " ");
    t = dc_rx().replace(&t, "");
    add(&mut out, &t);
    t = lead2_rx().replace(&t, "");
    add(&mut out, &t);
    for piece in aka_rx().split(&t, 0) {
        add(&mut out, &piece);
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    let cyr = words.iter().filter(|w| cyr_rx().is_match(w) && !lat_rx().is_match(w)).cloned().collect::<Vec<_>>().join(" ");
    let lat = words.iter().filter(|w| lat_rx().is_match(w) && !cyr_rx().is_match(w)).cloned().collect::<Vec<_>>().join(" ");
    if !cyr.is_empty() && !lat.is_empty() {
        add(&mut out, &cyr);
        add(&mut out, &lat);
        let l2 = trail_num_rx().replace(&lat, "");
        add(&mut out, &l2);
    }
    let head = head_rx().split(&t, 1).into_iter().next().unwrap_or_default();
    if head != t {
        add(&mut out, &head);
    }
    if let Some(c) = mnum_rx().caps(&t) {
        let g = c.get(1).unwrap_or("").to_string();
        add(&mut out, &g);
    }
    if !parent_title.is_empty() {
        add(&mut out, parent_title);
    }
    if !lat.is_empty() && cyr.is_empty() {
        let tr = translit_ru(&t);
        add(&mut out, &tr);
    }
    out
}

pub fn runtime_minutes(text: &str) -> Option<i64> {
    lazy_rx!(digits, r"\d+");
    digits().find(text).and_then(|(a, b)| text[a..b].parse::<i64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ожидаемые значения получены запуском Python-версии (make_catalog.py) на тех же именах.
    fn ct(s: &str) -> (String, String) {
        clean_title(s)
    }

    #[test]
    fn clean_titles() {
        assert_eq!(ct("Кин-дза-дза! (1986)"), ("Кин-дза-дза!".into(), "1986".into()));
        assert_eq!(ct("1408.(2007).BDRip.1080p.HEVC"), ("1408".into(), "2007".into()));
        assert_eq!(ct("Blade.Runner.2049.2017.BDRip"), ("Blade Runner 2049".into(), "2017".into()));
        assert_eq!(ct("[REC] (2007)"), ("REC".into(), "2007".into()));
        assert_eq!(ct("Succession (2018–2023)"), ("Succession".into(), "2018".into()));
        assert_eq!(ct("PokaSvadbaNeRazluchitNas"), ("Poka Svadba Ne Razluchit Nas".into(), "".into()));
        assert_eq!(ct("Donnie.Darko.2001.Directors.Cut.1080p"), ("Donnie Darko".into(), "2001".into()));
        assert_eq!(ct("01.Крепкий.орешек.(1988)"), ("Крепкий орешек".into(), "1988".into()));
        assert_eq!(ct("The Matrix (1999) [1080p]"), ("The Matrix".into(), "1999".into()));
        assert_eq!(ct("Чучело (1983)"), ("Чучело".into(), "1983".into()));
        assert_eq!(ct("Ocean's.Twelve.HDDVDRip.2160p.AI.UPSCALE"), ("Ocean's Twelve".into(), "".into()));
        assert_eq!(ct("The.Way.Way.Back.(2013).BDRip.1080p"), ("The Way Way Back".into(), "2013".into()));
    }

    #[test]
    fn episodes() {
        let e = episode_info("Breaking.Bad.S01E02.720p");
        assert_eq!((e.0, e.1, e.3), (Some(1), Some(2), true));
        let e = episode_info("1x05 Pilot");
        assert_eq!((e.0, e.1, e.3), (Some(1), Some(5), true));
        let e = episode_info("Серия 3");
        assert_eq!((e.0, e.1, e.3), (None, Some(3), false));
        let e = episode_info("03 - Название");
        assert_eq!((e.0, e.1), (None, Some(3)));
        let e = episode_info("Movie 2010");
        assert_eq!((e.0, e.1), (None, None));
        let e = episode_info("S02E10E11");
        assert_eq!((e.0, e.1), (Some(2), Some(10)));
    }

    #[test]
    fn season_dirs() {
        assert_eq!(season_dir_title("Gen V (Season 1) WEB-DL 1080p"), ("Gen V".into(), "".into()));
        assert_eq!(season_dir_title("Season 1"), ("".into(), "".into()));
        assert_eq!(season_dir_title("The.Boys.S01"), ("The Boys".into(), "".into()));
        assert_eq!(season_of_dir("Season 1"), Some(1));
        assert_eq!(season_of_dir("Specials"), Some(0));
        assert_eq!(season_of_dir("Movies"), None);
    }

    #[test]
    fn variants() {
        assert_eq!(translit_ru("Ironiya sudbi"), "Ирония судби");
        assert_eq!(title_variants("Ironiya sudbi", ""), vec!["Ironiya sudbi", "Ирония судби"]);
        assert_eq!(
            title_variants("Безумный Макс Mad Max", ""),
            vec!["Безумный Макс Mad Max", "Безумный Макс", "Mad Max"]
        );
        assert_eq!(
            title_variants("Эйс Вентура 2. Когда зовет природа", ""),
            vec!["Эйс Вентура 2. Когда зовет природа", "Эйс Вентура 2"]
        );
    }

    #[test]
    fn categories() {
        let kw = Keywords::new(&default_keywords());
        let f = kw.flags(&["TVShows".to_string(), "Nature".to_string()]);
        assert_eq!(pick_category(&f), "docs");
        let f = kw.flags(&["Cartoons".to_string()]);
        assert_eq!(pick_category(&f), "cartoons");
        let f = kw.flags(&["Movies".to_string(), "Avatar (2009)".to_string()]);
        assert_eq!(pick_category(&f), "movies");
        let f = kw.flags(&["Series".to_string()]);
        assert_eq!(pick_category(&f), "series");
    }
}
