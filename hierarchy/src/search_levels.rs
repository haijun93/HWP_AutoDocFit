//! Search hierarchy schema 5. Formatting roles are a separate API.
use crate::compat::{
    canonical_marker::canonical_marker,
    leading_marker::leading_marker_with_scheme,
    marker_scheme::{classify_marker, detect_scheme},
};
use serde_json::Value;
use std::collections::HashMap;

pub use crate::search_unify_marker::unify_marker;

fn string<'a>(p: &'a Value, key: &str) -> &'a str {
    p[key].as_str().unwrap_or("")
}
fn cell(p: &Value) -> bool {
    p["is_table_cell"].as_bool().unwrap_or(false)
}
fn markdown_table_row(text: &str) -> bool {
    let text = text.trim();
    text.len() >= 2 && !text.contains('\n') && text.starts_with('|') && text.ends_with('|')
}
fn heading(s: &str) -> Option<usize> {
    let n = s.chars().take_while(|c| *c == '#').count();
    if (1..=4).contains(&n)
        && s[n..]
            .chars()
            .next()
            .map(char::is_whitespace)
            .unwrap_or(true)
    {
        Some(n)
    } else {
        None
    }
}
fn family_rank(f: &str) -> Option<usize> {
    match f {
        "Ⅰ." | "I." => Some(1),
        "1." => Some(2),
        "가." | "(1)" => Some(3),
        "①" | "㉠" | "㉮" => Some(4),
        _ => None,
    }
}
fn numbered_level(n: usize) -> &'static str {
    match n {
        1 => "chapter",
        2 => "section",
        3 => "topic",
        _ => "body",
    }
}
fn symbol(s: &str) -> Option<&'static str> {
    match s {
        "□" | "ㅁ" | "■" => Some("square"),
        "ㅇ" | "○" | "◦" => Some("circle"),
        "-" | "–" | "—" | "−" | "－" => Some("dash"),
        "*" | "**" | "※" | "•" | "·" | "‧" | "∙" | "⋅" | "ㆍ" | "●" | "・" => {
            Some("asterisk")
        }
        _ => None,
    }
}
fn chapter(s: &str) -> Option<(u32, bool)> {
    let rest = s.strip_prefix('제')?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let tail = rest.get(digits.len()..)?.strip_prefix('장')?;
    if !tail.is_empty() && !tail.chars().next()?.is_whitespace() {
        return None;
    }
    Some((
        digits.parse().ok()?,
        !tail.trim().is_empty() && s.chars().count() <= 60,
    ))
}
fn ordinal(s: &str, f: &str) -> Option<u32> {
    let (_, end) = classify_marker(s)?;
    let token: String = s.chars().take(end).collect();
    crate::compat::marker_scheme::ordinal(f, &token)?
        .first()
        .copied()
        .and_then(|n| n.try_into().ok())
}

/// Adapter for clients using their own JSON representation.
pub fn classify_json(input: &str) -> Result<Vec<String>, String> {
    let paragraphs: Vec<Value> = serde_json::from_str(input).map_err(|e| e.to_string())?;
    Ok(classify(&paragraphs))
}

pub fn classify(paragraphs: &[Value]) -> Vec<String> {
    let path = paragraphs
        .first()
        .map(|p| string(p, "source_path"))
        .unwrap_or("");
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let plain = matches!(ext.as_str(), "txt" | "md");
    let mut labels = HashMap::new();
    let texts: Vec<String> = paragraphs
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let raw = string(p, "text");
            if !cell(p) {
                if let Some((label, body)) = raw.trim_start().split_once([':', '：']) {
                    if !body.trim().is_empty() {
                        let marker = match label.trim_end() {
                            "네모" => Some(("□", "square")),
                            "원" => Some(("ㅇ", "circle")),
                            "바" => Some(("-", "dash")),
                            "점" => Some(("•", "asterisk")),
                            "당구" => Some(("※", "asterisk")),
                            "주석" | "주석1" => Some(("*", "asterisk")),
                            "주석2" => Some(("**", "asterisk")),
                            _ => None,
                        };
                        if let Some((m, l)) = marker {
                            labels.insert(i, l);
                            return format!("{m} {}", body.trim_start());
                        }
                        if label.trim_end() == "개요" {
                            labels.insert(i, "body");
                        }
                    }
                }
            }
            raw.to_owned()
        })
        .collect();
    let table: Vec<bool> = paragraphs
        .iter()
        .enumerate()
        .map(|(i, p)| cell(p) || (plain && markdown_table_row(&texts[i])))
        .collect();
    let evidence: Vec<String> = texts
        .iter()
        .map(|t| {
            if plain && markdown_table_row(t) {
                String::new()
            } else {
                t.clone()
            }
        })
        .collect();
    let scheme = detect_scheme(&evidence);
    let heads: Vec<_> = evidence.iter().map(|t| heading(t.trim())).collect();
    let top = heads.iter().filter(|h| **h == Some(1)).count();
    let first = heads.iter().position(|h| *h == Some(1));
    let mut candidates = HashMap::new();
    let mut groups: Vec<(String, Vec<(usize, u32)>)> = Vec::new();
    if plain
        && !heads
            .iter()
            .enumerate()
            .any(|(i, h)| !cell(&paragraphs[i]) && h.is_some())
    {
        for (i, t) in texts.iter().enumerate() {
            if cell(&paragraphs[i]) || t.trim().chars().count() > 60 {
                continue;
            }
            let found = if let Some((n, true)) = chapter(t.trim()) {
                Some(("제N장".to_owned(), n))
            } else {
                classify_marker(t.trim()).and_then(|(f, end)| {
                    if matches!(f.as_str(), "Ⅰ." | "I." | "1." | "가.")
                        && !t
                            .trim()
                            .chars()
                            .skip(end)
                            .collect::<String>()
                            .trim()
                            .is_empty()
                    {
                        ordinal(t.trim(), &f).map(|n| (if f == "I." { "Ⅰ.".into() } else { f }, n))
                    } else {
                        None
                    }
                })
            };
            if let Some((f, n)) = found {
                candidates.insert(i, f.clone());
                if let Some(g) = groups.iter_mut().find(|g| g.0 == f) {
                    g.1.push((i, n));
                } else {
                    groups.push((f, vec![(i, n)]));
                }
            }
        }
    }
    let mut plain_levels = HashMap::new();
    for (f, rows) in groups {
        let i = rows[0].0;
        let follows = i > 0 && !candidates.contains_key(&(i - 1)) && heads[i - 1].is_none();
        if !follows && rows.windows(2).any(|w| w[1].1 == w[0].1 + 1) {
            let n = plain_levels.len() + 1;
            plain_levels.insert(f, numbered_level(n.min(3)));
        }
    }
    let chapter_numbers: Vec<_> = texts
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            !cell(&paragraphs[*i])
                && string(&paragraphs[*i], "style_name").is_empty()
                && paragraphs[*i]["outline_level"].is_null()
        })
        .filter_map(|(_, t)| chapter(t.trim()).filter(|x| x.1).map(|x| x.0))
        .collect();
    let confirmed_chapters = chapter_numbers.windows(2).any(|w| w[1] == w[0] + 1);
    let mut numbered: HashMap<String, usize> = scheme
        .family_levels
        .keys()
        .filter_map(|f| family_rank(f).map(|n| (f.clone(), n)))
        .collect();
    if !numbered.is_empty() {
        let order = ["Ⅰ.", "I.", "1.", "가.", "(1)", "①", "㉠", "㉮"];
        let observed: Vec<_> = order
            .iter()
            .filter_map(|f| {
                let rows: Vec<_> = evidence
                    .iter()
                    .filter(|t| classify_marker(t).map(|v| v.0 == *f).unwrap_or(false))
                    .collect();
                if rows.is_empty() {
                    None
                } else {
                    Some((
                        *f,
                        rows.iter()
                            .map(|t| t.chars().take_while(|c| c.is_whitespace()).count())
                            .min()
                            .unwrap(),
                    ))
                }
            })
            .collect();
        if observed.len() >= 2
            && observed
                .windows(2)
                .all(|w| family_rank(w[0].0) < family_rank(w[1].0) && w[0].1 <= w[1].1)
        {
            for (f, _) in observed {
                if matches!(f, "(1)" | "①" | "㉠" | "㉮") {
                    numbered.insert(f.into(), family_rank(f).unwrap());
                }
            }
        }
    }
    let mut levels: Vec<String> = paragraphs
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let s = texts[i].trim();
            let style = string(p, "style_name").to_lowercase();
            let outline = p["outline_level"].as_i64();
            let level = if table[i] {
                "table_cell"
            } else if ["문서제목", "문서 제목", "title"]
                .iter()
                .any(|w| style.contains(w))
                || style == "제목"
                || outline == Some(0)
            {
                "document_title"
            } else if [
                "heading 1",
                "heading1",
                "제목 1",
                "제목1",
                "개요 1",
                "개요1",
            ]
            .iter()
            .any(|w| style.contains(w))
                || outline == Some(1)
            {
                "chapter"
            } else if [
                "heading 2",
                "heading2",
                "제목 2",
                "제목2",
                "개요 2",
                "개요2",
            ]
            .iter()
            .any(|w| style.contains(w))
                || outline == Some(2)
            {
                "section"
            } else if [
                "heading 3",
                "heading3",
                "제목 3",
                "제목3",
                "개요 3",
                "개요3",
            ]
            .iter()
            .any(|w| style.contains(w))
                || outline == Some(3)
            {
                "topic"
            } else if let Some(d) = heads[i].filter(|d| *d < 4 || top <= 1) {
                if d == 1 {
                    if Some(i) == first {
                        "document_title"
                    } else {
                        "chapter"
                    }
                } else {
                    numbered_level(if top <= 1 { d - 1 } else { d })
                }
            } else if plain
                && chapter(s).is_some()
                && string(p, "style_name").is_empty()
                && outline.is_none()
            {
                if confirmed_chapters && chapter(s).unwrap().1 {
                    "chapter"
                } else {
                    "body"
                }
            } else {
                let classified = classify_marker(s);
                let family = classified.as_ref().map(|v| v.0.as_str()).unwrap_or("");
                if plain && matches!(family, "Ⅰ." | "I." | "1." | "가.") {
                    if s.chars().count() <= 60 {
                        *plain_levels
                            .get(if family == "I." { "Ⅰ." } else { family })
                            .unwrap_or(&"body")
                    } else {
                        "body"
                    }
                } else if let Some(n) = numbered.get(family) {
                    numbered_level(*n)
                } else if let Some(n) = scheme
                    .family_levels
                    .get(family)
                    .filter(|n| (1..=3).contains(*n))
                {
                    match n {
                        1 => "square",
                        2 => "circle",
                        3 => "dash",
                        _ => "body",
                    }
                } else {
                    let (m, r) = leading_marker_with_scheme(s, Some(&scheme));
                    let u = unify_marker(s);
                    let effective = if !u.0.is_empty() {
                        u.0.clone()
                    } else if !m.is_empty() {
                        canonical_marker(&m)
                    } else if !u.2.is_empty() {
                        canonical_marker(&u.2)
                    } else {
                        canonical_marker(family)
                    };
                    if let Some(l) = symbol(&effective)
                        .or_else(|| symbol(&canonical_marker(&m)))
                        .or_else(|| symbol(&canonical_marker(&u.2)))
                    {
                        l
                    } else {
                        let role = if r.is_empty() {
                            u.1.as_str()
                        } else {
                            r.as_str()
                        };
                        let l = match role {
                            "장" => "chapter",
                            "중제목" => "section",
                            "소제목" => "topic",
                            "부연설명" => "asterisk",
                            _ => "body",
                        };
                        if (family.starts_with('1')
                            || family.contains('가')
                            || matches!(family, "①" | "㉠" | "㉮"))
                            && scheme.canonical(s).is_none()
                            && !scheme.family_levels.contains_key(family)
                            && matches!(l, "section" | "topic")
                        {
                            "body"
                        } else {
                            l
                        }
                    }
                }
            };
            labels.get(&i).copied().unwrap_or(level).to_owned()
        })
        .collect();
    if !levels.iter().any(|l| l == "document_title") {
        if ext == "hwpx" {
            if let Some(start) = paragraphs.iter().position(cell).filter(|i| {
                *i < 3
                    && (paragraphs[*i]["table_id"].is_null()
                        || paragraphs[*i]["table_id"].as_i64() == Some(1))
            }) {
                let table_id = &paragraphs[start]["table_id"];
                let rows: Vec<_> = (start..paragraphs.len())
                    .filter(|i| table_id.is_null() || paragraphs[*i]["table_id"] == *table_id)
                    .collect();
                if let Some(i) = rows
                    .into_iter()
                    .take_while(|i| levels[*i] == "table_cell")
                    .find(|i| {
                        let s = texts[*i].trim();
                        ((4..=60).contains(&s.chars().count())
                            || !paragraphs[*i]["table_cell_id"].is_null())
                            && s.chars().any(char::is_alphabetic)
                    })
                {
                    let id = &paragraphs[i]["table_cell_id"];
                    for j in 0..paragraphs.len() {
                        if cell(&paragraphs[j])
                            && if id.is_null() {
                                j == i
                            } else {
                                paragraphs[j]["table_cell_id"] == *id
                            }
                        {
                            levels[j] = "document_title".into();
                        }
                    }
                }
            }
        } else if plain {
            if let Some(i) = texts
                .iter()
                .position(|t| t.trim().starts_with("제목1:") || t.trim().starts_with("제목:"))
            {
                levels[i] = "document_title".into();
            }
        }
    }
    levels
}

pub fn response(value: &Value) -> Value {
    let paragraphs = value
        .get("paragraphs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut result =
        crate::compat::analyze_hierarchy::analyze_hierarchy(&Value::Array(paragraphs.clone()));
    if paragraphs.iter().any(|p| p.get("source_path").is_some()) {
        result["search_levels"] = serde_json::json!(classify(&paragraphs));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn heading_tokens_require_space_and_stop_at_four() {
        assert!(!markdown_table_row("|"));
        assert!(!markdown_table_row("|\n|"));
        assert!(markdown_table_row("||"));
        assert!(markdown_table_row(" | cell | "));
        assert_eq!(heading("#### topic"), Some(4));
        assert_eq!(heading("#"), Some(1));
        assert_eq!(heading("#tag"), None);
        assert_eq!(heading("##### deep"), None);
        assert_eq!(heading("한글 본문"), None);
    }
    #[test]
    fn numbered_chapter_tokens_require_a_title() {
        assert_eq!(chapter("제1장 계획"), Some((1, true)));
        assert_eq!(chapter("제2장"), Some((2, false)));
        assert_eq!(chapter("제3장본문"), None);
        assert_eq!(ordinal("Ⅱ. 계획", "Ⅰ."), Some(2));
        assert_eq!(ordinal("II. 계획", "I."), Some(2));
    }
    #[test]
    fn markdown_context() {
        let p: Vec<_> = ["# 제목", "## 장", "### 절", "#### 주제"]
            .iter()
            .map(|t| json!({"text":t,"source_path":"x.md"}))
            .collect();
        assert_eq!(
            classify(&p),
            ["document_title", "chapter", "section", "topic"]
        );
    }
    #[test]
    fn labels_and_cells() {
        let p = vec![
            json!({"text":"네모: 제목","source_path":"x.txt"}),
            json!({"text":"개요: 요약"}),
            json!({"text":"원: 셀","is_table_cell":true}),
        ];
        assert_eq!(classify(&p), ["square", "body", "table_cell"]);
    }
    #[test]
    fn deeper_symbol_family_falls_through_to_canonical_marker() {
        let p: Vec<_> = ["# title", "□ item", "ㅇ text", "|", "||"]
            .iter()
            .map(|text| json!({"text":text, "source_path":"x.md"}))
            .collect();
        assert_eq!(
            classify(&p),
            ["document_title", "square", "circle", "dash", "table_cell"]
        );
    }
}
