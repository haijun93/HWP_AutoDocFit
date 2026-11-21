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

struct State {
    files: Vec<PathBuf>,
    target: usize,
}
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
fn draw(hdc: HDC, w: i32, h: i32, st: &State) -> usize {
    unsafe {
        let bg = CreateSolidBrush(color(248, 249, 251));
        let r = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        FillRect(hdc, &r, bg);
        DeleteObject(bg);
        text(hdc, 28, 24, "몽땅파일변환", color(26, 36, 52));
        text(
            hdc,
            28,
            54,
            "파일을 끌어 놓으세요. 원본 파일은 변경하지 않습니다.",
            color(78, 88, 104),
        );
        let targets = [
            "HWPX",
            "PDF",
            "PDF 하나로 합치기",
            "PNG",
            "JPG",
            "TXT",
            "MD",
        ];
        text(hdc, 28, 102, "변환 대상", color(45, 55, 70));
        for (i, t) in targets.iter().enumerate() {
            let x = 28 + i as i32 * 142;
            let selected = i == st.target;
            let b = CreateSolidBrush(if selected {
                color(40, 105, 190)
            } else {
                color(230, 234, 240)
            });
            let rr = RECT {
                left: x,
                top: 128,
                right: x + 130,
                bottom: 164,
            };
            FillRect(hdc, &rr, b);
            DeleteObject(b);
            text(
                hdc,
                x + 12,
                139,
                t,
                if selected {
                    color(255, 255, 255)
                } else {
                    color(45, 55, 70)
                },
            );
        }
        text(
            hdc,
            28,
            198,
            &format!("변환할 파일  ·  {}개", st.files.len()),
            color(45, 55, 70),
        );
        let border = CreateSolidBrush(color(255, 255, 255));
        let rr = RECT {
            left: 28,
            top: 222,
            right: w - 28,
            bottom: h - 104,
        };
        FillRect(hdc, &rr, border);
        DeleteObject(border);
        if st.files.is_empty() {
            text(hdc, 48, 250, "여기에 파일을 놓으세요", color(130, 140, 154));
        } else {
            for (i, p) in st.files.iter().take(12).enumerate() {
                text(
                    hdc,
                    48,
                    244 + i as i32 * 28,
                    &format!("☐  {}", p.file_name().unwrap_or_default().to_string_lossy()),
                    color(45, 55, 70),
                );
            }
        }
        let b = CreateSolidBrush(color(40, 105, 190));
        let rr = RECT {
            left: w - 220,
            top: h - 70,
            right: w - 28,
            bottom: h - 24,
        };
        FillRect(hdc, &rr, b);
        DeleteObject(b);
        text(hdc, w - 194, h - 55, "변환 시작", color(255, 255, 255));
        st.files.len()
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
                for i in 0..n {
                    let len = DragQueryFileW(drop, i, std::ptr::null_mut(), 0);
                    let mut buf = vec![0u16; len as usize + 1];
                    DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32);
                    let path = PathBuf::from(OsString::from_wide(&buf[..len as usize]));
                    if path.is_file() && !state(hwnd).files.contains(&path) {
                        state(hwnd).files.push(path)
                    }
                }
                DragFinish(drop);
                InvalidateRect(hwnd, null_mut(), 1);
                0
            }
            WM_LBUTTONUP => {
                let x = (lp & 0xffff) as i16 as i32;
                let y = ((lp >> 16) & 0xffff) as i16 as i32;
                let s = state(hwnd);
                if (128..=164).contains(&y) && (28..=28 + 7 * 142).contains(&x) {
                    s.target = (((x - 28) / 142).max(0) as usize).min(6);
                } else if y > 0 && y > 500 {
                    if s.files.is_empty() {
                        let p = wide("먼저 파일을 끌어다 놓으세요.");
                        let t = wide("몽땅파일변환");
                        MessageBoxW(hwnd, p.as_ptr(), t.as_ptr(), 0x40);
                    } else {
                        let formats = ["hwpx", "pdf", "pdfm", "png", "jpg", "txt", "md"];
                        let exe = std::env::var_os("DOCFIT_APP_EXE")
                            .map(PathBuf::from)
                            .or_else(|| {
                                std::env::current_exe()
                                    .ok()
                                    .and_then(|p| p.parent().map(|d| d.join("HWP_AutoDocFit.exe")))
                            });
                        if let Some(exe) = exe.filter(|p| p.is_file()) {
                            let mut cmd = std::process::Command::new(exe);
                            cmd.arg("convert");
                            for f in &s.files {
                                cmd.arg(f);
                            }
                            cmd.args(["--to", formats[s.target], "--notify"]);
                            if cmd.spawn().is_err() {
                                let p = wide("기존 변환 엔진을 시작하지 못했습니다.");
                                let t = wide("몽땅파일변환");
                                MessageBoxW(hwnd, p.as_ptr(), t.as_ptr(), 0x10);
                            }
                        } else {
                            let p = wide(
                                "기존 앱 실행 파일을 찾지 못했습니다. DOCFIT_APP_EXE를 지정해 주세요.",
                            );
                            let t = wide("몽땅파일변환");
                            MessageBoxW(hwnd, p.as_ptr(), t.as_ptr(), 0x10);
                        }
                    }
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
        let st = Box::into_raw(Box::new(State {
            files: Vec::new(),
            target: 0,
        }));
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let st = State {
        files: vec![
            "보고서.hwp".into(),
            "회의자료.hwpx".into(),
            "스캔본.pdf".into(),
        ],
        target: 1,
    };
    let shot = grid_core::gdi::render(1050, 650, |hdc| draw(hdc, 1050, 650, &st))?;
    grid_core::png::write(path, shot.width, shot.height, &shot.bgra)?;
    Ok(path.to_path_buf())
}
