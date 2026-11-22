//! Win32 창: 파일 드롭·열기 대화상자·규칙 입력(편집 컨트롤)·미리보기. 디스크의 파일 이름은 바꾸지 않는다.
use rename_win::{Model, RowStatus, invalid_reason, parse_digits, parse_open_buffer};
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::Path, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        LibraryLoader::GetModuleHandleW,
        Memory::{GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalUnlock},
    },
    UI::{
        Controls::Dialogs::*,
        HiDpi::*,
        Shell::{DROPFILES, DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
        WindowsAndMessaging::*,
    },
};

const TITLE: &str = "파일명 일괄변경 · 미리보기";
const ID_FIELD0: i32 = 101; // 접두사, 접미사, 자릿수, 찾을 말, 바꿀 말
const ID_OPEN: i32 = 201;
const ID_CLEAR: i32 = 202;
const EN_CHANGE: u32 = 0x0300;
const FIELD_LABELS: [&str; 5] = ["접두사", "접미사", "순번 자릿수", "찾을 말", "바꿀 말"];
/// 96dpi 기준 (x, y, w) — 라벨은 x-라벨폭 위치에 그린다.
const FIELD_POS: [(i32, i32, i32); 5] = [
    (100, 80, 150),
    (350, 80, 150),
    (610, 80, 48),
    (100, 118, 150),
    (350, 118, 150),
];
const LABEL_X: [i32; 5] = [24, 280, 500, 24, 280];
const FIELD_H: i32 = 26;
const LIST_TOP: i32 = 208;
const ROW_H: i32 = 26;
const BASE_W: i32 = 1120;
const BASE_H: i32 = 600;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}
fn sc(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

struct Ui {
    model: Model,
    dpi: u32,
    font: HFONT,
    edits: [HWND; 5],
    note: String,
    ready: bool,
}

unsafe fn ui(hwnd: HWND) -> &'static mut Ui {
    unsafe { &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui) }
}

fn make_font(dpi: u32) -> HFONT {
    let face = wide(grid_core::gdi::FACE);
    unsafe {
        CreateFontW(
            -sc(15, dpi),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            face.as_ptr(),
        )
    }
}

fn fill(hdc: HDC, r: RECT, c: COLORREF) {
    unsafe {
        let b = CreateSolidBrush(c);
        FillRect(hdc, &r, b);
        DeleteObject(b);
    }
}
fn put(hdc: HDC, dpi: u32, x: i32, y: i32, w: i32, s: &str, c: COLORREF) {
    let v: Vec<u16> = s.encode_utf16().collect();
    if v.is_empty() {
        return;
    }
    let mut r = RECT {
        left: sc(x, dpi),
        top: sc(y, dpi),
        right: sc(x + w, dpi),
        bottom: sc(y + 24, dpi),
    };
    unsafe {
        SetTextColor(hdc, c);
        SetBkMode(hdc, TRANSPARENT as i32);
        DrawTextW(
            hdc,
            v.as_ptr(),
            v.len() as i32,
            &mut r,
            DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS | DT_VCENTER,
        );
    }
}
fn status_label(m: &Model, i: usize) -> (String, COLORREF) {
    let r = &m.rows[i];
    match r.status {
        RowStatus::Changed => ("변경 예정".into(), rgb(30, 110, 60)),
        RowStatus::Unchanged => ("변화 없음".into(), rgb(130, 140, 154)),
        RowStatus::Conflict => ("충돌: 같은 이름이 생김".into(), rgb(190, 40, 40)),
        RowStatus::Invalid => (
            format!("이름 오류: {}", invalid_reason(&r.proposed).unwrap_or("")),
            rgb(200, 110, 0),
        ),
    }
}

/// 화면 전체를 그린다. `fake_fields`가 있으면(캡처) 편집 칸을 직접 그린다.
fn draw(hdc: HDC, w: i32, h: i32, u: &Ui, fake_fields: Option<[String; 5]>) {
    let dpi = u.dpi;
    let (bw, bh) = (w * 96 / dpi as i32, h * 96 / dpi as i32);
    fill(
        hdc,
        RECT { left: 0, top: 0, right: w, bottom: h },
        rgb(248, 249, 251),
    );
    unsafe { SelectObject(hdc, u.font) };
    let ink = rgb(45, 55, 70);
    let muted = rgb(100, 110, 125);
    put(hdc, dpi, 24, 14, 700, TITLE, rgb(26, 36, 52));
    put(
        hdc,
        dpi,
        24,
        42,
        bw - 48,
        "미리보기 전용입니다. 이 창은 디스크의 파일 이름을 바꾸지 않습니다.",
        muted,
    );
    for (i, label) in FIELD_LABELS.iter().enumerate() {
        put(hdc, dpi, LABEL_X[i], FIELD_POS[i].1 + 2, 90, label, ink);
        if let Some(f) = &fake_fields {
            let (x, y, fw) = FIELD_POS[i];
            let r = RECT {
                left: sc(x, dpi),
                top: sc(y, dpi),
                right: sc(x + fw, dpi),
                bottom: sc(y + FIELD_H, dpi),
            };
            fill(hdc, r, rgb(120, 130, 145));
            fill(
                hdc,
                RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 },
                rgb(255, 255, 255),
            );
            put(hdc, dpi, x + 5, y + 1, fw - 10, &f[i], ink);
        }
    }
    if fake_fields.is_some() {
        for (x, label) in [(700, "파일 열기…"), (830, "목록 비우기")] {
            let r = RECT {
                left: sc(x, dpi),
                top: sc(80, dpi),
                right: sc(x + 120, dpi),
                bottom: sc(80 + FIELD_H, dpi),
            };
            fill(hdc, r, rgb(225, 228, 234));
            put(hdc, dpi, x + 26, 81, 100, label, ink);
        }
    }
    put(
        hdc,
        dpi,
        530,
        120,
        bw - 550,
        "자릿수 0 = 순번 없음 · 찾아 바꾸기는 확장자를 제외한 이름에 적용",
        muted,
    );
    put(hdc, dpi, 24, 156, bw - 48, &u.model.summary(), ink);
    // 목록
    let col2 = 24 + (bw - 48 - 220) / 2;
    let stat = bw - 24 - 210;
    let hr = RECT { left: sc(16, dpi), top: sc(LIST_TOP - 24, dpi), right: sc(bw - 16, dpi), bottom: sc(LIST_TOP, dpi) };
    fill(hdc, hr, rgb(230, 234, 240));
    put(hdc, dpi, 24, LIST_TOP - 24, 300, "원래 이름", ink);
    put(hdc, dpi, col2, LIST_TOP - 24, 300, "미리보기 이름", ink);
    put(hdc, dpi, stat, LIST_TOP - 24, 200, "상태", ink);
    let list_bottom = bh - 44;
    fill(
        hdc,
        RECT { left: sc(16, dpi), top: sc(LIST_TOP, dpi), right: sc(bw - 16, dpi), bottom: sc(list_bottom, dpi) },
        rgb(255, 255, 255),
    );
    let m = &u.model;
    if m.files.is_empty() {
        put(hdc, dpi, 40, LIST_TOP + 40, 600, "여기에 파일을 놓으세요", rgb(130, 140, 154));
    }
    let visible = ((list_bottom - LIST_TOP) / ROW_H).max(0) as usize;
    for (k, i) in (m.scroll..m.rows.len()).take(visible).enumerate() {
        let y = LIST_TOP + k as i32 * ROW_H;
        let (label, c) = status_label(m, i);
        let row_ink = if m.rows[i].status == RowStatus::Unchanged { muted } else { ink };
        put(hdc, dpi, 24, y + 1, col2 - 36, &m.rows[i].original, row_ink);
        put(hdc, dpi, col2, y + 1, stat - col2 - 12, &m.rows[i].proposed, row_ink);
        put(hdc, dpi, stat, y + 1, 200, &label, c);
    }
    if !u.note.is_empty() {
        put(hdc, dpi, 24, bh - 34, bw - 48, &u.note, muted);
    }
}

fn read_text(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h);
        let mut buf = vec![0u16; n as usize + 1];
        let got = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..got.max(0) as usize])
    }
}

fn read_rules(u: &mut Ui) {
    let t: Vec<String> = u.edits.iter().map(|&h| read_text(h)).collect();
    u.model.rules.prefix = t[0].clone();
    u.model.rules.suffix = t[1].clone();
    u.model.rules.digits = parse_digits(&t[2]);
    u.model.rules.find = t[3].clone();
    u.model.rules.replace = t[4].clone();
    u.model.recompute();
}

fn fs_is_file(p: &str) -> bool {
    Path::new(p).is_file()
}
fn fs_list_dir(dir: &str) -> Vec<String> {
    let d = if dir.is_empty() { "." } else { dir };
    std::fs::read_dir(d)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn add_paths(hwnd: HWND, paths: Vec<String>) {
    let u = unsafe { ui(hwnd) };
    let total = paths.len();
    let n = u.model.add_paths(&paths, fs_is_file, fs_list_dir);
    u.note = if total > n {
        format!("{n}개 추가 · {}개 건너뜀(폴더이거나 이미 목록에 있음)", total - n)
    } else {
        format!("{n}개 추가")
    };
    unsafe { InvalidateRect(hwnd, null_mut(), 0) };
}

fn open_dialog(hwnd: HWND) {
    let mut buf = vec![0u16; 65536];
    let filter: Vec<u16> = "모든 파일\0*.*\0\0".encode_utf16().collect();
    let title = wide("이름을 바꿀 파일 고르기 (이름은 바뀌지 않습니다)");
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_ALLOWMULTISELECT
        | OFN_EXPLORER
        | OFN_FILEMUSTEXIST
        | OFN_PATHMUSTEXIST
        | OFN_HIDEREADONLY;
    if unsafe { GetOpenFileNameW(&mut ofn) } != 0 {
        add_paths(hwnd, parse_open_buffer(&buf));
    }
}

fn layout_children(hwnd: HWND) {
    let u = unsafe { ui(hwnd) };
    unsafe {
        for (i, &h) in u.edits.iter().enumerate() {
            let (x, y, w) = FIELD_POS[i];
            MoveWindow(h, sc(x, u.dpi), sc(y, u.dpi), sc(w, u.dpi), sc(FIELD_H, u.dpi), 1);
            SendMessageW(h, WM_SETFONT, u.font as usize, 1);
        }
        for (id, x) in [(ID_OPEN, 700), (ID_CLEAR, 830)] {
            let b = GetDlgItem(hwnd, id);
            MoveWindow(b, sc(x, u.dpi), sc(80, u.dpi), sc(120, u.dpi), sc(FIELD_H, u.dpi), 1);
            SendMessageW(b, WM_SETFONT, u.font as usize, 1);
        }
    }
}

fn drop_paths(drop: HDROP) -> Vec<String> {
    unsafe {
        let n = DragQueryFileW(drop, u32::MAX, null_mut(), 0);
        let mut v = Vec::new();
        for i in 0..n {
            let len = DragQueryFileW(drop, i, null_mut(), 0);
            let mut buf = vec![0u16; len as usize + 1];
            DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32);
            v.push(OsString::from_wide(&buf[..len as usize]).to_string_lossy().into_owned());
        }
        DragFinish(drop);
        v
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let cs = &*(lp as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_CREATE => {
                let u = ui(hwnd);
                u.dpi = GetDpiForWindow(hwnd).max(96);
                u.font = make_font(u.dpi);
                let inst = GetModuleHandleW(null_mut());
                let edit = wide("EDIT");
                for i in 0..5 {
                    let init = if i == 2 { wide("3") } else { wide("") };
                    let mut style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32;
                    if i == 2 {
                        style |= ES_NUMBER as u32;
                    }
                    u.edits[i] = CreateWindowExW(
                        WS_EX_CLIENTEDGE,
                        edit.as_ptr(),
                        init.as_ptr(),
                        style,
                        0,
                        0,
                        10,
                        10,
                        hwnd,
                        (ID_FIELD0 + i as i32) as usize as HMENU,
                        inst,
                        null_mut(),
                    );
                }
                let button = wide("BUTTON");
                for (id, label) in [(ID_OPEN, "파일 열기…"), (ID_CLEAR, "목록 비우기")] {
                    let t = wide(label);
                    CreateWindowExW(
                        0,
                        button.as_ptr(),
                        t.as_ptr(),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                        0,
                        0,
                        10,
                        10,
                        hwnd,
                        id as usize as HMENU,
                        inst,
                        null_mut(),
                    );
                }
                layout_children(hwnd);
                DragAcceptFiles(hwnd, 1);
                u.ready = true;
                u.model.recompute();
                0
            }
            WM_DROPFILES => {
                add_paths(hwnd, drop_paths(wp as HDROP));
                0
            }
            WM_COMMAND => {
                let id = (wp & 0xffff) as i32;
                let code = ((wp >> 16) & 0xffff) as u32;
                let u = ui(hwnd);
                if u.ready && code == EN_CHANGE && (ID_FIELD0..ID_FIELD0 + 5).contains(&id) {
                    read_rules(u);
                    InvalidateRect(hwnd, null_mut(), 0);
                } else if id == ID_OPEN && code == BN_CLICKED {
                    open_dialog(hwnd);
                } else if id == ID_CLEAR && code == BN_CLICKED {
                    u.model.clear();
                    u.note = "목록을 비웠습니다".into();
                    InvalidateRect(hwnd, null_mut(), 0);
                }
                0
            }
            WM_MOUSEWHEEL => {
                let u = ui(hwnd);
                let delta = ((wp >> 16) & 0xffff) as i16 as i32;
                let step = (delta / 120 * 3) as isize;
                let max = u.model.rows.len().saturating_sub(1) as isize;
                u.model.scroll = (u.model.scroll as isize - step).clamp(0, max) as usize;
                InvalidateRect(hwnd, null_mut(), 0);
                0
            }
            WM_DPICHANGED => {
                let u = ui(hwnd);
                u.dpi = (wp & 0xffff) as u32;
                DeleteObject(u.font);
                u.font = make_font(u.dpi);
                let r = &*(lp as *const RECT);
                SetWindowPos(hwnd, null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                layout_children(hwnd);
                InvalidateRect(hwnd, null_mut(), 1);
                0
            }
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let mem = CreateCompatibleDC(hdc);
                let bm = CreateCompatibleBitmap(hdc, r.right.max(1), r.bottom.max(1));
                let old = SelectObject(mem, bm);
                draw(mem, r.right, r.bottom, ui(hwnd), None);
                BitBlt(hdc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
                SelectObject(mem, old);
                DeleteObject(bm);
                DeleteDC(mem);
                EndPaint(hwnd, &ps);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn create_window(visible: bool) -> Result<(HWND, *mut Ui), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(null_mut());
        let cls = wide("DocFitRenameWin2");
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
        let st = Box::into_raw(Box::new(Ui {
            model: Model::default(),
            dpi: 96,
            font: null_mut(),
            edits: [null_mut(); 5],
            note: String::new(),
            ready: false,
        }));
        let title = wide(TITLE);
        let mut style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
        if visible {
            style |= WS_VISIBLE;
        }
        let hwnd = CreateWindowExW(
            0,
            cls.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            BASE_W,
            BASE_H,
            null_mut(),
            null_mut(),
            inst,
            st.cast(),
        );
        if hwnd.is_null() {
            drop(Box::from_raw(st));
            return Err("창을 만들지 못했습니다".into());
        }
        Ok((hwnd, st))
    }
}

pub fn run() -> Result<(), String> {
    let (hwnd, st) = create_window(true)?;
    unsafe {
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &mut m) == 0 {
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
        drop(Box::from_raw(st));
    }
    Ok(())
}

/// 예시 목록과 규칙으로 화면을 PNG로 저장한다(헤드리스). 편집 칸·단추는 직접 그린 모양이다.
pub fn capture(path: &Path, dpi: u32) -> Result<std::path::PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut u = Ui {
        model: Model::default(),
        dpi,
        font: make_font(dpi),
        edits: [null_mut(); 5],
        note: "6개 추가".into(),
        ready: true,
    };
    u.model.rules.prefix = "정리_".into();
    u.model.rules.digits = 3;
    u.model.rules.find = "압축".into();
    u.model.rules.replace = "압:축".into();
    let names = [
        "6C1D56A5-12B9-4F7D-9846-BA1255C43404.JPG",
        "현장 사진 2.jpg",
        "보고서(자간조정).hwpx",
        "압축 자료.zip",
        "이름 중복 확인.txt",
        "정리_이름 중복 확인_005.txt.bak",
    ];
    let paths: Vec<String> = names.iter().map(|n| format!("D:\\샘플\\{n}")).collect();
    let existing = |_: &str| {
        vec!["정리_현장 사진 2_002.jpg".to_string(), "정리_보고서(자간조정)_003.hwpx".to_string()]
    };
    u.model.add_paths(&paths, |_| true, existing);
    u.model.recompute();
    let (w, h) = (sc(BASE_W, dpi), sc(BASE_H, dpi));
    let fields = ["정리_".into(), String::new(), "3".into(), "압축".into(), "압:축".into()];
    let shot = grid_core::gdi::render(w, h, |hdc| {
        draw(hdc, w, h, &u, Some(fields));
        u.model.files.len()
    })?;
    grid_core::png::write(path, shot.width, shot.height, &shot.bgra)?;
    Ok(path.to_path_buf())
}

fn dir_snapshot(dir: &Path) -> Vec<String> {
    let mut v = fs_list_dir(&dir.to_string_lossy());
    v.sort();
    v
}
fn send_drop(hwnd: HWND, paths: &[String]) {
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.encode_utf16());
        list.push(0);
    }
    list.push(0);
    let head = std::mem::size_of::<DROPFILES>();
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, head + list.len() * 2);
        let p = GlobalLock(h) as *mut u8;
        let df = p as *mut DROPFILES;
        (*df).pFiles = head as u32;
        (*df).fWide = 1;
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, p.add(head), list.len() * 2);
        GlobalUnlock(h);
        SendMessageW(hwnd, WM_DROPFILES, h as usize, 0);
    }
}
fn set_field(hwnd: HWND, i: usize, text: &str) {
    let t = wide(text);
    unsafe { SetWindowTextW(ui(hwnd).edits[i], t.as_ptr()) };
}

/// 실제 창을 숨김 상태로 만들어 드롭·편집 컨트롤 입력이 미리보기에 반영되는지 확인한다.
/// 임시 폴더의 파일을 쓰며 이름 변경은 하지 않는다. 결과는 한 줄씩 `out`에 적는다.
pub fn selftest(out: &Path) -> Result<bool, String> {
    let dir = std::env::temp_dir().join(format!("renamewin4-selftest-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("하위")).map_err(|e| e.to_string())?;
    for n in ["a.txt", "b.txt", "정리_a_001.txt", "보고서.hwpx"] {
        std::fs::write(dir.join(n), b"x").map_err(|e| e.to_string())?;
    }
    let before = dir_snapshot(&dir);
    let (hwnd, st) = create_window(false)?;
    let mut log = Vec::new();
    let mut ok = true;
    let mut check = |name: &str, cond: bool| {
        ok &= cond;
        log.push(format!("{} {name}", if cond { "PASS" } else { "FAIL" }));
    };
    let p = |n: &str| dir.join(n).to_string_lossy().into_owned();
    send_drop(hwnd, &[p("a.txt"), p("b.txt"), p("하위"), p("a.txt"), p("보고서.hwpx")]);
    let u = unsafe { ui(hwnd) };
    check("드롭: 파일 3개만 추가(폴더·중복 제외)", u.model.files.len() == 3);
    check("드롭 직후 기본 규칙(순번 3자리)", u.model.rows[0].proposed == "a_001.txt");
    set_field(hwnd, 0, "정리_");
    let u = unsafe { ui(hwnd) };
    check("접두사 편집 → 규칙 반영", u.model.rules.prefix == "정리_");
    check("접두사 편집 → 미리보기 이름 갱신", u.model.rows[1].proposed == "정리_b_002.txt");
    check("기존 파일과 겹치면 충돌", u.model.rows[0].proposed == "정리_a_001.txt" && u.model.rows[0].status == RowStatus::Conflict);
    check("겹치지 않으면 변경 예정", u.model.rows[1].status == RowStatus::Changed);
    set_field(hwnd, 2, "0");
    set_field(hwnd, 3, "보고");
    set_field(hwnd, 4, "결과");
    let u = unsafe { ui(hwnd) };
    check("자릿수 0 → 순번 없음", u.model.rows[0].proposed == "정리_a.txt");
    check("찾아 바꾸기(한글) 적용", u.model.rows[2].proposed == "정리_결과서.hwpx");
    check("충돌 해소 후 변경 예정", u.model.rows[0].status == RowStatus::Changed);
    set_field(hwnd, 0, "a?");
    let u = unsafe { ui(hwnd) };
    check("금지 문자 → 이름 오류", u.model.rows.iter().all(|r| r.status == RowStatus::Invalid));
    unsafe { SendMessageW(hwnd, WM_COMMAND, ID_CLEAR as usize, 0) };
    let u = unsafe { ui(hwnd) };
    check("비우기 단추 → 목록 비움", u.model.files.is_empty());
    check("디스크의 파일 이름은 그대로", dir_snapshot(&dir) == before);
    unsafe {
        DestroyWindow(hwnd);
        drop(Box::from_raw(st));
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::write(out, log.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(ok)
}
