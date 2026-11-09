use serde_json::{json, Value};

fn get_attr<'a>(node: &'a Value, name: &str) -> Option<&'a str> {
    node.get("attrib")
        .and_then(|a| a.get(name))
        .or_else(|| node.get(name))
        .and_then(Value::as_str)
}

/// Extract character shape sequence (deduplicated consecutively) and main shape,
/// matching style_inventory._char_signature.
///
/// Python reference:
/// def _char_signature(runs, chars):
///     seq, lengths = [], Counter()
///     for run in runs:
///         text = _run_text(run)
///         if not text.strip():
///             continue
///         cid = run.get("charPrIDRef")
///         lengths[cid] += len(text.strip())
///         if not seq or seq[-1] != cid:
///             seq.append(cid)
///     main = lengths.most_common(1)[0][0] if lengths else None
///     return tuple(seq), main
pub fn char_signature(runs: &Value, _chars: Option<&Value>) -> Value {
    let empty_vec = Vec::new();
    let run_list = if let Some(arr) = runs.as_array() {
        arr
    } else if let Some(arr) = runs.get("runs").and_then(Value::as_array) {
        arr
    } else {
        &empty_vec
    };

    let mut seq: Vec<Option<String>> = Vec::new();
    let mut lengths: Vec<(Option<String>, usize)> = Vec::new();

    for run in run_list {
        let text = crate::style_inventory::run_text::run_text(run);
        let stripped = text.trim();
        if stripped.is_empty() {
            continue;
        }
        let cid = get_attr(run, "charPrIDRef").map(|s| s.to_string());
        let char_len = stripped.chars().count();

        if let Some(entry) = lengths.iter_mut().find(|(k, _)| *k == cid) {
            entry.1 += char_len;
        } else {
            lengths.push((cid.clone(), char_len));
        }

        if seq.is_empty() || seq.last().unwrap() != &cid {
            seq.push(cid);
        }
    }

    let mut max_len = 0;
    let mut main_cid = None;
    for (cid, len) in &lengths {
        if *len > max_len {
            max_len = *len;
            main_cid = cid.clone();
        }
    }

    json!([seq, main_cid])
}

pub fn char_signature_from_value(value: &Value) -> Value {
    if value.is_array() {
        char_signature(value, None)
    } else {
        let runs = value.get("runs").unwrap_or(value);
        let chars = value.get("chars");
        char_signature(runs, chars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_sequence_and_main_shape() {
        let runs = json!([
            {
                "tag": "hp:run",
                "attrib": {"charPrIDRef": "0"},
                "children": [{"tag": "hp:t", "text": "Hello"}]
            },
            {
                "tag": "hp:run",
                "attrib": {"charPrIDRef": "0"},
                "children": [{"tag": "hp:t", "text": " World"}]
            },
            {
                "tag": "hp:run",
                "attrib": {"charPrIDRef": "1"},
                "children": [{"tag": "hp:t", "text": "!"}]
            }
        ]);
        let res = char_signature(&runs, None);
        assert_eq!(res, json!([["0", "1"], "0"]));
    }

    #[test]
    fn skips_whitespace_runs() {
        let runs = json!([
            {
                "tag": "hp:run",
                "attrib": {"charPrIDRef": "0"},
                "children": [{"tag": "hp:t", "text": "   "}]
            },
            {
                "tag": "hp:run",
                "attrib": {"charPrIDRef": "2"},
                "children": [{"tag": "hp:t", "text": "Content"}]
            }
        ]);
        let res = char_signature(&runs, None);
        assert_eq!(res, json!([["2"], "2"]));
    }

    #[test]
    fn returns_empty_for_no_runs() {
        let runs = json!([]);
        let res = char_signature(&runs, None);
        assert_eq!(res, json!([[], null]));
    }
}
