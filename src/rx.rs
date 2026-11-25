//! Тонкая обёртка над fancy-regex (нужны lookbehind/lookahead, которых нет в обычном regex).
use fancy_regex::Regex;

pub struct Rx(Regex);

/// Результат совпадения с группами (позиции — байтовые смещения в исходной строке).
pub struct Caps {
    text: String,
    g: Vec<Option<(usize, usize)>>,
}

impl Caps {
    pub fn get(&self, i: usize) -> Option<&str> {
        match self.g.get(i) {
            Some(Some((a, b))) => Some(&self.text[*a..*b]),
            _ => None,
        }
    }
    pub fn start(&self, i: usize) -> usize {
        match self.g.get(i) {
            Some(Some((a, _))) => *a,
            _ => 0,
        }
    }
    pub fn end(&self, i: usize) -> usize {
        match self.g.get(i) {
            Some(Some((_, b))) => *b,
            _ => 0,
        }
    }
    pub fn num(&self, i: usize) -> Option<i64> {
        self.get(i).and_then(|s| s.trim().parse::<i64>().ok())
    }
}

impl Rx {
    pub fn new(p: &str) -> Rx {
        match Regex::new(p) {
            Ok(r) => Rx(r),
            Err(e) => panic!("неверное регулярное выражение {}: {}", p, e),
        }
    }

    /// Без учёта регистра.
    pub fn ci(p: &str) -> Rx {
        Rx::new(&format!("(?i){}", p))
    }

    pub fn is_match(&self, s: &str) -> bool {
        self.0.is_match(s).unwrap_or(false)
    }

    pub fn find(&self, s: &str) -> Option<(usize, usize)> {
        match self.0.find(s) {
            Ok(Some(m)) => Some((m.start(), m.end())),
            _ => None,
        }
    }

    pub fn caps(&self, s: &str) -> Option<Caps> {
        match self.0.captures(s) {
            Ok(Some(c)) => Some(Self::pack(s, &c)),
            _ => None,
        }
    }

    /// Все непересекающиеся совпадения (аналог re.finditer).
    pub fn all(&self, s: &str) -> Vec<Caps> {
        let mut out = Vec::new();
        for c in self.0.captures_iter(s) {
            if let Ok(c) = c {
                out.push(Self::pack(s, &c));
            }
        }
        out
    }

    fn pack(s: &str, c: &fancy_regex::Captures) -> Caps {
        let mut g = Vec::new();
        for i in 0..c.len() {
            g.push(c.get(i).map(|m| (m.start(), m.end())));
        }
        Caps { text: s.to_string(), g }
    }

    /// Замена всех совпадений на фиксированную строку (без подстановок $1).
    pub fn replace(&self, s: &str, rep: &str) -> String {
        let mut out = String::new();
        let mut last = 0usize;
        for c in self.0.captures_iter(s) {
            if let Ok(c) = c {
                if let Some(m) = c.get(0) {
                    out.push_str(&s[last..m.start()]);
                    out.push_str(rep);
                    last = m.end();
                }
            }
        }
        out.push_str(&s[last..]);
        out
    }

    /// Часть строки до первого совпадения (аналог re.split(...)[0]).
    pub fn head<'a>(&self, s: &'a str) -> &'a str {
        match self.find(s) {
            Some((a, _)) => &s[..a],
            None => s,
        }
    }

    /// Аналог re.split(pattern, s, maxsplit) для шаблонов без захватывающих групп.
    pub fn split(&self, s: &str, maxsplit: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut last = 0usize;
        for c in self.0.captures_iter(s) {
            if maxsplit > 0 && out.len() >= maxsplit {
                break;
            }
            if let Ok(c) = c {
                if let Some(m) = c.get(0) {
                    if m.end() == m.start() {
                        continue;
                    }
                    out.push(s[last..m.start()].to_string());
                    last = m.end();
                }
            }
        }
        out.push(s[last..].to_string());
        out
    }
}

/// re.escape
pub fn escape(s: &str) -> String {
    fancy_regex::escape(s).into_owned()
}
