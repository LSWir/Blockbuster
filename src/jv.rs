//! Небольшие помощники для работы с serde_json::Value (карточки каталога — это JSON-объекты).
use serde_json::{Map, Value};

pub type Obj = Map<String, Value>;

/// Истинность значения как в Python: пусто / 0 / null / false → false.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Значение ключа как строка ("" для отсутствующего, null и 0).
pub fn sv(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(x) => text_of(x),
        None => String::new(),
    }
}

pub fn text_of(x: &Value) -> String {
    match x {
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if n.as_f64().map(|f| f == 0.0).unwrap_or(false) {
                String::new()
            } else {
                n.to_string()
            }
        }
        _ => String::new(),
    }
}

pub fn so(o: &Obj, k: &str) -> String {
    match o.get(k) {
        Some(x) => text_of(x),
        None => String::new(),
    }
}

pub fn has(o: &Obj, k: &str) -> bool {
    o.get(k).map(truthy).unwrap_or(false)
}

pub fn list_of(v: &Value, k: &str) -> Vec<Value> {
    match v.get(k) {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    }
}

pub fn strs(v: &Value, k: &str) -> Vec<String> {
    list_of(v, k).iter().map(text_of).filter(|s| !s.is_empty()).collect()
}

pub fn svec(items: Vec<String>) -> Value {
    Value::Array(items.into_iter().map(Value::String).collect())
}

pub fn put<V: Into<Value>>(o: &mut Obj, k: &str, v: V) {
    o.insert(k.to_string(), v.into());
}

/// Первое «истинное» значение (Python: a or b or c).
pub fn first_truthy(vals: Vec<Value>) -> Value {
    for v in vals {
        if truthy(&v) {
            return v;
        }
    }
    Value::String(String::new())
}
