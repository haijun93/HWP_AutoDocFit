//! Read-only graph presentation. No filesystem, mutation, or implicit ID creation.
use crate::{Document, ExtraLine, OutlinerError};
use std::collections::BTreeMap;

pub const MAX_EMBED_DEPTH: usize = 8;
pub const MAX_DISPLAY_LINES: usize = 64;
pub const MAX_DISPLAY_CHARS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefTarget {
    Page(String),
    Block(String),
    Tag(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefToken {
    pub start: usize,
    pub end: usize,
    pub target: RefTarget,
    pub embed: bool,
}

/// Byte ranges, never UTF-16 offsets. Backticks shield code, including fenced code.
/// A lone backtick (no closing run) is ordinary text; an unclosed fence shields the rest.
pub fn reference_tokens(text: &str) -> Vec<RefToken> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut code = 0;
    while at < text.len() {
        let rest = &text[at..];
        if rest.starts_with('`') {
            let n = rest.bytes().take_while(|b| *b == b'`').count();
            if code == 0 {
                if n >= 3 || rest[n..].split(|c| c != '`').any(|run| run.len() == n) {
                    code = n;
                }
            } else if n == code {
                code = 0;
            }
            at += n;
            continue;
        }
        if code != 0 {
            at += rest.chars().next().unwrap().len_utf8();
            continue;
        }
        let macro_body = rest
            .strip_prefix("{{embed ")
            .and_then(|r| r.find("}}").map(|e| (&r[..e], e + 10)))
            .filter(|(body, _)| !body.contains('\n'));
        let (body, macro_len) = macro_body.map_or((rest, None), |(b, n)| (b.trim(), Some(n)));
        let patterns = [
            ("#[[", "]]", 2),
            ("[[", "]]", 0),
            ("(((", ")))", 3),
            ("((", "))", 1),
        ];
        let mut found = None;
        for (open, close, kind) in patterns {
            if let Some(inner) = body.strip_prefix(open) {
                if let Some(end) = inner.find(close) {
                    let name = inner[..end].trim();
                    let consumed = open.len() + end + close.len();
                    // Page and tag names may contain parentheses; block IDs may not.
                    if !name.is_empty()
                        && !name.contains(['\n', '\r', '[', ']'])
                        && (matches!(kind, 0 | 2) || !name.contains(['(', ')']))
                        && (macro_len.is_none() || consumed == body.len())
                    {
                        let target = match kind {
                            0 => RefTarget::Page(name.into()),
                            2 => RefTarget::Tag(name.into()),
                            _ => RefTarget::Block(name.into()),
                        };
                        found = Some((
                            target,
                            macro_len.unwrap_or(consumed),
                            macro_len.is_some() || kind == 3,
                        ));
                        break;
                    }
                }
            }
        }
        if found.is_none()
            && macro_len.is_none()
            && rest.starts_with('#')
            && (at == 0 || text[..at].chars().last().is_some_and(char::is_whitespace))
        {
            let name: String = rest[1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
                .collect();
            if !name.is_empty() {
                let n = name.len() + 1;
                found = Some((RefTarget::Tag(name), n, false));
            }
        }
        if let Some((target, n, embed)) = found {
            out.push(RefToken {
                start: at,
                end: at + n,
                target,
                embed,
            });
            at += n;
        } else {
            at += rest.chars().next().unwrap().len_utf8();
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefState {
    Found,
    Missing,
    Ambiguous,
    Cycle,
    Limit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplaySpan {
    pub text: String,
    pub target: Option<RefTarget>,
    pub state: RefState,
}

impl DisplaySpan {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            target: None,
            state: RefState::Found,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved<'a> {
    Found(&'a str, usize),
    Missing,
    Ambiguous,
}

/// Immutable graph snapshot with a live current document overlay. Duplicate permanent IDs fail closed.
pub struct References<'a> {
    current_page: &'a str,
    docs: BTreeMap<&'a str, &'a Document>,
    ids: BTreeMap<&'a str, Vec<(&'a str, usize)>>,
}

impl<'a> References<'a> {
    pub fn new(
        graph: &'a BTreeMap<String, Document>,
        page: &'a str,
        current: &'a Document,
    ) -> Self {
        let mut docs: BTreeMap<&str, &Document> =
            graph.iter().map(|(p, d)| (p.as_str(), d)).collect();
        docs.insert(page, current);
        let mut ids: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
        for (&p, d) in &docs {
            for (i, b) in d.blocks.iter().enumerate() {
                if let Some(id) = b.properties.get("id").filter(|id| !id.trim().is_empty()) {
                    ids.entry(id.trim()).or_default().push((p, i));
                }
            }
        }
        Self {
            current_page: page,
            docs,
            ids,
        }
    }

    pub fn resolve(&self, page: &str, target: &RefTarget) -> Resolved<'a> {
        match target {
            RefTarget::Page(name) | RefTarget::Tag(name) => {
                let name = crate::normalize_journal_name(name).unwrap_or_else(|_| name.clone());
                let matches: Vec<_> = self
                    .docs
                    .keys()
                    .copied()
                    .filter(|p| {
                        crate::normalize_journal_name(p)
                            .unwrap_or_else(|_| p.to_string())
                            .eq_ignore_ascii_case(&name)
                    })
                    .collect();
                match matches.as_slice() {
                    [p] => Resolved::Found(p, 0),
                    [] => Resolved::Missing,
                    _ => Resolved::Ambiguous,
                }
            }
            RefTarget::Block(id) => {
                if let Some(items) = self.ids.get(id.trim()) {
                    return match items.as_slice() {
                        [(p, i)] => Resolved::Found(p, *i),
                        _ => Resolved::Ambiguous,
                    };
                }
                // Numeric session IDs must never resolve into another file by accident.
                if page != self.current_page {
                    return Resolved::Missing;
                }
                self.docs
                    .get_key_value(page)
                    .and_then(|(&p, d)| {
                        d.blocks
                            .iter()
                            .position(|b| b.id.to_string() == id.trim())
                            .map(|i| Resolved::Found(p, i))
                    })
                    .unwrap_or(Resolved::Missing)
            }
        }
    }

    pub fn render(&self, page: &str, block: usize) -> Vec<Vec<DisplaySpan>> {
        let mut budget = Budget {
            chars: MAX_DISPLAY_CHARS,
            lines: MAX_DISPLAY_LINES,
        };
        let mut lines = Vec::new();
        self.render_block(page, block, &mut vec![], &mut budget, &mut lines, 0);
        lines
    }

    fn render_block(
        &self,
        page: &str,
        block: usize,
        stack: &mut Vec<(String, usize)>,
        budget: &mut Budget,
        out: &mut Vec<Vec<DisplaySpan>>,
        indent: usize,
    ) {
        let Some(b) = self.docs.get(page).and_then(|d| d.blocks.get(block)) else {
            return;
        };
        if budget.lines == 0 || budget.chars == 0 {
            return;
        }
        if stack.len() >= MAX_EMBED_DEPTH || stack.iter().any(|(p, i)| p == page && *i == block) {
            let state = if stack.len() >= MAX_EMBED_DEPTH {
                RefState::Limit
            } else {
                RefState::Cycle
            };
            push_line(
                out,
                vec![DisplaySpan {
                    text: if state == RefState::Cycle {
                        "↳ 순환 참조"
                    } else {
                        "↳ 확장 한도"
                    }
                    .into(),
                    target: None,
                    state,
                }],
                budget,
            );
            return;
        }
        stack.push((page.into(), block));
        // Same marker hiding as the plain row; tokenize the whole block so code fences stay literal.
        let body = b.text.replace("collapsed:: true", "").replace("collapsed::true", "");
        let tokens = reference_tokens(&body);
        let mut offset = 0;
        for part in body.split('\n') {
            if budget.lines == 0 || budget.chars == 0 {
                break;
            }
            let start = offset;
            offset += part.len() + 1;
            let mut line = vec![DisplaySpan::plain("  ".repeat(indent))];
            let mut at = 0;
            let mut embeds = Vec::new();
            for token in tokens.iter().filter(|t| t.start >= start && t.end <= start + part.len()) {
                let (from, to) = (token.start - start, token.end - start);
                line.push(DisplaySpan::plain(&part[at..from]));
                at = to;
                // Ordinary links and tags are shown exactly as written, existing target or not.
                if !token.embed && !matches!(token.target, RefTarget::Block(_)) {
                    line.push(DisplaySpan {
                        text: part[from..to].to_owned(),
                        target: Some(token.target.clone()),
                        state: RefState::Found,
                    });
                    continue;
                }
                let resolved = self.resolve(page, &token.target);
                let (text, state) = match resolved {
                    Resolved::Found(p, i) => match &token.target {
                        RefTarget::Block(_) => {
                            let text = self.inline(p, i, stack, 0);
                            (
                                format!("{}{}", if token.embed { "▣ " } else { "↗ " }, text),
                                RefState::Found,
                            )
                        }
                        RefTarget::Page(name) => (
                            format!("{}[[{name}]]", if token.embed { "▣ " } else { "" }),
                            RefState::Found,
                        ),
                        RefTarget::Tag(name) => (format!("#{name}"), RefState::Found),
                    },
                    Resolved::Missing => (
                        format!("{} · 없음", &part[from..to]),
                        RefState::Missing,
                    ),
                    Resolved::Ambiguous => (
                        format!("{} · 중복 ID/이름", &part[from..to]),
                        RefState::Ambiguous,
                    ),
                };
                line.push(DisplaySpan {
                    text,
                    target: Some(token.target.clone()),
                    state,
                });
                if token.embed {
                    embeds.push(token.target.clone());
                }
            }
            line.push(DisplaySpan::plain(&part[at..]));
            push_line(out, line, budget);
            for target in embeds {
                if let Resolved::Found(p, i) = self.resolve(page, &target) {
                    let doc = self.docs[p];
                    let end = if matches!(target, RefTarget::Page(_)) {
                        doc.blocks.len()
                    } else {
                        (i + 1..doc.blocks.len())
                            .find(|&n| doc.blocks[n].indent <= doc.blocks[i].indent)
                            .unwrap_or(doc.blocks.len())
                    };
                    let mut levels = Vec::new();
                    for n in i..end {
                        if budget.lines <= 1 || budget.chars == 0 {
                            push_line(
                                out,
                                vec![DisplaySpan {
                                    text: "↳ 확장 한도".into(),
                                    target: None,
                                    state: RefState::Limit,
                                }],
                                budget,
                            );
                            break;
                        }
                        while levels
                            .last()
                            .is_some_and(|level| *level >= doc.blocks[n].indent)
                        {
                            levels.pop();
                        }
                        let depth = levels.len();
                        levels.push(doc.blocks[n].indent);
                        self.render_block(p, n, stack, budget, out, indent + 1 + depth);
                    }
                }
            }
        }
        stack.pop();
    }

    fn inline(&self, page: &str, i: usize, stack: &[(String, usize)], depth: usize) -> String {
        if stack.iter().any(|(p, n)| p == page && *n == i) {
            return "순환 참조".into();
        }
        if depth >= MAX_EMBED_DEPTH {
            return "확장 한도".into();
        }
        let Some(b) = self.docs.get(page).and_then(|d| d.blocks.get(i)) else {
            return String::new();
        };
        let text = b.text.lines().next().unwrap_or("");
        let mut next = stack.to_vec();
        next.push((page.into(), i));
        let mut result = String::new();
        let mut at = 0;
        for t in reference_tokens(text) {
            result.push_str(&text[at..t.start]);
            at = t.end;
            if let RefTarget::Block(_) = &t.target {
                let value = match self.resolve(page, &t.target) {
                    Resolved::Found(p, n) => self.inline(p, n, &next, depth + 1),
                    Resolved::Missing => "참조 없음".into(),
                    Resolved::Ambiguous => "중복 ID".into(),
                };
                result.extend(value.chars().take(160));
            } else {
                result.push_str(&text[t.start..t.end]);
            }
            if result.chars().count() >= 160 {
                return result.chars().take(160).collect();
            }
        }
        result.push_str(&text[at..]);
        result.chars().take(160).collect()
    }
}

struct Budget {
    chars: usize,
    lines: usize,
}
fn push_line(out: &mut Vec<Vec<DisplaySpan>>, mut line: Vec<DisplaySpan>, budget: &mut Budget) {
    if budget.lines == 0 || budget.chars == 0 {
        return;
    }
    for span in &mut line {
        if budget.chars == 0 {
            span.text.clear();
            continue;
        }
        let count = span.text.chars().count();
        if count > budget.chars {
            span.text = span
                .text
                .chars()
                .take(budget.chars.saturating_sub(1))
                .chain(std::iter::once('…'))
                .collect();
            span.state = RefState::Limit;
        }
        budget.chars = budget.chars.saturating_sub(count);
    }
    line.retain(|s| !s.text.is_empty());
    out.push(line);
    budget.lines -= 1;
}

/// Explicit property edit. Continuation storage preserves spaces, CRLF and unrelated metadata.
pub fn set_property(
    doc: &mut Document,
    block_id: u64,
    key: &str,
    value: &str,
) -> Result<(), OutlinerError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        || value.contains(['\n', '\r', '\0'])
    {
        return Err(OutlinerError::Invalid(
            "속성 이름과 한 줄 값을 입력해 주세요".into(),
        ));
    }
    let b = doc
        .blocks
        .iter_mut()
        .find(|b| b.id == block_id)
        .ok_or_else(|| OutlinerError::Invalid("블록이 없습니다".into()))?;
    let before = b.clone();
    let same = |item: &ExtraLine| item.line.split_once("::").is_some_and(|(k, _)| k == key);
    let line = format!("{key}:: {}", value.trim());
    if let Some(first) = b.extra.iter().position(same) {
        // Replace in place and drop later duplicates, which would otherwise win.
        b.extra[first].line = line;
        let mut index = 0;
        b.extra.retain(|item| {
            index += 1;
            index <= first + 1 || !same(item)
        });
    } else if b.properties.contains_key(key) {
        return Err(OutlinerError::Invalid(
            "본문 줄에 적힌 속성은 본문 편집으로 고쳐 주세요".into(),
        ));
    } else {
        b.extra.push(ExtraLine {
            after: b.text.split('\n').count() - 1,
            line,
        });
    }
    crate::parse_serialize::refresh_metadata(b);
    doc.edited |= *b != before;
    Ok(())
}
