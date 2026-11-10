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
    let existing: std::collections::HashSet<String> = match request.get("existing_paths") { Some(Value::Arr(v)) => v.iter().filter_map(|x| if let Value::Str(s)=x {Some(path_key(s))} else {None}).collect(), _ => std::collections::HashSet::new() };
    let mut items: Vec<Value> = entries.iter().enumerate().map(|(position, entry)| {
        let get = |key: &str| entry.get(key);
        let string = |key: &str| match get(key) { Some(Value::Str(s)) => Value::Str(s.clone()), _ => Value::Null };
        let number = |key: &str| match get(key) { Some(Value::Num(n)) => *n, _ => 0.0 };
        let boolean = |key: &str| match get(key) { Some(Value::Bool(b)) => *b, _ => false };
        let path = match get("path") { Some(Value::Str(s)) => s.clone(), _ => String::new() };
        let name = match get("name") { Some(Value::Str(s)) => s.clone(), _ => String::new() };
        let path = path.replace('/', "\\");
        let (stem0, ext0) = split_name(&name);
        let (mut stem, mut ext) = (stem0.to_string(), ext0.to_string());
        let mut title_unsupported = false;
        let mut names_short = false;
        let parent = std::path::Path::new(&path).parent().unwrap_or(std::path::Path::new(""));
        for rule in match request.get("rules") { Some(Value::Arr(v)) => v.as_slice(), _ => &[] } {
            if matches!(rule.get("enabled"), Some(Value::Bool(false))) { continue; }
            let val = |k: &str, d: &str| match rule.get(k) { Some(Value::Str(x)) => x.clone(), _ => d.to_string() };
            match val("kind", "").as_str() {
                "replace" => {
                    let find = val("find", "");
                    let to = val("to", "");
                    let scope = val("scope", "stem");
                    let regex_mode = matches!(rule.get("regex"), Some(Value::Bool(true)));
                    let case_sensitive = !matches!(rule.get("case"), Some(Value::Bool(false)));
                    let first = val("count", "all") == "first";
                    let replace_text = |input: &str| -> String {
                        let pattern = if regex_mode { find.clone() } else { regex::escape(&find) };
                        let pattern = if case_sensitive { pattern } else { format!("(?i:{pattern})") };
                        match regex::Regex::new(&pattern) {
                            Ok(re) => {
                                let replacement = regex::NoExpand(&to);
                                if first { re.replacen(input, 1, replacement).into_owned() }
                                else { re.replace_all(input, replacement).into_owned() }
                            }
                            Err(_) => input.to_string(),
                        }
                    };
                    match scope.as_str() {
                        "ext" => ext = replace_text(&ext),
                        "name" => {
                            let whole = replace_text(&format!("{stem}{ext}"));
                            let (new_stem, new_ext) = split_name(&whole);
                            stem = new_stem.to_string();
                            ext = new_ext.to_string();
                        }
                        _ => stem = replace_text(&stem),
                    }
                },
                "add" => { let text=val("text", ""); let sep=val("sep", ""); if !text.is_empty() { if val("position", "prefix")=="suffix" { stem=format!("{stem}{}{text}", if stem.is_empty() { "" } else { &sep }); } else { stem=format!("{text}{}{stem}", if stem.is_empty() { "" } else { &sep }); } } },
                "uuid_replace" => {
                    let uuid_name = regex::Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$").unwrap();
                    if uuid_name.is_match(&stem) { stem = val("text", ""); }
                },
                "clear" => stem.clear(),
                "date" => {
                    let source = val("source", "mtime");
                    let timestamp = if source == "ctime" { number("ctime") } else { number("mtime") };
                    let moment = time::OffsetDateTime::from_unix_timestamp_nanos((timestamp * 1_000_000_000.0).round() as i128)
                        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
                    let local = moment.to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
                    let fields = [
                        ("YYYY", format!("{:04}", local.year())), ("YY", format!("{:02}", local.year().rem_euclid(100))),
                        ("MM", format!("{:02}", u8::from(local.month()))), ("DD", format!("{:02}", local.day())),
                        ("hh", format!("{:02}", local.hour())), ("mm", format!("{:02}", local.minute())),
                        ("ss", format!("{:02}", local.second())),
                    ];
                    let pattern = val("format", "YYYYMMDD");
                    let mut formatted = String::new();
                    let mut rest = pattern.as_str();
                    while !rest.is_empty() {
                        if let Some((token, value)) = fields.iter().find(|(token, _)| rest.starts_with(token)) {
                            formatted.push_str(value);
                            rest = &rest[token.len()..];
                        } else {
                            let ch = rest.chars().next().unwrap(); formatted.push(ch); rest = &rest[ch.len_utf8()..];
                        }
                    }
                    let position = val("position", "prefix");
                    let delimiter = val("delimiter", "_");
                    let (date_text, separator) = if delimiter == "()" || matches!(val("sep", "" ).as_str(), "()" | "bracket" | "paren") {
                        (format!("({formatted})"), String::new())
                    } else { (formatted, val("sep", "_")) };
                    stem = match position.as_str() {
                        "suffix" => format!("{stem}{}{date_text}", if stem.is_empty() { "" } else { &separator }),
                        "replace" => date_text,
                        _ => format!("{date_text}{}{stem}", if stem.is_empty() { "" } else { &separator }),
                    };
                },
                "digits_keep" => {
                    let re = regex::Regex::new(r"\d+").unwrap();
                    stem = re.find_iter(&stem).map(|m| m.as_str()).collect();
                },
                "digits_delete" => {
                    stem = regex::Regex::new(r"\d+").unwrap().replace_all(&stem, "").into_owned();
                },
                "digits_pad" => {
                    let re = regex::Regex::new(r"\d+").unwrap();
                    let matches: Vec<_> = re.find_iter(&stem).collect();
                    let picked = if val("which", "last") == "first" { matches.first() } else { matches.last() };
                    if let Some(m) = picked {
                        let width = rule.get("digits").and_then(|v| if let Value::Num(n)=v {Some(*n as usize)} else {None}).unwrap_or(3);
                        let padded = format!("{:0>width$}", m.as_str(), width=width);
                        stem.replace_range(m.start()..m.end(), &padded);
                    }
                },
                "number" => {
                    let start = rule.get("start").and_then(|v| if let Value::Num(n)=v {Some(*n as i64)} else {None}).unwrap_or(1);
                    let step = rule.get("step").and_then(|v| if let Value::Num(n)=v {Some(*n as i64)} else {None}).unwrap_or(1);
                    let digits = rule.get("digits").and_then(|v| if let Value::Num(n)=v {Some(*n as usize)} else {None}).unwrap_or(3);
                    let ordinal = if matches!(rule.get("per_folder"), Some(Value::Bool(true))) {
                        let current_parent = std::path::Path::new(&path).parent().map(|p| p.to_string_lossy().to_lowercase()).unwrap_or_default();
                        entries[..position].iter().filter(|prior| {
                            match prior.get("path") { Some(Value::Str(p)) => std::path::Path::new(p).parent().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default() == current_parent, _ => false }
                        }).count()
                    } else { position };
                    let n = start + ordinal as i64 * step;
                    let number = format!("{:0>width$}", n, width=digits);
                    let sep = val("sep", "_");
                    match val("position", "suffix").as_str() {
                        "prefix" => stem = format!("{number}{}{stem}", if stem.is_empty() {""} else {&sep}),
                        "replace" => stem = number,
                        _ => stem = format!("{stem}{}{number}", if stem.is_empty() {""} else {&sep}),
                    }
                },
                "number_shift" => {
                    let re = regex::Regex::new(r"\d+").unwrap();
                    let matches: Vec<_> = re.find_iter(&stem).collect();
                    let picked = if val("which", "first") == "last" {matches.last()} else {matches.first()};
                    if let Some(m) = picked {
                        let delta = rule.get("delta").and_then(|v| if let Value::Num(n)=v {Some(*n as i64)} else {None}).unwrap_or(1);
                        let value = m.as_str().parse::<i64>().unwrap_or(0) + delta;
                        if value >= 0 {
                            let keep = !matches!(rule.get("keep_width"), Some(Value::Bool(false)));
                            let width = if keep || m.as_str().starts_with('0') {m.as_str().len()} else {1};
                            let shifted = format!("{:0>width$}", value, width=width);
                            stem.replace_range(m.start()..m.end(), &shifted);
                        }
                    }
                },
                "case" => {
                    stem = match val("mode", "lower").as_str() {
                        "upper" => stem.to_uppercase(),
                        "title" => {
                            let mut out = String::new(); let mut start = true;
                            for ch in stem.chars() { if start { out.extend(ch.to_uppercase()); } else { out.push(ch); } start = !ch.is_alphanumeric(); }
                            out
                        },
                        "capitalize" => { let mut chars=stem.chars(); chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default() },
                        "sentence" => { let mut chars=stem.chars(); chars.next().map(|c| c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()).unwrap_or_default() },
                        _ => stem.to_lowercase(),
                    };
                },
                "strip_result_suffix" => {
                    let to = val("to", "");
                    for suffix in ["(자간조정)","(일괄적용)","(서식통일)","(서식적용)","(최종검수)","(쪽맞춤)","(서식복사)"] {
                        stem = stem.replace(suffix, &to);
                    }
                    stem = stem.trim().to_string();
                },
                "doc_title" => {
                    if ext.to_lowercase() != ".hwpx" { title_unsupported = true; }
                },
                "names_list" => {
                    let lines: Vec<String> = match rule.get("names") {
                        Some(Value::Str(s)) => s.replace('\r', "").split('\n').map(|line| line.trim().to_string()).collect(),
                        _ => Vec::new(),
                    };
                    if let Some(line) = lines.get(position).filter(|line| !line.is_empty()) {
                        if matches!(rule.get("keep_ext"), Some(Value::Bool(false))) {
                            let (new_stem, new_ext) = split_name(line);
                            stem = new_stem.to_string(); ext = new_ext.to_string();
                        } else { stem = line.clone(); }
                    } else { names_short = true; }
                },
                "del_pos" => {
                    let start = rule.get("start").and_then(|v| if let Value::Num(n)=v {Some(*n as usize)} else {None}).unwrap_or(1).max(1);
                    let end = rule.get("end").and_then(|v| if let Value::Num(n)=v {Some(*n as usize)} else {None}).unwrap_or(start).max(start);
                    let chars: Vec<char> = stem.chars().collect();
                    let (lo, hi) = if val("origin", "start") == "end" {
                        (chars.len().saturating_sub(end), chars.len().saturating_sub(start.saturating_sub(1)))
                    } else { (start.saturating_sub(1), end) };
                    stem = chars.iter().take(lo).chain(chars.iter().skip(hi)).collect();
                },
                "del_between" => {
                    let opener = val("open", "(");
                    let closer = val("close", ")");
                    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
                    let mut stack: Vec<usize> = Vec::new();
                    let mut i = 0;
                    while i < stem.len() {
                        if stem.is_char_boundary(i) && stem[i..].starts_with(&opener) && (opener != closer || stack.is_empty()) {
                            stack.push(i); i += opener.len(); continue;
                        }
                        if stem.is_char_boundary(i) && stem[i..].starts_with(&closer) && !stack.is_empty() {
                            let begin = stack.pop().unwrap();
                            spans.push((begin, i + closer.len(), stack.len()));
                            i += closer.len(); continue;
                        }
                        i += stem[i..].chars().next().map(char::len_utf8).unwrap_or(1);
                    }
                    let inner = val("nest", "outer") == "inner";
                    let mut chosen: Vec<(usize, usize)> = spans.iter().filter_map(|(b,e,d)| {
                        let select = if inner { !spans.iter().any(|(b2,e2,_)| b2 < b && e < e2) } else { *d == 0 };
                        if select {Some((*b,*e))} else {None}
                    }).collect();
                    chosen.sort_unstable_by(|a,b| b.0.cmp(&a.0));
                    let marks = matches!(rule.get("keep_marks"), Some(Value::Bool(true)));
                    for (b,e) in chosen { let replacement = if marks {format!("{opener}{closer}")} else {String::new()}; stem.replace_range(b..e, &replacement); }
                    stem = regex::Regex::new(r" {2,}").unwrap().replace_all(stem.trim(), " ").into_owned();
                },
                "ext_del" => ext.clear(),
                "ext_add" => { stem.push_str(&ext); ext=val("ext", ""); },
                "ext_replace" => ext=val("ext", ""),
                "ext_case" => ext=if val("mode", "lower")=="upper" { ext.to_uppercase() } else { ext.to_lowercase() },
                _ => {}
            }
        }
        let new_name = format!("{stem}{ext}");
        let new_path = parent.join(&new_name).to_string_lossy().replace('/', "\\");
        let fp = vec![Value::Num(number("size")), Value::Num((number("mtime") * 1000.0).trunc())];
        let old_name = match get("name") { Some(Value::Str(s)) => s.as_str(), _ => "" };
        let invalid_reason = if new_name.trim().is_empty() { Some("이름이 비어 있습니다.") } else { None };
        let status = if invalid_reason.is_some() { "invalid" } else if new_name == old_name { "unchanged" } else { "ok" };
        let mut warnings: Vec<Value> = Vec::new();
        if title_unsupported { warnings.push(Value::from("title_unsupported")); }
        if names_short { warnings.push(Value::from("names_short")); }
        let folder = matches!(get("is_folder"), Some(Value::Bool(true)));
        if !folder && split_name(&new_name).1.to_lowercase() != split_name(old_name).1.to_lowercase() { warnings.push(Value::from("ext_changed")); }
        let new_path_value = if invalid_reason.is_some() { Value::Null } else { Value::from(new_path.clone()) };
        plan_parts.push(Value::Arr(vec![Value::from(path.clone()), new_path_value.clone(), Value::from(status), Value::Arr(fp.clone())]));
        Value::obj(vec![
            ("id", string("id")), ("old_path", Value::from(path)), ("old_name", string("name")),
            ("new_name", Value::from(new_name)), ("new_path", new_path_value), ("status", Value::from(status)),
            ("reason", invalid_reason.map(Value::from).unwrap_or(Value::Null)), ("warnings", Value::Arr(warnings)), ("overridden", Value::Bool(false)),
            ("fingerprint", Value::Arr(fp)),
            ("is_folder", Value::Bool(boolean("is_folder"))),
        ])
    }).collect();
    mark_conflicts(&mut items, &existing);
    let payload = json_compact(&Value::Arr(plan_parts));
    let digest = Sha256::digest(payload.as_bytes());
    let plan_id = digest.iter().take(16).map(|b| format!("{b:02x}")).collect::<String>();
    let mut summary = vec![("ok", 0_i64), ("unchanged", 0_i64), ("conflict", 0_i64), ("invalid", 0_i64), ("skipped", 0_i64), ("executables", 0_i64)];
    for row in &items { if let Some(Value::Str(status)) = row.get("status") { if let Some((_, n)) = summary.iter_mut().find(|(k, _)| *k == status) { *n += 1; } } }
    Value::obj(vec![
        ("plan_id", Value::from(plan_id)),
        ("items", Value::Arr(items)),
        ("summary", Value::obj(summary.into_iter().map(|(k,v)|(k,Value::from(v))).collect())),
    ])
}

fn mark_conflicts(items: &mut [Value], existing: &std::collections::HashSet<String>) {
    loop {
        let mut changed = false;
        let moving: std::collections::HashSet<String> = items.iter()
            .filter(|item| status(item) == "ok")
            .filter_map(|item| string_field(item, "old_path").map(path_key)).collect();
        let staying: std::collections::HashSet<String> = items.iter()
            .filter(|item| status(item) != "ok")
            .filter_map(|item| string_field(item, "old_path").map(path_key)).collect();
        let mut targets: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
        for (index, item) in items.iter().enumerate() {
            if status(item) == "ok" {
                if let Some(path) = string_field(item, "new_path") {
                    targets.entry(path_key(path)).or_default().push(index);
                }
            }
        }
        for (key, group) in targets {
            if group.len() > 1 {
                for index in group {
                    set_conflict(&mut items[index], "목록 안에서 새 이름이 겹칩니다.");
                }
                changed = true;
                continue;
            }
            let index = group[0];
            let old_key = string_field(&items[index], "old_path").map(path_key).unwrap_or_default();
            if key == old_key { continue; } // 대소문자만 바뀌는 이름은 임시 이름 경유로 처리한다.
            if staying.contains(&key) || ( !moving.contains(&key) && existing.contains(&key)) {
                set_conflict(&mut items[index], "같은 이름의 파일이 이미 있습니다.");
                changed = true;
            }
        }
        if !changed { break; }
    }
}

fn status(item: &Value) -> &str { string_field(item, "status").unwrap_or("") }
fn string_field<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    match item.get(key) { Some(Value::Str(value)) => Some(value.as_str()), _ => None }
}
fn path_key(path: &str) -> String { path.replace('/', "\\").to_lowercase() }
fn set_conflict(item: &mut Value, reason: &str) { if let Value::Obj(fields)=item { for (k,v) in fields { if k=="status" {*v=Value::from("conflict")} else if k=="reason" {*v=Value::from(reason)} } } }

fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') { Some(i) if i > 0 => (&name[..i], &name[i..]), _ => (name, "") }
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
