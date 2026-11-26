//! Rust 창에서 실제 지원하는 명령 목록. 표시 문자열과 실행 값을 한곳에 둔다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { Save, Undo, Redo, Left, Right, Today, New, Rename, Todo, Collapse, Expand }

pub const PALETTE: &[(Action, &str)] = &[
    (Action::Save, "저장 · Save · Ctrl+S"),
    (Action::Undo, "실행 취소 · Undo · Ctrl+Z"),
    (Action::Redo, "다시 실행 · Redo · Ctrl+Y"),
    (Action::Left, "왼쪽 패널 전환 · Toggle left"),
    (Action::Right, "오른쪽 패널 전환 · Toggle right"),
    (Action::Today, "오늘 일지 열기 · Today"),
    (Action::New, "새 페이지 · New page · Ctrl+N"),
    (Action::Rename, "페이지 이름 변경 · Rename"),
    (Action::Todo, "TODO 상태 순환 · Ctrl+Enter"),
    (Action::Collapse, "블록 접기 · Collapse · Ctrl+↑"),
    (Action::Expand, "블록 펼치기 · Expand · Ctrl+↓"),
];

pub fn palette(query: &str) -> Vec<(Action, String)> {
    let q = query.trim().to_lowercase();
    PALETTE.iter().filter(|(_, s)| s.to_lowercase().contains(&q)).map(|(a, s)| (*a, (*s).into())).collect()
}

pub const SLASH: &[(&str, &str)] = &[
    ("TODO", "TODO · 할 일"), ("DONE", "DONE · 완료"),
    ("h1", "h1 · 제목 1"), ("h2", "h2 · 제목 2"), ("h3", "h3 · 제목 3"),
    ("today", "today · 오늘 날짜"), ("page", "page · 페이지 참조"), ("block", "block · 블록 참조"),
];

pub fn slash(query: &str) -> Vec<(String, String)> {
    let q = query.to_lowercase();
    SLASH.iter().filter(|(_, label)| label.to_lowercase().contains(&q))
        .map(|(key, label)| ((*key).into(), (*label).into())).collect()
}

/// UTF-16 문맥을 치환한다. 제목·할 일은 블록 맨 앞, 참조 명령은 괄호 안에 캐럿을 둔다.
pub fn insert_slash(text: &str, ctx: &crate::links::LinkCtx, key: &str, today: &str) -> (String, usize) {
    let value = match key { "today" => today, "page" => "[[]]", "block" => "(())", _ => "" };
    let (mut new, mut caret) = crate::links::apply_completion(text, ctx, value);
    if matches!(key, "TODO" | "DONE" | "h1" | "h2" | "h3") {
        let prefix = match key { "h1" => "# ", "h2" => "## ", "h3" => "### ", "TODO" => "TODO ", _ => "DONE " };
        // 기존 접두는 중복하지 않는다. 상태 표지는 core가 확정 시 분리한다.
        let rest = ["TODO ", "DONE ", "### ", "## ", "# "].iter()
            .find_map(|p| new.strip_prefix(p)).unwrap_or(&new).to_string();
        new = format!("{prefix}{rest}");
        caret = new.encode_utf16().count();
    } else if matches!(key, "page" | "block") { caret -= 2; }
    (new, caret)
}
