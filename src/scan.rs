//! Сканирование библиотеки: фильмы и сериалы → список карточек (JSON-объекты).
use crate::ctx::{Ctx, R};
use crate::jv::{first_truthy, has, put, so, truthy, Obj};
use crate::nfo::{find_nfo, find_poster, parse_nfo};
use crate::probe::tag_info;
use crate::titles::*;
use crate::util::*;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

struct Ep {
    abs: PathBuf,
    rel: String,
    pr: Obj,
    season: i64,
    num: Option<i64>,
    title: String,
    file: String,
    video: String,
    size: String,
    size_bytes: u64,
    mtime: i64,
    runtime_min: Option<f64>,
    runtime: String,
    quality: String,
    resolution: String,
    vcodec: String,
    audio: Value,
}

struct Group {
    category: String,
    title: String,
    year: String,
    dirs: Vec<PathBuf>,
    episodes: Vec<Ep>,
    root: PathBuf,
}

struct Scan {
    root: PathBuf,
    root_name: String,
    movies: Vec<Value>,
    shows: Vec<Group>,
    show_idx: HashMap<(String, String), usize>,
    ep_counter: u32,
    ignore: HashSet<String>,
}

fn base_item(title: &str, year: &str, nfo: &Obj, online: &Obj) -> Obj {
    let t = if has(nfo, "title") { so(nfo, "title") } else { title.to_string() };
    let orig = so(nfo, "original");
    let mut o = Obj::new();
    let tmdb = first_truthy(vec![
        nfo.get("tmdb").cloned().unwrap_or(Value::Null),
        online.get("tmdb_id").cloned().unwrap_or(Value::Null),
    ]);
    put(&mut o, "tmdb", tmdb);
    put(&mut o, "rating_src", if has(nfo, "rating") { so(nfo, "rating_src") } else { String::new() });
    put(&mut o, "rt", so(nfo, "rt"));
    put(&mut o, "votes", nfo.get("votes").cloned().unwrap_or(Value::String(String::new())));
    put(&mut o, "original", if !orig.is_empty() && orig != t { orig } else { String::new() });
    put(&mut o, "title", t);
    put(&mut o, "year", if has(nfo, "year") { so(nfo, "year") } else { year.to_string() });
    for k in ["plot", "tagline", "studio", "mpaa", "rating", "imdb"] {
        put(&mut o, k, so(nfo, k));
    }
    for k in ["genres", "directors", "actors", "countries"] {
        put(&mut o, k, nfo.get(k).cloned().unwrap_or(Value::Array(Vec::new())));
    }
    o
}

fn merge_online(nfo: &mut Obj, online: &Obj) {
    let had_rating = has(nfo, "rating");
    for (k, v) in online {
        if ["poster_url", "tmdb_id", "_v", "_omdb_pending"].contains(&k.as_str()) || !truthy(v) || has(nfo, k) {
            continue;
        }
        if (k == "rating_src" || k == "votes") && had_rating {
            continue; // рейтинг взят из .nfo — источник и голоса не подменяем
        }
        nfo.insert(k.clone(), v.clone());
    }
}

fn tech_fields(item: &mut Obj, pr: &Obj) {
    item.insert("vinfo".to_string(), pr.get("vinfo").cloned().unwrap_or(Value::Null));
    item.insert("audio_tracks".to_string(), pr.get("audio_tracks").cloned().unwrap_or(Value::Array(Vec::new())));
    item.insert("sub_tracks".to_string(), pr.get("sub_tracks").cloned().unwrap_or(Value::Array(Vec::new())));
}

fn pr_f64(pr: &Obj, k: &str) -> Option<f64> {
    pr.get(k).and_then(|x| x.as_f64())
}

fn median(v: &mut Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

impl Ctx {
    fn is_container(&self, sc: &Scan, path: &Path) -> bool {
        if path == sc.root.as_path() {
            return true;
        }
        let name = file_name(path);
        !has_year(&name) && !self.keywords.flags(&[name]).is_empty()
    }

    pub fn scan_library(&mut self, root_dir: &Path) -> R<Vec<Value>> {
        let root_name = match root_dir.file_name() {
            Some(n) => n.to_string_lossy().into_owned(),
            None => root_dir.to_string_lossy().into_owned(),
        };
        let ignore: HashSet<String> = self
            .cfg
            .ignore_folders
            .iter()
            .map(|x| x.trim().to_lowercase())
            .filter(|x| !x.is_empty())
            .collect();
        let mut sc = Scan {
            root: root_dir.to_path_buf(),
            root_name,
            movies: Vec::new(),
            shows: Vec::new(),
            show_idx: HashMap::new(),
            ep_counter: 0,
            ignore,
        };
        self.walk(&mut sc, root_dir, 0)?;
        let groups = std::mem::take(&mut sc.shows);
        let mut items: Vec<Value> = std::mem::take(&mut sc.movies);
        for g in groups {
            let v = self.build_show(g)?;
            items.push(v);
        }
        items.sort_by_key(|x| x.get("title").and_then(|t| t.as_str()).unwrap_or("").to_lowercase());
        Ok(items)
    }

    fn walk(&mut self, sc: &mut Scan, dirpath: &Path, depth: usize) -> R<()> {
        self.check()?;
        if depth > 40 {
            return Ok(());
        }
        let mut dirnames: Vec<String> = Vec::new();
        let mut filenames: Vec<String> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dirpath) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if e.path().is_dir() {
                    dirnames.push(name);
                } else {
                    filenames.push(name);
                }
            }
        }
        for d in &dirnames {
            if sc.ignore.contains(&d.to_lowercase()) {
                self.ignored.push(dirpath.join(d).to_string_lossy().into_owned());
            }
        }
        let mut kept: Vec<String> = dirnames
            .into_iter()
            .filter(|d| {
                let l = d.to_lowercase();
                !is_skip_dir(&l) && !sc.ignore.contains(&l) && !d.starts_with('.') && !d.starts_with('@')
            })
            .collect();
        kept.sort_by_key(|d| d.to_lowercase());
        let videos: Vec<String> =
            filenames.iter().filter(|f| is_video(f) && !f.to_lowercase().contains("sample")).cloned().collect();
        if !videos.is_empty() {
            self.process_videos(sc, dirpath, &filenames, videos)?;
        }
        for d in kept {
            self.walk(sc, &dirpath.join(&d), depth + 1)?;
        }
        Ok(())
    }

    fn process_videos(&mut self, sc: &mut Scan, dirpath: &Path, filenames: &[String], videos: Vec<String>) -> R<()> {
        let rel_dir = rel_str(&sc.root, dirpath);
        let in_root = rel_dir.is_empty();
        let mut parts: Vec<String> = vec![sc.root_name.clone()];
        if !in_root {
            parts.extend(rel_dir.split('/').map(|s| s.to_string()));
        }
        let flags = self.keywords.flags(&parts);
        let base_category = pick_category(&flags);
        let series_ctx = flags.iter().any(|f| f == "series");
        let dname = file_name(dirpath);
        let season_dir: Option<i64> = if in_root { None } else { season_of_dir(&dname) };

        let mut infos: HashMap<String, EpInfo> = HashMap::new();
        for v in &videos {
            infos.insert(v.clone(), episode_info(&stem_of(v)));
        }
        let ep_like = infos.values().filter(|i| i.1.is_some()).count();
        let episodic_dir = videos.len() >= 2 && (ep_like as f64) >= f64::max(2.0, 0.6 * videos.len() as f64);
        let single = videos.len() == 1;

        let mut sorted = videos.clone();
        sorted.sort();
        for video in sorted {
            let stem = stem_of(&video);
            let video_abs = dirpath.join(&video);
            let video_rel = rel_str(&sc.root, &video_abs);
            let (season_f, ep_f, m_ep, strong) = infos.get(&video).cloned().unwrap_or((None, None, None, false));
            let (size, mtime) = match std::fs::metadata(&video_abs) {
                Ok(m) => (m.len(), mtime_secs(&m)),
                Err(_) => (0u64, 0i64),
            };
            let pr = self.probe_cached(&video_abs, &video_rel, size, mtime)?;
            let is_ep = strong || series_ctx || season_dir.is_some() || (episodic_dir && ep_f.is_some());

            // ───── серия сериала ─────
            if is_ep {
                let category = if base_category != "movies" { base_category.clone() } else { "series".to_string() };
                let parent: PathBuf = dirpath.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| dirpath.to_path_buf());
                let mut override_t: Option<(String, String)> = None; // название сериала, записанное прямо в папке сезона
                let show_dir: Option<PathBuf>;
                if season_dir.is_some() && !in_root {
                    let (d_title, d_year) = season_dir_title(&dname);
                    let p_container = self.is_container(sc, &parent);
                    if !d_title.is_empty() {
                        if p_container {
                            override_t = Some((d_title.clone(), d_year.clone()));
                        } else {
                            let stripped = strip_season(&file_name(&parent));
                            let (p_title, _) = clean_title(strip_chars(&stripped, " ._-[]()"));
                            if norm_title(&p_title) != norm_title(&d_title) {
                                // напр. "The Boys and Gen V/Gen V (Season 1)" и ".../The.Boys.S01" — разные сериалы
                                override_t = Some((d_title.clone(), d_year.clone()));
                            }
                        }
                    }
                    show_dir = if override_t.is_some() || p_container { Some(dirpath.to_path_buf()) } else { Some(parent.clone()) };
                } else if in_root || self.is_container(sc, dirpath) {
                    show_dir = None;
                } else {
                    show_dir = Some(dirpath.to_path_buf());
                }

                let mut src = String::new();
                if let Some(sd) = &show_dir {
                    src = strip_chars(&strip_season(&file_name(sd)), " ._-[]()").to_string();
                }
                if src.is_empty() {
                    src = match m_ep {
                        Some((a, _)) if a > 0 && strong => stem[..a].to_string(),
                        _ => stem.clone(),
                    };
                }
                let (title, year) = match override_t {
                    Some(o) => o,
                    None => clean_title(&src),
                };

                let key = (category.clone(), title.to_lowercase());
                let idx = match sc.show_idx.get(&key) {
                    Some(i) => *i,
                    None => {
                        sc.shows.push(Group {
                            category: category.clone(),
                            title: title.clone(),
                            year: year.clone(),
                            dirs: Vec::new(),
                            episodes: Vec::new(),
                            root: sc.root.clone(),
                        });
                        let i = sc.shows.len() - 1;
                        sc.show_idx.insert(key, i);
                        i
                    }
                };
                let g = &mut sc.shows[idx];
                if let Some(sd) = &show_dir {
                    if !g.dirs.contains(sd) {
                        g.dirs.push(sd.clone());
                    }
                }
                if !year.is_empty() && g.year.is_empty() {
                    g.year = year.clone();
                }
                let season = season_f.or(season_dir).unwrap_or(1);
                let ep_title = if m_ep.is_some() || !lead_matches(&stem) { episode_title(&stem, m_ep) } else { stem.clone() };
                let rmin = pr_f64(&pr, "runtime_min");
                g.episodes.push(Ep {
                    abs: video_abs.clone(),
                    rel: video_rel.clone(),
                    season,
                    num: ep_f,
                    title: ep_title,
                    file: video.clone(),
                    video: quote(&video_rel),
                    size: if size > 0 { human_size(size) } else { String::new() },
                    size_bytes: size,
                    mtime,
                    runtime_min: rmin,
                    runtime: match rmin {
                        Some(m) if m > 0.0 => human_duration(m),
                        _ => String::new(),
                    },
                    quality: so(&pr, "quality"),
                    resolution: so(&pr, "resolution"),
                    vcodec: so(&pr, "vcodec"),
                    audio: pr.get("audio").cloned().unwrap_or(Value::Array(Vec::new())),
                    pr: pr.clone(),
                });
                sc.ep_counter += 1;
                if sc.ep_counter % 25 == 0 {
                    self.say(&format!("    ... обработано серий: {}", sc.ep_counter));
                }
                continue;
            }

            // ───── фильм ─────
            self.say(&format!("  + [{}] {}", category_label(&base_category), video_rel));
            // название берём из папки, если в ней один файл — но не из папки-раздела и не когда год есть только в имени файла
            let use_dir = !in_root
                && single
                && !self.is_container(sc, dirpath)
                && (has_year(&dname) || !has_year(&stem));
            let name_src = if use_dir { dname.clone() } else { stem.clone() };
            let (title, year) = clean_title(&name_src);

            let mut nfo = match find_nfo(dirpath, filenames, &stem, single) {
                Some(p) => parse_nfo(&p),
                None => Obj::new(),
            };
            // теги самого mkv (MOVIE/IMDB, MOVIE/TITLE …): приоритет ниже .nfo, но выше названия файла
            let tg = tag_info(&pr);
            if !has(&nfo, "imdb") && has(&tg, "imdb") {
                nfo.insert("imdb".to_string(), tg.get("imdb").cloned().unwrap_or(Value::Null));
            }

            let mut online = Obj::new();
            if self.online {
                let (mut p_title, mut p_year) = (String::new(), String::new());
                if !in_root && !single && has_year(&dname) && !self.is_container(sc, dirpath) {
                    let r = clean_title(&dname); // «Кин-дза-дза! (1986)» для файлов Part.One/Two
                    p_title = r.0;
                    p_year = r.1;
                }
                let q_title = if has(&nfo, "title") {
                    so(&nfo, "title")
                } else if has(&tg, "title") {
                    so(&tg, "title")
                } else {
                    title.clone()
                };
                let q_year = if has(&nfo, "year") {
                    so(&nfo, "year")
                } else if has(&tg, "year") {
                    so(&tg, "year")
                } else {
                    year.clone()
                };
                let q_imdb = so(&nfo, "imdb");
                online = self.smart_fetch(&q_title, &q_year, &q_imdb, "movie", &p_title, &p_year)?;
                merge_online(&mut nfo, &online);
            }
            // чего не дал ни .nfo, ни интернет (или без интернета) — берём из тегов mkv
            for k in ["title", "year", "plot", "genres", "directors", "actors", "mpaa"] {
                if !has(&nfo, k) && has(&tg, k) {
                    nfo.insert(k.to_string(), tg.get(k).cloned().unwrap_or(Value::Null));
                }
            }

            let runtime_min: Option<f64> = match runtime_minutes(&so(&nfo, "runtime")) {
                Some(m) if m > 0 => Some(m as f64),
                _ => pr_f64(&pr, "runtime_min"),
            };
            // постер: 1) файл рядом с видео → 2) интернет → 3) обложка внутри MKV/MP4 (медленно)
            let poster: String = match find_poster(dirpath, filenames, &stem, single) {
                Some(pf) => self.poster_data_uri(&pf),
                None => {
                    let mut p = self.online_poster(&online)?;
                    if p.is_empty() {
                        if let Some(cover) = self.embedded_cover(&video_abs, &video_rel, size, mtime, &pr) {
                            p = self.poster_data_uri(&cover);
                        }
                    }
                    p
                }
            };

            let mut item = base_item(&title, &year, &nfo, &online);
            let dir_rel = dirname_rel(&video_rel);
            put(&mut item, "folder", quote(&dir_rel));
            put(&mut item, "kind", "movie");
            put(&mut item, "category", base_category.clone());
            put(&mut item, "runtime", match runtime_min { Some(m) => human_duration(m), None => String::new() });
            put(&mut item, "resolution", so(&pr, "resolution"));
            put(&mut item, "quality", so(&pr, "quality"));
            put(&mut item, "vcodec", so(&pr, "vcodec"));
            put(&mut item, "audio", pr.get("audio").cloned().unwrap_or(Value::Array(Vec::new())));
            put(&mut item, "size", if size > 0 { human_size(size) } else { String::new() });
            put(&mut item, "size_bytes", size);
            put(&mut item, "added", mtime);
            put(&mut item, "file", video.clone());
            put(&mut item, "path", dir_rel);
            put(&mut item, "video", quote(&video_rel));
            put(&mut item, "poster", poster);
            tech_fields(&mut item, &pr);
            sc.movies.push(Value::Object(item));
        }
        Ok(())
    }

    /// Собирает карточку сериала из сгруппированных серий.
    fn build_show(&mut self, g: Group) -> R<Value> {
        let title = g.title.clone();
        let year = g.year.clone();
        self.say(&format!("  + [{}] {} — серий: {}", category_label(&g.category), title, g.episodes.len()));

        // .nfo и постер из папки сериала
        let mut nfo = Obj::new();
        let mut poster_file: Option<PathBuf> = None;
        for d in &g.dirs {
            let files = list_dir(d);
            let low: HashMap<String, String> = files.iter().map(|f| (f.to_lowercase(), f.clone())).collect();
            if nfo.is_empty() {
                if let Some(f) = low.get("tvshow.nfo") {
                    nfo = parse_nfo(&d.join(f));
                }
            }
            if poster_file.is_none() {
                let only: Vec<String> = files.iter().filter(|f| d.join(f).is_file()).cloned().collect();
                poster_file = find_poster(d, &only, "", true);
            }
        }
        let mut online = Obj::new();
        let mut tmdb_media = "tv".to_string();
        if self.online {
            let q_title = if has(&nfo, "title") { so(&nfo, "title") } else { title.clone() };
            let q_year = if has(&nfo, "year") { so(&nfo, "year") } else { year.clone() };
            let q_imdb = so(&nfo, "imdb");
            online = self.smart_fetch(&q_title, &q_year, &q_imdb, "tv", "", "")?;
            if online.is_empty() {
                // фильм, разбитый на «Серия 1 / Серия 2» (напр. «Чучело (1983)») — в TMDB это фильм, а не сериал
                online = self.smart_fetch(&q_title, &q_year, &q_imdb, "movie", "", "")?;
                if !online.is_empty() {
                    tmdb_media = "movie".to_string();
                }
            }
            merge_online(&mut nfo, &online);
        }

        // сезоны и серии
        let root = g.root.clone();
        let dirs = g.dirs.clone();
        let category = g.category.clone();
        let mut seasons: BTreeMap<i64, Vec<Ep>> = BTreeMap::new();
        for e in g.episodes {
            seasons.entry(e.season).or_default().push(e);
        }
        let mut names_by_season: HashMap<i64, Obj> = HashMap::new();
        let tid = online.get("tmdb_id").and_then(|x| x.as_i64());
        if self.online && tmdb_media == "tv" && !self.cfg.tmdb_key.is_empty() && !self.dead.contains("tmdb") {
            if let Some(tid) = tid {
                let keys: Vec<i64> = seasons.keys().cloned().collect();
                for s in keys {
                    let n = self.tmdb_season_names(tid, s)?;
                    names_by_season.insert(s, n);
                }
            }
        }

        let mut out_seasons: Vec<Value> = Vec::new();
        let mut all_eps: Vec<Ep> = Vec::new();
        for (s, mut eps) in seasons {
            eps.sort_by(|a, b| {
                let ka = (a.num.unwrap_or(1_000_000), a.file.to_lowercase());
                let kb = (b.num.unwrap_or(1_000_000), b.file.to_lowercase());
                ka.cmp(&kb)
            });
            for (i, e) in eps.iter_mut().enumerate() {
                if e.num.is_none() {
                    e.num = Some(i as i64 + 1);
                }
                if e.title.is_empty() {
                    if let Some(m) = names_by_season.get(&s) {
                        let k = e.num.unwrap_or(0).to_string();
                        e.title = so(m, &k);
                    }
                }
            }
            let list: Vec<Value> = eps
                .iter()
                .map(|e| {
                    let mut o = Obj::new();
                    put(&mut o, "num", e.num.unwrap_or(0));
                    put(&mut o, "title", e.title.clone());
                    put(&mut o, "file", e.file.clone());
                    put(&mut o, "video", e.video.clone());
                    put(&mut o, "size", e.size.clone());
                    put(&mut o, "runtime", e.runtime.clone());
                    put(&mut o, "quality", e.quality.clone());
                    put(&mut o, "resolution", e.resolution.clone());
                    Value::Object(o)
                })
                .collect();
            let mut so_ = Obj::new();
            put(&mut so_, "number", s);
            put(&mut so_, "episodes", Value::Array(list));
            out_seasons.push(Value::Object(so_));
            all_eps.extend(eps);
        }

        let first_idx = all_eps.iter().position(|e| !e.resolution.is_empty()).unwrap_or(0);
        let mut runtimes: Vec<f64> = all_eps.iter().filter_map(|e| e.runtime_min).filter(|m| *m > 0.0).collect();
        let rt_min: Option<f64> = if !runtimes.is_empty() {
            Some(median(&mut runtimes))
        } else {
            runtime_minutes(&so(&nfo, "runtime")).map(|m| m as f64)
        };
        let total: u64 = all_eps.iter().map(|e| e.size_bytes).sum();

        // постер: 1) файл в папке сериала → 2) интернет → 3) обложка, вшитая в серию (медленно, поэтому последняя)
        let poster: String = match poster_file {
            Some(pf) => self.poster_data_uri(&pf),
            None => {
                let mut p = self.online_poster(&online)?;
                if p.is_empty() {
                    for e in all_eps.iter() {
                        if let Some(cover) = self.embedded_cover(&e.abs, &e.rel, e.size_bytes, e.mtime, &e.pr) {
                            p = self.poster_data_uri(&cover);
                            break;
                        }
                    }
                }
                p
            }
        };

        let mut item = base_item(&title, &year, &nfo, &online);
        let folder_rel: String = if !dirs.is_empty() {
            rel_str(&root, &common_path(&dirs)) // общая папка всех сезонов
        } else {
            dirname_rel(&all_eps[0].rel)
        };
        let first = &all_eps[first_idx];
        let seasons_count = out_seasons
            .iter()
            .filter(|s| s.get("number").and_then(|n| n.as_i64()) != Some(0))
            .count();
        let seasons_count = if seasons_count > 0 { seasons_count } else { out_seasons.len() };
        put(&mut item, "folder", quote(&folder_rel));
        put(&mut item, "kind", "show");
        put(&mut item, "tmdb_media", tmdb_media);
        put(&mut item, "category", category);
        put(&mut item, "runtime", match rt_min { Some(m) if m > 0.0 => format!("{} / серия", human_duration(m)), _ => String::new() });
        put(&mut item, "resolution", first.resolution.clone());
        put(&mut item, "quality", first.quality.clone());
        put(&mut item, "vcodec", first.vcodec.clone());
        put(&mut item, "audio", first.audio.clone());
        put(&mut item, "size", if total > 0 { human_size(total) } else { String::new() });
        put(&mut item, "size_bytes", total);
        put(&mut item, "added", all_eps.iter().map(|e| e.mtime).max().unwrap_or(0));
        put(&mut item, "file", all_eps[0].file.clone());
        put(&mut item, "path", folder_rel);
        put(&mut item, "video", all_eps[0].video.clone());
        put(&mut item, "poster", poster);
        put(&mut item, "seasons", Value::Array(out_seasons));
        put(&mut item, "seasons_count", seasons_count as i64);
        put(&mut item, "episodes_count", all_eps.len() as i64);
        tech_fields(&mut item, &first.pr);
        Ok(Value::Object(item))
    }
}
