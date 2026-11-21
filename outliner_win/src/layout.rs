//! 페이지 모드의 창 배치(Win32 없음): 위쪽 줄·왼쪽 페이지 목록·가운데 블록 목록·오른쪽 역링크 패널과
//! 목록 항목 계산. 그리기(`paint_shell`)와 마우스 판정이 같은 값을 쓴다.
use crate::edit::EditHost;
use crate::find::BackItem;
use crate::paint::scale;
use crate::shell::Shell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct R {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

impl R {
    pub fn new(l: i32, t: i32, r: i32, b: i32) -> R {
        R { l, t, r, b }
    }
    pub fn w(&self) -> i32 {
        self.r - self.l
    }
    pub fn h(&self) -> i32 {
        self.b - self.t
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.l && x < self.r && y >= self.t && y < self.b
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Today,
    New,
    Rename,
    Delete,
    /// 문서 끝에 새 블록(Flutter 머리글의 + 단추).
    NewBlock,
    /// 선택·편집 중인 블록 들여쓰기(Flutter 블록 줄의 단추).
    Indent,
    Outdent,
}

impl Btn {
    pub fn label(self) -> &'static str {
        match self {
            Btn::Today => "오늘",
            Btn::New => "＋ 새 페이지",
            Btn::Rename => "이름 바꾸기",
            Btn::Delete => "삭제",
            Btn::NewBlock => "＋ 새 블록",
            Btn::Indent => "들여쓰기",
            Btn::Outdent => "내어쓰기",
        }
    }
}

/// 위쪽 줄 단추 7개(앞 4개 = 페이지, 뒤 3개 = 블록). 폭만 고정.
pub const BUTTONS: [Btn; 7] = [Btn::Today, Btn::New, Btn::Rename, Btn::Delete, Btn::NewBlock, Btn::Indent, Btn::Outdent];

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub dpi: u32,
    pub w: i32,
    pub h: i32,
    pub panels: bool,
    pub top: R,
    pub search_label: R,
    pub search_edit: R,
    pub buttons: [R; 7],
    pub left: Option<R>,
    pub center: R,
    pub right: Option<R>,
    pub head_h: i32,
    pub item_h: i32,
    pub result_h: i32,
    pub popup_h: i32,
}

pub fn compute(w: i32, h: i32, dpi: u32, panels: bool) -> Layout {
    let (head_h, item_h, result_h, popup_h) = (scale(34, dpi), scale(26, dpi), scale(30, dpi), scale(24, dpi));
    if !panels {
        return Layout {
            dpi,
            w,
            h,
            panels,
            top: R::default(),
            search_label: R::default(),
            search_edit: R::default(),
            buttons: [R::default(); 7],
            left: None,
            center: R::new(0, 0, w, h),
            right: None,
            head_h,
            item_h,
            result_h,
            popup_h,
        };
    }
    let top_h = scale(48, dpi);
    let pad = scale(12, dpi);
    let show_left = w >= scale(560, dpi);
    let show_right = w >= scale(900, dpi);
    let (lw, rw) = (scale(200, dpi), scale(250, dpi));
    let left = show_left.then(|| R::new(0, top_h, lw, h));
    let right = show_right.then(|| R::new(w - rw, top_h, w, h));
    let center = R::new(left.map_or(0, |r| r.r), top_h, right.map_or(w, |r| r.l), h);
    let label = R::new(pad, 0, pad + scale(40, dpi), top_h);
    let edit = R::new(label.r, scale(8, dpi), label.r + scale(190, dpi), top_h - scale(8, dpi));
    let widths = [scale(46, dpi), scale(92, dpi), scale(88, dpi), scale(46, dpi), scale(84, dpi), scale(64, dpi), scale(64, dpi)];
    let mut buttons = [R::default(); 7];
    let mut x = edit.r + scale(14, dpi);
    for (i, bw) in widths.iter().enumerate() {
        buttons[i] = R::new(x, scale(8, dpi), x + bw, top_h - scale(8, dpi));
        x += bw + scale(if i == 3 { 16 } else { 5 }, dpi);
    }
    Layout {
        dpi,
        w,
        h,
        panels,
        top: R::new(0, 0, w, top_h),
        search_label: label,
        search_edit: edit,
        buttons,
        left,
        center,
        right,
        head_h,
        item_h,
        result_h,
        popup_h,
    }
}

impl Layout {
    /// 가운데 블록 목록의 크기(`draw_ext` 에 그대로 넘긴다).
    pub fn center_size(&self) -> (i32, i32) {
        (self.center.w(), self.center.h())
    }

    pub fn button_at(&self, x: i32, y: i32) -> Option<Btn> {
        if !self.panels {
            return None;
        }
        BUTTONS.iter().zip(self.buttons.iter()).find(|(_, r)| r.contains(x, y)).map(|(b, _)| *b)
    }

    /// 패널 안 목록이 시작하는 y.
    pub fn list_top(&self, panel: R) -> i32 {
        panel.t + self.head_h
    }

    /// 왼쪽·오른쪽 패널에서 화면 y 에 해당하는 보이는 칸 번호(첫 칸 0).
    pub fn slot_at(&self, panel: R, y: i32) -> Option<usize> {
        let top = self.list_top(panel);
        (y >= top && y < panel.b).then(|| ((y - top) / self.item_h) as usize)
    }

    pub fn left_slots(&self) -> usize {
        self.left.map_or(0, |l| ((l.h() - self.head_h) / self.item_h).max(0) as usize)
    }

    pub fn right_slots(&self) -> usize {
        self.right.map_or(0, |r| ((r.h() - self.head_h) / self.item_h).max(0) as usize)
    }

    pub fn result_slot_at(&self, y: i32) -> Option<usize> {
        let top = self.center.t + self.head_h;
        (y >= top && y < self.center.b).then(|| ((y - top) / self.result_h) as usize)
    }

    pub fn result_slots(&self) -> usize {
        ((self.center.h() - self.head_h) / self.result_h).max(0) as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeftItem {
    Label(String),
    Page(String),
}

/// 검색 칸 글로 거른 왼쪽 목록(Flutter 의 페이지 이름 필터): 최근 목록은 그대로, 전체 목록만 이름에 글이 들어 있는 것으로 줄인다.
/// 글이 비면 `left_items` 와 같다.
pub fn left_items_filtered(pages: &[String], recent: &[String], query: &str) -> Vec<LeftItem> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return left_items(pages, recent);
    }
    let mut v = Vec::new();
    let recents: Vec<&String> = recent.iter().filter(|r| pages.contains(r)).take(8).collect();
    if !recents.is_empty() {
        v.push(LeftItem::Label("최근 페이지".into()));
        v.extend(recents.into_iter().map(|r| LeftItem::Page(r.clone())));
    }
    let hit: Vec<&String> = pages.iter().filter(|p| p.to_lowercase().contains(&q)).collect();
    v.push(LeftItem::Label(format!("전체 페이지 ({}/{})", hit.len(), pages.len())));
    v.extend(hit.into_iter().map(|p| LeftItem::Page(p.clone())));
    v
}

/// 왼쪽 목록: "최근" 머리글 + 최근 페이지(최대 8) + "전체 페이지" 머리글 + 모든 페이지.
pub fn left_items(pages: &[String], recent: &[String]) -> Vec<LeftItem> {
    let mut v = Vec::new();
    let recents: Vec<&String> = recent.iter().filter(|r| pages.contains(r)).take(8).collect();
    if !recents.is_empty() {
        v.push(LeftItem::Label("최근 페이지".into()));
        v.extend(recents.into_iter().map(|r| LeftItem::Page(r.clone())));
    }
    v.push(LeftItem::Label(format!("전체 페이지 ({})", pages.len())));
    v.extend(pages.iter().map(|p| LeftItem::Page(p.clone())));
    v
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RightItem {
    Group(String),
    /// `Shell::back` 안의 번호.
    Block(usize),
}

/// 오른쪽 역링크 목록: 페이지 이름 줄 아래에 그 페이지의 블록들.
pub fn right_items(back: &[BackItem]) -> Vec<RightItem> {
    let mut v = Vec::new();
    let mut last: Option<&str> = None;
    for (i, b) in back.iter().enumerate() {
        if last != Some(b.page.as_str()) {
            v.push(RightItem::Group(b.page.clone()));
            last = Some(b.page.as_str());
        }
        v.push(RightItem::Block(i));
    }
    v
}

/// 자동완성 팝업 위치(창 전체 좌표). `edit` 는 가운데 목록 기준 EDIT 사각형.
pub fn popup_rect(layout: &Layout, edit: crate::layout::R, rows: usize) -> R {
    let pw = scale(320, layout.dpi).min(layout.w - 2 * scale(8, layout.dpi)).max(scale(120, layout.dpi));
    let ph = rows as i32 * layout.popup_h + 2 * scale(2, layout.dpi);
    let left = (layout.center.l + edit.l).min(layout.w - pw).max(0);
    let below = layout.center.t + edit.b + scale(2, layout.dpi);
    let top = if below + ph <= layout.h { below } else { (layout.center.t + edit.t - ph - scale(2, layout.dpi)).max(0) };
    R::new(left, top, left + pw, top + ph)
}

/// 시험·창이 같이 쓰는 줄 수 계산(가운데 목록의 한 화면 줄 수).
pub fn page_rows<H: EditHost>(layout: &Layout, shell: &Shell<H>) -> usize {
    let _ = shell;
    crate::paint::Metrics::new(layout.dpi).page_rows(layout.center.h())
}
