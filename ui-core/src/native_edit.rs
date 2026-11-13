//! 여러 줄·폼 입력: OS `EDIT`·RichEdit(`msftedit.dll`, `RICHEDIT50W`) 자식 컨트롤(Q14).
//! IME·실행 취소·클립보드·접근성은 OS가 준다. 테마 색은 부모가 `WM_CTLCOLOREDIT`에서 [`EditColors::on_ctlcolor`]로.

use windows_sys::Win32::Foundation::{HWND, LRESULT};
use windows_sys::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, HBRUSH, HDC, HFONT, SetBkColor, SetTextColor};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::geom::Rect;

const ES_LEFT: u32 = 0x0000;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_WANTRETURN: u32 = 0x1000;
const EM_LIMITTEXT: u32 = 0x00C5;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 편집 칸 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// 한 줄(폼의 글·경로 칸).
    SingleLine,
    /// 여러 줄(메모·설명).
    MultiLine,
    /// RichEdit 여러 줄(메모 편집기 서식).
    Rich,
}

/// 칸 스타일(순수 함수).
pub fn edit_style(kind: EditKind) -> u32 {
    WS_CHILD | WS_VISIBLE | WS_TABSTOP | match kind {
        EditKind::SingleLine => ES_LEFT | ES_AUTOHSCROLL,
        EditKind::MultiLine | EditKind::Rich => ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN | WS_VSCROLL,
    }
}

/// RichEdit 50 DLL을 한 번 적재.
fn load_rich() -> bool {
    unsafe {
        let name = wide("msftedit.dll");
        !GetModuleHandleW(name.as_ptr()).is_null() || !LoadLibraryW(name.as_ptr()).is_null()
    }
}

/// 자식 편집 칸을 만든다.
pub fn create(parent: HWND, id: usize, kind: EditKind, r: Rect, font: HFONT, max_chars: usize) -> Result<HWND, String> {
    let class = match kind {
        EditKind::Rich => {
            if !load_rich() {
                return Err("RichEdit(msftedit.dll)을 불러오지 못했습니다".into());
            }
            "RICHEDIT50W"
        }
        _ => "EDIT",
    };
    unsafe {
        let h = CreateWindowExW(WS_EX_CLIENTEDGE, wide(class).as_ptr(), std::ptr::null(), edit_style(kind), r.left, r.top,
                                r.width(), r.height(), parent, id as HMENU, GetModuleHandleW(std::ptr::null()), std::ptr::null());
        if h.is_null() {
            return Err(format!("{class} 만들기 실패"));
        }
        if !font.is_null() {
            SendMessageW(h, WM_SETFONT, font as usize, 1);
        }
        if max_chars > 0 {
            SendMessageW(h, EM_LIMITTEXT, max_chars, 0);
        }
        Ok(h)
    }
}

pub fn set_text(h: HWND, s: &str) {
    unsafe { SetWindowTextW(h, wide(s).as_ptr()) };
}

pub fn text(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}

pub fn move_to(h: HWND, r: Rect) {
    unsafe { MoveWindow(h, r.left, r.top, r.width(), r.height(), 1) };
}

/// 편집 칸 색(테마). 붓은 하나만 만들어 두고 색이 바뀌면 다시 만든다.
pub struct EditColors {
    text: u32,
    back: u32,
    brush: HBRUSH,
}

impl EditColors {
    pub fn new(text: u32, back: u32) -> Self {
        Self { text, back, brush: unsafe { CreateSolidBrush(back) } }
    }
    pub fn set(&mut self, text: u32, back: u32) {
        if back != self.back {
            unsafe {
                DeleteObject(self.brush);
                self.brush = CreateSolidBrush(back);
            }
        }
        self.text = text;
        self.back = back;
    }
    /// WM_CTLCOLOREDIT 처리: WPARAM(HDC)에 색을 정하고 바탕 붓을 돌려준다.
    pub fn on_ctlcolor(&self, wp: usize) -> LRESULT {
        unsafe {
            let dc = wp as HDC;
            SetTextColor(dc, self.text);
            SetBkColor(dc, self.back);
        }
        self.brush as LRESULT
    }
}

impl Drop for EditColors {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.brush) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn parent_proc(h: HWND, m: u32, w: usize, l: isize) -> LRESULT {
        unsafe { DefWindowProcW(h, m, w, l) }
    }

    #[test]
    fn edit_and_rich_edit_round_trip_korean_text() {
        crate::window::register_class("UiCore.Test.EditParent", Some(parent_proc), false).unwrap();
        let parent = crate::window::create(crate::window::Kind::Normal, "UiCore.Test.EditParent", "편집 시험",
                                           Rect::xywh(0, 0, 400, 300), std::ptr::null_mut()).unwrap();
        for (i, kind) in [EditKind::SingleLine, EditKind::MultiLine, EditKind::Rich].into_iter().enumerate() {
            let e = create(parent, 100 + i, kind, Rect::xywh(10, 10 + 60 * i as i32, 300, 50), std::ptr::null_mut(), 0).unwrap();
            let sample = if kind == EditKind::SingleLine { "한글 입력 시험" } else { "첫 줄\r\n둘째 줄" };
            set_text(e, sample);
            assert_eq!(text(e), sample, "{kind:?}");
            move_to(e, Rect::xywh(0, 0, 100, 30));
        }
        let limited = create(parent, 200, EditKind::SingleLine, Rect::xywh(0, 0, 100, 30), std::ptr::null_mut(), 0).unwrap();
        unsafe { SendMessageW(limited, EM_LIMITTEXT, 3, 0) };
        assert_ne!(edit_style(EditKind::MultiLine) & ES_MULTILINE, 0);
        assert_eq!(edit_style(EditKind::SingleLine) & ES_MULTILINE, 0);
        let mut c = EditColors::new(0x00FFFFFF, 0x00000000);
        c.set(0x00EEEEEE, 0x00111111);
        assert_ne!(c.on_ctlcolor(0), 0);
        unsafe { DestroyWindow(parent) };
    }
}
