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
    /// 블록 안 줄 수(최소 1) — 화면에서 차지하는 줄 칸.
    pub lines: usize,
    /// Read-only reference/embed presentation; editing always uses Document::blocks.
    pub presentation: Vec<Vec<outliner_core::references::DisplaySpan>>,
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
    if kept.len() == words.len() {
        block.text.clone()
    } else if block.text.contains('\n') {
        // 줄 구조를 유지하며 속성 글자만 뺀다.
        block.text.replace("collapsed:: true", "").replace("collapsed::true", "")
    } else {
        kept.join(" ")
    }
}

/// 보이는 줄 목록: 접힌 블록 뒤에서 들여쓰기가 더 깊은 연속 블록은 숨긴다(Flutter `_visibleBlocks` 와 같은 규칙).
pub fn build_rows(doc: &Document) -> Vec<Row> {
    scoped_rows(doc, None)
}

fn scoped_rows(doc: &Document, root: Option<usize>) -> Vec<Row> {
    let depth = depths(&doc.blocks);
    let start = root.unwrap_or(0);
    let end = root.map_or(doc.blocks.len(), |i| subtree_end(doc, i));
    let base = root.map_or(0, |i| depth[i]);
    let mut rows = Vec::new();
    let mut hide_deeper_than: Option<usize> = None;
    for (i, block) in doc.blocks.iter().enumerate().take(end).skip(start) {
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
            depth: depth[i] - base,
            has_children,
            folded: block.folded,
            status: block.status.clone(),
            text: display_text(block),
            lines: block.text.split('\n').count(),
            presentation: Vec::new(),
        });
    }
    rows
}

pub fn subtree_end(doc: &Document, block: usize) -> usize {
    (block + 1..doc.blocks.len()).find(|&i| doc.blocks[i].indent <= doc.blocks[block].indent).unwrap_or(doc.blocks.len())
}

#[derive(Clone, Debug)]
struct FocusVisit { root: Option<u64>, selected: Option<u64>, first: usize }

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
    pub selection_anchor: Option<usize>,
    pub selection_end: Option<usize>,
    /// 파일에 저장하지 않는 블록 ID 기반 화면 범위.
    pub zoom: Option<u64>,
    zoom_back: Vec<FocusVisit>,
}

impl View {
    pub fn from_markdown(text: &str) -> Result<View, String> {
        let mut doc = outliner_core::parse(text).map_err(|e| e.to_string())?;
        apply_collapsed_property(&mut doc);
        let rows = build_rows(&doc);
        Ok(View { doc, rows, selected: 0, first: 0, selection_anchor: None, selection_end: None, zoom: None, zoom_back: Vec::new() })
    }

    pub fn rebuild(&mut self) {
        let root = self.zoom.and_then(|id| self.doc.blocks.iter().position(|b| b.id == id));
        if self.zoom.is_some() && root.is_none() { self.clear_zoom(); }
        self.rows = scoped_rows(&self.doc, root);
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.first = self.first.min(self.rows.len().saturating_sub(1));
        self.clear_selection();
    }

    pub fn clear_zoom(&mut self) { self.zoom = None; self.zoom_back.clear(); }
    pub fn can_zoom_back(&self) -> bool { !self.zoom_back.is_empty() }

    pub fn focus(&mut self, root: Option<u64>) -> bool {
        if root == self.zoom { return false; }
        if root.is_some_and(|id| !self.doc.blocks.iter().any(|b| b.id == id)) { return false; }
        self.zoom_back.push(FocusVisit {
            root: self.zoom,
            selected: self.rows.get(self.selected).map(|r| self.doc.blocks[r.block].id),
            first: self.first,
        });
        self.zoom = root;
        self.selected = 0;
        self.first = 0;
        self.rebuild();
        true
    }

    pub fn zoom_back(&mut self, page: usize) -> bool {
        let Some(visit) = self.zoom_back.pop() else { return false; };
        self.zoom = visit.root;
        self.rebuild();
        self.selected = visit.selected.and_then(|id| self.rows.iter().position(|r| self.doc.blocks[r.block].id == id)).unwrap_or(0);
        self.first = ensure_visible(visit.first, self.selected, self.rows.len(), page);
        true
    }

    /// 페이지를 제외한 조상 경로. 맨 끝은 현재 줌 블록이다.
    pub fn ancestors(&self) -> Vec<usize> {
        let Some(root) = self.zoom.and_then(|id| self.doc.blocks.iter().position(|b| b.id == id)) else { return Vec::new(); };
        let mut path = vec![root];
        let mut indent = self.doc.blocks[root].indent;
        for i in (0..root).rev() {
            if self.doc.blocks[i].indent < indent { path.push(i); indent = self.doc.blocks[i].indent; }
        }
        path.reverse();
        path
    }

    /// 줄 `row` 가 맨 위 줄부터 몇 번째 줄 칸에서 시작하는가.
    pub fn slot_of(&self, row: usize) -> usize {
        self.rows.iter().take(row).skip(self.first).map(|r| r.lines).sum()
    }

    /// 화면 y → 줄 번호(여러 줄 블록 반영).
    pub fn row_at(&self, y: i32, header_h: i32, row_h: i32) -> Option<usize> {
        if y < header_h || row_h <= 0 {
            return None;
        }
        let mut slot = ((y - header_h) / row_h) as usize;
        for (i, row) in self.rows.iter().enumerate().skip(self.first) {
            if slot < row.lines {
                return Some(i);
            }
            slot -= row.lines;
        }
        None
    }

    /// 줄 `row` 전체가 보이도록 한 맨 위 줄 번호.
    pub fn visible_first(&self, row: usize, page: usize) -> usize {
        let mut first = ensure_visible(self.first, row, self.rows.len(), page);
        while first < row && self.rows[first..=row].iter().map(|r| r.lines).sum::<usize>() > page {
            first += 1;
        }
        first
    }

    /// 지금 줄 하나만 블록 선택.
    pub fn select_current(&mut self) {
        self.selection_anchor = Some(self.selected);
        self.selection_end = Some(self.selected);
    }

    pub fn select_all(&mut self) {
        let last = self.rows.len().saturating_sub(1);
        self.selection_anchor = Some(0);
        self.selection_end = Some(last);
        self.selected = last;
    }

    /// 선택 상태에서 Ctrl+A: 부모 블록을 선택한다(최상위면 전체).
    pub fn select_parent(&mut self) {
        let depth = self.rows.get(self.selected).map_or(0, |r| r.depth);
        match (0..self.selected).rev().find(|&i| self.rows[i].depth < depth) {
            Some(parent) => {
                self.selected = parent;
                self.select_current();
            }
            None => self.select_all(),
        }
    }

    pub fn block_count(&self) -> usize {
        self.doc.blocks.len()
    }

    pub fn select(&mut self, nav: Nav, page: usize) {
        self.selected = move_selection(self.selected, self.rows.len(), nav, page);
        self.clear_selection();
        self.first = ensure_visible(self.first, self.selected, self.rows.len(), page);
    }

    pub fn extend_selection(&mut self, nav: Nav, page: usize) {
        if self.rows.is_empty() { return; }
        let anchor = *self.selection_anchor.get_or_insert(self.selected);
        self.selected = move_selection(self.selected, self.rows.len(), nav, page);
        self.selection_anchor = Some(anchor);
        self.selection_end = Some(self.selected);
        self.first = ensure_visible(self.first, self.selected, self.rows.len(), page);
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_end = None;
    }

    pub fn selected_block_ids(&self) -> Vec<u64> {
        let Some(anchor) = self.selection_anchor else {
            return self.rows.get(self.selected).map(|r| vec![self.doc.blocks[r.block].id]).unwrap_or_default();
        };
        let end = self.selection_end.unwrap_or(anchor);
        let (lo, hi) = (anchor.min(end), anchor.max(end));
        self.rows[lo..=hi].iter().map(|r| self.doc.blocks[r.block].id).collect()
    }

    pub fn set_selected(&mut self, index: usize, page: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
        self.clear_selection();
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
        self.rebuild();
        self.selected = self.rows.iter().position(|r| r.block == keep).unwrap_or(0);
        self.first = ensure_visible(self.first, self.selected, self.rows.len(), page);
        true
    }

    /// 화면 줄 안에서 문서 블록 `block` 의 줄 번호.
    pub fn row_of_block(&self, block: usize) -> Option<usize> {
        self.rows.iter().position(|r| r.block == block)
    }

    /// 블록 `block` 이 보이도록(접힌 조상은 펼침) 그 줄을 선택한다. 블록이 없으면 false.
    pub fn reveal_block(&mut self, block: usize, page: usize) -> bool {
        if block >= self.doc.blocks.len() {
            return false;
        }
        if let Some(root) = self.zoom.and_then(|id| self.doc.blocks.iter().position(|b| b.id == id)) {
            if block < root || block >= subtree_end(&self.doc, root) { self.clear_zoom(); }
        }
        let mut need = self.doc.blocks[block].indent;
        for i in (0..block).rev() {
            if self.doc.blocks[i].indent < need {
                need = self.doc.blocks[i].indent;
                self.doc.blocks[i].folded = false;
            }
        }
        self.rebuild();
        match self.row_of_block(block) {
            Some(row) => {
                self.set_selected(row, page);
                true
            }
            None => false,
        }
    }
}
