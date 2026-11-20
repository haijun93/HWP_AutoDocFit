//! 읽기 전용 블록 목록의 순수 모델·레이아웃 계산(Win32 없이 시험한다).
//!
//! `outliner_core::parse` 결과를 화면 줄(`Row`)로 바꾼다: 들여쓰기 깊이, 접힌 블록의 하위 숨김,
//! 자식 유무. 스크롤·선택 이동 계산도 여기서 한다.
use outliner_core::{Block, Document};

/// 한 화면 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// `Document::blocks` 안의 순서.
    pub block: usize,
    /// 화면 들여쓰기 단계(엔진의 공백 폭이 아니라 "조상 개수").
    pub depth: usize,
    pub has_children: bool,
    pub folded: bool,
    /// None / "TODO" / "DONE".
    pub status: Option<String>,
    /// 화면에 그릴 글(`collapsed:: true` 속성 글자는 뺀다).
    pub text: String,
}

/// 파일에는 접힘이 저장되지 않으므로(엔진 `parse` 는 항상 펼침) Logseq 관례인 `collapsed:: true`
/// 속성이 있는 블록을 접힌 상태로 시작시킨다.
pub fn apply_collapsed_property(doc: &mut Document) {
    for block in &mut doc.blocks {
        if block.properties.get("collapsed").map(String::as_str) == Some("true") {
            block.folded = true;
        }
    }
}

/// 블록마다 "조상 개수"를 구한다. 엔진 들여쓰기 폭(1칸·2칸·탭=4)이 섞여 있어도 단계가 맞는다.
pub fn depths(blocks: &[Block]) -> Vec<usize> {
    let mut stack: Vec<usize> = Vec::new();
    blocks
        .iter()
        .map(|b| {
            while stack.last().is_some_and(|&top| top >= b.indent) {
                stack.pop();
            }
            let depth = stack.len();
            stack.push(b.indent);
            depth
        })
        .collect()
}

fn display_text(block: &Block) -> String {
    let words: Vec<&str> = block.text.split_whitespace().collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if words[i] == "collapsed::" && i + 1 < words.len() {
            i += 2;
            continue;
        }
        if words[i] == "collapsed::true" {
            i += 1;
            continue;
        }
        kept.push(words[i]);
        i += 1;
    }
    if kept.len() == words.len() { block.text.clone() } else { kept.join(" ") }
}

/// 보이는 줄 목록: 접힌 블록 뒤에서 들여쓰기가 더 깊은 연속 블록은 숨긴다(Flutter `_visibleBlocks` 와 같은 규칙).
pub fn build_rows(doc: &Document) -> Vec<Row> {
    let depth = depths(&doc.blocks);
    let mut rows = Vec::new();
    let mut hide_deeper_than: Option<usize> = None;
    for (i, block) in doc.blocks.iter().enumerate() {
        if let Some(limit) = hide_deeper_than {
            if block.indent > limit {
                continue;
            }
            hide_deeper_than = None;
        }
        let has_children = doc.blocks.get(i + 1).is_some_and(|next| next.indent > block.indent);
        if block.folded && has_children {
            hide_deeper_than = Some(block.indent);
        }
        rows.push(Row {
            block: i,
            depth: depth[i],
            has_children,
            folded: block.folded,
            status: block.status.clone(),
            text: display_text(block),
        });
    }
    rows
}

/// 한 화면에 들어가는 줄 수(최소 1).
pub fn rows_per_page(client_h: i32, header_h: i32, row_h: i32) -> usize {
    (((client_h - header_h).max(0)) / row_h.max(1)).max(1) as usize
}

/// 맨 위 줄 번호를 범위 안으로.
pub fn clamp_first(first: usize, total: usize, page: usize) -> usize {
    first.min(total.saturating_sub(page))
}

/// 선택 줄이 화면에 보이도록 맨 위 줄을 조정한다.
pub fn ensure_visible(first: usize, selected: usize, total: usize, page: usize) -> usize {
    let first = if selected < first {
        selected
    } else if selected >= first + page {
        selected + 1 - page
    } else {
        first
    };
    clamp_first(first, total, page)
}

/// 화면 y 좌표 → 줄 번호(머리글 위·목록 밖이면 None).
pub fn row_at(y: i32, header_h: i32, row_h: i32, first: usize, total: usize) -> Option<usize> {
    if y < header_h || row_h <= 0 {
        return None;
    }
    let index = first + ((y - header_h) / row_h) as usize;
    (index < total).then_some(index)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

pub fn move_selection(selected: usize, total: usize, nav: Nav, page: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let last = total - 1;
    match nav {
        Nav::Up => selected.saturating_sub(1),
        Nav::Down => (selected + 1).min(last),
        Nav::PageUp => selected.saturating_sub(page.max(1)),
        Nav::PageDown => (selected + page.max(1)).min(last),
        Nav::Home => 0,
        Nav::End => last,
    }
}

/// 화면 상태: 문서·보이는 줄·선택·스크롤.
pub struct View {
    pub doc: Document,
    pub rows: Vec<Row>,
    /// `rows` 안의 선택 줄.
    pub selected: usize,
    /// 맨 위에 보이는 줄.
    pub first: usize,
}

impl View {
    pub fn from_markdown(text: &str) -> Result<View, String> {
        let mut doc = outliner_core::parse(text).map_err(|e| e.to_string())?;
        apply_collapsed_property(&mut doc);
        let rows = build_rows(&doc);
        Ok(View { doc, rows, selected: 0, first: 0 })
    }

    pub fn block_count(&self) -> usize {
        self.doc.blocks.len()
    }

    pub fn select(&mut self, nav: Nav, page: usize) {
        self.selected = move_selection(self.selected, self.rows.len(), nav, page);
        self.first = ensure_visible(self.first, self.selected, self.rows.len(), page);
    }

    pub fn set_selected(&mut self, index: usize, page: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
        self.first = ensure_visible(self.first, self.selected, self.rows.len(), page);
    }

    pub fn scroll_by(&mut self, delta: isize, page: usize) {
        let first = (self.first as isize + delta).max(0) as usize;
        self.first = clamp_first(first, self.rows.len(), page);
    }

    pub fn set_first(&mut self, first: usize, page: usize) {
        self.first = clamp_first(first, self.rows.len(), page);
    }

    /// 선택 블록 접기/펼치기. 하위가 없으면 아무것도 하지 않는다. 선택은 같은 블록에 머문다.
    pub fn set_fold(&mut self, fold: bool, page: usize) -> bool {
        let Some(row) = self.rows.get(self.selected) else { return false };
        if !row.has_children || row.folded == fold {
            return false;
        }
        let id = self.doc.blocks[row.block].id;
        let op = if fold { "fold" } else { "unfold" };
        if outliner_core::edit(&mut self.doc, op, id, None).is_err() {
            return false;
        }
        let keep = row.block;
        self.rows = build_rows(&self.doc);
        self.selected = self.rows.iter().position(|r| r.block == keep).unwrap_or(0);
        self.first = ensure_visible(self.first, self.selected, self.rows.len(), page);
        true
    }
}
