//! Онлайн-данные: TMDB (постер, описание, жанры, актёры) + OMDb (рейтинг IMDb, Rotten Tomatoes).
use crate::ctx::{Ctx, Fail, R};
use crate::jv::{has, list_of, put, so, sv, truthy, Obj};
use crate::rx::Rx;
use crate::titles::title_variants;
use crate::util::{md5hex, urlencode};
use serde_json::Value;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

pub const CACHE_VERSION: i64 = 4; // при смене версии старый кэш обновляется автоматически

fn pair(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

fn first_name(arr: &[Value], key: &str) -> String {
    arr.first().map(|x| sv(x, key)).unwrap_or_default()
}

/// Название для сравнения: только буквы и цифры в нижнем регистре.
fn norm_title(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Оценка кандидата из поиска TMDB: точное совпадение названия, близость года, известность (число голосов).
fn score_candidate(c: &Value, query: &str, year: &str, media: &str) -> f64 {
    let (tk, ok, dk) = if media == "movie" { ("title", "original_title", "release_date") } else { ("name", "original_name", "first_air_date") };
    let q = norm_title(query);
    let mut score = 0.0;
    if !q.is_empty() && (norm_title(&sv(c, tk)) == q || norm_title(&sv(c, ok)) == q) {
        score += 100.0;
    }
    if let (Ok(want), Ok(got)) = (year.parse::<i64>(), year4(&sv(c, dk)).parse::<i64>()) {
        score -= 8.0 * ((want - got).abs().min(6) as f64);
    }
    let votes = c.get("vote_count").and_then(|x| x.as_f64()).unwrap_or(0.0);
    score += (1.0 + votes).ln() * 3.0;
    score
}

fn year4(s: &str) -> String {
    s.chars().take(4).collect()
}

impl Ctx {
    /// GET с повторами. Ok(None) — «ничего нет» (404 и т.п.), Err(Auth) — 401/403, Err(Stop) — нажат «Стоп».
    pub fn http_bytes(&mut self, url: &str, headers: &[(String, String)]) -> R<Option<Vec<u8>>> {
        let mut err: Option<String> = None;
        for attempt in 0..3u64 {
            self.check()?;
            let mut req = self.agent.get(url);
            for (k, v) in headers {
                req = req.set(k, v);
            }
            match req.call() {
                Ok(resp) => {
                    let mut buf: Vec<u8> = Vec::new();
                    let mut rd = resp.into_reader();
                    match rd.read_to_end(&mut buf) {
                        Ok(_) => return Ok(Some(buf)),
                        Err(e) => {
                            err = Some(e.to_string());
                            std::thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
                Err(ureq::Error::Status(code, _)) => {
                    if code == 401 || code == 403 {
                        return Err(Fail::Auth);
                    }
                    if code == 429 {
                        std::thread::sleep(Duration::from_secs(2 * (attempt + 1)));
                        continue;
                    }
                    return Ok(None);
                }
                Err(e) => {
                    err = Some(e.to_string()); // нет сети, DNS, таймаут, блокировка…
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
        if let Some(e) = err {
            self.stats.net_fail += 1;
            self.last_net_err = e;
        }
        Ok(None)
    }

    pub fn http_json(&mut self, url: &str, headers: &[(String, String)]) -> R<Option<Value>> {
        match self.http_bytes(url, headers)? {
            Some(b) => match serde_json::from_slice::<Value>(&b) {
                Ok(v) => Ok(Some(v)),
                Err(e) => {
                    self.stats.net_fail += 1;
                    self.last_net_err = format!("ответ не JSON: {}", e);
                    Ok(None)
                }
            },
            None => Ok(None),
        }
    }

    fn tmdb_get(&mut self, path: &str, params: Vec<(String, String)>) -> R<Option<Value>> {
        let key = self.cfg.tmdb_key.clone();
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut params = params;
        if key.len() > 40 {
            headers.push(pair("Authorization", &format!("Bearer {}", key)));
        } else {
            params.push(pair("api_key", &key));
        }
        let url = format!("https://api.themoviedb.org/3{}?{}", path, urlencode(&params));
        self.http_json(&url, &headers)
    }

    fn tmdb_lookup(&mut self, title: &str, year: &str, imdb_id: &str, media: &str) -> R<Obj> {
        let lang = self.cfg.language.clone();
        let mut tid: Option<i64> = None;
        if !imdb_id.is_empty() {
            let r = self.tmdb_get(&format!("/find/{}", imdb_id), vec![pair("external_source", "imdb_id")])?;
            let res_key = if media == "movie" { "movie_results" } else { "tv_results" };
            if let Some(r) = r {
                if let Some(first) = list_of(&r, res_key).first() {
                    tid = first.get("id").and_then(|x| x.as_i64());
                }
            }
        }
        if tid.is_none() {
            let yparam = if media == "movie" { "year" } else { "first_air_date_year" };
            // ищем и с годом, и без: поиск «с годом» иногда отдаёт малоизвестный фильм с тем же названием,
            // а «без года» — другой фильм (напр. «The Way Back» вместо «The Way Way Back»); лучшего выбираем по оценке
            let years: Vec<&str> = if year.is_empty() { vec![""] } else { vec![year, ""] };
            let mut cands: Vec<Value> = Vec::new();
            for y in years {
                let mut params = vec![pair("query", title), pair("language", &lang), pair("include_adult", "false")];
                if !y.is_empty() {
                    params.push(pair(yparam, y));
                }
                let r = self.tmdb_get(&format!("/search/{}", media), params)?;
                if let Some(r) = r {
                    for c in list_of(&r, "results").into_iter().take(10) {
                        let id = c.get("id").and_then(|x| x.as_i64());
                        if id.is_some() && !cands.iter().any(|x| x.get("id").and_then(|v| v.as_i64()) == id) {
                            cands.push(c);
                        }
                    }
                }
            }
            let mut best: Option<(f64, i64)> = None;
            for c in cands.iter() {
                if let Some(id) = c.get("id").and_then(|x| x.as_i64()) {
                    let sc = score_candidate(c, title, year, media);
                    if best.map(|b| sc > b.0).unwrap_or(true) {
                        best = Some((sc, id));
                    }
                }
            }
            tid = best.map(|b| b.1);
        }
        let tid = match tid {
            Some(t) => t,
            None => return Ok(Obj::new()),
        };
        let append = if media == "movie" { "credits" } else { "credits,external_ids" };
        let d = match self.tmdb_get(
            &format!("/{}/{}", media, tid),
            vec![pair("language", &lang), pair("append_to_response", append), pair("include_image_language", &format!("{},en,null", &lang[..2.min(lang.len())]))],
        )? {
            Some(d) => d,
            None => return Ok(Obj::new()),
        };
        let mut overview = sv(&d, "overview");
        let mut tagline = sv(&d, "tagline");
        if overview.is_empty() && !lang.to_lowercase().starts_with("en") {
            if let Some(e) = self.tmdb_get(&format!("/{}/{}", media, tid), vec![pair("language", "en-US")])? {
                overview = sv(&e, "overview");
                if tagline.is_empty() {
                    tagline = sv(&e, "tagline");
                }
            }
        }
        let credits = d.get("credits").cloned().unwrap_or(Value::Null);
        let names = |arr: Vec<Value>| -> Vec<String> { arr.iter().map(|c| sv(c, "name")).collect() };
        let mut res = Obj::new();
        if media == "movie" {
            let crew = list_of(&credits, "crew");
            let mut directors: Vec<String> =
                crew.iter().filter(|c| sv(c, "job") == "Director").map(|c| sv(c, "name")).collect();
            directors.truncate(3);
            put(&mut res, "title", sv(&d, "title"));
            put(&mut res, "original", sv(&d, "original_title"));
            put(&mut res, "year", year4(&sv(&d, "release_date")));
            put(&mut res, "runtime", sv(&d, "runtime"));
            put(&mut res, "directors", Value::from(directors));
            put(&mut res, "countries", Value::from(names(list_of(&d, "production_countries"))));
            put(&mut res, "studio", first_name(&list_of(&d, "production_companies"), "name"));
            put(&mut res, "imdb", sv(&d, "imdb_id"));
        } else {
            let rt = list_of(&d, "episode_run_time");
            let runtime = match rt.first() {
                Some(x) => crate::jv::text_of(x),
                None => String::new(),
            };
            let mut directors = names(list_of(&d, "created_by"));
            directors.truncate(3);
            let mut countries = names(list_of(&d, "production_countries"));
            if countries.is_empty() {
                countries = crate::jv::strs(&d, "origin_country");
            }
            let ext = d.get("external_ids").cloned().unwrap_or(Value::Null);
            put(&mut res, "title", sv(&d, "name"));
            put(&mut res, "original", sv(&d, "original_name"));
            put(&mut res, "year", year4(&sv(&d, "first_air_date")));
            put(&mut res, "runtime", runtime);
            put(&mut res, "directors", Value::from(directors));
            put(&mut res, "countries", Value::from(countries));
            put(&mut res, "studio", first_name(&list_of(&d, "networks"), "name"));
            put(&mut res, "imdb", sv(&ext, "imdb_id"));
        }
        let genres = names(list_of(&d, "genres"));
        let mut actors = names(list_of(&credits, "cast"));
        actors.truncate(6);
        let va = d.get("vote_average").and_then(|x| x.as_f64()).unwrap_or(0.0);
        put(&mut res, "plot", overview);
        put(&mut res, "tagline", tagline);
        put(&mut res, "genres", Value::from(genres));
        put(&mut res, "actors", Value::from(actors));
        put(&mut res, "rating", if va > 0.0 { format!("{:.1}", va) } else { String::new() });
        put(&mut res, "rating_src", if va > 0.0 { "TMDB" } else { "" });
        put(&mut res, "votes", sv(&d, "vote_count"));
        let pp = sv(&d, "poster_path");
        put(&mut res, "poster_url", if pp.is_empty() { String::new() } else { format!("https://image.tmdb.org/t/p/w342{}", pp) });
        put(&mut res, "tmdb_id", tid);
        Ok(res)
    }

    /// Запрос к OMDb; при исчерпанном лимите или неверном ключе переходит к следующему ключу.
    fn omdb_request(&mut self, params: &[(String, String)]) -> R<Value> {
        let bad_rx = Rx::ci(r"limit|invalid api key");
        loop {
            let key = {
                let found = self.cfg.omdb_keys.iter().find(|k| !self.omdb_bad.contains(*k));
                match found {
                    Some(k) => k.clone(),
                    None => return Err(Fail::Auth),
                }
            };
            let mut q: Vec<(String, String)> = params.to_vec();
            q.push(pair("apikey", &key));
            let url = format!("https://www.omdbapi.com/?{}", urlencode(&q));
            let r: Value = match self.http_json(&url, &[]) {
                Ok(Some(v)) => v,
                Ok(None) => Value::Null,
                Err(Fail::Auth) => {
                    let mut o = Obj::new();
                    put(&mut o, "Response", "False");
                    put(&mut o, "Error", "Request limit reached!");
                    Value::Object(o)
                }
                Err(Fail::Stop) => return Err(Fail::Stop),
            };
            if sv(&r, "Response") == "False" && bad_rx.is_match(&sv(&r, "Error")) {
                self.omdb_bad.insert(key.clone());
                let left = self.cfg.omdb_keys.iter().filter(|k| !self.omdb_bad.contains(*k)).count();
                let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
                let more = if left > 0 { format!(", перехожу к следующему (осталось ключей: {})", left) } else { String::new() };
                self.say(&format!("  ! OMDb: ключ …{} не работает (лимит или неверный){}", tail, more));
                continue;
            }
            return Ok(r);
        }
    }

    fn omdb_lookup(&mut self, title: &str, year: &str, imdb_id: &str, media: &str) -> R<Obj> {
        let mut params = vec![pair("plot", "short")];
        if !imdb_id.is_empty() {
            params.push(pair("i", imdb_id));
        } else {
            params.push(pair("t", title));
            params.push(pair("type", if media == "movie" { "movie" } else { "series" }));
            if !year.is_empty() {
                params.push(pair("y", year));
            }
        }
        let r = self.omdb_request(&params)?;
        if sv(&r, "Response") != "True" {
            return Ok(Obj::new());
        }
        let na = |k: &str| -> String {
            let s = sv(&r, k);
            if s == "N/A" { String::new() } else { s }
        };
        let lst = |k: &str| -> Vec<String> {
            na(k).split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
        };
        let digits = Rx::new(r"\d+");
        let runtime = {
            let t = na("Runtime");
            match digits.find(&t) {
                Some((a, b)) => t[a..b].to_string(),
                None => String::new(),
            }
        };
        let tomato = list_of(&r, "Ratings")
            .iter()
            .find(|x| sv(x, "Source") == "Rotten Tomatoes")
            .map(|x| sv(x, "Value"))
            .unwrap_or_default();
        let mut actors = lst("Actors");
        actors.truncate(6);
        let rating = na("imdbRating");
        let votes: String = na("imdbVotes").chars().filter(|c| c.is_ascii_digit()).collect();
        let mut o = Obj::new();
        put(&mut o, "title", na("Title"));
        put(&mut o, "year", year4(&na("Year")));
        put(&mut o, "plot", na("Plot"));
        put(&mut o, "runtime", runtime);
        put(&mut o, "genres", Value::from(lst("Genre")));
        put(&mut o, "directors", Value::from(lst("Director")));
        put(&mut o, "actors", Value::from(actors));
        put(&mut o, "countries", Value::from(lst("Country")));
        put(&mut o, "studio", na("Production"));
        put(&mut o, "mpaa", na("Rated"));
        put(&mut o, "rating_src", if rating.is_empty() { "" } else { "IMDb" });
        put(&mut o, "rating", rating);
        put(&mut o, "votes", votes);
        put(&mut o, "rt", tomato);
        put(&mut o, "imdb", na("imdbID"));
        put(&mut o, "poster_url", na("Poster"));
        Ok(o)
    }

    fn omdb_unavailable(&mut self) {
        if !self.dead.contains("omdb") {
            self.say("  ! OMDb: все ключи исчерпали лимит или неверны — OMDb отключён до конца запуска");
        }
        self.dead.insert("omdb".to_string());
    }

    pub fn fetch_online(&mut self, title: &str, year: &str, imdb_id: &str, media: &str, quiet: bool) -> R<Obj> {
        self.check()?;
        let key = format!("{}|{}|{}|{}|{}", media, title.to_lowercase(), year, imdb_id, self.cfg.language);
        if !self.refresh {
            let cached = match self.cache.get(&key) {
                Some(Value::Object(c)) if c.get("_v").and_then(|x| x.as_i64()) == Some(CACHE_VERSION) => Some(c.clone()),
                _ => None,
            };
            if let Some(mut c) = cached {
                if has(&c, "_omdb_pending") && !self.cfg.omdb_keys.is_empty() && !self.dead.contains("omdb") {
                    // раньше OMDb был недоступен — дополняем
                    let imdb = if has(&c, "imdb") { so(&c, "imdb") } else { imdb_id.to_string() };
                    match self.omdb_lookup(title, year, &imdb, media) {
                        Ok(o) => {
                            if !o.is_empty() {
                                merge_omdb(&mut c, &o);
                            }
                            c.remove("_omdb_pending");
                            self.cache.insert(key.clone(), Value::Object(c.clone()));
                        }
                        Err(Fail::Auth) => self.omdb_unavailable(),
                        Err(Fail::Stop) => return Err(Fail::Stop),
                    }
                }
                self.stats.online_cached += 1;
                return Ok(c);
            }
        }
        let net_before = self.stats.net_fail;
        let mut res = Obj::new();
        if !self.cfg.tmdb_key.is_empty() && !self.dead.contains("tmdb") {
            match self.tmdb_lookup(title, year, imdb_id, media) {
                Ok(r) => res = r,
                Err(Fail::Auth) => {
                    self.dead.insert("tmdb".to_string());
                    self.say("  ! TMDB: неверный API-ключ — TMDB отключён");
                }
                Err(Fail::Stop) => return Err(Fail::Stop),
            }
        }
        if !self.cfg.omdb_keys.is_empty() && !self.dead.contains("omdb") {
            let imdb = if has(&res, "imdb") { so(&res, "imdb") } else { imdb_id.to_string() };
            match self.omdb_lookup(title, year, &imdb, media) {
                Ok(o) => {
                    if !o.is_empty() {
                        res = merge_omdb_into(res, &o);
                    }
                }
                Err(Fail::Auth) => self.omdb_unavailable(),
                Err(Fail::Stop) => return Err(Fail::Stop),
            }
        }
        let omdb_missing = !self.cfg.omdb_keys.is_empty() && self.dead.contains("omdb");
        if !res.is_empty() {
            put(&mut res, "_v", CACHE_VERSION);
            if omdb_missing {
                // TMDB-данные сохраняем, а OMDb (рейтинг, RT) допишется при следующем запуске
                put(&mut res, "_omdb_pending", true);
            } else {
                res.remove("_omdb_pending");
            }
            self.cache.insert(key, Value::Object(res.clone()));
            self.stats.online_ok += 1;
        } else {
            self.stats.online_miss += 1;
            if self.stats.net_fail > net_before {
                let e = self.last_net_err.clone();
                self.say(&format!("    (онлайн: нет связи с сервером для «{}» {}: {})", title, year, e));
            } else if !quiet {
                self.say(&format!("    (онлайн: ничего не найдено для «{}» {})", title, year));
            }
        }
        Ok(res)
    }

    /// fetch_online по вариантам названия; хит кэшируется под каждым из вариантов.
    pub fn smart_fetch(
        &mut self,
        title: &str,
        year: &str,
        imdb_id: &str,
        media: &str,
        parent_title: &str,
        parent_year: &str,
    ) -> R<Obj> {
        let variants = title_variants(title, parent_title);
        for (i, v) in variants.iter().enumerate() {
            let y: &str = if year.is_empty() && !parent_title.is_empty() && v == parent_title && !parent_year.is_empty() { parent_year } else { year }; // год из имени файла важнее года папки
            let net0 = self.stats.net_fail;
            let id = if i == 0 { imdb_id } else { "" };
            let r = self.fetch_online(v, y, id, media, true)?;
            if self.stats.net_fail > net0 {
                // нет связи — перебирать варианты бессмысленно
                let e = self.last_net_err.clone();
                self.say(&format!("    (онлайн: нет связи с сервером для «{}»: {})", title, e));
                return Ok(Obj::new());
            }
            if !r.is_empty() && i > 0 && !y.is_empty() {
                let ry = so(&r, "year");
                if !ry.is_empty() && ry.chars().all(|c| c.is_ascii_digit()) {
                    if let (Ok(a), Ok(b)) = (ry.parse::<i64>(), y.parse::<i64>()) {
                        if (a - b).abs() > 1 {
                            continue; // приблизительный вариант нашёл фильм другого года — не берём
                        }
                    }
                }
            }
            if !r.is_empty() {
                if i > 0 {
                    self.say(&format!("    (найдено по варианту названия «{}»)", v));
                }
                return Ok(r);
            }
        }
        let tried: Vec<String> = variants.iter().take(6).cloned().collect();
        self.say(&format!("    (онлайн: ничего не найдено для «{}» {}; пробовал: {})", title, year, tried.join(", ")));
        Ok(Obj::new())
    }

    /// {'1': 'Название серии', ...} для сезона сериала.
    pub fn tmdb_season_names(&mut self, tid: i64, season: i64) -> R<Obj> {
        let key = format!("tvseason|{}|{}|{}", tid, season, self.cfg.language);
        if !self.refresh {
            if let Some(Value::Object(c)) = self.cache.get(&key) {
                return Ok(c.clone());
            }
        }
        let lang = self.cfg.language.clone();
        let d = match self.tmdb_get(&format!("/tv/{}/season/{}", tid, season), vec![pair("language", &lang)]) {
            Ok(d) => d,
            Err(Fail::Auth) => {
                self.dead.insert("tmdb".to_string());
                return Ok(Obj::new());
            }
            Err(Fail::Stop) => return Err(Fail::Stop),
        };
        let mut names = Obj::new();
        if let Some(d) = d {
            for e in list_of(&d, "episodes") {
                let n = sv(&e, "name");
                if !n.is_empty() {
                    let num = match e.get("episode_number").and_then(|x| x.as_i64()) {
                        Some(x) => x.to_string(),
                        None => String::new(),
                    };
                    names.insert(num, Value::String(n));
                }
            }
        }
        if !names.is_empty() {
            self.cache.insert(key, Value::Object(names.clone()));
        }
        Ok(names)
    }

    /// Постер из интернета → файл в кэше → data-URI.
    pub fn online_poster(&mut self, online: &Obj) -> R<String> {
        let url = so(online, "poster_url");
        if url.is_empty() {
            return Ok(String::new());
        }
        let pdir = self.cache_sub("posters");
        let fp: PathBuf = pdir.join(format!("{}.jpg", md5hex(&url)));
        if !fp.is_file() {
            let data = match self.http_bytes(&url, &[]) {
                Ok(d) => d,
                Err(Fail::Auth) => None,
                Err(Fail::Stop) => return Err(Fail::Stop),
            };
            match data {
                Some(d) if !d.is_empty() => {
                    if std::fs::write(&fp, d).is_err() {
                        return Ok(String::new());
                    }
                }
                _ => return Ok(String::new()),
            }
        }
        Ok(self.poster_data_uri(&fp))
    }
}

/// Добавляет данные OMDb к результату TMDB: рейтинг IMDb и Rotten Tomatoes приоритетнее, остальное — только пробелы.
pub fn merge_omdb(res: &mut Obj, o: &Obj) {
    if has(o, "rating") {
        res.insert("rating".to_string(), o["rating"].clone());
        put(res, "rating_src", "IMDb");
        let v = o.get("votes").cloned().unwrap_or(Value::String(String::new()));
        res.insert("votes".to_string(), v); // голоса — те же, что и у рейтинга
    }
    if has(o, "rt") {
        res.insert("rt".to_string(), o["rt"].clone());
    }
    for k in ["imdb", "plot", "genres", "directors", "actors", "countries", "studio", "runtime", "poster_url", "mpaa"] {
        let empty = res.get(k).map(|v| !truthy(v)).unwrap_or(true);
        if empty && has(o, k) {
            res.insert(k.to_string(), o[k].clone());
        }
    }
}

fn merge_omdb_into(res: Obj, o: &Obj) -> Obj {
    if res.is_empty() {
        return o.clone();
    }
    let mut r = res;
    merge_omdb(&mut r, o);
    r
}
