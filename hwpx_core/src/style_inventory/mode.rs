use serde_json::Value;

/// Finds the most frequent element in a counter mapping or pairs list,
/// matching style_inventory._mode.
///
/// Python reference:
/// def _mode(counter, default=None):
///     return counter.most_common(1)[0][0] if counter else default
pub fn mode(counter: &Value, default: Option<&Value>) -> Value {
    let mut items: Vec<(Value, f64)> = Vec::new();

    match counter {
        Value::Array(arr) => {
            for item in arr {
                if let Some(pair) = item.as_array() {
                    if pair.len() >= 2 {
                        let cnt = pair[1]
                            .as_f64()
                            .or_else(|| pair[1].as_i64().map(|i| i as f64))
                            .unwrap_or(0.0);
                        items.push((pair[0].clone(), cnt));
                    }
                }
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                let cnt = v
                    .as_f64()
                    .or_else(|| v.as_i64().map(|i| i as f64))
                    .unwrap_or(0.0);
                items.push((Value::String(k.clone()), cnt));
            }
        }
        _ => {}
    }

    if items.is_empty() {
        return default.cloned().unwrap_or(Value::Null);
    }

    let mut best_key = &items[0].0;
    let mut max_count = items[0].1;

    for (key, cnt) in items.iter().skip(1) {
        if *cnt > max_count {
            max_count = *cnt;
            best_key = key;
        }
    }

    best_key.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn returns_default_when_empty() {
        assert_eq!(mode(&json!({}), None), Value::Null);
        assert_eq!(mode(&json!([]), Some(&json!("default"))), json!("default"));
        assert_eq!(mode(&Value::Null, Some(&json!(0))), json!(0));
    }

    #[test]
    fn finds_most_common_from_pairs() {
        let pairs = json!([["a", 2], ["b", 5], ["c", 1]]);
        assert_eq!(mode(&pairs, None), json!("b"));
    }

    #[test]
    fn preserves_first_encountered_on_tie() {
        let pairs = json!([["first", 3], ["second", 3], ["third", 1]]);
        assert_eq!(mode(&pairs, None), json!("first"));
    }

    #[test]
    fn preserves_number_key_types_from_pairs() {
        let pairs = json!([[10, 2], [20, 7], [30, 1]]);
        assert_eq!(mode(&pairs, None), json!(20));

        let float_pairs = json!([[0.0, 1], [15.5, 4], [2.5, 3]]);
        assert_eq!(mode(&float_pairs, None), json!(15.5));
    }

    #[test]
    fn handles_object_map() {
        let obj = json!({"small": 1, "large": 10});
        assert_eq!(mode(&obj, None), json!("large"));
    }
}
