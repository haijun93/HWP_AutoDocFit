//! `[[페이지]]` 링크 순수 함수(Win32 없음): 입력 중 자동완성 문맥·후보·치환, 글 속 링크 구간(그리기·클릭).
//! 위치는 모두 UTF-16 오프셋(Win32 EDIT 의 선택 범위와 같은 단위).

/// 링크 또는 참조 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    Page,
    Block,
    TripleBlock,
}

/// 캐럿이 `[[` 또는 `((`, `(((` 뒤에 있을 때의 문맥.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkCtx {
    /// 여는 괄호 첫 글자의 UTF-16 위치.
    pub start16: usize,
    /// 캐럿 위치(= 입력한 질의의 끝).
    pub caret16: usize,
    /// 치환이 덮을 끝(캐럿 바로 뒤에 닫는 괄호가 이미 있으면 그것까지).
    pub end16: usize,
    /// 여는 괄호와 캐럿 사이 글자.
    pub query: String,
    /// 참조 종류.
    pub kind: RefKind,
}

pub const MAX_QUERY_CHARS: usize = 60;

fn byte_at(s: &str, off16: usize) -> usize {
    let mut n = 0;
    for (b, c) in s.char_indices() {
        if n >= off16 || n + c.len_utf16() > off16 {
            return b;
        }
        n += c.len_utf16();
    }
    s.len()
}

fn utf16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// 캐럿 앞쪽에서 가장 가까운 `[[` 가 아직 닫히지 않았으면 문맥을 돌려준다.
pub fn link_context(text: &str, caret16: usize) -> Option<LinkCtx> {
    let cb = byte_at(text, caret16);
    let before = &text[..cb];
    let open = before.rfind("[[")?;
    let query = &before[open + 2..];
    if query.contains(']') || query.contains('[') || query.contains('\n') || query.chars().count() > MAX_QUERY_CHARS {
        return None;
    }
    let after = &text[cb..];
    let end_b = if after.starts_with("]]") { cb + 2 } else { cb };
    Some(LinkCtx {
        start16: utf16(&before[..open]),
        caret16: utf16(before),
        end16: utf16(&text[..end_b]),
        query: query.to_string(),
        kind: RefKind::Page,
    })
}

/// 캐럿 앞쪽에서 가장 가까운 `(((` 또는 `((` 가 아직 닫히지 않았으면 문맥을 돌려준다.
pub fn block_ref_context(text: &str, caret16: usize) -> Option<LinkCtx> {
    let cb = byte_at(text, caret16);
    let before = &text[..cb];
    let closing_end = |closing: &str| {
        let after = &text[cb..];
        let trimmed = after.trim_start_matches([' ', '\t']);
        if trimmed.starts_with(closing) {
            Some(cb + (after.len() - trimmed.len()) + closing.len())
        } else {
            None
        }
    };
    if let Some(open) = before.rfind("(((") {
        let query = &before[open + 3..];
        if !query.contains(')') && !query.contains('(') && !query.contains('\n') && query.chars().count() <= MAX_QUERY_CHARS {
            let end_b = closing_end(")))").unwrap_or(cb);
            return Some(LinkCtx {
                start16: utf16(&before[..open]),
                caret16: utf16(before),
                end16: utf16(&text[..end_b]),
                query: query.to_string(),
                kind: RefKind::TripleBlock,
            });
        }
    }
    if let Some(open) = before.rfind("((") {
        let query = &before[open + 2..];
        if !query.contains(')') && !query.contains('(') && !query.contains('\n') && query.chars().count() <= MAX_QUERY_CHARS {
            let end_b = closing_end("))").unwrap_or(cb);
            return Some(LinkCtx {
                start16: utf16(&before[..open]),
                caret16: utf16(before),
                end16: utf16(&text[..end_b]),
                query: query.to_string(),
                kind: RefKind::Block,
            });
        }
    }
    None
}

/// 캐럿 위치의 자동완성 문맥(페이지 링크 또는 블록 참조).
pub fn any_completion_context(text: &str, caret16: usize) -> Option<LinkCtx> {
    // 더 뒤에 있는 문맥을 우선한다.
    let link = link_context(text, caret16);
    let block = block_ref_context(text, caret16);
    match (link, block) {
        (Some(l), Some(b)) => if l.start16 >= b.start16 { Some(l) } else { Some(b) },
        (Some(l), None) => Some(l),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// 질의로 후보 이름을 거른다: 앞부분 일치 → 포함 일치 순(대소문자 무시). 질의가 비면 `recent` 먼저.
pub fn candidates(pages: &[String], recent: &[String], query: &str, limit: usize) -> Vec<String> {
    let q = query.trim().to_lowercase();
    let mut out: Vec<String> = Vec::new();
    let push = |s: &String, out: &mut Vec<String>| {
        if !out.contains(s) {
            out.push(s.clone());
        }
    };
    if q.is_empty() {
        for s in recent.iter().filter(|s| pages.contains(s)) {
            push(s, &mut out);
        }
        for s in pages {
            push(s, &mut out);
        }
    } else {
        for s in pages.iter().filter(|s| s.to_lowercase().starts_with(&q)) {
            push(s, &mut out);
        }
        for s in pages.iter().filter(|s| s.to_lowercase().contains(&q)) {
            push(s, &mut out);
        }
    }
    out.truncate(limit);
    out
}

/// 후보를 골랐을 때의 새 글과 캐럿(UTF-16): `[[질의` → `[[이름]]`, `((질의` → `((id))`.
pub fn apply_completion(text: &str, ctx: &LinkCtx, name: &str) -> (String, usize) {
    let (a, b) = (byte_at(text, ctx.start16), byte_at(text, ctx.end16));
    let (open, close) = match ctx.kind {
        RefKind::Page => ("[[", "]]"),
        RefKind::Block => ("((", "))"),
        RefKind::TripleBlock => ("(((", ")))"),
    };
    let new = format!("{}{}{}{}{}", &text[..a], open, name, close, &text[b..]);
    let caret = ctx.start16 + open.len() + utf16(name) + close.len();
    (new, caret)
}

/// 글을 일반 조각과 링크 조각으로 나눈다. 링크 조각의 `text` 는 대괄호를 포함한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seg {
    pub text: String,
    /// 링크면 페이지 이름.
    pub link: Option<String>,
}

pub fn segments(text: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut plain = String::new();
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("[[") {
            if let Some(end) = stripped.find("]]") {
                let inner = &stripped[..end];
                if !inner.trim().is_empty() && !inner.contains('[') && !inner.contains(']') {
                    if !plain.is_empty() {
                        out.push(Seg { text: std::mem::take(&mut plain), link: None });
                    }
                    out.push(Seg { text: format!("[[{inner}]]"), link: Some(inner.trim().to_string()) });
                    rest = &stripped[end + 2..];
                    continue;
                }
            }
        }
        let c = rest.chars().next().unwrap();
        plain.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if !plain.is_empty() {
        out.push(Seg { text: plain, link: None });
    }
    out
}

/// UTF-16 위치 `pos16` 가 어느 링크 위에 있는가(Ctrl+클릭용).
pub fn link_at(text: &str, pos16: usize) -> Option<String> {
    let mut at = 0;
    for seg in segments(text) {
        let len = utf16(&seg.text);
        if let Some(name) = &seg.link {
            if pos16 >= at && pos16 < at + len {
                return Some(name.clone());
            }
        }
        at += len;
    }
    None
}
