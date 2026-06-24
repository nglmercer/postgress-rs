use serde_json::Value as JsonValue;

#[derive(Debug, Clone)]
pub enum JsonbValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonbValue>),
    Object(Vec<(String, JsonbValue)>),
}

impl JsonbValue {
    pub fn from_serde(val: JsonValue) -> Self {
        match val {
            JsonValue::Null => JsonbValue::Null,
            JsonValue::Bool(b) => JsonbValue::Bool(b),
            JsonValue::Number(n) => JsonbValue::Number(n.as_f64().unwrap_or(0.0)),
            JsonValue::String(s) => JsonbValue::String(s),
            JsonValue::Array(arr) => JsonbValue::Array(arr.into_iter().map(Self::from_serde).collect()),
            JsonValue::Object(map) => JsonbValue::Object(map.into_iter().map(|(k, v)| (k, Self::from_serde(v))).collect()),
        }
    }

    pub fn to_serde(&self) -> JsonValue {
        match self {
            JsonbValue::Null => JsonValue::Null,
            JsonbValue::Bool(b) => JsonValue::Bool(*b),
            JsonbValue::Number(n) => JsonValue::Number(serde_json::Number::from_f64(*n).unwrap_or_else(|| serde_json::Number::from(0))),
            JsonbValue::String(s) => JsonValue::String(s.clone()),
            JsonbValue::Array(arr) => JsonValue::Array(arr.iter().map(|v| v.to_serde()).collect()),
            JsonbValue::Object(obj) => {
                let map: serde_json::Map<String, JsonValue> = obj.iter().map(|(k, v)| (k.clone(), v.to_serde())).collect();
                JsonValue::Object(map)
            }
        }
    }

    pub fn parse(json_str: &str) -> Result<Self, serde_json::Error> {
        let val: JsonValue = serde_json::from_str(json_str)?;
        Ok(Self::from_serde(val))
    }

    pub fn to_string_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.to_serde()).unwrap_or_else(|_| "null".to_string())
    }
}

pub fn jsonb_get(json: &JsonbValue, key: &str) -> Option<JsonbValue> {
    match json {
        JsonbValue::Object(obj) => obj.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
        _ => None,
    }
}

pub fn jsonb_get_text(json: &JsonbValue, key: &str) -> Option<String> {
    jsonb_get(json, key).and_then(|v| match v {
        JsonbValue::String(s) => Some(s),
        other => Some(other.to_string_pretty()),
    })
}

pub fn jsonb_path(json: &JsonbValue, path: &[&str]) -> Option<JsonbValue> {
    if path.is_empty() {
        return Some(json.clone());
    }
    let (first, rest) = path.split_first()?;
    match json {
        JsonbValue::Object(obj) => {
            let val = obj.iter().find(|(k, _)| k == *first)?.1.clone();
            jsonb_path(&val, rest)
        }
        JsonbValue::Array(arr) => {
            let idx: usize = first.parse().ok()?;
            arr.get(idx).and_then(|v| jsonb_path(v, rest))
        }
        _ => None,
    }
}

pub fn jsonb_path_text(json: &JsonbValue, path: &[&str]) -> Option<String> {
    jsonb_path(json, path).map(|v| match v {
        JsonbValue::String(s) => s,
        other => other.to_string_pretty(),
    })
}

pub fn jsonb_contains(json: &JsonbValue, other: &JsonbValue) -> bool {
    match (json, other) {
        (JsonbValue::Object(a), JsonbValue::Object(b)) => {
            b.iter().all(|(k, v)| {
                a.iter().find(|(ak, _)| ak == k).map_or(false, |(_, av)| jsonb_contains(av, v))
            })
        }
        (JsonbValue::Array(a), JsonbValue::Array(b)) => {
            b.iter().all(|bv| a.iter().any(|av| jsonb_contains(av, bv)))
        }
        (a, b) => a.to_string_pretty() == b.to_string_pretty(),
    }
}

pub fn jsonb_contained_by(json: &JsonbValue, other: &JsonbValue) -> bool {
    jsonb_contains(other, json)
}

pub fn jsonb_exists(json: &JsonbValue, key: &str) -> bool {
    matches!(json, JsonbValue::Object(obj) if obj.iter().any(|(k, _)| k == key))
}

pub fn jsonb_exists_any(json: &JsonbValue, keys: &[&str]) -> bool {
    keys.iter().any(|k| jsonb_exists(json, k))
}

pub fn jsonb_exists_all(json: &JsonbValue, keys: &[&str]) -> bool {
    keys.iter().all(|k| jsonb_exists(json, k))
}

pub fn jsonb_set(json: &JsonbValue, path: &[&str], new_val: JsonbValue) -> JsonbValue {
    if path.is_empty() {
        return new_val;
    }
    let (first, rest) = path.split_first().unwrap();
    match json {
        JsonbValue::Object(obj) => {
            let mut new_obj: Vec<(String, JsonbValue)> = obj.clone();
            if let Some(pos) = new_obj.iter().position(|(k, _)| k == *first) {
                if rest.is_empty() {
                    new_obj[pos].1 = new_val;
                } else {
                    let updated = jsonb_set(&new_obj[pos].1, rest, new_val);
                    new_obj[pos].1 = updated;
                }
            } else if rest.is_empty() {
                new_obj.push((first.to_string(), new_val));
            } else {
                let mut empty = JsonbValue::Object(vec![]);
                empty = jsonb_set(&empty, rest, new_val);
                new_obj.push((first.to_string(), empty));
            }
            JsonbValue::Object(new_obj)
        }
        JsonbValue::Array(arr) => {
            let idx: usize = first.parse().unwrap_or(0);
            let mut new_arr = arr.clone();
            if idx < new_arr.len() {
                if rest.is_empty() {
                    new_arr[idx] = new_val;
                } else {
                    new_arr[idx] = jsonb_set(&new_arr[idx], rest, new_val);
                }
            } else if rest.is_empty() {
                new_arr.push(new_val);
            } else {
                let mut empty = JsonbValue::Object(vec![]);
                empty = jsonb_set(&empty, rest, new_val);
                new_arr.push(empty);
            }
            JsonbValue::Array(new_arr)
        }
        _ => new_val,
    }
}

pub fn jsonb_delete(json: &JsonbValue, path: &[&str]) -> JsonbValue {
    if path.is_empty() {
        return json.clone();
    }
    let (first, rest) = path.split_first().unwrap();
    match json {
        JsonbValue::Object(obj) => {
            if rest.is_empty() {
                JsonbValue::Object(obj.iter().filter(|(k, _)| k != *first).cloned().collect())
            } else {
                let new_obj: Vec<(String, JsonbValue)> = obj.iter().map(|(k, v)| {
                    if k == *first {
                        (k.clone(), jsonb_delete(v, rest))
                    } else {
                        (k.clone(), v.clone())
                    }
                }).collect();
                JsonbValue::Object(new_obj)
            }
        }
        JsonbValue::Array(arr) => {
            if rest.is_empty() {
                let idx: usize = first.parse().unwrap_or(0);
                JsonbValue::Array(arr.iter().enumerate().filter(|(i, _)| *i != idx).map(|(_, v)| v.clone()).collect())
            } else {
                let new_arr: Vec<JsonbValue> = arr.iter().enumerate().map(|(i, v)| {
                    if i.to_string() == *first {
                        jsonb_delete(v, rest)
                    } else {
                        v.clone()
                    }
                }).collect();
                JsonbValue::Array(new_arr)
            }
        }
        _ => json.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonb_parse() {
        let json = JsonbValue::parse(r#"{"name": "test", "value": 42}"#).unwrap();
        assert!(matches!(json, JsonbValue::Object(_)));
    }

    #[test]
    fn test_jsonb_get() {
        let json = JsonbValue::parse(r#"{"name": "test", "value": 42}"#).unwrap();
        assert_eq!(jsonb_get_text(&json, "name"), Some("test".to_string()));
        assert_eq!(jsonb_get_text(&json, "missing"), None);
    }

    #[test]
    fn test_jsonb_path() {
        let json = JsonbValue::parse(r#"{"a": {"b": {"c": 1}}}"#).unwrap();
        assert!(jsonb_path(&json, &["a", "b", "c"]).is_some());
        assert!(jsonb_path(&json, &["a", "x"]).is_none());
    }

    #[test]
    fn test_jsonb_contains() {
        let a = JsonbValue::parse(r#"{"a": 1, "b": 2}"#).unwrap();
        let b = JsonbValue::parse(r#"{"a": 1}"#).unwrap();
        assert!(jsonb_contains(&a, &b));
        assert!(!jsonb_contains(&b, &a));
    }

    #[test]
    fn test_jsonb_exists() {
        let json = JsonbValue::parse(r#"{"a": 1, "b": 2}"#).unwrap();
        assert!(jsonb_exists(&json, "a"));
        assert!(!jsonb_exists(&json, "c"));
        assert!(jsonb_exists_any(&json, &["a", "c"]));
        assert!(!jsonb_exists_all(&json, &["a", "c"]));
    }

    #[test]
    fn test_jsonb_set() {
        let json = JsonbValue::parse(r#"{"a": 1}"#).unwrap();
        let updated = jsonb_set(&json, &["b"], JsonbValue::Number(2.0));
        let result = jsonb_get_text(&updated, "b").unwrap();
        assert!(result == "2" || result == "2.0");
    }

    #[test]
    fn test_jsonb_delete() {
        let json = JsonbValue::parse(r#"{"a": 1, "b": 2}"#).unwrap();
        let deleted = jsonb_delete(&json, &["a"]);
        assert!(jsonb_get(&deleted, "a").is_none());
        assert!(jsonb_get(&deleted, "b").is_some());
    }

    #[test]
    fn test_jsonb_array() {
        let json = JsonbValue::parse(r#"[1, 2, 3]"#).unwrap();
        assert!(matches!(json, JsonbValue::Array(_)));
        if let JsonbValue::Array(arr) = &json {
            assert_eq!(arr.len(), 3);
        }
    }
}
