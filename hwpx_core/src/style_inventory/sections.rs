use serde_json::Value;

/// Extracts section XML entries from names and sorts them numerically,
/// matching style_inventory._sections.
///
/// Python reference:
/// names = [n for n in z.namelist() if re.fullmatch(r"Contents/section\d+\.xml", n)]
/// return sorted(names, key=lambda n: int(re.search(r"(\d+)", n.rsplit("/", 1)[-1]).group(1)))
pub fn sections(input: &Value) -> Value {
    let raw_names: Vec<String> = match input {
        Value::Array(arr) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        Value::Object(map) => map
            .get("names")
            .or_else(|| map.get("namelist"))
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let prefix = "Contents/section";
    let suffix = ".xml";

    let mut matched: Vec<(usize, u64, String)> = Vec::new();

    for (idx, name) in raw_names.into_iter().enumerate() {
        if name.starts_with(prefix) && name.ends_with(suffix) {
            let middle = &name[prefix.len()..name.len() - suffix.len()];
            if !middle.is_empty() && middle.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(num) = middle.parse::<u64>() {
                    matched.push((idx, num, name));
                }
            }
        }
    }

    // Python's sorted is stable: sort by numeric key first, preserving order of appearance for ties
    matched.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    let result: Vec<Value> = matched
        .into_iter()
        .map(|(_, _, name)| Value::String(name))
        .collect();

    Value::Array(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_sections_numerically() {
        let input = json!({
            "names": [
                "Contents/section10.xml",
                "Contents/section2.xml",
                "Contents/section1.xml",
                "Contents/section0.xml",
                "Contents/header.xml",
                "settings.xml"
            ]
        });
        assert_eq!(
            sections(&input),
            json!([
                "Contents/section0.xml",
                "Contents/section1.xml",
                "Contents/section2.xml",
                "Contents/section10.xml"
            ])
        );
    }

    #[test]
    fn filters_out_invalid_names() {
        let input = json!({
            "names": [
                "Contents/sectionA.xml",
                "Contents/section_1.xml",
                "section1.xml",
                "Contents/section.xml",
                "Contents/section0.xml.bak"
            ]
        });
        assert_eq!(sections(&input), json!([]));
    }

    #[test]
    fn handles_array_input_directly() {
        let input = json!([
            "Contents/section2.xml",
            "Contents/section1.xml"
        ]);
        assert_eq!(
            sections(&input),
            json!([
                "Contents/section1.xml",
                "Contents/section2.xml"
            ])
        );
    }
}
