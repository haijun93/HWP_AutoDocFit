//! 블록 편집 키 규칙(OUTLINEKEY1). Flutter 탭과 Win32 창이 같은 함수를 쓴다.
//!
//! 순수 함수: (문서, 편집 중 블록·커서·편집 칸 글, 선택, 접힘) → 새 문서와 새 커서·선택.
//! 규칙의 근거와 Logseq 와의 차이는 `docs/OUTLINEKEY1_DESIGN_2026-11-26.md`.
//! 블록은 "문서 순서 번호"(0부터)로, 커서는 UTF-16 오프셋으로 주고받는다.

use crate::parse_serialize::{block_from_text, is_property_line, refresh_metadata};
use crate::{parse, serialize, Block, Document, LineNode, OutlinerError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct KeyInput {
    /// enter·newline·backspace·delete·indent·outdent·move_up·move_down·up·down·left·right·cycle_todo·
    /// collapse·expand·toggle_collapse·commit·delete_selection·copy·cut·paste
    pub action: String,
    /// 편집 중(또는 대상) 블록.
    pub block: Option<usize>,
    pub start: usize,
    pub end: usize,
    /// 편집 칸의 현재 글(아직 문서에 반영 전일 수 있다).
    pub text: Option<String>,
    pub selection: Vec<usize>,
    pub folded: Vec<usize>,
    pub paste: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Cursor {
    pub block: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct KeyOutcome {
    /// 결과 마크다운을 다시 파싱한 정규 문서(접힘 상태 포함).
    pub document: Document,
    pub markdown: String,
    /// false 면 아무것도 바꾸지 않았다 — 화면의 기본 글 편집에 맡긴다.
    pub handled: bool,
    /// 마크다운이 바뀌었다(= 실행 취소 한 단계).
    pub changed: bool,
    pub cursor: Option<Cursor>,
    pub selection: Vec<usize>,
    pub folded: Vec<usize>,
    pub note: Option<String>,
    pub clipboard: Option<String>,
}

fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// UTF-16 오프셋 → 바이트 위치(서로게이트 쌍 한가운데면 앞 경계).
fn byte_at(s: &str, offset: usize) -> usize {
    let mut n = 0;
    for (b, c) in s.char_indices() {
        if n + c.len_utf16() > offset {
            return b;
        }
        n += c.len_utf16();
    }
    s.len()
}

fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', " ")
}

/// 병합으로 사라지는 블록의 속성 줄을 남는 블록으로 옮긴다(참조되는 `id::` 를 잃지 않게).
pub(crate) fn carry_extra(into: &mut Block, from: &Block) {
    let lines = into.text.split('\n').count().saturating_sub(1);
    for item in &from.extra {
        if !into.extra.iter().any(|e| e.line == item.line) {
            let mut item = item.clone();
            item.after = lines;
            into.extra.push(item);
        }
    }
    refresh_metadata(into);
}

// ---------------------------------------------------------------- 트리 도우미

fn end_of(blocks: &[Block], i: usize) -> usize {
    let mut e = i + 1;
    while e < blocks.len() && blocks[e].indent > blocks[i].indent {
        e += 1;
    }
    e
}

fn parent_of(blocks: &[Block], i: usize) -> Option<usize> {
    (0..i).rev().find(|&j| blocks[j].indent < blocks[i].indent)
}

fn has_children(blocks: &[Block], i: usize) -> bool {
    end_of(blocks, i) > i + 1
}

fn prev_sibling(blocks: &[Block], i: usize) -> Option<usize> {
    let parent = parent_of(blocks, i);
    let first = parent.map_or(0, |p| p + 1);
    (first..i)
        .rev()
        .find(|&j| parent_of(blocks, j) == parent)
}

fn next_sibling(blocks: &[Block], i: usize) -> Option<usize> {
    let k = end_of(blocks, i);
    (k < blocks.len() && parent_of(blocks, k) == parent_of(blocks, i)).then_some(k)
}

/// 접힘을 반영해 화면에 보이는 블록 번호.
fn visible(blocks: &[Block]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < blocks.len() {
        out.push(i);
        i = if blocks[i].folded {
            end_of(blocks, i)
        } else {
            i + 1
        };
    }
    out
}

fn prev_visible(blocks: &[Block], i: usize) -> Option<usize> {
    visible(blocks).into_iter().take_while(|&v| v < i).last()
}

fn next_visible(blocks: &[Block], i: usize) -> Option<usize> {
    visible(blocks).into_iter().find(|&v| v > i)
}

/// 하위를 새로 받을 때의 들여쓰기 폭(이미 자식이 있으면 그 폭).
fn child_indent(blocks: &[Block], parent: usize) -> usize {
    if has_children(blocks, parent) {
        blocks[parent + 1].indent
    } else {
        blocks[parent].indent + 1
    }
}

/// 뿌리들과 그 하위를 `indent` 폭으로 옮긴다. 하위 범위는 폭을 바꾸기 전에 모두 구한다.
fn shift_run(doc: &mut Document, run: &[u64], indent: usize) {
    let ranges: Vec<(usize, usize)> = run
        .iter()
        .filter_map(|id| index_of(doc, *id))
        .map(|i| (i, end_of(&doc.blocks, i)))
        .collect();
    for (i, end) in ranges {
        let old = doc.blocks[i].indent;
        for block in &mut doc.blocks[i..end] {
            block.indent = block.indent - old + indent;
        }
    }
    doc.edited = true;
}

// ---------------------------------------------------------------- 줄 순서 조작

fn seq_of(doc: &Document, i: usize) -> usize {
    let id = doc.blocks[i].id;
    doc.sequence
        .iter()
        .position(|n| *n == LineNode::Block(id))
        .expect("block in sequence")
}

fn index_of(doc: &Document, id: u64) -> Option<usize> {
    doc.blocks.iter().position(|b| b.id == id)
}

/// 블록 `a`..`b` 사이(둘 다 제외)에 빈 줄이 아닌 보존 줄이 있는가.
fn raw_between(doc: &Document, a: usize, b: usize) -> bool {
    doc.sequence[seq_of(doc, a) + 1..seq_of(doc, b)]
        .iter()
        .any(|n| matches!(n, LineNode::Raw(raw) if !raw.trim().is_empty()))
}

/// 순서를 바꾼 뒤 줄끝과 블록 배열 순서를 맞춘다.
fn settle(doc: &mut Document) {
    let count = doc.sequence.len();
    for (i, ending) in doc.line_endings.iter_mut().enumerate() {
        if i + 1 == count && !doc.trailing_newline {
            ending.clear();
        } else if ending.is_empty() {
            *ending = doc.newline.clone();
        }
    }
    let order: Vec<u64> = doc
        .sequence
        .iter()
        .filter_map(|n| match n {
            LineNode::Block(id) => Some(*id),
            LineNode::Raw(_) => None,
        })
        .collect();
    doc.blocks
        .sort_by_key(|b| order.iter().position(|id| *id == b.id));
    doc.edited = true;
}

fn new_block(doc: &Document, text: &str, indent: usize) -> Block {
    let id = doc.blocks.iter().map(|b| b.id).max().unwrap_or(0) + 1;
    block_from_text(text, id, indent)
}

/// 순서 위치 `at` 에 블록을 넣는다.
fn insert_at(doc: &mut Document, at: usize, block: Block) {
    doc.sequence.insert(at, LineNode::Block(block.id));
    let ending = doc.newline.clone();
    doc.line_endings.insert(at, ending);
    doc.blocks.push(block);
    settle(doc);
}

fn remove_block(doc: &mut Document, i: usize) -> Block {
    let at = seq_of(doc, i);
    doc.sequence.remove(at);
    doc.line_endings.remove(at);
    let block = doc.blocks.remove(i);
    settle(doc);
    block
}

/// 블록 `from` 부터 블록 `last` 까지(보존 줄 포함)의 순서 구간.
fn span(doc: &Document, from: usize, last: usize) -> std::ops::Range<usize> {
    seq_of(doc, from)..seq_of(doc, last) + 1
}

/// 순서 구간 `range` 를 떼어 `to`(떼기 전 기준, 구간 밖) 앞에 넣는다.
fn move_span(doc: &mut Document, range: std::ops::Range<usize>, to: usize) {
    let nodes: Vec<LineNode> = doc.sequence.drain(range.clone()).collect();
    let endings: Vec<String> = doc.line_endings.drain(range.clone()).collect();
    let to = if to >= range.end {
        to - nodes.len()
    } else {
        to
    };
    doc.sequence.splice(to..to, nodes);
    doc.line_endings.splice(to..to, endings);
}

// ---------------------------------------------------------------- 선택·묶음

/// 선택에서 다른 선택 블록의 하위인 것을 뺀 뿌리 블록(id).
fn roots(blocks: &[Block], selection: &[usize]) -> Vec<u64> {
    let mut sorted: Vec<usize> = selection
        .iter()
        .copied()
        .filter(|&i| i < blocks.len())
        .collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut covered = 0;
    let mut out = Vec::new();
    for i in sorted {
        if i < covered {
            continue;
        }
        covered = end_of(blocks, i);
        out.push(blocks[i].id);
    }
    out
}

/// 뿌리들을 "같은 부모의 붙어 있는 형제" 묶음으로 나눈다.
fn runs(blocks: &[Block], roots: &[u64]) -> Vec<Vec<u64>> {
    let mut out: Vec<Vec<u64>> = Vec::new();
    for id in roots {
        let Some(i) = blocks.iter().position(|b| b.id == *id) else {
            continue;
        };
        let joins = out.last().is_some_and(|run| {
            let last = blocks
                .iter()
                .position(|b| Some(&b.id) == run.last())
                .unwrap_or(0);
            end_of(blocks, last) == i && parent_of(blocks, last) == parent_of(blocks, i)
        });
        match out.last_mut() {
            Some(run) if joins => run.push(*id),
            _ => out.push(vec![*id]),
        }
    }
    out
}

fn indent_run(doc: &mut Document, run: &[u64]) -> bool {
    let Some(first) = index_of(doc, run[0]) else {
        return false;
    };
    let Some(previous) = prev_sibling(&doc.blocks, first) else {
        return false;
    };
    let indent = child_indent(&doc.blocks, previous);
    doc.blocks[previous].folded = false;
    shift_run(doc, run, indent);
    true
}

/// 논리적 내어쓰기: 부모의 하위 전체 뒤 형제로 옮긴다. 뒤에 남은 형제는 부모의 자식으로 남는다.
fn outdent_run(doc: &mut Document, run: &[u64]) -> bool {
    let (Some(first), Some(last)) = (
        index_of(doc, run[0]),
        index_of(doc, run[run.len() - 1]),
    ) else {
        return false;
    };
    let Some(parent) = parent_of(&doc.blocks, first) else {
        return false;
    };
    let indent = doc.blocks[parent].indent;
    let run_end = end_of(&doc.blocks, last);
    let parent_end = end_of(&doc.blocks, parent);
    let range = span(doc, first, run_end - 1);
    let destination = seq_of(doc, parent_end - 1) + 1;
    shift_run(doc, run, indent);
    if destination > range.end {
        move_span(doc, range, destination);
    }
    settle(doc);
    true
}

fn move_run(doc: &mut Document, run: &[u64], up: bool) -> bool {
    let (Some(first), Some(last)) = (
        index_of(doc, run[0]),
        index_of(doc, run[run.len() - 1]),
    ) else {
        return false;
    };
    let run_end = end_of(&doc.blocks, last);
    let range = span(doc, first, run_end - 1);
    let sibling = if up {
        prev_sibling(&doc.blocks, first)
    } else {
        next_sibling(&doc.blocks, last)
    };
    if let Some(other) = sibling {
        // 형제와 자리바꿈(사이의 보존 줄은 제자리).
        let other_range = span(doc, other, end_of(&doc.blocks, other) - 1);
        if up {
            let gap = range.start - other_range.end;
            let to = other_range.start;
            let len = range.len();
            move_span(doc, range, to);
            let moved_other = other_range.start + len..other_range.end + len;
            let destination = moved_other.end + gap;
            move_span(doc, moved_other, destination);
        } else {
            let gap = other_range.start - range.end;
            let to = range.start;
            let len = other_range.len();
            move_span(doc, other_range, to);
            let moved_run = range.start + len..range.end + len;
            let destination = moved_run.end + gap;
            move_span(doc, moved_run, destination);
        }
        settle(doc);
        return true;
    }
    let Some(parent) = parent_of(&doc.blocks, first) else {
        return false;
    };
    // 부모 경계를 넘는다: 위 = 부모의 앞 형제의 마지막 자식, 아래 = 부모의 뒤 형제의 첫 자식.
    let uncle = if up {
        prev_sibling(&doc.blocks, parent)
    } else {
        next_sibling(&doc.blocks, parent)
    };
    let Some(uncle) = uncle else {
        return false;
    };
    let indent = child_indent(&doc.blocks, uncle);
    let destination = if up {
        seq_of(doc, end_of(&doc.blocks, uncle) - 1) + 1
    } else {
        seq_of(doc, uncle) + 1
    };
    doc.blocks[uncle].folded = false;
    shift_run(doc, run, indent);
    move_span(doc, range, destination);
    settle(doc);
    true
}

fn markdown_of(doc: &Document, roots: &[u64]) -> String {
    let mut lines = Vec::new();
    for id in roots {
        let Some(i) = index_of(doc, *id) else {
            continue;
        };
        let base = doc.blocks[i].indent;
        for block in &doc.blocks[i..end_of(&doc.blocks, i)] {
            let mut one = block.clone();
            one.indent -= base;
            one.extra.clear(); // 복사본에는 id:: 를 넣지 않는다
            let single = Document {
                original: String::new(),
                sequence: vec![LineNode::Block(one.id)],
                blocks: vec![one],
                edited: true,
                line_endings: vec![String::new()],
                newline: "\n".into(),
                trailing_newline: false,
            };
            lines.push(serialize(&single));
        }
    }
    lines.join("\n")
}

// ---------------------------------------------------------------- 붙여넣기

struct Pasted {
    width: usize,
    text: String,
    status: Option<String>,
}

fn is_bullet(line: &str) -> bool {
    let rest = line.trim_start();
    rest.starts_with("- ") || rest == "-"
}

/// 클립보드 글 → (깊이, 글, 상태). `- ` 목록이면 목록 구조를, 아니면 줄마다 한 블록(들여쓰기 = 계층).
fn parse_paste(text: &str) -> Vec<(usize, String, Option<String>)> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let bullets = text.lines().any(is_bullet);
    let mut items: Vec<Pasted> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let rest = line.trim_start_matches([' ', '\t']);
        let width: usize = line[..line.len() - rest.len()]
            .chars()
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum();
        if bullets && !is_bullet(line) {
            if let Some(item) = items.last_mut() {
                if width > item.width {
                    let more = rest.trim_end();
                    if !is_property_line(more) {
                        item.text.push('\n');
                        item.text.push_str(more);
                    }
                    continue;
                }
            }
        }
        let mut body = if bullets && is_bullet(line) {
            rest.strip_prefix("- ").unwrap_or("").to_owned()
        } else {
            rest.trim_end().to_owned()
        };
        let mut status = None;
        for marker in ["TODO", "DONE"] {
            if let Some(after) = body.strip_prefix(marker).and_then(|r| r.strip_prefix(' ')) {
                status = Some(marker.to_owned());
                body = after.to_owned();
                break;
            }
        }
        items.push(Pasted {
            width,
            text: body.replace('\t', " "),
            status,
        });
    }
    let mut stack: Vec<usize> = Vec::new();
    items
        .into_iter()
        .map(|item| {
            while stack.last().is_some_and(|&w| w >= item.width) {
                stack.pop();
            }
            stack.push(item.width);
            (stack.len() - 1, item.text, item.status)
        })
        .collect()
}

// ---------------------------------------------------------------- 키 처리

struct Work {
    doc: Document,
    handled: bool,
    cursor: Option<(u64, usize, usize)>,
    selection: Vec<u64>,
    note: Option<String>,
    clipboard: Option<String>,
}

impl Work {
    fn at(&mut self, i: usize, offset: usize) {
        self.cursor = Some((self.doc.blocks[i].id, offset, offset));
    }

    fn note(&mut self, message: &str) {
        self.note = Some(message.to_owned());
    }

    fn set_block_text(&mut self, i: usize, text: String) {
        if self.doc.blocks[i].text != text {
            self.doc.blocks[i].text = text;
            refresh_metadata(&mut self.doc.blocks[i]);
            self.doc.edited = true;
        }
    }

    fn enter(&mut self, i: usize, start: usize, end: usize) {
        let block = &self.doc.blocks[i];
        let text = block.text.clone();
        let (a, b) = (byte_at(&text, start), byte_at(&text, end));
        // 코드 울타리 안: 새 블록이 아니라 줄바꿈.
        if text[..a].matches("```").count() % 2 == 1 {
            return self.newline(i, start, end);
        }
        // `[[…]]`·`((…))` 안: 나누지 않고 닫는 괄호 뒤로.
        if a == b {
            for (open, close) in [("[[", "]]"), ("((", "))")] {
                let (Some(o), Some(c)) = (text[..a].rfind(open), text[a..].find(close)) else {
                    continue;
                };
                let inside = &text[o + 2..a];
                let rest = &text[a..a + c];
                if !inside.contains(close) && !rest.contains(open) && !inside.contains('\n') {
                    let at = len16(&text[..a + c + 2]);
                    return self.at(i, at);
                }
            }
        }
        let empty = text.is_empty() && block.status.is_none();
        if empty
            && next_sibling(&self.doc.blocks, i).is_none()
            && parent_of(&self.doc.blocks, i).is_some()
        {
            let id = block.id;
            outdent_run(&mut self.doc, &[id]);
            self.cursor = Some((id, 0, 0));
            return;
        }
        let left = text[..a].to_owned();
        let right = text[b..].trim_start().to_owned();
        let indent = block.indent;
        if left.trim().is_empty() && !right.is_empty() {
            // 맨 앞: 위에 빈 블록.
            self.set_block_text(i, right);
            let above = new_block(&self.doc, "", indent);
            let id = above.id;
            let at = seq_of(&self.doc, i);
            insert_at(&mut self.doc, at, above);
            self.cursor = Some((id, 0, 0));
            return;
        }
        let folded = self.doc.blocks[i].folded;
        let children = has_children(&self.doc.blocks, i);
        let (at, indent) = if children && !folded {
            (seq_of(&self.doc, i) + 1, child_indent(&self.doc.blocks, i))
        } else {
            (
                seq_of(&self.doc, end_of(&self.doc.blocks, i) - 1) + 1,
                indent,
            )
        };
        self.set_block_text(i, left);
        let block = new_block(&self.doc, &right, indent);
        let id = block.id;
        insert_at(&mut self.doc, at, block);
        self.cursor = Some((id, 0, 0));
    }

    fn newline(&mut self, i: usize, start: usize, end: usize) {
        let mut text = self.doc.blocks[i].text.clone();
        let (a, b) = (byte_at(&text, start), byte_at(&text, end));
        text.replace_range(a..b, "\n");
        let at = len16(&text[..a]) + 1;
        self.set_block_text(i, text);
        self.at(i, at);
    }

    /// 블록 `from` 의 글·상태·속성을 `into` 뒤에 이어붙이고 지운다. 이음매 오프셋을 돌려준다.
    fn join(&mut self, into: usize, from: usize) -> usize {
        let from_block = self.doc.blocks[from].clone();
        let into_id = self.doc.blocks[into].id;
        let target = &mut self.doc.blocks[into];
        let seam = len16(&target.text);
        if target.text.is_empty() && target.status.is_none() {
            target.status = from_block.status.clone();
        } else if let Some(status) = &from_block.status {
            target.text.push_str(status);
            target.text.push(' ');
        }
        target.text.push_str(&from_block.text);
        carry_extra(target, &from_block);
        remove_block(&mut self.doc, from);
        // 사라진 블록의 하위가 남는 블록의 하위가 되도록 폭을 맞춘다.
        let into = index_of(&self.doc, into_id).expect("kept block");
        let floor = self.doc.blocks[into].indent;
        let mut k = from.max(into + 1);
        if let Some(child) = self.doc.blocks.get(k).map(|b| b.indent) {
            if child > from_block.indent && child <= floor {
                while k < self.doc.blocks.len() && self.doc.blocks[k].indent > from_block.indent {
                    self.doc.blocks[k].indent += floor + 1 - child;
                    k += 1;
                }
            }
        }
        seam
    }

    fn backspace(&mut self, i: usize) {
        let blocks = &self.doc.blocks;
        if blocks.len() <= 1 {
            return;
        }
        let block = &blocks[i];
        let children = has_children(blocks, i);
        if i == 0 {
            // 문서 첫 블록: 비어 있고 하위가 없을 때만 지운다.
            if block.text.is_empty() && block.status.is_none() && !children {
                remove_block(&mut self.doc, 0);
                self.at(0, 0);
            }
            return;
        }
        let left = prev_sibling(blocks, i).or_else(|| parent_of(blocks, i));
        if children && left.is_some_and(|l| has_children(blocks, l)) {
            return self.note("하위 블록이 있어 앞 블록과 합칠 수 없습니다");
        }
        let Some(previous) = prev_visible(blocks, i) else {
            return;
        };
        if raw_between(&self.doc, previous, i) {
            return self.note("사이에 보존 줄(제목 등)이 있어 합칠 수 없습니다");
        }
        let folded = self.doc.blocks[i].folded;
        if children {
            self.doc.blocks[previous].folded = folded;
        }
        let id = self.doc.blocks[previous].id;
        let seam = self.join(previous, i);
        self.cursor = Some((id, seam, seam));
    }

    fn delete(&mut self, i: usize) {
        let blocks = &self.doc.blocks;
        let children = has_children(blocks, i);
        let folded = children && blocks[i].folded;
        let sibling = next_sibling(blocks, i);
        let Some(next) = (if folded || !children {
            sibling
        } else {
            Some(i + 1)
        }) else {
            return;
        };
        if (folded && has_children(blocks, next)) || (!folded && children && has_children(blocks, i + 1))
        {
            return self.note("하위 블록이 있어 다음 블록과 합칠 수 없습니다");
        }
        if raw_between(&self.doc, i, next) {
            return self.note("사이에 보존 줄(제목 등)이 있어 합칠 수 없습니다");
        }
        if !children {
            self.doc.blocks[i].folded = false;
        }
        let id = self.doc.blocks[i].id;
        let seam = self.join(i, next);
        self.cursor = Some((id, seam, seam));
    }

    /// ↑↓: 앞/뒤 보이는 블록으로, 줄 안 위치(열)를 유지한다.
    fn vertical(&mut self, i: usize, caret: usize, up: bool) {
        let text = &self.doc.blocks[i].text;
        let before = &text[..byte_at(text, caret)];
        let column = len16(before.rsplit('\n').next().unwrap_or(""));
        let target = if up {
            prev_visible(&self.doc.blocks, i)
        } else {
            next_visible(&self.doc.blocks, i)
        };
        let Some(t) = target else {
            let edge = if up { 0 } else { len16(text) };
            return self.at(i, edge);
        };
        let other = &self.doc.blocks[t].text;
        let (line_start, line) = if up {
            let start = other.rfind('\n').map_or(0, |p| p + 1);
            (start, &other[start..])
        } else {
            (0, other.split('\n').next().unwrap_or(""))
        };
        let offset = len16(&other[..line_start]) + len16(&line[..byte_at(line, column)]);
        self.at(t, offset);
    }

    fn cycle_todo(&mut self, targets: &[usize], skip_empty: bool) {
        for &i in targets {
            let block = &mut self.doc.blocks[i];
            if skip_empty && block.text.is_empty() && block.status.is_none() {
                continue;
            }
            block.status = match block.status.as_deref() {
                None => Some("TODO".into()),
                Some("TODO") => Some("DONE".into()),
                _ => None,
            };
            self.doc.edited = true;
        }
    }

    fn delete_roots(&mut self, roots: &[u64]) {
        let Some(first) = roots.first().and_then(|id| index_of(&self.doc, *id)) else {
            return;
        };
        let keep = prev_visible(&self.doc.blocks, first).map(|p| self.doc.blocks[p].id);
        let place = seq_of(&self.doc, first);
        for id in roots {
            if let Some(i) = index_of(&self.doc, *id) {
                for k in (i..end_of(&self.doc.blocks, i)).rev() {
                    remove_block(&mut self.doc, k);
                }
            }
        }
        if self.doc.blocks.is_empty() {
            let empty = new_block(&self.doc, "", 0);
            let at = place.min(self.doc.sequence.len());
            insert_at(&mut self.doc, at, empty);
        }
        match keep.and_then(|id| index_of(&self.doc, id)) {
            Some(i) => {
                let end = len16(&self.doc.blocks[i].text);
                self.at(i, end);
            }
            None => self.at(0, 0),
        }
        self.selection.clear();
    }

    /// 붙여넣기. `anchor` 뒤(또는 빈 편집 블록 자리)에 블록 트리를 넣는다.
    fn paste(&mut self, anchor: usize, editing: bool, text: &str) {
        let mut items = parse_paste(text);
        if items.is_empty() {
            return;
        }
        let block = &self.doc.blocks[anchor];
        let empty = editing && block.text.is_empty() && block.status.is_none();
        let children = has_children(&self.doc.blocks, anchor);
        let open = children && !self.doc.blocks[anchor].folded;
        let anchor_id = block.id;
        let mut base = block.indent;
        let mut at;
        let mut last = anchor_id;
        if empty && (!children || items.len() == 1) {
            // 빈 블록 대체(속성은 유지).
            let (_, first, status) = items.remove(0);
            self.set_block_text(anchor, first);
            self.doc.blocks[anchor].status = status;
            self.doc.edited = true;
            at = seq_of(&self.doc, anchor) + 1;
        } else if editing && open {
            at = seq_of(&self.doc, anchor) + 1;
            base = child_indent(&self.doc.blocks, anchor);
        } else {
            at = seq_of(&self.doc, end_of(&self.doc.blocks, anchor) - 1) + 1;
        }
        for (depth, body, status) in items {
            let mut block = new_block(&self.doc, "", base + depth);
            block.text = body;
            block.status = status;
            refresh_metadata(&mut block);
            last = block.id;
            insert_at(&mut self.doc, at, block);
            at += 1;
        }
        let i = index_of(&self.doc, last).unwrap_or(0);
        let end = len16(&self.doc.blocks[i].text);
        self.cursor = Some((last, end, end));
        self.selection.clear();
    }
}

/// 한 키(명령)를 적용한다. 실패하면 문서를 바꾸지 않는다.
pub fn apply_key(document: &Document, input: &KeyInput) -> Result<KeyOutcome, OutlinerError> {
    let mut doc = document.clone();
    for &i in &input.folded {
        if let Some(block) = doc.blocks.get_mut(i) {
            block.folded = true;
        }
    }
    let original = serialize(&doc);
    let count = doc.blocks.len();
    if let Some(i) = input.block.filter(|&i| i >= count) {
        return Err(OutlinerError::Invalid(format!("블록을 찾을 수 없습니다: {i}")));
    }
    let selection: Vec<usize> = input
        .selection
        .iter()
        .copied()
        .filter(|&i| i < count)
        .collect();
    let selected_ids: Vec<u64> = selection.iter().map(|&i| doc.blocks[i].id).collect();
    let before = doc.clone();
    let mut w = Work {
        doc,
        handled: true,
        cursor: None,
        selection: selected_ids,
        note: None,
        clipboard: None,
    };
    let block = if selection.is_empty() {
        input.block
    } else {
        None
    };
    if let (Some(i), Some(text)) = (input.block, &input.text) {
        w.set_block_text(i, clean(text));
    }
    let (mut start, mut end) = (input.start, input.end.max(input.start));
    if let Some(i) = block {
        let length = len16(&w.doc.blocks[i].text);
        start = start.min(length);
        end = end.min(length);
        w.cursor = Some((w.doc.blocks[i].id, start, end));
    }
    let targets: Vec<u64> = if selection.is_empty() {
        block.map(|i| w.doc.blocks[i].id).into_iter().collect()
    } else {
        roots(&w.doc.blocks, &selection)
    };
    let action = input.action.as_str();
    match action {
        "commit" => {}
        "enter" => {
            if let Some(i) = block {
                w.enter(i, start, end); // 선택한 글은 지우고 나눈다
            }
        }
        "newline" => {
            if let Some(i) = block {
                w.newline(i, start, end);
            }
        }
        "backspace" => match block {
            Some(i) if start == 0 && end == 0 => w.backspace(i),
            _ => w.handled = false,
        },
        "delete" => match block {
            Some(i) if start == end && end == len16(&w.doc.blocks[i].text) => w.delete(i),
            _ => w.handled = false,
        },
        "indent" => {
            for run in runs(&w.doc.blocks, &targets) {
                if !indent_run(&mut w.doc, &run) {
                    w.note("위에 같은 단계의 블록이 없어 들여쓸 수 없습니다");
                }
            }
        }
        "outdent" => {
            let mut all = runs(&w.doc.blocks, &targets);
            all.reverse(); // 뒤 묶음부터 옮겨야 앞 묶음 순서가 유지된다
            for run in all {
                if !outdent_run(&mut w.doc, &run) {
                    w.note("이미 최상위 블록입니다");
                }
            }
        }
        "move_up" | "move_down" => {
            let all = runs(&w.doc.blocks, &targets);
            if all.len() > 1 {
                w.note("붙어 있는 같은 단계 블록만 함께 옮길 수 있습니다");
            } else if let Some(run) = all.first() {
                if !move_run(&mut w.doc, run, action == "move_up") {
                    w.note(if action == "move_up" {
                        "위로 옮길 자리가 없습니다"
                    } else {
                        "아래로 옮길 자리가 없습니다"
                    });
                }
            }
        }
        "up" | "down" => {
            if let Some(i) = block {
                w.vertical(i, start, action == "up");
            }
        }
        "left" => match block.and_then(|i| prev_visible(&w.doc.blocks, i)) {
            Some(p) if start == 0 && end == 0 => {
                let at = len16(&w.doc.blocks[p].text);
                w.at(p, at);
            }
            _ => w.handled = false,
        },
        "right" => match block {
            Some(i) if start == end && end == len16(&w.doc.blocks[i].text) => {
                match next_visible(&w.doc.blocks, i) {
                    Some(n) => w.at(n, 0),
                    None => w.handled = false,
                }
            }
            _ => w.handled = false,
        },
        "cycle_todo" => {
            let indices: Vec<usize> = if selection.is_empty() {
                block.into_iter().collect()
            } else {
                selection.clone()
            };
            w.cycle_todo(&indices, !selection.is_empty());
        }
        "collapse" | "expand" | "toggle_collapse" => {
            let indices: Vec<usize> = if selection.is_empty() {
                block.into_iter().collect()
            } else {
                selection.clone()
            };
            for i in indices {
                if has_children(&w.doc.blocks, i) {
                    w.doc.blocks[i].folded = match action {
                        "collapse" => true,
                        "expand" => false,
                        _ => !w.doc.blocks[i].folded,
                    };
                }
            }
        }
        "copy" => w.clipboard = Some(markdown_of(&w.doc, &targets)),
        "cut" => {
            w.clipboard = Some(markdown_of(&w.doc, &targets));
            w.delete_roots(&targets);
        }
        "delete_selection" => w.delete_roots(&targets),
        "paste" => {
            let value = input
                .paste
                .as_deref()
                .ok_or_else(|| OutlinerError::Invalid("paste가 필요합니다".into()))?;
            match (block, targets.last().and_then(|id| index_of(&w.doc, *id))) {
                (Some(i), _) => {
                    if start != end {
                        let mut text = w.doc.blocks[i].text.clone();
                        let range = byte_at(&text, start)..byte_at(&text, end);
                        text.replace_range(range, "");
                        w.set_block_text(i, text);
                    }
                    w.paste(i, true, value);
                }
                (None, Some(last)) => w.paste(last, false, value),
                _ => w.handled = false,
            }
        }
        other => {
            return Err(OutlinerError::Invalid(format!(
                "지원하지 않는 키 동작: {other}"
            )))
        }
    }
    if !w.handled {
        w.doc = before;
        w.cursor = None;
    }
    finish(w, original)
}

/// 결과를 다시 파싱해 블록 번호를 정규화하고, 파일 형식이 블록 구조를 보존하는지 확인한다.
fn finish(w: Work, original: String) -> Result<KeyOutcome, OutlinerError> {
    let markdown = serialize(&w.doc);
    let changed = markdown != original;
    let position = |id: u64| index_of(&w.doc, id);
    let cursor = w.cursor.and_then(|(id, start, end)| {
        position(id).map(|block| {
            let length = len16(&w.doc.blocks[block].text);
            Cursor {
                block,
                start: start.min(length),
                end: end.min(length),
            }
        })
    });
    let selection: Vec<usize> = {
        let mut s: Vec<usize> = w.selection.iter().filter_map(|id| position(*id)).collect();
        s.sort_unstable();
        s
    };
    let mut document = if changed {
        let parsed = parse(&markdown)?;
        if parsed.blocks.len() != w.doc.blocks.len() {
            return Err(OutlinerError::Invalid(
                "파일 형식으로 담을 수 없는 편집입니다(블록 수가 달라짐)".into(),
            ));
        }
        parsed
    } else {
        w.doc.clone()
    };
    for (block, old) in document.blocks.iter_mut().zip(&w.doc.blocks) {
        block.folded = old.folded;
    }
    let folded = document
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.folded)
        .map(|(i, _)| i)
        .collect();
    Ok(KeyOutcome {
        document,
        markdown,
        handled: w.handled,
        changed,
        cursor,
        selection,
        folded,
        note: w.note,
        clipboard: w.clipboard,
    })
}

// ---------------------------------------------------------------- 글자 입력(괄호 짝)

fn closer(open: char) -> Option<char> {
    match open {
        '[' => Some(']'),
        '(' => Some(')'),
        '{' => Some('}'),
        '`' => Some('`'),
        _ => None,
    }
}

fn from_units(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// 글자 하나를 칠 때의 짝 규칙. `None` 이면 화면의 기본 입력에 맡긴다.
/// 돌려주는 값은 (새 글, 선택 시작, 선택 끝) UTF-16.
pub fn type_char(text: &str, start: usize, end: usize, ch: char) -> Option<(String, usize, usize)> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let (start, end) = (start.min(units.len()), end.min(units.len()));
    let at = |i: usize| units.get(i).and_then(|&u| char::from_u32(u as u32));
    let previous = start.checked_sub(1).and_then(at);
    if start == end && matches!(ch, ']' | ')' | '}' | '`') && at(start) == Some(ch) {
        if ch != '`' || previous != Some('`') {
            return Some((text.to_owned(), start + 1, start + 1));
        }
    }
    let close = closer(ch)?;
    if start == end && ch == '(' && !matches!(previous, None | Some(' ' | '\n' | ']' | '(')) {
        return None;
    }
    let mut out: Vec<u16> = units[..start].to_vec();
    out.push(ch as u16);
    out.extend_from_slice(&units[start..end]);
    out.push(close as u16);
    out.extend_from_slice(&units[end..]);
    Some((from_units(&out), start + 1, end + 1))
}

/// 빈 짝 한가운데의 Backspace 는 둘 다 지운다.
pub fn pair_backspace(text: &str, caret: usize) -> Option<(String, usize)> {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    if caret == 0 || caret >= units.len() {
        return None;
    }
    let open = char::from_u32(units[caret - 1] as u32)?;
    if closer(open).map(|c| c as u16) != Some(units[caret]) {
        return None;
    }
    units.drain(caret - 1..=caret);
    Some((from_units(&units), caret - 1))
}
