#![windows_subsystem = "windows"]
use rename_win::{PreviewRow, RowStatus, mark_collisions, preview_name};
use std::{ffi::c_void, mem::zeroed, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, EndPaint, GetStockObject, HBRUSH, HDC, PAINTSTRUCT, SetBkMode, SetTextColor,
        TRANSPARENT, TextOutW, WHITE_BRUSH,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW,
        GetMessageW, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage, RegisterClassW, SW_SHOW,
        ShowWindow, TranslateMessage, WM_DESTROY, WM_PAINT, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    },
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn draw_text(hdc: HDC, x: i32, y: i32, s: &str) {
    let w = wide(s);
    unsafe {
        TextOutW(hdc, x, y, w.as_ptr(), (w.len() - 1) as i32);
    }
}
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = unsafe { zeroed() };
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            unsafe {
                SetBkMode(hdc, TRANSPARENT as i32);
                SetTextColor(hdc, 0x00202020);
                draw_text(hdc, 24, 22, "파일명 일괄변경 · 미리보기");
                draw_text(
                    hdc,
                    24,
                    52,
                    "이번 단계는 규칙 계산과 표시만 합니다. 파일 이름은 변경되지 않습니다.",
                );
                draw_text(
                    hdc,
                    24,
                    98,
                    "규칙 미리보기: 접두사 ‘정리_’ + 순번(3자리) · 확장자 유지",
                );
                draw_text(hdc, 24, 144, "원래 이름");
                draw_text(hdc, 470, 144, "미리보기 이름");
                draw_text(hdc, 930, 144, "상태");
                let originals = [
                    "6C1D56A5-12B9-4F7D-9846-BA1255C43404.JPG",
                    "현장 사진 2.jpg",
                    "보고서(자간조정).hwpx",
                    "압축 자료.zip",
                    "이름 중복 확인.txt",
                ];
                let mut rows: Vec<_> = originals
                    .iter()
                    .enumerate()
                    .map(|(i, n)| PreviewRow {
                        original: n.to_string(),
                        proposed: preview_name(n, "정리_", i + 1, 3),
                        status: RowStatus::Changed,
                    })
                    .collect();
                mark_collisions(&mut rows, &["정리_현장 사진 2_002.jpg"]);
                for (i, row) in rows.iter().enumerate() {
                    let y = 184 + i as i32 * 42;
                    draw_text(hdc, 24, y, &row.original);
                    draw_text(hdc, 470, y, &row.proposed);
                    draw_text(
                        hdc,
                        930,
                        y,
                        if row.status == RowStatus::Conflict {
                            "충돌"
                        } else {
                            "변경 예정"
                        },
                    );
                }
                draw_text(
                    hdc,
                    24,
                    430,
                    "목록은 예시 데이터입니다 · 파일 선택/끌어놓기와 실제 변경은 후속 단계",
                );
                EndPaint(hwnd, &ps);
            }
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}
fn main() {
    unsafe {
        let instance = GetModuleHandleW(null_mut());
        let class = wide("DocFitRenameWin");
        let title = wide("파일명 일괄변경 · 미리보기");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: GetStockObject(WHITE_BRUSH) as HBRUSH,
            lpszClassName: class.as_ptr(),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hIcon: null_mut(),
            lpszMenuName: null_mut(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1120,
            540,
            null_mut(),
            null_mut(),
            instance,
            null_mut::<c_void>(),
        );
        ShowWindow(hwnd, SW_SHOW);
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
