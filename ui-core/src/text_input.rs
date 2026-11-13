//! 한 줄 입력(직접 그림 + IME) 상태 기계. 검색칸 전용(Q14). 여러 줄·폼 입력은 [`crate::native_edit`].
//!
//! 상태 = 확정 글자 + 커서(글자 단위) + 선택 기준점 + IME 조합 중 글자 + 실행 취소 기록.
//! | 사건 | 결과 |
//! |---|---|
//! | 글자 입력 | 선택 지우고 커서 자리에 넣음, 조합 글자 비움(상한 넘으면 무시) |
//! | 조합(GCS_COMPSTR) | 조합 글자만 바꿈(확정 글자 그대로) — 화면에는 커서 자리에 보임 |
//! | 조합 끝 | 조합 글자 비움(확정 글자는 WM_CHAR로 따로 옴) |
//! | ← → Home End (+Shift) | 커서 이동(Shift면 선택 늘림) |
//! | Backspace·Delete | 선택 있으면 선택 지움, 없으면 한 글자 |
//! | Ctrl+A·C·X·V·Z | 전체 선택·복사 글(돌려줌)·잘라내기·붙여넣기·되돌리기 |
//! popup·kvf처럼 "끝에 붙이기·끝 지우기"만 쓰면 커서는 늘 끝에 있다.

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::Input::Ime::{
    CFS_POINT, COMPOSITIONFORM, GCS_COMPSTR, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext,
    ImmSetCompositionWindow,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineEdit {
    chars: Vec<char>,
    caret: usize,
    anchor: Option<usize>,
    /// IME 조합 중 글자.
    pub comp: String,
    pub max_chars: usize,
    undo: Vec<(Vec<char>, usize)>,
}

/// 키 처리 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditResult {
    /// 처리하지 않은 키(창이 다른 일에 쓴다: ↑↓·Enter·Esc 등).
    Ignored,
    /// 커서·선택만 바뀜.
    Moved,
    /// 글자가 바뀜(검색 다시 하기).
    Changed,
    /// 클립보드에 넣을 글.
    Copy(String),
    /// 잘라내기: 클립보드에 넣고 글자도 바뀜.
    Cut(String),
    /// 붙여넣기 요청(창이 클립보드를 읽어 [`LineEdit::paste`]).
    PasteRequest,
}

/// 편집 키.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Home,
    End,
    Back,
    Delete,
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
}

impl LineEdit {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars, ..Default::default() }
    }
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }
    /// 확정 글자 + 조합 글자(커서 자리에 끼움) — 검색어.
    pub fn needle(&self) -> String {
        let mut s: String = self.chars[..self.caret].iter().collect();
        s.push_str(&self.comp);
        s.extend(&self.chars[self.caret..]);
        s
    }
    pub fn caret(&self) -> usize {
        self.caret
    }
    pub fn len(&self) -> usize {
        self.chars.len()
    }
    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
    /// 선택 범위(글자 단위, 시작<끝). 없으면 None.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        (a != self.caret).then(|| (a.min(self.caret), a.max(self.caret)))
    }
    pub fn selected_text(&self) -> String {
        self.selection().map(|(a, b)| self.chars[a..b].iter().collect()).unwrap_or_default()
    }
    fn remember(&mut self) {
        if self.undo.last().map(|(c, _)| c) != Some(&self.chars) {
            self.undo.push((self.chars.clone(), self.caret));
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
        }
    }
    fn delete_selection(&mut self) -> bool {
        if let Some((a, b)) = self.selection() {
            self.chars.drain(a..b);
            self.caret = a;
            self.anchor = None;
            true
        } else {
            self.anchor = None;
            false
        }
    }
    /// 글자 하나(WM_CHAR). 제어 문자는 무시. 바뀌면 true.
    pub fn insert_char(&mut self, ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        let selected = self.selection().map(|(a, b)| b - a).unwrap_or(0);
        if self.max_chars > 0 && self.chars.len() - selected >= self.max_chars {
            return false;
        }
        self.remember();
        self.delete_selection();
        self.chars.insert(self.caret, ch);
        self.caret += 1;
        self.comp.clear();
        true
    }
    /// 붙여넣기: 줄바꿈은 공백으로, 상한까지만.
    pub fn paste(&mut self, s: &str) -> bool {
        self.remember();
        let had = self.delete_selection();
        let mut n = 0;
        for ch in s.chars().map(|c| if c == '\r' || c == '\n' || c == '\t' { ' ' } else { c }) {
            if ch.is_control() || (self.max_chars > 0 && self.chars.len() >= self.max_chars) {
                continue;
            }
            self.chars.insert(self.caret, ch);
            self.caret += 1;
            n += 1;
        }
        had || n > 0
    }
    /// IME 조합 글자 바꿈.
    pub fn set_composition(&mut self, s: String) {
        self.comp = s;
    }
    pub fn end_composition(&mut self) {
        self.comp.clear();
    }
    /// 모두 지움(Esc).
    pub fn clear(&mut self) {
        if !self.chars.is_empty() {
            self.remember();
        }
        self.chars.clear();
        self.caret = 0;
        self.anchor = None;
        self.comp.clear();
    }
    /// 끝 글자 지움(popup·kvf 백스페이스와 같음: 커서가 끝일 때). 바뀌면 true.
    pub fn backspace(&mut self) -> bool {
        self.remember();
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        self.caret -= 1;
        self.chars.remove(self.caret);
        true
    }
    pub fn key(&mut self, key: Key, shift: bool) -> EditResult {
        let before = (self.caret, self.anchor);
        let moved = |s: &mut Self, to: usize| {
            if shift {
                if s.anchor.is_none() {
                    s.anchor = Some(s.caret);
                }
            } else {
                s.anchor = None;
            }
            s.caret = to.min(s.chars.len());
        };
        match key {
            Key::Left => {
                let to = match (shift, self.selection()) {
                    (false, Some((a, _))) => a,
                    _ => self.caret.saturating_sub(1),
                };
                moved(self, to)
            }
            Key::Right => {
                let to = match (shift, self.selection()) {
                    (false, Some((_, b))) => b,
                    _ => self.caret + 1,
                };
                moved(self, to)
            }
            Key::Home => moved(self, 0),
            Key::End => moved(self, usize::MAX),
            Key::SelectAll => {
                self.anchor = Some(0);
                self.caret = self.chars.len();
            }
            Key::Back => return if self.backspace() { EditResult::Changed } else { EditResult::Ignored },
            Key::Delete => {
                self.remember();
                if self.delete_selection() {
                    return EditResult::Changed;
                }
                if self.caret < self.chars.len() {
                    self.chars.remove(self.caret);
                    return EditResult::Changed;
                }
                return EditResult::Ignored;
            }
            Key::Copy => {
                let s = self.selected_text();
                return if s.is_empty() { EditResult::Ignored } else { EditResult::Copy(s) };
            }
            Key::Cut => {
                let s = self.selected_text();
                if s.is_empty() {
                    return EditResult::Ignored;
                }
                self.remember();
                self.delete_selection();
                return EditResult::Cut(s);
            }
            Key::Paste => return EditResult::PasteRequest,
            Key::Undo => {
                return match self.undo.pop() {
                    Some((c, caret)) => {
                        self.chars = c;
                        self.caret = caret.min(self.chars.len());
                        self.anchor = None;
                        EditResult::Changed
                    }
                    None => EditResult::Ignored,
                };
            }
        }
        if (self.caret, self.anchor) == before { EditResult::Ignored } else { EditResult::Moved }
    }
}

/// 가상 키 → 편집 키(Ctrl 조합 포함). 편집 키가 아니면 None.
pub fn key_of(vk: u16, ctrl: bool) -> Option<Key> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_DELETE, VK_END, VK_HOME, VK_LEFT, VK_RIGHT};
    Some(match vk {
        VK_LEFT => Key::Left,
        VK_RIGHT => Key::Right,
        VK_HOME => Key::Home,
        VK_END => Key::End,
        VK_BACK => Key::Back,
        VK_DELETE => Key::Delete,
        0x41 if ctrl => Key::SelectAll,
        0x43 if ctrl => Key::Copy,
        0x58 if ctrl => Key::Cut,
        0x56 if ctrl => Key::Paste,
        0x5A if ctrl => Key::Undo,
        _ => return None,
    })
}

/// WM_IME_COMPOSITION에서 조합 중 글자를 읽는다(GCS_COMPSTR가 없으면 None). popup·kvf가 같이 쓴다.
///
/// # Safety
/// `hwnd`는 이 스레드의 창이어야 한다.
pub unsafe fn composition_string(hwnd: HWND, lp: isize) -> Option<String> {
    if lp as u32 & GCS_COMPSTR == 0 {
        return None;
    }
    unsafe {
        let imc = ImmGetContext(hwnd);
        if imc.is_null() {
            return None;
        }
        let bytes = ImmGetCompositionStringW(imc, GCS_COMPSTR, std::ptr::null_mut(), 0);
        let mut buf = vec![0u16; (bytes.max(0) as usize) / 2];
        if !buf.is_empty() {
            ImmGetCompositionStringW(imc, GCS_COMPSTR, buf.as_mut_ptr() as _, bytes as u32);
        }
        ImmReleaseContext(hwnd, imc);
        Some(String::from_utf16_lossy(&buf))
    }
}

/// 조합 창(후보 창) 위치를 입력칸 커서 옆으로(창 안 좌표).
///
/// # Safety
/// `hwnd`는 이 스레드의 창이어야 한다.
pub unsafe fn set_composition_pos(hwnd: HWND, x: i32, y: i32) {
    unsafe {
        let imc = ImmGetContext(hwnd);
        if imc.is_null() {
            return;
        }
        let form = COMPOSITIONFORM { dwStyle: CFS_POINT, ptCurrentPos: POINT { x, y }, rcArea: RECT { left: 0, top: 0, right: 0, bottom: 0 } };
        ImmSetCompositionWindow(imc, &form);
        ImmReleaseContext(hwnd, imc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_backspace_like_popup() {
        let mut e = LineEdit::new(40);
        for c in "항목".chars() {
            assert!(e.insert_char(c));
        }
        e.set_composition("글".into());
        assert_eq!(e.needle(), "항목글");
        assert!(e.insert_char('글'));
        assert_eq!(e.needle(), "항목글", "확정되면 조합 글자는 비움");
        assert!(e.backspace());
        assert_eq!(e.text(), "항목");
        assert!(!e.insert_char('\u{8}'));
        e.clear();
        assert!(!e.backspace());
        assert!(e.is_empty());
    }

    #[test]
    fn limit_counts_chars_not_bytes() {
        let mut e = LineEdit::new(3);
        for c in "가나다라".chars() {
            e.insert_char(c);
        }
        assert_eq!(e.text(), "가나다");
        e.key(Key::Home, true);
        e.key(Key::SelectAll, false);
        assert!(e.insert_char('x'), "선택을 바꾸는 입력은 상한 안");
        assert_eq!(e.text(), "x");
    }

    #[test]
    fn caret_selection_clipboard_and_undo() {
        let mut e = LineEdit::new(0);
        e.paste("hello\nworld");
        assert_eq!(e.text(), "hello world");
        assert_eq!(e.key(Key::Home, false), EditResult::Moved);
        e.key(Key::Right, true);
        e.key(Key::Right, true);
        assert_eq!(e.selected_text(), "he");
        assert_eq!(e.key(Key::Copy, false), EditResult::Copy("he".into()));
        assert_eq!(e.key(Key::Cut, false), EditResult::Cut("he".into()));
        assert_eq!(e.text(), "llo world");
        e.set_composition("한".into());
        assert_eq!(e.needle(), "한llo world", "조합 글자는 커서 자리");
        e.end_composition();
        assert_eq!(e.key(Key::Undo, false), EditResult::Changed);
        assert_eq!(e.text(), "hello world");
        e.key(Key::End, false);
        assert_eq!(e.key(Key::Right, false), EditResult::Ignored);
        assert_eq!(e.key(Key::Delete, false), EditResult::Ignored);
        e.key(Key::Left, false);
        assert_eq!(e.key(Key::Delete, false), EditResult::Changed);
        assert_eq!(e.text(), "hello worl");
        assert_eq!(e.key(Key::Paste, false), EditResult::PasteRequest);
        assert_eq!(key_of(0x56, true), Some(Key::Paste));
        assert_eq!(key_of(0x56, false), None);
    }
}
