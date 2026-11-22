#![allow(unsafe_op_in_unsafe_fn)]
use std::{
    ptr,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Gdi::*, GdiPlus::{FillModeAlternate, GdipAddPathArc, GdipClosePathFigure, GdipCreateFromHDC, GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath, GdipDeletePen, GdipDrawEllipse, GdipDrawPath, GdipFillEllipse, GdipFillPath, GdipSetPixelOffsetMode, GdipSetSmoothingMode, GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, GdiplusStartupOutput, GpBrush, GpGraphics, GpPath, GpPen, GpSolidFill, PixelOffsetModeHalf, SmoothingModeAntiAlias, UnitPixel}},
    System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetTickCount64, Threading::*},
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook},
        HiDpi::*,
        Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture},
        WindowsAndMessaging::{EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, *},
    },
};
static STATE: OnceLock<Mutex<bool>> = OnceLock::new();
static HWND_VALUE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static LAST_MOUSE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
static HOVER_TAB: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static HOVER_TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static KEY_FOCUS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static WINDOW_STATES: OnceLock<Mutex<std::collections::HashMap<u8, dock::WindowState>>> = OnceLock::new();
/// 사이드바와 같은 Material Icons 글리프만 담은 서브셋 글꼴(Apache-2.0, dock/assets/MaterialIcons-LICENSE.txt).
static ICON_FONT: &[u8] = include_bytes!("../assets/MaterialIconsDock.otf");
const ICON_FONT_FACE: &str = "Material Icons";
static ICON_FONT_OK: OnceLock<bool> = OnceLock::new();
/// 프로세스 전용 등록(AddFontMemResourceEx). 실패하면 기존 벡터 아이콘으로 그린다.
fn icon_font_ready() -> bool {
    *ICON_FONT_OK.get_or_init(|| unsafe {
        let mut n = 0u32;
        let h = AddFontMemResourceEx(ICON_FONT.as_ptr() as *const _, ICON_FONT.len() as u32, ptr::null(), &mut n);
        !h.is_null() && n > 0
    })
}
unsafe fn draw_glyph(dc: HDC, glyph: char, ox: i32, oy: i32, size: i32, fg: u32) -> bool {
    let font = CreateFontW(-size, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_TT_ONLY_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, (DEFAULT_PITCH | FF_DONTCARE) as u32, wide(ICON_FONT_FACE).as_ptr());
    if font.is_null() { return false; }
    let old = SelectObject(dc, font);
    let mut buf = [0u16; 2]; let text = glyph.encode_utf16(&mut buf);
    let (prev_mode, prev_color, prev_align) = (SetBkMode(dc, TRANSPARENT as i32), SetTextColor(dc, fg), SetTextAlign(dc, TA_LEFT | TA_TOP));
    let mut tm: TEXTMETRICW = std::mem::zeroed(); GetTextMetricsW(dc, &mut tm);
    let mut sz: SIZE = std::mem::zeroed(); GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut sz);
    let x = ox + (size - sz.cx) / 2; let y = oy + (size - tm.tmHeight) / 2;
    let ok = ExtTextOutW(dc, x, y, 0, ptr::null(), text.as_ptr(), text.len() as u32, ptr::null()) != 0;
    SetTextAlign(dc, prev_align); SetTextColor(dc, prev_color); SetBkMode(dc, prev_mode);
    SelectObject(dc, old); DeleteObject(font);
    ok
}
/// GDI+ 안티앨리어싱 도형(알약·링·점). WinRust 방식을 참고해 독자 구현: SmoothingModeAntiAlias + PixelOffsetModeHalf, 둥근 사각형은 GdipAddPathArc 경로.
/// 시작 실패나 그래픽스 생성 실패 시 `Aa::new`가 None을 돌려주며 호출부가 기존 GDI 도형으로 폴백한다.
static GDIP_TOKEN: OnceLock<usize> = OnceLock::new();
fn gdip_token() -> usize {
    *GDIP_TOKEN.get_or_init(|| unsafe {
        let input = GdiplusStartupInput { GdiplusVersion: 1, DebugEventCallback: 0, SuppressBackgroundThread: 0, SuppressExternalCodecs: 0 };
        let mut token = 0usize;
        let mut out: GdiplusStartupOutput = std::mem::zeroed();
        if GdiplusStartup(&mut token, &input, &mut out) == 0 { token } else { 0 }
    })
}
/// 프로세스 종료 직전 한 번 호출(시작한 적 없거나 실패했으면 아무 일도 하지 않음).
fn gdip_shutdown() { if let Some(&t) = GDIP_TOKEN.get() { if t != 0 { unsafe { GdiplusShutdown(t) } } } }
/// COLORREF(0x00BBGGRR) → 불투명 ARGB.
fn argb(c: u32) -> u32 { 0xFF00_0000 | ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF) }
struct Aa(*mut GpGraphics);
impl Aa {
    unsafe fn new(dc: HDC) -> Option<Aa> {
        if gdip_token() == 0 { return None; }
        let mut g: *mut GpGraphics = ptr::null_mut();
        if GdipCreateFromHDC(dc, &mut g) != 0 || g.is_null() { return None; }
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        GdipSetPixelOffsetMode(g, PixelOffsetModeHalf);
        Some(Aa(g))
    }
    /// 면 색(fill)과 1픽셀 테두리(line)를 가진 둥근 사각형. 좌표는 GDI RoundRect와 같은 정수 경계(오른쪽·아래 제외)를 받아 선이 픽셀 가운데에 오도록 0.5 안쪽으로 맞춘다.
    unsafe fn round_rect(&self, l: i32, t: i32, r: i32, b: i32, diameter: i32, fill: u32, line: u32) -> bool {
        let (x, y, w, h) = (l as f32 + 0.5, t as f32 + 0.5, (r - l - 1) as f32, (b - t - 1) as f32);
        let d = (diameter as f32).min(w).min(h);
        let mut path: *mut GpPath = ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) != 0 || path.is_null() { return false; }
        GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
        GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
        GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
        GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
        GdipClosePathFigure(path);
        let (mut brush, mut pen): (*mut GpSolidFill, *mut GpPen) = (ptr::null_mut(), ptr::null_mut());
        let ok = GdipCreateSolidFill(argb(fill), &mut brush) == 0 && GdipCreatePen1(argb(line), 1.0, UnitPixel, &mut pen) == 0;
        if ok { GdipFillPath(self.0, brush as *mut GpBrush, path); GdipDrawPath(self.0, pen, path); }
        if !brush.is_null() { GdipDeleteBrush(brush as *mut GpBrush); }
        if !pen.is_null() { GdipDeletePen(pen); }
        GdipDeletePath(path);
        ok
    }
    /// 외접 사각형(GDI Ellipse 인자와 같은 정수 경계)의 원. fill이면 면, 아니면 1픽셀 선.
    unsafe fn ellipse(&self, l: i32, t: i32, r: i32, b: i32, color: u32, fill: bool) -> bool {
        let (x, y, w, h) = (l as f32, t as f32, (r - l) as f32, (b - t) as f32);
        if fill {
            let mut brush: *mut GpSolidFill = ptr::null_mut();
            if GdipCreateSolidFill(argb(color), &mut brush) != 0 || brush.is_null() { return false; }
            GdipFillEllipse(self.0, brush as *mut GpBrush, x, y, w, h);
            GdipDeleteBrush(brush as *mut GpBrush);
        } else {
            let mut pen: *mut GpPen = ptr::null_mut();
            if GdipCreatePen1(argb(color), 1.0, UnitPixel, &mut pen) != 0 || pen.is_null() { return false; }
            GdipDrawEllipse(self.0, pen, x + 0.5, y + 0.5, w - 1.0, h - 1.0);
            GdipDeletePen(pen);
        }
        true
    }
}
impl Drop for Aa { fn drop(&mut self) { unsafe { GdipDeleteGraphics(self.0); } } }
const CLASS: &str = "HwpAutoDocFit.Dock";
#[repr(C)]
struct HighContrast { cb_size: u32, flags: u32, scheme: *mut u16 }
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn run() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--capture-dir") { let _ = capture_mode(&args); gdip_shutdown(); return; }
    unsafe {
        let mutex_name = wide("Local\\HwpAutoDocFit.Dock.Singleton");
        let m = CreateMutexW(ptr::null(), 1, mutex_name.as_ptr());
        if m.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        STATE.get_or_init(|| Mutex::new(true));
        let instance = GetModuleHandleW(ptr::null());
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: ptr::null_mut(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TOPMOST,
            class.as_ptr(),
            wide("DocFit Dock").as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }
        HWND_VALUE.store(hwnd as isize, std::sync::atomic::Ordering::Relaxed);
        reposition(hwnd);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        if auto_hide_enabled() { SetTimer(hwnd, 3, 100, None); }
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            ptr::null_mut(),
            Some(foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        thread::spawn(pipe_server);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CloseHandle(m);
        gdip_shutdown();
    }
}
unsafe fn dock_layout_for_monitor(hwnd: HWND, monitor: HMONITOR) -> dock::DockRect {
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(monitor, &mut mi);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let menus = visible_menus();
    let logical_width = ((menus.len() as i32 * 36) + 32).min((mi.rcMonitor.right - mi.rcMonitor.left) * 96 / dpi as i32);
    dock::dock_rect(
        dock::Rect {
            left: mi.rcWork.left,
            top: mi.rcWork.top,
            right: mi.rcWork.right,
            bottom: mi.rcWork.bottom,
        },
        dock::Rect {
            left: mi.rcMonitor.left,
            top: mi.rcMonitor.top,
            right: mi.rcMonitor.right,
            bottom: mi.rcMonitor.bottom,
        },
        dock::Edge::Auto,
        dpi,
        logical_width.max(160),
        56,
        18,
    )
}
unsafe fn reposition(hwnd: HWND) {
    let foreground = GetForegroundWindow();
    let monitor = MonitorFromWindow(if foreground.is_null() { hwnd } else { foreground }, MONITOR_DEFAULTTONEAREST);
    let d = dock_layout_for_monitor(hwnd, monitor);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        d.rect.left,
        d.rect.top,
        d.rect.width(),
        d.rect.height(),
        SWP_NOACTIVATE,
    );
    refresh(hwnd);
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED => {
            reposition(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let key = ((l as u32 as u16 as u64) << 16) | (l as u32 as u16 as u64);
            if LAST_MOUSE.swap(key, std::sync::atomic::Ordering::Relaxed) != key {
                let mut r: RECT = std::mem::zeroed(); GetClientRect(hwnd, &mut r);
                let menus = visible_menus();
                if !menus.is_empty() {
                    let x = (l as u32 as u16) as i32;
                    let spacing = (r.right - 32).max(1) / menus.len() as i32;
                    let idx = ((x - 16) / spacing.max(1)).clamp(0, menus.len() as i32 - 1) as usize;
                    let tab = menus[idx].tab as i32;
                    if HOVER_TAB.swap(tab, std::sync::atomic::Ordering::Relaxed) != tab {
                        HOVER_TICK.store(GetTickCount64(), std::sync::atomic::Ordering::Relaxed);
                        SetTimer(hwnd, 2, 420, None);
                        refresh(hwnd);
                    }
                if GetCapture() != hwnd {
                    SetCapture(hwnd);
                }
                SetTimer(hwnd, 1, 16, None);
                }
            }
            DefWindowProcW(hwnd, msg, w, l)
        }
        WM_TIMER => {
            if w == 3 {
                if auto_hide_enabled() && IsWindowVisible(hwnd) == 0 && *STATE.get().unwrap().lock().unwrap() {
                    let mut point: POINT = std::mem::zeroed();
                    if GetCursorPos(&mut point) != 0 {
                        let foreground = GetForegroundWindow();
                        let monitor = MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST);
                        let mut mi: MONITORINFO = std::mem::zeroed(); mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                        if GetMonitorInfoW(monitor, &mut mi) != 0 {
                            let bounds = dock::Rect { left: mi.rcMonitor.left, top: mi.rcMonitor.top, right: mi.rcMonitor.right, bottom: mi.rcMonitor.bottom };
                            let layout = dock_layout_for_monitor(hwnd, monitor);
                            if dock::pointer_on_reveal_edge((point.x, point.y), bounds, layout.edge, 2) {
                                reposition(hwnd);
                                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                            }
                        }
                    }
                }
                return 0;
            }
            if w == 2 { KillTimer(hwnd, 2); refresh(hwnd); return 0; }
            let mut p: POINT = std::mem::zeroed();
            GetCursorPos(&mut p);
            let key = ((p.y as u16 as u64) << 16) | (p.x as u16 as u64);
            if LAST_MOUSE.swap(key, std::sync::atomic::Ordering::Relaxed) == key {
                KillTimer(hwnd, 1);
                ReleaseCapture();
            }
            0
        }
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_LBUTTONUP => {
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut r);
            let x = (l as u32 as u16) as i32;
            let menus = visible_menus();
            let spacing = (r.right - 32).max(1) / menus.len().max(1) as i32;
            let index = ((x - 16) / spacing.max(1)).clamp(0, menus.len().saturating_sub(1) as i32) as usize;
            if let Some(item) = menus.get(index) { activate_tab(item.tab); }
            0
        }
        WM_KEYDOWN => {
            let menus = visible_menus();
            if !menus.is_empty() {
                let focus = KEY_FOCUS.load(std::sync::atomic::Ordering::Relaxed).min(menus.len() - 1);
                let key = match w as u32 { 0x25 => dock::Key::Left, 0x27 => dock::Key::Right, 0x0D => dock::Key::Enter, _ => dock::Key::Other };
                let (next, action) = dock::keyboard_step(dock::FocusState { selected: focus, count: menus.len() }, key);
                KEY_FOCUS.store(next.selected, std::sync::atomic::Ordering::Relaxed);
                if let dock::KeyAction::Activate(i) = action { activate_tab(menus[i].tab); }
                refresh(hwnd);
                if key != dock::Key::Other { return 0; }
            }
            DefWindowProcW(hwnd, msg, w, l)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
unsafe extern "system" fn foreground_changed(
    _: HWINEVENTHOOK,
    _: u32,
    foreground: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    let dock = HWND_VALUE.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if foreground == dock || foreground.is_null() {
        return;
    }
    reposition(dock);
    let mut fr: RECT = std::mem::zeroed();
    let mut mi: MONITORINFO = std::mem::zeroed();
    if GetWindowRect(foreground, &mut fr) == 0 {
        return;
    }
    let monitor = MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST);
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut mi) == 0 {
        return;
    }
    let full = fr.left <= mi.rcMonitor.left
        && fr.top <= mi.rcMonitor.top
        && fr.right >= mi.rcMonitor.right
        && fr.bottom >= mi.rcMonitor.bottom;
    let hidden = if auto_hide_enabled() {
        let d = dock_layout_for_monitor(dock, monitor);
        dock::foreground_covers_dock(
            dock::Rect { left: fr.left, top: fr.top, right: fr.right, bottom: fr.bottom },
            d.rect,
        )
    } else { full };
    if hidden {
        ShowWindow(dock, SW_HIDE);
    } else if *STATE.get().unwrap().lock().unwrap() {
        ShowWindow(dock, SW_SHOWNOACTIVATE);
    }
}
/// per-pixel alpha 창은 WM_PAINT 로 그려지지 않으므로 직접 UpdateLayeredWindow 로 갱신한다(InvalidateRect 대용).
unsafe fn refresh(hwnd: HWND) {
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut r);
    if r.right < 8 || r.bottom < 8 { return; }
    let mut hc = HighContrast { cb_size: std::mem::size_of::<HighContrast>() as u32, flags: 0, scheme: ptr::null_mut() };
    let high_contrast = SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cb_size, &mut hc as *mut _ as *mut _, 0) != 0 && (hc.flags & 1) != 0;
    let palette = resolved_palette(high_contrast, None);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let menus = visible_menus();
    let focus = KEY_FOCUS.load(std::sync::atomic::Ordering::Relaxed);
    let Some(dib) = render_layered(dpi, palette, &menus, focus, false, r.right, r.bottom) else { return };
    let size = SIZE { cx: r.right, cy: r.bottom };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: LAYER_OPACITY, AlphaFormat: AC_SRC_ALPHA as u8 };
    UpdateLayeredWindow(hwnd, ptr::null_mut(), ptr::null(), &size, dib.dc, &src, 0, &blend, ULW_ALPHA);
}
unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    BeginPaint(hwnd, &mut ps);
    EndPaint(hwnd, &ps);
    refresh(hwnd);
}
/// 창 전체 불투명도(기존 LWA_ALPHA 235 와 같은 값).
const LAYER_OPACITY: u8 = 235;

/// 알약 본체(면+1픽셀 테두리). 알약 마스크도 같은 함수로 그려 기하가 어긋나지 않게 한다. GDI+ 실패 시 GDI RoundRect 폴백.
unsafe fn draw_pill(dc: HDC, r: RECT, fill: u32, line: u32) {
    if Aa::new(dc).is_some_and(|g| g.round_rect(2, 2, r.right - 2, r.bottom - 2, 24, fill, line)) { return; }
    let bg = CreateSolidBrush(fill); let edge = CreatePen(PS_SOLID, 1, line);
    let (ob, op) = (SelectObject(dc, bg), SelectObject(dc, edge));
    RoundRect(dc, 2, 2, r.right - 2, r.bottom - 2, 24, 24);
    SelectObject(dc, ob); SelectObject(dc, op); DeleteObject(bg); DeleteObject(edge);
}
/// 32비트 상향식이 아닌 하향식 DIB 섹션 + 메모리 DC.
struct Dib { dc: HDC, bmp: HBITMAP, old: HGDIOBJ, bits: *mut u8, w: i32, h: i32 }
impl Dib {
    unsafe fn new(w: i32, h: i32) -> Option<Dib> {
        if w <= 0 || h <= 0 { return None; }
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32; bi.bmiHeader.biWidth = w; bi.bmiHeader.biHeight = -h;
        bi.bmiHeader.biPlanes = 1; bi.bmiHeader.biBitCount = 32; bi.bmiHeader.biCompression = BI_RGB;
        let dc = CreateCompatibleDC(ptr::null_mut());
        if dc.is_null() { return None; }
        let mut bits: *mut core::ffi::c_void = ptr::null_mut();
        let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
        if bmp.is_null() || bits.is_null() { DeleteDC(dc); return None; }
        let old = SelectObject(dc, bmp);
        Some(Dib { dc, bmp, old, bits: bits as *mut u8, w, h })
    }
    unsafe fn pixels(&mut self) -> &mut [u8] { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h * 4) as usize) }
}
impl Drop for Dib { fn drop(&mut self) { unsafe { SelectObject(self.dc, self.old); DeleteObject(self.bmp); DeleteDC(self.dc); } } }
/// 독 전체를 그려 per-pixel alpha(premultiplied BGRA) DIB 로 돌려준다. 알약 밖은 알파 0(완전 투명).
/// 방식: ① 검정으로 초기화된 DIB 에 평소처럼 그린다(알약 가장자리 AA 픽셀은 검정과 섞여 이미 coverage 만큼 premultiplied 상태).
/// ② 같은 알약을 흰색으로 그린 마스크 DIB 에서 coverage 를 읽어 알파로 쓰고, 색이 알파를 넘지 않게 한다(글리프·글자는 알약 안쪽이라 알파 255).
unsafe fn render_layered(dpi: u32, palette: dock::Palette, menus: &[dock::MenuItem], focus: usize, labels: bool, w: i32, h: i32) -> Option<Dib> {
    let mut main = Dib::new(w, h)?;
    let mut mask = Dib::new(w, h)?;
    let r = RECT { left: 0, top: 0, right: w, bottom: h };
    draw_dock(main.dc, r, dpi, palette, menus, focus, labels);
    draw_pill(mask.dc, r, 0x00FF_FFFF, 0x00FF_FFFF);
    GdiFlush();
    let m: Vec<u8> = mask.pixels().to_vec();
    dock::apply_pill_mask(main.pixels(), &m);
    Some(main)
}

unsafe fn draw_dock(mem: HDC, r: RECT, dpi: u32, palette: dock::Palette, menus: &[dock::MenuItem], focus: usize, labels: bool) {
    draw_pill(mem, r, palette.background, palette.border);
    let size = dock::icon_size_px(dpi, 24);
    let sc = |n: i32| ((n as i64 * dpi as i64 + 48) / 96) as i32;
    let pad = sc(16);
    let spacing = (r.right - 2 * pad).max(1) / menus.len().max(1) as i32;
    let y = if labels { sc(29) } else { r.bottom / 2 };
    for (i, item) in menus.iter().enumerate() {
        let x = pad + spacing * i as i32 + spacing / 2;
        if i > 0 && menus[i - 1].group != item.group {
            let divider = CreatePen(PS_SOLID, 1, palette.border); let op = SelectObject(mem, divider);
            MoveToEx(mem, x-spacing/2, sc(6), ptr::null_mut()); LineTo(mem, x-spacing/2, r.bottom-sc(6));
            SelectObject(mem, op); DeleteObject(divider);
        }
        let glyph_drawn = icon_font_ready() && dock::icon_glyph(item.tab).is_some_and(|g| draw_glyph(mem, g, x-size/2, y-size/2, size, palette.foreground));
        if !glyph_drawn { draw_icon(mem, dock::icon_shapes(item.tab), x-size/2, y-size/2, size, palette.foreground, palette.accent); }
        if focus == i {
            let (l, t, rr, bb) = (x-size/2-sc(3), y-size/2-sc(3), x+size/2+sc(3), y+size/2+sc(3));
            if !Aa::new(mem).is_some_and(|g| g.ellipse(l, t, rr, bb, palette.accent, false)) {
                let pen=CreatePen(PS_SOLID,1,palette.accent); let op=SelectObject(mem,pen); let ob=SelectObject(mem,GetStockObject(NULL_BRUSH as i32));
                Ellipse(mem,l,t,rr,bb);
                SelectObject(mem,ob); SelectObject(mem,op); DeleteObject(pen);
            }
        }
        let state = WINDOW_STATES.get_or_init(|| Mutex::new(std::collections::HashMap::new())).lock().unwrap().get(&item.tab).copied().unwrap_or(dock::WindowState::Closed);
        if dock::running_dot(state) {
            let (l, t, rr, bb) = (x-sc(3), y+size/2+sc(2), x+sc(3), y+size/2+sc(8));
            if !Aa::new(mem).is_some_and(|g| g.ellipse(l, t, rr, bb, palette.accent, true)) { let dot=CreateSolidBrush(palette.accent); let ob=SelectObject(mem,dot); Ellipse(mem,l,t,rr,bb); SelectObject(mem,ob); DeleteObject(dot); }
        }
        if labels {
            let name=wide(item.name); SetBkMode(mem, TRANSPARENT as i32); SetTextColor(mem,palette.foreground);
            let font=CreateFontW(-sc(9),0,0,0,FW_NORMAL as i32,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,CLEARTYPE_QUALITY as u32,(DEFAULT_PITCH|FF_DONTCARE) as u32,wide("맑은 고딕").as_ptr());
            let old_font=SelectObject(mem,font);
            let mut tr=RECT{left:x-spacing/2+1,top:y+size/2+sc(8),right:x+spacing/2-1,bottom:r.bottom-sc(4)};
            DrawTextW(mem,name.as_ptr(),-1,&mut tr,DT_CENTER|DT_VCENTER|DT_SINGLELINE|DT_END_ELLIPSIS);
            SelectObject(mem,old_font); DeleteObject(font);
        }
        if HOVER_TAB.load(std::sync::atomic::Ordering::Relaxed)==item.tab as i32 && GetTickCount64().saturating_sub(HOVER_TICK.load(std::sync::atomic::Ordering::Relaxed))>=400 && !labels {
            let name=wide(item.name); SetBkMode(mem,TRANSPARENT as i32); SetTextColor(mem,palette.foreground);
            let mut tr=RECT{left:(x-70).max(2),top:r.bottom-22,right:(x+70).min(r.right-2),bottom:r.bottom-4};
            DrawTextW(mem,name.as_ptr(),-1,&mut tr,DT_CENTER|DT_VCENTER|DT_SINGLELINE);
        }
    }
}
fn capture_mode(args: &[String]) -> Result<(), String> {
    let mut dir=None; let mut dpi=96u32; let mut dark=false; let mut hc=false;
    let mut i=0; while i<args.len() { match args[i].as_str() {
        "--capture-dir" => { i+=1; dir=args.get(i).cloned(); },
        "--dpi" => { i+=1; dpi=args.get(i).ok_or("--dpi 값 없음")?.parse().map_err(|_| "잘못된 --dpi")?; },
        "--dark" => dark=true, "--hc" => hc=true, other => return Err(format!("알 수 없는 인자: {other}")),
    } i+=1; }
    if dpi<48 || dpi>384 { return Err("--dpi 범위는 48..384".into()); }
    let dir=std::path::PathBuf::from(dir.ok_or("--capture-dir 필요")?); std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    unsafe {
        let width=((16*104+32) as u64*dpi as u64+48)/96; let width=width as i32; let height=((88u64*dpi as u64+48)/96) as i32;
        let p=resolved_palette(hc,Some(dark));
        WINDOW_STATES.get_or_init(||Mutex::new(std::collections::HashMap::new())).lock().unwrap().insert(0,dock::WindowState::OpenFocused);
        let all=dock::menu_items(&[],true);
        let mut dib=render_layered(dpi,p,&all,0,true,width,height).ok_or("메모리 비트맵 생성 실패")?;
        let px=dib.pixels().to_vec();
        let suffix=if hc{"hc"}else if dark{"dark"}else{"light"};
        // 알파 포함 PNG(원본 알파 확인용)와, 마젠타 위에 실제 불투명도로 합성한 RGB PNG(눈으로 모서리 확인용)
        let straight=dock::unpremultiply_bgra_to_rgba(&px);
        let over=dock::composite_over_bgra(&px,LAYER_OPACITY,[255,0,255]);
        let name=format!("dock_{suffix}_{dpi}.png");
        std::fs::write(dir.join(&name),dock::encode_png(width as u32,height as u32,3,&over)).map_err(|e|e.to_string())?;
        std::fs::write(dir.join(format!("dock_{suffix}_{dpi}_alpha.png")),dock::encode_png(width as u32,height as u32,4,&straight)).map_err(|e|e.to_string())?;
        println!("{name}");
    } Ok(())
}
fn visible_menus() -> Vec<dock::MenuItem> { let hidden = hidden_tabs(); dock::menu_items(&hidden.iter().copied().collect::<Vec<_>>(), false) }
fn setting_dark() -> bool {
    let Some(appdata)=std::env::var_os("APPDATA") else{return ui_theme::system_dark()};
    let path=std::path::PathBuf::from(appdata).join("HwpAutoDocFit").join("settings.json");
    let mode=std::fs::read_to_string(path).ok().and_then(|s|serde_json::from_str::<serde_json::Value>(&s).ok()).and_then(|v|v.get("theme_mode").and_then(|x|x.as_str()).map(str::to_owned));
    match mode.as_deref(){Some("dark")=>true,Some("light")=>false,_=>ui_theme::system_dark()}
}
fn auto_hide_enabled() -> bool {
    let Some(appdata) = std::env::var_os("APPDATA") else { return false };
    let path = std::path::PathBuf::from(appdata).join("HwpAutoDocFit").join("settings.json");
    std::fs::read_to_string(path).ok().is_some_and(|s| dock::parse_auto_hide(&s))
}
fn resolved_palette(force_hc: bool, dark_override: Option<bool>) -> dock::Palette {
    let sys=ui_theme::sys_colors();
    let hc=force_hc||ui_theme::high_contrast_on();
    let mode=dark_override.unwrap_or_else(setting_dark);
    let base=if mode{ui_theme::Palette::DARK}else{ui_theme::Palette::LIGHT};
    let normal=dock::Palette{foreground:base.text,accent:base.accent,background:base.bg,border:base.border};
    dock::palette_for_mode(normal,hc,sys.window,sys.window_text,sys.highlight,sys.highlight_text)
}
fn activate_tab(tab: u8) {
    let states = WINDOW_STATES.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut states = states.lock().unwrap();
    let previous = states.get(&tab).copied().unwrap_or(dock::WindowState::Closed);
    let action = dock::click_action(previous);
    if action == dock::ClickAction::Close {
        states.insert(tab, dock::WindowState::Closed);
    } else {
        states.insert(tab, dock::WindowState::OpenFocused);
    }
    drop(states);
    if action == dock::ClickAction::Close { let _ = close_tab(tab); } else { let _ = open_tab(tab); }
    let hwnd = HWND_VALUE.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    unsafe { refresh(hwnd); }
}

fn close_tab(tab: u8) -> bool {
    use windows_sys::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};
    let Some(name) = dock::tab_close_event_name(tab) else { return false };
    let name = wide(&name);
    unsafe {
        let event = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        if event.is_null() { return false; }
        let result = SetEvent(event) != 0;
        CloseHandle(event);
        result
    }
}
unsafe fn draw_icon(dc: HDC, shapes: &[dock::IconShape], ox: i32, oy: i32, size: i32, fg: u32, accent: u32) {
    let s = size as f32 / 24.;
    for shape in shapes {
        let color = match shape { dock::IconShape::Circle { filled: true, .. } | dock::IconShape::Polygon { filled: true, .. } => accent, _ => fg };
        let pen = CreatePen(PS_SOLID, (size/12).max(1), color); let oldp = SelectObject(dc, pen);
        let owns_brush = matches!(shape, dock::IconShape::Circle { filled: true, .. } | dock::IconShape::Rect { filled: true, .. } | dock::IconShape::Polygon { filled: true, .. });
        let brush = if owns_brush { CreateSolidBrush(color) } else { GetStockObject(NULL_BRUSH as i32) };
        let oldb = SelectObject(dc, brush);
        match shape {
            dock::IconShape::Line{x1,y1,x2,y2} => { MoveToEx(dc, ox+(x1*s) as i32, oy+(y1*s) as i32, ptr::null_mut()); LineTo(dc, ox+(x2*s) as i32, oy+(y2*s) as i32); }
            dock::IconShape::Rect{left,top,right,bottom,..} => { Rectangle(dc, ox+(left*s) as i32, oy+(top*s) as i32, ox+(right*s) as i32, oy+(bottom*s) as i32); }
            dock::IconShape::Circle{cx,cy,radius,..} => { Ellipse(dc, ox+((cx-radius)*s) as i32, oy+((cy-radius)*s) as i32, ox+((cx+radius)*s) as i32, oy+((cy+radius)*s) as i32); }
            dock::IconShape::Polygon{points,..} => { let pts: Vec<POINT> = points.iter().map(|(x,y)| POINT{x:ox+(*x*s) as i32,y:oy+(*y*s) as i32}).collect(); Polygon(dc, pts.as_ptr(), pts.len() as i32); }
        }
        SelectObject(dc, oldb); SelectObject(dc, oldp); DeleteObject(pen);
        if owns_brush { DeleteObject(brush); }
    }
}
fn pipe_server() {
    use windows_sys::Win32::{
        Storage::FileSystem::{PIPE_ACCESS_DUPLEX, ReadFile, WriteFile},
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
            PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
        },
    };
    let name = wide("\\\\.\\pipe\\HwpAutoDocFit.dock");
    loop {
        unsafe {
            let h = CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                4,
                4096,
                4096,
                0,
                ptr::null(),
            );
            if h == INVALID_HANDLE_VALUE {
                thread::sleep(Duration::from_millis(500));
                continue;
            }
            if ConnectNamedPipe(h, ptr::null_mut()) != 0 || GetLastError() == ERROR_PIPE_CONNECTED {
                let mut b = [0u8; 2048];
                let mut n = 0;
                let ok = ReadFile(h, b.as_mut_ptr(), b.len() as u32, &mut n, ptr::null_mut());
                let response = if ok != 0 && n > 0 {
                    let req = serde_json::from_slice::<serde_json::Value>(&b[..n as usize])
                        .unwrap_or_default();
                    let cmd = req.get("cmd").and_then(|x| x.as_str()).unwrap_or("");
                    let hwnd = HWND_VALUE.load(std::sync::atomic::Ordering::Relaxed) as HWND;
                    let result = match cmd {
                        "ready" => serde_json::json!({"ok":true,"ready":true}),
                        "show" => {
                            *STATE.get().unwrap().lock().unwrap() = true;
                            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                            serde_json::json!({"ok":true})
                        }
                        "hide" => {
                            *STATE.get().unwrap().lock().unwrap() = false;
                            ShowWindow(hwnd, SW_HIDE);
                            serde_json::json!({"ok":true})
                        }
                        "quit" => {
                            PostMessageW(hwnd, WM_CLOSE, 0, 0);
                            serde_json::json!({"ok":true})
                        }
                        "open_tab" => {
                            let tab = req.get("tab").and_then(|v| v.as_u64()).unwrap_or(255) as u8;
                            let ok = open_tab(tab);
                            serde_json::json!({"ok":ok,"tab":tab})
                        }
                        _ => serde_json::json!({"ok":false,"error":"unknown command"}),
                    };
                    serde_json::to_vec(&result).unwrap_or_default()
                } else {
                    br#"{"ok":false,"error":"invalid request"}"#.to_vec()
                };
                let mut out = response;
                out.push(b'\n');
                let mut written = 0;
                WriteFile(
                    h,
                    out.as_ptr(),
                    out.len() as u32,
                    &mut written,
                    ptr::null_mut(),
                );
                DisconnectNamedPipe(h);
            }
            CloseHandle(h);
        }
    }
}

/// Keep OS calls at this boundary; routing decisions are covered by the pure library tests.
fn open_tab(tab: u8) -> bool {
    use windows_sys::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};
    match dock::route(tab) {
        dock::Route::Invalid => false,
        dock::Route::Search => {
            // ktm's existing --show path forwards WM_HOTKEY to its singleton window.
            let Some(exe) = sibling_exe("ktm.exe") else { return false };
            std::process::Command::new(exe).arg("--show").spawn().is_ok()
        }
        dock::Route::VirtualFolder => run_sibling("kvf.exe", Some("--show")),
        dock::Route::Clipboard => launch_sibling("kagent.exe"),
        dock::Route::MainTab(tab) => {
            let Some(name) = dock::tab_event_name(tab) else { return false };
            let name = wide(&name);
            unsafe {
                let event = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
                if !event.is_null() {
                    let result = SetEvent(event) != 0;
                    CloseHandle(event);
                    return result;
                }
            }
            let Some(exe) = sibling_exe("HwpAutoDocFit.exe") else { return false };
            std::process::Command::new(exe).spawn().is_ok()
        }
    }
}
fn hidden_tabs() -> std::collections::BTreeSet<u8> {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return dock::parse_hidden_tabs("");
    };
    let path = std::path::PathBuf::from(appdata)
        .join("HwpAutoDocFit")
        .join("settings.json");
    match std::fs::read_to_string(path) {
        Ok(contents) => dock::parse_hidden_tabs(&contents),
        Err(_) => dock::parse_hidden_tabs(""),
    }
}
fn sibling_exe(name: &str) -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.join(name)).filter(|p| p.is_file())
}
fn launch_sibling(name: &str) -> bool {
    sibling_exe(name).and_then(|p| std::process::Command::new(p).spawn().ok()).is_some()
}
fn run_sibling(name: &str, arg: Option<&str>) -> bool {
    let Some(exe) = sibling_exe(name) else { return false };
    let mut command = std::process::Command::new(exe);
    if let Some(arg) = arg { command.arg(arg); }
    command.spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 헤드리스 렌더 결과(premultiplied BGRA)에서 알약 밖 모서리 알파는 0, 알약 안쪽은 불투명, 색은 알파를 넘지 않는다.
    fn check(dpi: u32, dark: bool, hc: bool) {
        unsafe {
            let w = (((16 * 104 + 32) as u64 * dpi as u64 + 48) / 96) as i32; let h = ((88u64 * dpi as u64 + 48) / 96) as i32;
            let p = resolved_palette(hc, Some(dark));
            let all = dock::menu_items(&[], true);
            let mut d = render_layered(dpi, p, &all, 0, true, w, h).expect("render");
            let px = d.pixels().to_vec();
            let at = |x: i32, y: i32| { let i = ((y * w + x) * 4) as usize; [px[i], px[i + 1], px[i + 2], px[i + 3]] };
            for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1), (1, 1), (w - 2, h - 2), (3, 3)] {
                assert_eq!(at(x, y), [0, 0, 0, 0], "코너 ({x},{y}) dpi={dpi} dark={dark} hc={hc} 가 투명이어야 함");
            }
            assert_eq!(at(w / 2, h / 2)[3], 255, "알약 안쪽은 불투명");
            assert_eq!(at(w / 2, 4)[3], 255, "알약 위쪽 변 안쪽은 불투명");
            assert!(px.chunks_exact(4).all(|q| q[0] <= q[3] && q[1] <= q[3] && q[2] <= q[3]), "premultiplied 위반");
            let transparent = px.chunks_exact(4).filter(|q| q[3] == 0).count();
            assert!(transparent > 0 && transparent < px.len() / 4 / 10, "투명 픽셀은 모서리 쪽에만 있어야 함: {transparent}");
        }
    }
    #[test] fn corners_transparent_light_100() { check(96, false, false); }
    #[test] fn corners_transparent_light_125() { check(120, false, false); }
    #[test] fn corners_transparent_light_150() { check(144, false, false); }
    #[test] fn corners_transparent_dark_100() { check(96, true, false); }
    #[test] fn corners_transparent_dark_125() { check(120, true, false); }
    #[test] fn corners_transparent_dark_150() { check(144, true, false); }
    #[test] fn corners_transparent_high_contrast() { check(96, false, true); }
}
