//! Batch rename preview scaffold. This module never mutates the filesystem.
use crate::json::Value;
use sha2::{Digest, Sha256};

pub fn preview(request: &Value) -> Value {
    let empty = Vec::new();
    let entries = match request.get("entries") {
        Some(Value::Arr(items)) => items,
        _ => &empty,
    };
    let mut plan_parts = Vec::new();
    let items = entries.iter().map(|entry| {
        let get = |key: &str| entry.get(key);
        let string = |key: &str| match get(key) { Some(Value::Str(s)) => Value::Str(s.clone()), _ => Value::Null };
        let number = |key: &str| match get(key) { Some(Value::Num(n)) => *n, _ => 0.0 };
        let boolean = |key: &str| match get(key) { Some(Value::Bool(b)) => *b, _ => false };
        let path = match get("path") { Some(Value::Str(s)) => s.clone(), _ => String::new() };
        let name = match get("name") { Some(Value::Str(s)) => s.clone(), _ => String::new() };
        let path = path.replace('/', "\\");
        let parent = std::path::Path::new(&path).parent().unwrap_or(std::path::Path::new(""));
        let new_path = parent.join(&name).to_string_lossy().replace('/', "\\");
        let fp = vec![Value::Num(number("size")), Value::Num((number("mtime") * 1000.0).trunc())];
        plan_parts.push(Value::Arr(vec![Value::from(path.clone()), Value::from(new_path.clone()), Value::from("unchanged"), Value::Arr(fp.clone())]));
        Value::obj(vec![
            ("id", string("id")), ("old_path", Value::from(path)), ("old_name", Value::from(name.clone())),
            ("new_name", Value::from(name)), ("new_path", Value::from(new_path)), ("status", Value::from("unchanged")),
            ("reason", Value::Null), ("warnings", Value::Arr(vec![])), ("overridden", Value::Bool(false)),
            ("fingerprint", Value::Arr(fp)),
            ("is_folder", Value::Bool(boolean("is_folder"))),
        ])
    }).collect();
    let payload = json_compact(&Value::Arr(plan_parts));
    let digest = Sha256::digest(payload.as_bytes());
    let plan_id = digest.iter().take(16).map(|b| format!("{b:02x}")).collect::<String>();
    Value::obj(vec![
        ("plan_id", Value::from(plan_id)),
        ("items", Value::Arr(items)),
        ("summary", Value::obj(vec![("ok", 0_i64.into()), ("unchanged", entries.len().into()), ("conflict", 0_i64.into()), ("invalid", 0_i64.into()), ("skipped", 0_i64.into()), ("executables", 0_i64.into())])),
    ])
}

fn json_compact(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) if n.fract() == 0.0 => format!("{}", *n as i64),
        Value::Num(n) => n.to_string(),
        Value::Str(s) => {
            let mut out = String::from("\"");
            for c in s.chars() { match c { '"' => out.push_str("\\\""), '\\' => out.push_str("\\\\"), '\n' => out.push_str("\\n"), '\r' => out.push_str("\\r"), '\t' => out.push_str("\\t"), c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)), c => out.push(c) } }
            out.push('"'); out
        },
        Value::Arr(items) => format!("[{}]", items.iter().map(json_compact).collect::<Vec<_>>().join(",")),
        Value::Obj(items) => format!("{{{}}}", items.iter().map(|(k,v)| format!("{}:{}", crate::json::to_string(&Value::Str(k.clone())), json_compact(v))).collect::<Vec<_>>().join(",")),
    }
}
