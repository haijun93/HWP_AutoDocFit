//! 제외 폴더 편집 패널(DOCVIEWWIN2): 모델(목록·선택·저장·충돌)과 배치. 그리기는 `paint::draw_panel`, 입력은 `window`.
//! 저장은 `settings::save_excluded`(= `docfit_config::set`, 목록 검증·원자 저장·revision 충돌 검사)만 쓴다.
use crate::layout::{Rect, scale};
use crate::settings::{self, SaveError};
use std::path::{Path, PathBuf};

/// 패널 아래 안내 줄의 종류(색을 정한다).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Error,
}

#[derive(Clone, Debug)]
pub struct ExcludePanel {
    pub items: Vec<String>,
    pub selected: Option<usize>,
    /// 목록 첫 줄(줄 번호).
    pub scroll: usize,
    pub message: String,
    pub tone: Tone,
    /// 마지막으로 읽거나 저장한 시점의 설정 revision.
    pub revision: String,
    /// 이 패널에서 저장한 변경이 있는가(닫을 때 다시 검색할지 정한다).
    pub changed: bool,
    settings: PathBuf,
}

impl ExcludePanel {
    /// 설정 파일에서 목록을 읽어 연다.
    pub fn open(settings_path: &Path) -> ExcludePanel {
        let mut p = ExcludePanel { items: Vec::new(), selected: None, scroll: 0, message: String::new(), tone: Tone::Info, revision: String::new(), changed: false, settings: settings_path.to_path_buf() };
        p.reload();
        if let Some(e) = settings::load(settings_path).error {
            p.say(Tone::Error, e);
        }
        p
    }

    pub fn settings_path(&self) -> &Path {
        &self.settings
    }

    fn say(&mut self, tone: Tone, msg: impl Into<String>) {
        self.tone = tone;
        self.message = msg.into();
    }

    /// 파일에서 다시 읽는다(선택은 같은 폴더에 남긴다).
    pub fn reload(&mut self) {
        let keep = self.selected.and_then(|i| self.items.get(i)).map(|s| settings::folder_key(s));
        let loaded = settings::load(&self.settings);
        self.items = loaded.excluded;
        settings::sort_folders(&mut self.items);
        self.revision = loaded.revision;
        self.selected = keep.and_then(|k| self.items.iter().position(|s| settings::folder_key(s) == k));
        self.scroll = self.scroll.min(self.items.len().saturating_sub(1));
    }

    /// 새 목록을 저장한다. 성공하면 true. 충돌이면 아무것도 쓰지 않고 파일의 최신 내용을 다시 읽는다.
    fn commit(&mut self, mut next: Vec<String>, ok_message: String) -> bool {
        settings::sort_folders(&mut next);
        match settings::save_excluded(&self.settings, &next, &self.revision) {
            Ok(rev) => {
                self.items = next;
                self.revision = rev;
                self.changed = true;
                self.say(Tone::Info, ok_message);
                true
            }
            Err(SaveError::Conflict) => {
                self.reload();
                self.say(Tone::Error, "다른 곳에서 설정이 바뀌어 저장하지 않았습니다. 최신 목록을 다시 불러왔으니 확인한 뒤 다시 시도해 주세요.");
                false
            }
            Err(SaveError::Failed(e)) => {
                self.say(Tone::Error, format!("저장하지 못했습니다: {e}"));
                false
            }
        }
    }

    /// 폴더를 목록에 더해 저장한다.
    pub fn add(&mut self, folder: &str) -> bool {
        let folder = folder.trim();
        if folder.is_empty() {
            self.say(Tone::Error, "폴더 경로가 비어 있습니다.");
            return false;
        }
        let key = settings::folder_key(folder);
        if self.items.iter().any(|s| settings::folder_key(s) == key) {
            self.say(Tone::Info, "이미 제외 목록에 있는 폴더입니다.");
            return false;
        }
        let mut next = self.items.clone();
        next.push(folder.to_string());
        let ok = self.commit(next, format!("제외 폴더에 추가했습니다: {folder}"));
        if ok {
            self.selected = self.items.iter().position(|s| settings::folder_key(s) == key);
            self.scroll_to_selected(usize::MAX);
        }
        ok
    }

    /// 선택한 폴더를 목록에서 빼고 저장한다.
    pub fn remove_selected(&mut self) -> bool {
        let Some(i) = self.selected.filter(|i| *i < self.items.len()) else {
            self.say(Tone::Info, "빼려는 폴더를 먼저 선택해 주세요.");
            return false;
        };
        let gone = self.items[i].clone();
        let mut next = self.items.clone();
        next.remove(i);
        let ok = self.commit(next, format!("제외 폴더에서 뺐습니다: {gone}"));
        if ok {
            self.selected = if self.items.is_empty() { None } else { Some(i.min(self.items.len() - 1)) };
            self.scroll = self.scroll.min(self.items.len().saturating_sub(1));
        }
        ok
    }

    pub fn select(&mut self, i: Option<usize>) {
        self.selected = i.filter(|i| *i < self.items.len());
    }

    /// 위·아래 이동(선택이 없으면 첫 줄·끝 줄).
    pub fn step(&mut self, delta: i32, visible_rows: usize) {
        if self.items.is_empty() {
            return;
        }
        let last = self.items.len() - 1;
        let next = match self.selected {
            None => if delta < 0 { last } else { 0 },
            Some(i) => (i as i64 + delta as i64).clamp(0, last as i64) as usize,
        };
        self.selected = Some(next);
        self.scroll_to_selected(visible_rows);
    }

    /// 선택 줄이 보이게 스크롤(`visible_rows` 가 `usize::MAX` 면 모르는 값이라 맨 위 보정만).
    pub fn scroll_to_selected(&mut self, visible_rows: usize) {
        let Some(i) = self.selected else { return };
        if i < self.scroll {
            self.scroll = i;
        } else if visible_rows != usize::MAX && visible_rows > 0 && i >= self.scroll + visible_rows {
            self.scroll = i + 1 - visible_rows;
        }
    }

    pub fn wheel(&mut self, rows: i32, visible_rows: usize) {
        let max = self.items.len().saturating_sub(visible_rows);
        self.scroll = (self.scroll as i64 + rows as i64).clamp(0, max as i64) as usize;
    }
}

/// 패널이 가리킨 곳.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Add,
    Remove,
    Close,
    /// 목록의 항목 번호.
    Row(usize),
    /// 패널 안 빈 곳.
    Inside,
    /// 패널 밖(모달이라 눌러도 아무 일 없음).
    Outside,
}

/// 패널 배치(창 크기·dpi 로 계산). 모든 칸은 대화상자 안에 들어간다.
#[derive(Clone, Copy, Debug)]
pub struct PanelLayout {
    pub dialog: Rect,
    pub title: Rect,
    pub hint: Rect,
    pub list: Rect,
    pub message: Rect,
    pub add: Rect,
    pub remove: Rect,
    pub close: Rect,
    pub row_h: i32,
}

impl PanelLayout {
    pub fn new(w: i32, h: i32, dpi: u32) -> PanelLayout {
        let s = |v: i32| scale(v, dpi);
        let dw = (w - s(48)).clamp(s(300), s(640));
        let dh = (h - s(120)).clamp(s(260), s(470));
        let left = (w - dw) / 2;
        let top = ((h - dh) / 2).max(s(8));
        let dialog = Rect::new(left, top, left + dw, top + dh);
        let pad = s(20);
        let inner_l = dialog.left + pad;
        let inner_r = dialog.right - pad;
        let title = Rect::new(inner_l, dialog.top + s(14), inner_r, dialog.top + s(46));
        let hint = Rect::new(inner_l, title.bottom + s(2), inner_r, title.bottom + s(2) + s(40));
        let btn_h = s(36);
        let btn_top = dialog.bottom - pad - btn_h;
        let message = Rect::new(inner_l, btn_top - s(34), inner_r, btn_top - s(6));
        let list = Rect::new(inner_l, hint.bottom + s(8), inner_r, message.top - s(4));
        let (bw, gap) = (s(110), s(8));
        let add = Rect::new(inner_l, btn_top, inner_l + bw + s(10), btn_top + btn_h);
        let remove = Rect::new(add.right + gap, btn_top, add.right + gap + bw, btn_top + btn_h);
        let close = Rect::new(inner_r - bw, btn_top, inner_r, btn_top + btn_h);
        PanelLayout { dialog, title, hint, list, message, add, remove, close, row_h: s(34) }
    }

    /// 목록에 한 번에 보이는 줄 수.
    pub fn visible_rows(&self) -> usize {
        (self.list.height() / self.row_h).max(1) as usize
    }

    /// 목록의 `row`(화면 위쪽부터 0) 번째 줄 칸.
    pub fn row_rect(&self, row: usize) -> Rect {
        let top = self.list.top + row as i32 * self.row_h;
        Rect::new(self.list.left, top, self.list.right, top + self.row_h)
    }

    pub fn hit(&self, x: i32, y: i32, panel: &ExcludePanel) -> Hit {
        if !self.dialog.contains(x, y) {
            return Hit::Outside;
        }
        if self.add.contains(x, y) {
            return Hit::Add;
        }
        if self.remove.contains(x, y) {
            return Hit::Remove;
        }
        if self.close.contains(x, y) {
            return Hit::Close;
        }
        if self.list.contains(x, y) {
            let row = ((y - self.list.top) / self.row_h) as usize;
            let i = panel.scroll + row;
            if row < self.visible_rows() && i < panel.items.len() {
                return Hit::Row(i);
            }
        }
        Hit::Inside
    }
}
