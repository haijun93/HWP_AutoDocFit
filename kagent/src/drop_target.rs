//! 관리창 끌어 놓기(OLE `IDropTarget`, windows-sys 위 손 vtable — ui-core a11y와 같은 방식).
//!
//! Flutter 관리창(단축키 모드)의 "파일·폴더·이미지·웹 링크를 여기에 끌어 놓으세요"와 같은 일:
//! 탐색기 파일·폴더(CF_HDROP)와 브라우저 주소(UniformResourceLocatorW, 없으면 http(s) 글 하나)를 받아
//! 경로·주소 목록을 창에 `PostMessageW(msg, 0, Box<Vec<String>>)`로 넘긴다(받는 쪽이 `take`로 꺼낸다).
//! 실제 추가는 파이프 `add_files`와 같은 길(그림 파일은 그림 항목, 주소는 웹 항목).
//!
//! 순수 부분(`urls_from_text`)만 따로 시험하고, COM 쪽은 창 시험에서 `RegisterDragDrop` 성공만 본다.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{HWND, POINTL, S_OK};
use windows_sys::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL};
use windows_sys::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_NONE, ReleaseStgMedium};
use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows_sys::core::{GUID, HRESULT};

const E_NOINTERFACE: HRESULT = 0x8000_4002u32 as i32;
const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
const IID_IDROPTARGET: GUID = GUID::from_u128(0x00000122_0000_0000_c000_000000000046);

#[repr(C)]
struct DataObjectVtbl {
    query_interface: usize,
    add_ref: usize,
    release: usize,
    get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
    get_data_here: usize,
    query_get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC) -> HRESULT,
}

#[repr(C)]
struct Vtbl {
    query_interface: unsafe extern "system" fn(*mut Target, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut Target) -> u32,
    release: unsafe extern "system" fn(*mut Target) -> u32,
    drag_enter: unsafe extern "system" fn(*mut Target, *mut c_void, u32, POINTL, *mut u32) -> HRESULT,
    drag_over: unsafe extern "system" fn(*mut Target, u32, POINTL, *mut u32) -> HRESULT,
    drag_leave: unsafe extern "system" fn(*mut Target) -> HRESULT,
    drop: unsafe extern "system" fn(*mut Target, *mut c_void, u32, POINTL, *mut u32) -> HRESULT,
}

#[repr(C)]
pub struct Target {
    vtbl: *const Vtbl,
    refs: AtomicU32,
    hwnd: HWND,
    msg: u32,
    accept: bool,
}

static VTBL: Vtbl = Vtbl { query_interface, add_ref, release: release_impl, drag_enter, drag_over, drag_leave, drop: on_drop };

/// 끌어 놓기 받는 개체를 만든다(참조 1). `RegisterDragDrop`에 넘기고, 창을 없앨 때 `RevokeDragDrop`.
pub fn new(hwnd: HWND, msg: u32) -> *mut c_void {
    Box::into_raw(Box::new(Target { vtbl: &VTBL, refs: AtomicU32::new(1), hwnd, msg, accept: false })) as *mut c_void
}

/// 참조 하나 놓기(`new`가 준 참조, RegisterDragDrop은 따로 잡는다).
///
/// # Safety
/// `p`는 [`new`]가 돌려준 값이어야 한다.
pub unsafe fn release(p: *mut c_void) {
    unsafe { release_impl(p as *mut Target) };
}

/// 창 메시지 LPARAM에서 놓은 목록을 꺼낸다(한 번만).
///
/// # Safety
/// `lp`는 이 모듈이 보낸 메시지의 값이어야 한다.
pub unsafe fn take(lp: isize) -> Vec<String> {
    if lp == 0 {
        return Vec::new();
    }
    *unsafe { Box::from_raw(lp as *mut Vec<String>) }
}

unsafe extern "system" fn query_interface(this: *mut Target, riid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_NOINTERFACE;
        }
        let id = &*riid;
        let same = |g: &GUID| g.data1 == id.data1 && g.data2 == id.data2 && g.data3 == id.data3 && g.data4 == id.data4;
        if same(&IID_IUNKNOWN) || same(&IID_IDROPTARGET) {
            add_ref(this);
            *out = this as *mut c_void;
            S_OK
        } else {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}

unsafe extern "system" fn add_ref(this: *mut Target) -> u32 {
    unsafe { (*this).refs.fetch_add(1, Ordering::AcqRel) + 1 }
}

unsafe extern "system" fn release_impl(this: *mut Target) -> u32 {
    unsafe {
        let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if n == 0 {
            drop(Box::from_raw(this));
        }
        n
    }
}

fn format(cf: u16) -> FORMATETC {
    FORMATETC { cfFormat: cf, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT, lindex: -1, tymed: TYMED_HGLOBAL as u32 }
}

fn url_format() -> u16 {
    let name: Vec<u16> = "UniformResourceLocatorW\0".encode_utf16().collect();
    unsafe { RegisterClipboardFormatW(name.as_ptr()) as u16 }
}

unsafe fn has(data: *mut c_void, cf: u16) -> bool {
    unsafe {
        let vt = &**(data as *mut *const DataObjectVtbl);
        (vt.query_get_data)(data, &format(cf)) == S_OK
    }
}

/// 놓은 자료에서 경로·주소 목록(받을 수 없는 자료면 빈 목록).
unsafe fn read(data: *mut c_void) -> Vec<String> {
    unsafe {
        let vt = &**(data as *mut *const DataObjectVtbl);
        let mut out = Vec::new();
        let mut medium: STGMEDIUM = std::mem::zeroed();
        if (vt.get_data)(data, &format(CF_HDROP), &mut medium) == S_OK {
            let drop = medium.u.hGlobal as HDROP;
            let n = DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0);
            for i in 0..n.min(100) {
                let len = DragQueryFileW(drop, i, std::ptr::null_mut(), 0) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32);
                out.push(String::from_utf16_lossy(&buf[..len]));
            }
            ReleaseStgMedium(&mut medium);
            return out;
        }
        for (cf, url_only) in [(url_format(), false), (CF_UNICODETEXT, true)] {
            let mut medium: STGMEDIUM = std::mem::zeroed();
            if (vt.get_data)(data, &format(cf), &mut medium) != S_OK {
                continue;
            }
            let h = medium.u.hGlobal;
            let p = GlobalLock(h) as *const u16;
            if !p.is_null() {
                let max = GlobalSize(h) / 2;
                let mut n = 0;
                while n < max && *p.add(n) != 0 {
                    n += 1;
                }
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
                GlobalUnlock(h);
                out = if url_only { urls_from_text(&text) } else { vec![text.trim().to_string()] };
            }
            ReleaseStgMedium(&mut medium);
            if !out.is_empty() {
                break;
            }
        }
        out
    }
}

/// 끌어 놓은 글이 http(s) 주소 한 개면 그 주소(그 밖의 글은 받지 않음 — 웹 링크만).
pub fn urls_from_text(text: &str) -> Vec<String> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    if (lower.starts_with("http://") || lower.starts_with("https://")) && t.len() > 8 && !t.contains(char::is_whitespace) {
        vec![t.to_string()]
    } else {
        Vec::new()
    }
}

unsafe extern "system" fn drag_enter(this: *mut Target, data: *mut c_void, _keys: u32, _pt: POINTL, effect: *mut u32) -> HRESULT {
    unsafe {
        let ok = !data.is_null() && (has(data, CF_HDROP) || has(data, url_format()) || has(data, CF_UNICODETEXT));
        (*this).accept = ok;
        if !effect.is_null() {
            *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE };
        }
        S_OK
    }
}

unsafe extern "system" fn drag_over(this: *mut Target, _keys: u32, _pt: POINTL, effect: *mut u32) -> HRESULT {
    unsafe {
        if !effect.is_null() {
            *effect = if (*this).accept { DROPEFFECT_COPY } else { DROPEFFECT_NONE };
        }
        S_OK
    }
}

unsafe extern "system" fn drag_leave(this: *mut Target) -> HRESULT {
    unsafe { (*this).accept = false };
    S_OK
}

unsafe extern "system" fn on_drop(this: *mut Target, data: *mut c_void, _keys: u32, _pt: POINTL, effect: *mut u32) -> HRESULT {
    unsafe {
        let list = if data.is_null() { Vec::new() } else { read(data) };
        if !effect.is_null() {
            *effect = if list.is_empty() { DROPEFFECT_NONE } else { DROPEFFECT_COPY };
        }
        (*this).accept = false;
        // 빈 목록도 보낸다(받는 쪽이 "지원하지 않는 자료" 안내).
        let boxed = Box::into_raw(Box::new(list)) as isize;
        if PostMessageW((*this).hwnd, (*this).msg, 0, boxed) == 0 {
            drop_box(boxed);
        }
        S_OK
    }
}

fn drop_box(p: isize) {
    unsafe { drop(Box::from_raw(p as *mut Vec<String>)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_single_http_links_are_taken_from_text() {
        assert_eq!(urls_from_text(" https://example.com/a?b=1 \r\n"), ["https://example.com/a?b=1"]);
        assert_eq!(urls_from_text("HTTP://x.org"), ["HTTP://x.org"]);
        assert!(urls_from_text("https://a b").is_empty());
        assert!(urls_from_text("file:///C:/x").is_empty());
        assert!(urls_from_text("그냥 글").is_empty());
    }

    #[test]
    fn com_reference_counting_frees_on_last_release() {
        let p = new(std::ptr::null_mut(), 0) as *mut Target;
        unsafe {
            let mut out = std::ptr::null_mut();
            assert_eq!(query_interface(p, &IID_IDROPTARGET, &mut out), S_OK);
            assert_eq!(out, p as *mut c_void);
            let other = GUID::from_u128(1);
            assert_eq!(query_interface(p, &other, &mut out), E_NOINTERFACE);
            assert_eq!(release_impl(p), 1);
            assert_eq!(release_impl(p), 0);
        }
    }

    #[test]
    fn take_returns_posted_list_once() {
        let p = Box::into_raw(Box::new(vec!["C:\\a.txt".to_string()])) as isize;
        assert_eq!(unsafe { take(p) }, ["C:\\a.txt"]);
        assert!(unsafe { take(0) }.is_empty());
    }
}
