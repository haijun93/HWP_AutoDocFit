use serde_json::Value;
use std::collections::BTreeSet;

/// Detects inline formatting effects that differ from the main character style,
/// matching style_inventory.inline_effects.
///
/// Python reference:
/// def inline_effects(main, other):
///     if not main or not other:
///         return set()
///     out = set()
///     if other["superscript"]:
///         out.add("위첨자")
///     if other["subscript"]:
///         out.add("아래첨자")
///     if other["color"].upper() != main["color"].upper():
///         out.add(f"글자색 {other['color'].upper()}")
///     if other["shade"] and other["shade"] != main["shade"]:
///         out.add(f"음영 {other['shade']}")
///     if other["bold"] and not main["bold"]:
///         out.add("굵게")
///     if other["italic"] and not main["italic"]:
///         out.add("기울임")
///     if other["underline"] != "NONE" and main["underline"] == "NONE":
///         out.add("밑줄")
///     return out
pub fn inline_effects(main: Option<&Value>, other: Option<&Value>) -> Vec<String> {
    let main_obj = match main {
        Some(Value::Object(m)) => m,
        _ => return Vec::new(),
    };
    let other_obj = match other {
        Some(Value::Object(o)) => o,
        _ => return Vec::new(),
    };

    let mut out: BTreeSet<String> = BTreeSet::new();

    if other_obj
        .get("superscript")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        out.insert("위첨자".to_string());
    }

    if other_obj
        .get("subscript")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        out.insert("아래첨자".to_string());
    }

    let other_color = other_obj
        .get("color")
        .and_then(Value::as_str)
        .unwrap_or("#000000")
        .to_ascii_uppercase();
    let main_color = main_obj
        .get("color")
        .and_then(Value::as_str)
        .unwrap_or("#000000")
        .to_ascii_uppercase();
    if other_color != main_color {
        out.insert(format!("글자색 {other_color}"));
    }

    let other_shade = other_obj
        .get("shade")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let main_shade = main_obj
        .get("shade")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if let Some(shade) = other_shade {
        if Some(shade) != main_shade {
            out.insert(format!("음영 {shade}"));
        }
    }

    let other_bold = other_obj
        .get("bold")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let main_bold = main_obj
        .get("bold")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if other_bold && !main_bold {
        out.insert("굵게".to_string());
    }

    let other_italic = other_obj
        .get("italic")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let main_italic = main_obj
        .get("italic")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if other_italic && !main_italic {
        out.insert("기울임".to_string());
    }

    let other_underline = other_obj
        .get("underline")
        .and_then(Value::as_str)
        .unwrap_or("NONE");
    let main_underline = main_obj
        .get("underline")
        .and_then(Value::as_str)
        .unwrap_or("NONE");
    if other_underline != "NONE" && main_underline == "NONE" {
        out.insert("밑줄".to_string());
    }

    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn returns_empty_when_missing() {
        assert_eq!(inline_effects(None, None), Vec::<String>::new());
        assert_eq!(inline_effects(Some(&json!({})), None), Vec::<String>::new());
        assert_eq!(inline_effects(None, Some(&json!({}))), Vec::<String>::new());
    }

    #[test]
    fn detects_all_effects() {
        let main = json!({
            "superscript": false,
            "subscript": false,
            "color": "#000000",
            "shade": null,
            "bold": false,
            "italic": false,
            "underline": "NONE"
        });
        let other = json!({
            "superscript": true,
            "subscript": true,
            "color": "#ff0000",
            "shade": "#ffff00",
            "bold": true,
            "italic": true,
            "underline": "BOTTOM"
        });
        let effects = inline_effects(Some(&main), Some(&other));
        assert_eq!(
            effects,
            vec![
                "글자색 #FF0000",
                "굵게",
                "기울임",
                "밑줄",
                "아래첨자",
                "위첨자",
                "음영 #ffff00"
            ]
        );
    }

    #[test]
    fn ignores_effects_already_present_in_main() {
        let main = json!({
            "superscript": false,
            "subscript": false,
            "color": "#FF0000",
            "shade": "#FFFF00",
            "bold": true,
            "italic": true,
            "underline": "BOTTOM"
        });
        let other = json!({
            "superscript": false,
            "subscript": false,
            "color": "#ff0000",
            "shade": "#ffff00",
            "bold": true,
            "italic": true,
            "underline": "BOTTOM"
        });
        let effects = inline_effects(Some(&main), Some(&other));
        assert!(effects.is_empty());
    }

    #[test]
    fn case_insensitive_color_matching() {
        let main = json!({
            "color": "#000000",
            "bold": false, "italic": false, "superscript": false, "subscript": false
        });
        let other = json!({
            "color": "#000000",
            "bold": false, "italic": false, "superscript": false, "subscript": false
        });
        assert!(inline_effects(Some(&main), Some(&other)).is_empty());

        let other_diff = json!({
            "color": "#0000ff",
            "bold": false, "italic": false, "superscript": false, "subscript": false
        });
        assert_eq!(
            inline_effects(Some(&main), Some(&other_diff)),
            vec!["글자색 #0000FF"]
        );
    }
}
