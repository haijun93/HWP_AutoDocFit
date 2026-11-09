/// Return the local part of an ElementTree tag, matching style_inventory._tag.
pub fn tag(tag: &str) -> &str {
    tag.rsplit('}').next().unwrap_or(tag)
}

#[cfg(test)]
mod tests {
    use super::tag;

    #[test]
    fn strips_namespace_uri() {
        assert_eq!(tag("{urn:hancom}p"), "p");
        assert_eq!(tag("plain"), "plain");
    }
}
