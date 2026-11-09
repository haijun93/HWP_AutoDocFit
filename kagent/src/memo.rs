//! 메모장 저장 감지와 메모 갱신 규칙(정책 없는 순수 부분).

/// 비어 있는 메모는 기록하지 않고, 내용이 있으면 기존 메모 항목 ID를 갱신 대상으로 반환한다.
pub fn should_save(text: &str) -> bool { !text.trim().is_empty() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_memo_adds_or_updates_same_item_and_ignores_empty() {
        let mut memo: Option<(u64, String)> = None;
        let mut apply = |text: &str| -> Option<u64> {
            if !should_save(text) { return None; }
            if let Some((id, old)) = memo.as_mut() {
                *old = text.to_owned();
                Some(*id)
            } else {
                memo = Some((1, text.to_owned()));
                Some(1)
            }
        };
        assert_eq!(apply("첫 메모"), Some(1));
        assert_eq!(apply("   \r\n"), None);
        assert_eq!(apply("수정한 메모"), Some(1));
        assert_eq!(memo, Some((1, "수정한 메모".into())));
    }
}
