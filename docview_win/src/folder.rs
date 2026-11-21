//! 폴더 선택 대화상자(`SHBrowseForFolderW`, 새 대화상자 스타일). `windows-sys` 에 IFileDialog COM 래퍼가 없어
//! `settings_win`·`kagent` 와 같은 셸 대화상자를 쓴다(사용자 눈에는 폴더 선택 창).
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows_sys::Win32::UI::Shell::{BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, SHBrowseForFolderW, SHGetPathFromIDListW};
use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;

const BFFM_INITIALIZED: u32 = 1;
const BFFM_SETSELECTIONW: u32 = 0x0400 + 103;

unsafe extern "system" fn browse_cb(hwnd: HWND, msg: u32, _lp: LPARAM, data: LPARAM) -> i32 {
    if msg == BFFM_INITIALIZED && data != 0 {
        unsafe { SendMessageW(hwnd, BFFM_SETSELECTIONW, 1, data) };
    }
    0
}

/// 고르면 경로, 취소하면 None. `initial` 이 비어 있지 않으면 그 폴더에서 시작한다.
pub fn pick_folder(owner: HWND, title: &str, initial: &str) -> Option<String> {
    unsafe {
        CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
        let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
        let title = wide(title);
        let initial = wide(initial.trim());
        let mut display = vec![0u16; 260];
        let info = BROWSEINFOW {
            hwndOwner: owner,
            pidlRoot: null_mut(),
            pszDisplayName: display.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: Some(browse_cb),
            lParam: if initial.len() > 1 { initial.as_ptr() as LPARAM } else { 0 },
            iImage: 0,
        };
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return None;
        }
        let mut path = vec![0u16; 32768];
        let ok = SHGetPathFromIDListW(pidl, path.as_mut_ptr()) != 0;
        CoTaskMemFree(pidl as *const _);
        if !ok {
            return None;
        }
        let len = path.iter().position(|c| *c == 0).unwrap_or(path.len());
        Some(String::from_utf16_lossy(&path[..len]))
    }
}
