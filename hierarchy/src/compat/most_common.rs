use serde_json::Value;
pub fn most_common(values: &[Value]) -> Option<&Value> {
    let mut best = None;
    let mut best_count = 0usize;
    for (i, value) in values.iter().enumerate() {
        let count = values.iter().filter(|other| *other == value).count();
        if count > best_count { best = Some(&values[i]); best_count = count; }
    }
    best
}
