use crate::json::Value;

pub fn run(input: &str, stage: usize) -> Result<(), String> {
    let value = crate::json::parse(input).ok_or("invalid classify-levels JSON")?;
    let paragraphs = match value { Value::Arr(items) => items, _ => return Err("expected array of paragraph objects".into()) };
    let results = classify(&paragraphs, stage);
    println!("{}", crate::json::to_string(&Value::Arr(results)));
    Ok(())
}

fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value { Value::Obj(items) => items.iter().find(|(name, _)| name == key).map(|(_, value)| value), _ => None }
}
fn text(value: &Value, key: &str) -> String {
    field(value, key).and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None }).unwrap_or_default()
}
fn truthy(value: &Value, key: &str) -> bool { matches!(field(value, key), Some(Value::Bool(true))) }
fn integer(value: &Value, key: &str) -> Option<i64> {
    match field(value, key) { Some(Value::Num(n)) if n.fract() == 0.0 => Some(*n as i64), _ => None }
}

fn classify(paragraphs: &[Value], stage: usize) -> Vec<Value> {
    let texts: Vec<String> = paragraphs.iter().map(|p| text(p, "text")).collect();
    let scheme = hierarchy::compat::marker_scheme::detect_scheme(&texts);
    classify_with_scheme(paragraphs, stage, &scheme)
}

fn classify_with_scheme(paragraphs: &[Value], stage: usize, scheme: &hierarchy::compat::marker_scheme::MarkerScheme) -> Vec<Value> {
    paragraphs.iter().map(|p| {
        let style = text(p, "style_name").to_lowercase();
        let outline = integer(p, "outline_level");
        let mapped = if stage >= 1 && truthy(p, "is_table_cell") { Some("table_cell") }
        else if stage >= 2 && (["문서제목", "문서 제목", "title"].iter().any(|w| style.contains(w)) || style == "제목" || outline == Some(0)) { Some("document_title") }
        else if stage >= 3 && (["heading 1", "개요 1", "개요1"].iter().any(|w| style.contains(w)) || outline == Some(1)) { Some("chapter") }
        else if stage >= 4 && (["heading 2", "개요 2", "개요2"].iter().any(|w| style.contains(w)) || outline == Some(2)) { Some("section") }
        else if stage >= 5 && (["heading 3", "개요 3", "개요3"].iter().any(|w| style.contains(w)) || outline == Some(3)) { Some("topic") }
        else { None };
        let level = mapped.unwrap_or_else(|| {
            if stage == 0 { return "body"; }
            let raw = text(p, "text");
            let trimmed = raw.trim();
            let (marker, role) = hierarchy::compat::leading_marker::leading_marker_with_scheme(trimmed, Some(&scheme));
            let unified = klib_unify_marker::unify_marker(trimmed);
            let canonical = |s: &str| hierarchy::compat::canonical_marker::canonical_marker(s);
            let classified = hierarchy::compat::marker_scheme::classify_marker(trimmed);
            let classified_marker = classified.as_ref().map(|v| canonical(&v.0)).unwrap_or_default();
            let effective = if !unified.0.is_empty() { unified.0.clone() } else if !marker.is_empty() { canonical(&marker) } else if !unified.2.is_empty() { canonical(&unified.2) } else { classified_marker };
            if stage >= 6 {
            if let Some(level) = marker_class_level(&effective).or_else(|| marker_class_level(&canonical(&marker))).or_else(|| marker_class_level(&canonical(&unified.2))) {
                level
            } else {
                if stage < 7 { return "body"; }
                let role = if role.is_empty() { unified.1.as_str() } else { role.as_str() };
                let mapped = match role { "장" => "chapter", "중제목" => "section", "소제목" => "topic", "본문" | "내용" => "body", "부연설명" => "asterisk", _ => "body" };
                if classified.as_ref().is_some_and(|(family, _)| family.starts_with('1') || family.contains('가') || ["(1)", "(A)", "(a)"].contains(&family.as_str()))
                    && scheme.canonical(trimmed).is_none() && ["section", "topic"].contains(&mapped) { "body" } else { mapped }
            }
            } else { "body" }
        });
        Value::Str(level.into())
    }).collect()
}

pub(crate) fn classify_extracted(paragraphs: &[crate::extract::Paragraph], scheme: &hierarchy::compat::marker_scheme::MarkerScheme) -> Vec<String> {
    let values: Vec<Value> = paragraphs.iter().map(|p| Value::obj(vec![
        ("text", p.text.clone().into()), ("is_table_cell", p.is_table_cell.into()),
        ("style_name", p.style_name.clone().map(Value::Str).unwrap_or(Value::Null)),
        ("outline_level", p.outline_level.map(Value::from).unwrap_or(Value::Null)),
    ])).collect();
    classify_with_scheme(&values, 7, scheme).into_iter().map(|v| match v { Value::Str(s) => s, _ => "body".into() }).collect()
}

// Use the already ported Rust unify_marker implementation through a small local adapter.
mod klib_unify_marker {
    pub fn unify_marker(text: &str) -> (String, String, String) { crate::unify_marker::unify_marker(text) }
}

fn marker_class_level(class: &str) -> Option<&'static str> {
    match class {
        "□" | "ㅁ" | "■" => Some("square"),
        "ㅇ" | "○" | "◦" => Some("circle"),
        "-" | "–" | "—" => Some("dash"),
        "*" | "**" | "※" | "•" | "·" | "‧" | "∙" | "⋅" | "ㆍ" | "●" | "・" => Some("asterisk"),
        _ => None,
    }
}
