use serde_json::{json, Value};
use std::collections::BTreeSet;

fn is_dot_marker(c: char) -> bool {
    matches!(c, '•' | '·' | '‧' | '∙' | '⋅' | 'ㆍ' | '●')
}

fn canonical_marker(marker: &str) -> String {
    if marker.chars().count() == 1 {
        let c = marker.chars().next().unwrap();
        if is_dot_marker(c) {
            return "•".to_string();
        }
        if c == '□' || c == 'ㅁ' {
            return "□".to_string();
        }
    }
    marker.to_string()
}

/// Logical indentation and leading marker detector, matching style_hierarchy.leading_marker.
pub fn leading_marker(text: &str) -> (String, String) {
    let stripped = text.trim_start();
    if stripped.is_empty() {
        return (String::new(), String::new());
    }

    // 1. ASCII Roman numerals followed by . or ． then whitespace or end
    let chars: Vec<char> = stripped.chars().collect();
    let mut roman_len = 0;
    while roman_len < chars.len() && matches!(chars[roman_len], 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M') {
        roman_len += 1;
    }
    if roman_len > 0 {
        let valid_len = if roman_len == 1 {
            matches!(chars[0], 'I' | 'V' | 'X')
        } else {
            true
        };
        if valid_len && roman_len < chars.len() && (chars[roman_len] == '.' || chars[roman_len] == '．') {
            let after_dot = roman_len + 1;
            if after_dot == chars.len() || chars[after_dot].is_whitespace() {
                let matched: String = chars[..after_dot].iter().collect();
                return (matched, "중제목".to_string());
            }
        }
    }

    // 2. ❶~❿ at start followed by whitespace or end -> ("-", "내용")
    let first = chars[0];
    if ('❶'..='❿').contains(&first) {
        if chars.len() == 1 || chars[1].is_whitespace() {
            return ("-".to_string(), "내용".to_string());
        }
    }

    // 3. Circled markers: [①-⑳㉠-㉻❶-❿➊-➓](?=\s|$)
    if ('①'..='⑳').contains(&first)
        || ('㉠'..='㉻').contains(&first)
        || ('❶'..='❿').contains(&first)
        || ('➊'..='➓').contains(&first)
    {
        if chars.len() == 1 || chars[1].is_whitespace() {
            return (first.to_string(), String::new());
        }
    }

    // 4. MARKERS in order
    // ("장", r"(?:제\s?\d{1,2}\s?[장편부]|\d{1,2}\s?장|[Cc]hapter\s?\d{1,2})(?=\s|$|[.．:：])")
    let is_chapter_lookahead = |idx: usize| {
        if idx == chars.len() {
            true
        } else {
            let c = chars[idx];
            c.is_whitespace() || matches!(c, '.' | '．' | ':' | '：')
        }
    };

    // Sub-case A: 제\s?\d{1,2}\s?[장편부]
    if chars[0] == '제' {
        let mut idx = 1;
        if idx < chars.len() && chars[idx] == ' ' {
            idx += 1;
        }
        let mut digit_cnt = 0;
        while idx < chars.len() && chars[idx].is_ascii_digit() && digit_cnt < 2 {
            idx += 1;
            digit_cnt += 1;
        }
        if digit_cnt >= 1 && idx < chars.len() {
            if chars[idx] == ' ' {
                idx += 1;
            }
            if idx < chars.len() && matches!(chars[idx], '장' | '편' | '부') {
                let end = idx + 1;
                if is_chapter_lookahead(end) {
                    let matched: String = chars[..end].iter().collect();
                    return (canonical_marker(&matched), "장".to_string());
                }
            }
        }
    }

    // Sub-case B: \d{1,2}\s?장
    if chars[0].is_ascii_digit() {
        let mut idx = 1;
        let mut digit_cnt = 1;
        if idx < chars.len() && chars[idx].is_ascii_digit() {
            idx += 1;
            digit_cnt += 1;
        }
        if digit_cnt <= 2 && idx < chars.len() {
            let mut j = idx;
            if chars[j] == ' ' {
                j += 1;
            }
            if j < chars.len() && chars[j] == '장' {
                let end = j + 1;
                if is_chapter_lookahead(end) {
                    let matched: String = chars[..end].iter().collect();
                    return (canonical_marker(&matched), "장".to_string());
                }
            }
        }
    }

    // Sub-case C: [Cc]hapter\s?\d{1,2}
    let is_chapter_word = if chars.len() >= 7 {
        let word: String = chars[..7].iter().collect();
        word == "Chapter" || word == "chapter"
    } else {
        false
    };
    if is_chapter_word {
        let mut idx = 7;
        if idx < chars.len() && chars[idx] == ' ' {
            idx += 1;
        }
        let mut digit_cnt = 0;
        while idx < chars.len() && chars[idx].is_ascii_digit() && digit_cnt < 2 {
            idx += 1;
            digit_cnt += 1;
        }
        if digit_cnt >= 1 && is_chapter_lookahead(idx) {
            let matched: String = chars[..idx].iter().collect();
            return (canonical_marker(&matched), "장".to_string());
        }
    }

    // ("중제목", r"(?:[Ⅰ-Ⅿ]+|[가-하])(?=\s|[.．])")
    let is_midtitle_lookahead = |idx: usize| {
        if idx < chars.len() {
            let c = chars[idx];
            c.is_whitespace() || c == '.' || c == '．'
        } else {
            false
        }
    };
    if ('Ⅰ'..='Ⅿ').contains(&first) {
        let mut idx = 1;
        while idx < chars.len() && ('Ⅰ'..='Ⅿ').contains(&chars[idx]) {
            idx += 1;
        }
        if is_midtitle_lookahead(idx) {
            let matched: String = chars[..idx].iter().collect();
            return (canonical_marker(&matched), "중제목".to_string());
        }
    }
    if ('가'..='하').contains(&first) && is_midtitle_lookahead(1) {
        let matched: String = chars[..1].iter().collect();
        return (canonical_marker(&matched), "중제목".to_string());
    }

    // ("소제목", r"(?:[□ㅁ■]|\d{1,2}[.．]?)(?=\s|$)")
    let is_whitespace_or_end = |idx: usize| idx == chars.len() || chars[idx].is_whitespace();
    if matches!(first, '□' | 'ㅁ' | '■') && is_whitespace_or_end(1) {
        let matched: String = chars[..1].iter().collect();
        return (canonical_marker(&matched), "소제목".to_string());
    }
    if first.is_ascii_digit() {
        let mut idx = 1;
        if idx < chars.len() && chars[idx].is_ascii_digit() {
            idx += 1;
        }
        let num_end = idx;
        if idx < chars.len() && (chars[idx] == '.' || chars[idx] == '．') {
            idx += 1;
        }
        if is_whitespace_or_end(idx) {
            let matched: String = chars[..idx].iter().collect();
            return (canonical_marker(&matched), "소제목".to_string());
        } else if is_whitespace_or_end(num_end) {
            let matched: String = chars[..num_end].iter().collect();
            return (canonical_marker(&matched), "소제목".to_string());
        }
    }

    // ("본문", r"(?:[ㅇ○◦☞]|[가-하]\))(?=\s|$)")
    if matches!(first, 'ㅇ' | '○' | '◦' | '☞') && is_whitespace_or_end(1) {
        let matched: String = chars[..1].iter().collect();
        return (canonical_marker(&matched), "본문".to_string());
    }
    if ('가'..='하').contains(&first) && chars.len() > 1 && chars[1] == ')' && is_whitespace_or_end(2) {
        let matched: String = chars[..2].iter().collect();
        return (canonical_marker(&matched), "본문".to_string());
    }

    // ("내용", r"(?:-|\d+\))(?=\s|$)")
    if first == '-' && is_whitespace_or_end(1) {
        return ("-".to_string(), "내용".to_string());
    }
    if first.is_ascii_digit() {
        let mut idx = 1;
        while idx < chars.len() && chars[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx < chars.len() && chars[idx] == ')' && is_whitespace_or_end(idx + 1) {
            let matched: String = chars[..idx + 1].iter().collect();
            return (canonical_marker(&matched), "내용".to_string());
        }
    }

    // ("부연설명", r"(?:\*\*?|※|[•·‧∙⋅ㆍ●]|\([가-하0-9]+\)[.．]?)(?=\s|$)")
    if first == '*' {
        let mut idx = 1;
        if idx < chars.len() && chars[idx] == '*' {
            idx += 1;
        }
        if is_whitespace_or_end(idx) {
            let matched: String = chars[..idx].iter().collect();
            return (canonical_marker(&matched), "부연설명".to_string());
        }
    }
    if first == '※' && is_whitespace_or_end(1) {
        return ("※".to_string(), "부연설명".to_string());
    }
    if is_dot_marker(first) && is_whitespace_or_end(1) {
        return ("•".to_string(), "부연설명".to_string());
    }
    if first == '(' {
        let mut idx = 1;
        let mut has_inner = false;
        while idx < chars.len() && (('가'..='하').contains(&chars[idx]) || chars[idx].is_ascii_digit()) {
            idx += 1;
            has_inner = true;
        }
        if has_inner && idx < chars.len() && chars[idx] == ')' {
            idx += 1;
            if idx < chars.len() && (chars[idx] == '.' || chars[idx] == '．') {
                idx += 1;
            }
            if is_whitespace_or_end(idx) {
                let matched: String = chars[..idx].iter().collect();
                return (canonical_marker(&matched), "부연설명".to_string());
            }
        }
    }

    (String::new(), String::new())
}

fn get_node_attr<'a>(node: &'a Value, name: &str) -> Option<&'a str> {
    node.get("attrib")
        .and_then(|a| a.get(name))
        .or_else(|| node.get(name))
        .and_then(Value::as_str)
}

/// Computes paragraph style key and metrics, matching style_inventory.paragraph_style_type.
///
/// Python reference:
/// def paragraph_style_type(para, defs):
///     runs = _paragraph_runs(para)
///     text = "".join(_run_text(r) for r in runs)
///     if not text.strip():
///         return None
///     chars, paras = defs["chars"], defs["paras"]
///     seq, main = _char_signature(runs, chars)
///     c, p = chars.get(main), paras.get(para.get("paraPrIDRef"))
///     if c is None or p is None:
///         return None
///     marker, role = leading_marker(text)
///     emphasis = any(chars.get(cid, {}).get("bold") for cid in seq) and not c["bold"]
///     effects = set()
///     for cid in seq:
///         if cid != main:
///             effects |= inline_effects(c, chars.get(cid))
///     key = (_marker_class(marker), c["font"].get("hangul"), c["size_pt"], c["bold"], c["italic"],
///            c["underline"], c["color"].upper(), c["shade"], c["ratio"], p["align"], round(_mm(p["left"])))
///     ls = p["line_spacing"]
///     return key, {
///         "marker": _marker_class(marker), "role": role, "main_char": main,
///         "para": para.get("paraPrIDRef"), "style": para.get("styleIDRef"), "char_seq": seq,
///         "spacing": c["spacing"], "indent_mm": _mm(p["indent"]), "prev_pt": p["prev"] / 100,
///         "line": ls["value"] if ls and ls["type"] == "PERCENT" else None,
///         "emphasis": emphasis, "effects": tuple(sorted(effects)), "text": text.strip(),
///     }
pub fn paragraph_style_type(para: &Value, defs: &Value) -> Option<Value> {
    let runs = crate::style_inventory::paragraph_runs::paragraph_runs(para);
    let run_list = runs.as_array()?;

    let mut full_text = String::new();
    for r in run_list {
        full_text.push_str(&crate::style_inventory::run_text::run_text(r));
    }
    let text_trimmed = full_text.trim();
    if text_trimmed.is_empty() {
        return None;
    }

    let chars = defs.get("chars")?;
    let paras = defs.get("paras")?;

    let sig = crate::style_inventory::char_signature::char_signature(&runs, Some(chars));
    let seq_val = sig.get(0)?.as_array()?;
    let main_val = sig.get(1)?;
    let main_cid = match main_val {
        Value::String(s) => s.as_str(),
        _ => return None,
    };

    let c = chars.get(main_cid)?;
    let para_pr_id = get_node_attr(para, "paraPrIDRef")?;
    let p = paras.get(para_pr_id)?;

    let (marker, role) = leading_marker(&full_text);
    let marker_class = crate::style_inventory::marker_class::marker_class(&marker);

    let c_bold = c.get("bold").and_then(Value::as_bool).unwrap_or(false);
    let mut any_bold = false;
    for cid_val in seq_val {
        if let Some(cid_str) = cid_val.as_str() {
            if chars
                .get(cid_str)
                .and_then(|info| info.get("bold"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                any_bold = true;
                break;
            }
        }
    }
    let emphasis = any_bold && !c_bold;

    let mut effects_set: BTreeSet<String> = BTreeSet::new();
    for cid_val in seq_val {
        if let Some(cid_str) = cid_val.as_str() {
            if cid_str != main_cid {
                if let Some(other_char) = chars.get(cid_str) {
                    for eff in crate::style_inventory::inline_effects::inline_effects(Some(c), Some(other_char)) {
                        effects_set.insert(eff);
                    }
                }
            }
        }
    }
    let effects: Vec<String> = effects_set.into_iter().collect();

    let hangul_font = c
        .get("font")
        .and_then(|f| f.get("hangul"))
        .cloned()
        .unwrap_or(Value::Null);
    let size_pt = c.get("size_pt").cloned().unwrap_or(json!(0.0));
    let italic = c.get("italic").cloned().unwrap_or(json!(false));
    let underline = c.get("underline").cloned().unwrap_or(json!("NONE"));
    let color_upper = c
        .get("color")
        .and_then(Value::as_str)
        .unwrap_or("#000000")
        .to_ascii_uppercase();
    let shade = c.get("shade").cloned().unwrap_or(Value::Null);
    let ratio = c.get("ratio").cloned().unwrap_or(json!(100));

    let align = p.get("align").cloned().unwrap_or(json!("JUSTIFY"));

    let left_hwpunit = p
        .get("left")
        .and_then(Value::as_f64)
        .or_else(|| p.get("left").and_then(Value::as_i64).map(|i| i as f64))
        .unwrap_or(0.0);
    let left_mm = (left_hwpunit / (7200.0 / 25.4) * 10.0).round() / 10.0;
    let left_mm_round = left_mm.round() as i64;

    let key = json!([
        marker_class,
        hangul_font,
        size_pt,
        c_bold,
        italic,
        underline,
        color_upper,
        shade,
        ratio,
        align,
        left_mm_round
    ]);

    let indent_hwpunit = p
        .get("indent")
        .and_then(Value::as_f64)
        .or_else(|| p.get("indent").and_then(Value::as_i64).map(|i| i as f64))
        .unwrap_or(0.0);
    let indent_mm = (indent_hwpunit / (7200.0 / 25.4) * 10.0).round() / 10.0;

    let prev_val = p
        .get("prev")
        .and_then(Value::as_f64)
        .or_else(|| p.get("prev").and_then(Value::as_i64).map(|i| i as f64))
        .unwrap_or(0.0);
    let prev_pt = prev_val / 100.0;

    let ls = p.get("line_spacing");
    let line_val = if ls
        .and_then(|l| l.get("type"))
        .and_then(Value::as_str)
        == Some("PERCENT")
    {
        ls.and_then(|l| l.get("value")).cloned().unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    let style_id = get_node_attr(para, "styleIDRef").map(|s| json!(s)).unwrap_or(Value::Null);

    let meas = json!({
        "marker": marker_class,
        "role": role,
        "main_char": main_cid,
        "para": para_pr_id,
        "style": style_id,
        "char_seq": seq_val,
        "spacing": c.get("spacing").cloned().unwrap_or(json!(0)),
        "indent_mm": indent_mm,
        "prev_pt": prev_pt,
        "line": line_val,
        "emphasis": emphasis,
        "effects": effects,
        "text": text_trimmed
    });

    Some(json!([key, meas]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tests_leading_marker_patterns() {
        assert_eq!(leading_marker(""), (String::new(), String::new()));
        assert_eq!(leading_marker("   "), (String::new(), String::new()));
        assert_eq!(leading_marker("I. 중제목 시작"), ("I.".to_string(), "중제목".to_string()));
        assert_eq!(leading_marker("III． 중제목 시작"), ("III．".to_string(), "중제목".to_string()));
        assert_eq!(leading_marker("가. 중제목"), ("가".to_string(), "중제목".to_string()));
        assert_eq!(leading_marker("Ⅰ 중제목"), ("Ⅰ".to_string(), "중제목".to_string()));
        assert_eq!(leading_marker("제1장 총칙"), ("제1장".to_string(), "장".to_string()));
        assert_eq!(leading_marker("1장 서론"), ("1장".to_string(), "장".to_string()));
        assert_eq!(leading_marker("Chapter 1 Intro"), ("Chapter 1".to_string(), "장".to_string()));
        assert_eq!(leading_marker("□ 소제목"), ("□".to_string(), "소제목".to_string()));
        assert_eq!(leading_marker("ㅁ 소제목"), ("□".to_string(), "소제목".to_string()));
        assert_eq!(leading_marker("1. 소제목"), ("1.".to_string(), "소제목".to_string()));
        assert_eq!(leading_marker("12. 소제목"), ("12.".to_string(), "소제목".to_string()));
        assert_eq!(leading_marker("ㅇ 본문"), ("ㅇ".to_string(), "본문".to_string()));
        assert_eq!(leading_marker("가) 본문"), ("가)".to_string(), "본문".to_string()));
        assert_eq!(leading_marker("- 내용"), ("-".to_string(), "내용".to_string()));
        assert_eq!(leading_marker("1) 내용"), ("1)".to_string(), "내용".to_string()));
        assert_eq!(leading_marker("❶ 내용"), ("-".to_string(), "내용".to_string()));
        assert_eq!(leading_marker("① 원문자"), ("①".to_string(), String::new()));
        assert_eq!(leading_marker("㉠ 괄호"), ("㉠".to_string(), String::new()));
        assert_eq!(leading_marker("※ 부연"), ("※".to_string(), "부연설명".to_string()));
        assert_eq!(leading_marker("• 부연"), ("•".to_string(), "부연설명".to_string()));
        assert_eq!(leading_marker("● 부연"), ("•".to_string(), "부연설명".to_string()));
        assert_eq!(leading_marker("(1) 부연"), ("(1)".to_string(), "부연설명".to_string()));
        assert_eq!(leading_marker("일반 문단입니다."), (String::new(), String::new()));
    }

    #[test]
    fn returns_none_on_empty_text() {
        let para = json!({
            "tag": "hp:p",
            "attrib": {"paraPrIDRef": "0"},
            "children": []
        });
        let defs = json!({"chars": {}, "paras": {}});
        assert_eq!(paragraph_style_type(&para, &defs), None);
    }

    #[test]
    fn parses_paragraph_style_type_basic() {
        let para = json!({
            "tag": "hp:p",
            "attrib": {"paraPrIDRef": "p0", "styleIDRef": "s0"},
            "children": [
                {
                    "tag": "hp:run",
                    "attrib": {"charPrIDRef": "c0"},
                    "children": [{"tag": "hp:t", "text": "□ 소제목 내용"}]
                }
            ]
        });
        let defs = json!({
            "chars": {
                "c0": {
                    "font": {"hangul": "함초롬바탕"},
                    "size_pt": 10.0,
                    "bold": false,
                    "italic": false,
                    "underline": "NONE",
                    "color": "#000000",
                    "shade": null,
                    "ratio": 100,
                    "spacing": 0
                }
            },
            "paras": {
                "p0": {
                    "align": "JUSTIFY",
                    "left": 0,
                    "indent": 0,
                    "prev": 0,
                    "line_spacing": {"type": "PERCENT", "value": 160}
                }
            }
        });

        let result = paragraph_style_type(&para, &defs);
        assert!(result.is_some());
        let val = result.unwrap();
        let arr = val.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        let key = &arr[0];
        let meas = &arr[1];
        assert_eq!(key[0], "□");
        assert_eq!(key[1], "함초롬바탕");
        assert_eq!(meas["marker"], "□");
        assert_eq!(meas["role"], "소제목");
        assert_eq!(meas["main_char"], "c0");
        assert_eq!(meas["para"], "p0");
        assert_eq!(meas["style"], "s0");
        assert_eq!(meas["text"], "□ 소제목 내용");
    }
}
