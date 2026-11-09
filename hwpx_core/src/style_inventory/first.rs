use serde_json::Value;

/// Find the first element whose local tag matches name, in document order.
pub fn first<'a>(tree: &'a Value, name: &str) -> Option<&'a Value> {
    if tree.get("tag").and_then(Value::as_str).map(crate::style_inventory::tag::tag) == Some(name) {
        return Some(tree);
    }
    tree.get("children")?.as_array()?.iter().find_map(|child| first(child, name))
}

#[cfg(test)]
mod tests {
    use super::first;
    use serde_json::json;

    #[test]
    fn finds_root_or_first_descendant() {
        let tree = json!({"tag":"root", "children":[{"tag":"{ns}case", "children":[]}, {"tag":"case"}]});
        assert_eq!(first(&tree, "root"), Some(&tree));
        assert_eq!(first(&tree, "case").unwrap()["tag"], "{ns}case");
        assert_eq!(first(&tree, "missing"), None);
    }
}
