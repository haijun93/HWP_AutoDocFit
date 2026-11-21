use crate::logic::{self, Hit, Layout, Model, TARGETS};
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::*,
        Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
        WindowsAndMessaging::*,
    },
};

type State = Model;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn state(hwnd: HWND) -> &'static mut State {
    unsafe { &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State) }
}
fn text(hdc: HDC, x: i32, y: i32, s: &str, color: COLORREF) {
    let v: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        SetTextColor(hdc, color);
        SetBkMode(hdc, TRANSPARENT as i32);
        TextOutW(hdc, x, y, v.as_ptr(), v.len() as i32);
    }
}
fn color(r: u8, g: u8, b: u8) -> COLORREF {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}
fn fill(hdc: HDC, r: logic::Rect, c: COLORREF) {
    unsafe {
        let b = CreateSolidBrush(c);
        let rr = RECT {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        };
        FillRect(hdc, &rr, b);
        DeleteObject(b);
    }
}
fn draw(hdc: HDC, w: i32, h: i32, st: &State) {
    let lay = Layout::new(w, h);
    fill(
        hdc,
        logic::Rect {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        },
        color(248, 249, 251),
    );
    text(hdc, 28, 24, "몽땅파일변환", color(26, 36, 52));
    text(
        hdc,
        28,
        54,
        "파일을 끌어 놓으세요. 원본 파일은 변경하지 않습니다.",
        color(78, 88, 104),
    );
    text(hdc, 28, 102, "변환 대상", color(45, 55, 70));
    for (i, (label, _)) in TARGETS.iter().enumerate() {
        let r = lay.target_rect(i);
        let selected = i == st.target;
        fill(
            hdc,
            r,
            if selected {
                color(40, 105, 190)
            } else {
                color(230, 234, 240)
            },
        );
        text(
            hdc,
            r.left + 12,
            r.top + 11,
            label,
            if selected {
                color(255, 255, 255)
            } else {
                color(45, 55, 70)
            },
        );
    }
    text(hdc, 28, 198, &st.header(), color(45, 55, 70));
    fill(hdc, lay.list_rect(), color(255, 255, 255));
    if st.files.is_empty() {
        text(hdc, 48, 250, "여기에 파일을 놓으세요", color(130, 140, 154));
    } else {
        let (rows, more) = st.rows(lay.visible_rows());
        for (i, row) in rows.iter().enumerate() {
            text(hdc, 48, logic::ROW_TOP + i as i32 * logic::ROW_STEP, row, color(45, 55, 70));
        }
        if more > 0 {
            text(
                hdc,
                48,
                logic::ROW_TOP + rows.len() as i32 * logic::ROW_STEP,
                &format!("외 {more}개"),
                color(130, 140, 154),
            );
        }
    }
    let s = lay.start_rect();
    fill(hdc, s, color(40, 105, 190));
    text(hdc, s.left + 26, s.top + 15, "변환 시작", color(255, 255, 255));
}
fn message(hwnd: HWND, body: &str, flags: u32) {
    let p = wide(body);
    let t = wide("몽땅파일변환");
    unsafe {
        MessageBoxW(hwnd, p.as_ptr(), t.as_ptr(), flags);
    }
}
fn start_conversion(hwnd: HWND, s: &State) {
    let Some(args) = s.engine_args() else {
        message(hwnd, "먼저 파일을 끌어다 놓으세요.", 0x40);
        return;
    };
    let exe = logic::resolve_engine(
        std::env::var_os("DOCFIT_APP_EXE").map(PathBuf::from),
        std::env::current_exe().ok(),
        |p| p.is_file(),
    );
    match exe {
        Some(exe) => {
            if std::process::Command::new(exe).args(args).spawn().is_err() {
                message(hwnd, "기존 변환 엔진을 시작하지 못했습니다.", 0x10);
            }
        }
        None => message(
            hwnd,
            "기존 앱 실행 파일을 찾지 못했습니다. DOCFIT_APP_EXE를 지정해 주세요.",
            0x10,
        ),
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let cs = &*(lp as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                1
            }
            WM_CREATE => {
                DragAcceptFiles(hwnd, 1);
                0
            }
            WM_DROPFILES => {
                let drop = wp as HDROP;
                let n = DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0);
                let mut paths = Vec::new();
                for i in 0..n {
                    let len = DragQueryFileW(drop, i, std::ptr::null_mut(), 0);
                    let mut buf = vec![0u16; len as usize + 1];
                    DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32);
                    paths.push(PathBuf::from(OsString::from_wide(&buf[..len as usize])));
                }
                DragFinish(drop);
                state(hwnd).add_dropped(paths, |p| p.is_file());
                InvalidateRect(hwnd, null_mut(), 1);
                0
            }
            WM_LBUTTONUP => {
                let x = (lp & 0xffff) as i16 as i32;
                let y = ((lp >> 16) & 0xffff) as i16 as i32;
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                match Layout::new(r.right, r.bottom).hit(x, y) {
                    Hit::Target(i) => state(hwnd).select(i),
                    Hit::Start => start_conversion(hwnd, state(hwnd)),
                    Hit::None => {}
                }
                InvalidateRect(hwnd, null_mut(), 1);
                0
            }
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                draw(hdc, r.right, r.bottom, state(hwnd));
                EndPaint(hwnd, &ps);
                0
            }
            WM_DESTROY => {
                let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
                if !p.is_null() {
                    drop(Box::from_raw(p));
                }
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
pub fn run() -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(std::ptr::null());
        let cls = wide("ConvertWinStage1");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            hInstance: inst,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszClassName: cls.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let st = Box::into_raw(Box::new(State::default()));
        let title = wide("몽땅파일변환");
        let hwnd = CreateWindowExW(
            0,
            cls.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1050,
            650,
            null_mut(),
            null_mut(),
            inst,
            st.cast(),
        );
        if hwnd.is_null() {
            drop(Box::from_raw(st));
            return Err("창을 만들지 못했습니다".into());
        }
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, null_mut(), 0, 0) > 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
        Ok(())
    }
}

pub fn capture(path: &std::path::Path) -> Result<PathBuf, String> {
    capture_model(path, &logic::sample_model())
}

pub fn capture_model(path: &std::path::Path, st: &Model) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let shot = grid_core::gdi::render(1050, 650, |hdc| {
        draw(hdc, 1050, 650, st);
        st.files.len()
    })?;
    grid_core::png::write(path, shot.width, shot.height, &shot.bgra)?;
    Ok(path.to_path_buf())
}
