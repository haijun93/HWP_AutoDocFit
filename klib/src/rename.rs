//! Batch rename preview scaffold. This module never mutates the filesystem.
use crate::json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use zip::ZipArchive;
use unicode_normalization::UnicodeNormalization;

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
        let mut exif_missing = false;
        let mut title_error: Option<&str> = None;
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
                    else if let Ok(Some(title)) = read_hwpx_title(&path) {
                        let title = sanitize_title(&title);
                        let max_len = rule.get("max_length").and_then(|v| if let Value::Num(n)=v {Some(*n as usize)} else {None}).unwrap_or(60);
                        let title: String = title.chars().take(max_len).collect();
                        let title = title.trim_end_matches(['.', ' ']);
                        if title.is_empty() { title_error = Some("문서 제목이 비어 있습니다."); }
                        else {
                            let position = val("position", "replace"); let sep = val("sep", "_");
                            stem = place(&stem, title, &position, &sep);
                        }
                    } else if title_error.is_none() { title_error = Some("문서 제목을 읽지 못했습니다."); }
                },
                "exif_date" => {
                    if let Some(moment) = read_exif_datetime(&path, &ext) {
                        let formatted = format_date(moment, &val("format", "YYYYMMDD"));
                        let position = val("position", "prefix");
                        let sep = if matches!(val("delimiter", "").as_str(), "()" | "bracket" | "paren") || matches!(val("sep", "").as_str(), "()" | "bracket" | "paren") {
                            String::new()
                        } else { val("sep", "_") };
                        let text = if sep.is_empty() { format!("({formatted})") } else { formatted };
                        stem = place(&stem, &text, &position, &sep);
                    } else { exif_missing = true; }
                },
                "cleanup" => {
                    let nfc = !matches!(rule.get("nfc"), Some(Value::Bool(false)));
                    if nfc { stem = stem.nfc().collect(); ext = ext.nfc().collect(); }
                    let width = val("width", "keep");
                    stem = map_width(&stem, &width);
                    match val("underscore", "keep").as_str() { "to_space" => stem = stem.replace('_', " "), "from_space" => stem = stem.replace(' ', "_"), _ => {} }
                    if !matches!(rule.get("collapse_spaces"), Some(Value::Bool(false))) {
                        stem = regex::Regex::new(r" {2,}").unwrap().replace_all(&stem, " ").into_owned();
                        stem = regex::Regex::new(r"_{2,}").unwrap().replace_all(&stem, "_").into_owned();
                    }
                    if !matches!(rule.get("strip"), Some(Value::Bool(false))) { stem = stem.trim_matches([' ', '_', '-']).to_string(); }
                },
                "template" => {
                    let template = val("template", "{date}_{dept}_{title}");
                    let raw_title = if ext.eq_ignore_ascii_case(".hwpx") { read_hwpx_title(&path) } else { Ok(None) };
                    let title = if ext.eq_ignore_ascii_case(".hwpx") { raw_title.as_ref().ok().and_then(|x| x.as_deref()).map(sanitize_title) }
                               else { if template.contains("{title}") { title_unsupported = true; } None };
                    if ext.eq_ignore_ascii_case(".hwpx") && raw_title.is_err() { title_error = Some("문서 제목을 읽지 못했습니다."); }
                    let title = title.unwrap_or_else(|| stem.clone());
                    let max_len = rule.get("max_length").and_then(|v| if let Value::Num(n)=v {Some(*n as usize)} else {None}).unwrap_or(60);
                    let title: String = title.chars().take(max_len).collect();
                    let timestamp = number("mtime");
                    let moment = time::OffsetDateTime::from_unix_timestamp_nanos((timestamp * 1_000_000_000.0).round() as i128).unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
                        .to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
                    let date = format_date(moment, "YYYYMMDD");
                    let n = format!("{:0>width$}", position + 1, width=rule.get("digits").and_then(|v| if let Value::Num(n)=v {Some(*n as usize)} else {None}).unwrap_or(1));
                    let mut output = template.replace("{date}", &date).replace("{title}", title.trim_end_matches(['.', ' '])).replace("{orig}", &stem).replace("{dept}", &val("dept", "")).replace("{n}", &n);
                    output = output.chars().take(255).collect();
                    stem = output;
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
        let invalid_reason = if let Some(reason) = title_error { Some(reason) }
            else if new_name.trim().is_empty() { Some("이름이 비어 있습니다.") } else { None };
        let status = if invalid_reason.is_some() { "invalid" } else if new_name == old_name { "unchanged" } else { "ok" };
        let mut warnings: Vec<Value> = Vec::new();
        if title_unsupported { warnings.push(Value::from("title_unsupported")); }
        if exif_missing { warnings.push(Value::from("exif_missing")); }
        if names_short { warnings.push(Value::from("names_short")); }
        let folder = matches!(get("is_folder"), Some(Value::Bool(true)));
        if !folder && split_name(&new_name).1.to_lowercase() != split_name(old_name).1.to_lowercase() { warnings.push(Value::from("ext_changed")); }
        let new_path_value = if invalid_reason.is_some() { Value::Null } else { Value::from(new_path.clone()) };
        let new_name_value = if title_error.is_some() { Value::Null } else { Value::from(new_name.clone()) };
        plan_parts.push(Value::Arr(vec![Value::from(path.clone()), new_path_value.clone(), Value::from(status), Value::Arr(fp.clone())]));
        Value::obj(vec![
            ("id", string("id")), ("old_path", Value::from(path)), ("old_name", string("name")),
            ("new_name", new_name_value), ("new_path", new_path_value), ("status", Value::from(status)),
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

fn place(stem: &str, text: &str, position: &str, sep: &str) -> String {
    if text.is_empty() { return stem.to_string(); }
    match position {
        "prefix" => format!("{text}{}{stem}", if stem.is_empty() { "" } else { sep }),
        "suffix" => format!("{stem}{}{text}", if stem.is_empty() { "" } else { sep }),
        _ => text.to_string(),
    }
}

fn map_width(text: &str, width: &str) -> String {
    text.chars().map(|c| match width {
        "half" if c == '\u{3000}' => ' ',
        "half" if ('\u{ff01}'..='\u{ff5e}').contains(&c) => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
        "full" if c == ' ' => '\u{3000}',
        "full" if ('!'..='~').contains(&c) => char::from_u32(c as u32 + 0xfee0).unwrap_or(c),
        _ => c,
    }).collect()
}

fn format_date(moment: time::OffsetDateTime, pattern: &str) -> String {
    let fields = [
        ("YYYY", format!("{:04}", moment.year())), ("YY", format!("{:02}", moment.year().rem_euclid(100))),
        ("MM", format!("{:02}", u8::from(moment.month()))), ("DD", format!("{:02}", moment.day())),
        ("hh", format!("{:02}", moment.hour())), ("mm", format!("{:02}", moment.minute())), ("ss", format!("{:02}", moment.second())),
    ];
    let mut out = String::new(); let mut rest = pattern;
    while !rest.is_empty() {
        if let Some((token, value)) = fields.iter().find(|(token, _)| rest.starts_with(token)) {
            out.push_str(value); rest = &rest[token.len()..];
        } else { let ch = rest.chars().next().unwrap(); out.push(ch); rest = &rest[ch.len_utf8()..]; }
    }
    out
}

fn read_hwpx_title(path: &str) -> Result<Option<String>, ()> {
    let file = std::fs::File::open(path).map_err(|_| ())?;
    let mut zip = ZipArchive::new(file).map_err(|_| ())?;
    let mut names: Vec<String> = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().and_then(|f| f.name().ok().map(|n| n.to_string())))
        .filter(|name| name.starts_with("Contents/section") && name.ends_with(".xml"))
        .filter(|name| name[16..name.len()-4].chars().all(|c| c.is_ascii_digit()))
        .collect();
    names.sort_by_key(|name| name[16..name.len()-4].parse::<usize>().unwrap_or(usize::MAX));
    let name = match names.first() { Some(name) => name, None => return Err(()) };
    let mut xml = String::new(); zip.by_name(name).map_err(|_| ())?.read_to_string(&mut xml).map_err(|_| ())?;
    let mut paragraphs = Vec::new();
    let mut rest = xml.as_str();
    while let Some(start) = rest.find("<hp:p") {
        rest = &rest[start..]; let Some(open_end) = rest.find('>') else { break; }; rest = &rest[open_end + 1..];
        let Some(end) = rest.find("</hp:p>") else { break; };
        let paragraph = &rest[..end]; rest = &rest[end + 7..];
        let text = xml_text(paragraph).trim().to_string(); if !text.is_empty() { paragraphs.push(text); }
    }
    Ok(Some(paragraphs.into_iter().next().unwrap_or_default()))
}

fn xml_text(xml: &str) -> String {
    let mut out = String::new(); let mut in_tag = false; let mut entity = String::new(); let mut in_entity = false;
    for ch in xml.chars() {
        if in_tag { if ch == '>' { in_tag = false; } continue; }
        if ch == '<' { in_tag = true; continue; }
        if in_entity {
            if ch == ';' { out.push(match entity.as_str() { "amp" => '&', "lt" => '<', "gt" => '>', "quot" => '"', "apos" => '\'', _ => ' ' }); entity.clear(); in_entity = false; }
            else if entity.len() < 12 { entity.push(ch); }
        } else if ch == '&' { in_entity = true; } else { out.push(ch); }
    }
    out
}

fn read_exif_datetime(path: &str, ext: &str) -> Option<time::OffsetDateTime> {
    if !matches!(ext.to_ascii_lowercase().as_str(), ".jpg" | ".jpeg" | ".tif" | ".tiff" | ".png" | ".webp" | ".heic") { return None; }
    let data = std::fs::read(path).ok()?;
    let tiff = data.windows(6).position(|w| w == b"Exif\0\0").map(|p| p + 6).unwrap_or(0);
    let little = data.get(tiff..tiff + 2)? == b"II";
    if !little && data.get(tiff..tiff + 2)? != b"MM" { return None; }
    let u16at = |p: usize| -> Option<u16> { let b: [u8; 2] = data.get(p..p+2)?.try_into().ok()?; Some(if little {u16::from_le_bytes(b)} else {u16::from_be_bytes(b)}) };
    let u32at = |p: usize| -> Option<u32> { let b: [u8; 4] = data.get(p..p+4)?.try_into().ok()?; Some(if little {u32::from_le_bytes(b)} else {u32::from_be_bytes(b)}) };
    if u16at(tiff + 2)? != 42 { return None; }
    let ifd0 = tiff + u32at(tiff + 4)? as usize;
    let count = u16at(ifd0)? as usize;
    let mut exif_ifd = None; let mut datetime = None;
    for i in 0..count {
        let p = ifd0 + 2 + i * 12;
        match u16at(p)? { 0x8769 => exif_ifd = Some(tiff + u32at(p + 8)? as usize), 0x0132 => datetime = Some((p, false)), _ => {} }
    }
    if let Some(dir) = exif_ifd {
        let n = u16at(dir)? as usize;
        for i in 0..n { let p = dir + 2 + i * 12; if u16at(p)? == 0x9003 { datetime = Some((p, true)); break; } }
    }
    let (entry, _is_exif) = datetime?;
    let kind = u16at(entry + 2)?;
    let count = u32at(entry + 4)? as usize;
    if kind != 2 || count < 19 { return None; }
    let value = u32at(entry + 8)? as usize;
    let start = if count <= 4 { entry + 8 } else { tiff + value };
    let raw = std::str::from_utf8(data.get(start..start + 19)?).ok()?;
    let digits: Vec<i32> = raw.split(|c: char| !c.is_ascii_digit()).filter(|v| !v.is_empty()).map(|v| v.parse().ok()).collect::<Option<_>>()?;
    if digits.len() != 6 { return None; }
    let date = time::Date::from_calendar_date(digits[0], time::Month::try_from(digits[1] as u8).ok()?, digits[2] as u8).ok()?;
    let time = time::Time::from_hms(digits[3] as u8, digits[4] as u8, digits[5] as u8).ok()?;
    Some(time::PrimitiveDateTime::new(date, time).assume_utc())
}

fn sanitize_title(title: &str) -> String {
    regex::Regex::new(r#"[\\/:*?"<>|\x00-\x1f]"#).unwrap().replace_all(title, "_").trim_end_matches(['.', ' ']).trim().to_string()
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
