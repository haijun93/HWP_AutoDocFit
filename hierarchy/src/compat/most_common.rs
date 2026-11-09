//! `docfit_core.style_hierarchy._most_common` 1:1 이식.
//! 슬라이스(또는 Counter)에서 가장 자주 등장하는 첫 번째 최빈값을 반환한다.
//! 비어있으면 None(또는 기본값)을 반환한다.

use serde_json::Value;

pub fn most_common(values: &[Value]) -> Option<&Value> {
    let mut best = None;
    let mut best_count = 0usize;
    for (i, value) in values.iter().enumerate() {
        let count = values.iter().filter(|other| *other == value).count();
        if count > best_count {
            best = Some(&values[i]);
            best_count = count;
        }
    }
    best
}

pub fn _most_common(values: &[Value]) -> Option<&Value> {
    most_common(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_most_common_empty() {
        let empty: Vec<Value> = vec![];
        assert_eq!(most_common(&empty), None);
        assert_eq!(_most_common(&empty), None);
    }

    #[test]
    fn test_most_common_single() {
        let vals = vec![json!(1)];
        assert_eq!(most_common(&vals), Some(&json!(1)));
    }

    #[test]
    fn test_most_common_tie_breaks_to_first() {
        let vals = vec![json!("a"), json!("b")];
        assert_eq!(most_common(&vals), Some(&json!("a")));
    }

    #[test]
    fn test_most_common_frequent() {
        let vals = vec![json!(1), json!(2), json!(2), json!(1), json!(2)];
        assert_eq!(most_common(&vals), Some(&json!(2)));
    }

    #[test]
    fn test_most_common_complex_types() {
        let vals = vec![json!([1]), json!([2]), json!([1])];
        assert_eq!(most_common(&vals), Some(&json!([1])));

        let obj_vals = vec![
            json!({"role": "장", "pt": 15}),
            json!({"role": "중제목", "pt": 13}),
            json!({"role": "장", "pt": 15}),
        ];
        assert_eq!(most_common(&obj_vals), Some(&json!({"role": "장", "pt": 15})));
    }
}
