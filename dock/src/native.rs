#![allow(unsafe_op_in_unsafe_fn)]
use std::{
    ptr,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
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
const CLASS: &str = "HwpAutoDocFit.Dock";
#[repr(C)]
struct HighContrast { cb_size: u32, flags: u32, scheme: *mut u16 }
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn run() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--capture-dir") { let _ = capture_mode(&args); return; }
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
        SetLayeredWindowAttributes(hwnd, 0, 235, LWA_ALPHA);
        reposition(hwnd);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
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
    }
}
unsafe fn reposition(hwnd: HWND) {
    let foreground = GetForegroundWindow();
    let monitor = MonitorFromWindow(if foreground.is_null() { hwnd } else { foreground }, MONITOR_DEFAULTTONEAREST);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(monitor, &mut mi);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let menus = visible_menus();
    let logical_width = ((menus.len() as i32 * 36) + 32).min((mi.rcMonitor.right - mi.rcMonitor.left) * 96 / dpi as i32);
    let d = dock::dock_rect(
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
    );
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        d.rect.left,
        d.rect.top,
        d.rect.width(),
        d.rect.height(),
        SWP_NOACTIVATE,
    );
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
                        InvalidateRect(hwnd, ptr::null(), 0);
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
            if w == 2 { KillTimer(hwnd, 2); InvalidateRect(hwnd, ptr::null(), 0); return 0; }
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
                InvalidateRect(hwnd, ptr::null(), 0);
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
    if full {
        ShowWindow(dock, SW_HIDE);
    } else if *STATE.get().unwrap().lock().unwrap() {
        ShowWindow(dock, SW_SHOWNOACTIVATE);
    }
}
unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut r);
    let mem = CreateCompatibleDC(dc);
    let bmp = CreateCompatibleBitmap(dc, r.right, r.bottom);
    let old = SelectObject(mem, bmp);
    let mut hc = HighContrast { cb_size: std::mem::size_of::<HighContrast>() as u32, flags: 0, scheme: ptr::null_mut() };
    let high_contrast = SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cb_size, &mut hc as *mut _ as *mut _, 0) != 0 && (hc.flags & 1) != 0;
    let palette = resolved_palette(high_contrast, None);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let menus = visible_menus();
    let focus = KEY_FOCUS.load(std::sync::atomic::Ordering::Relaxed);
    draw_dock(mem, r, dpi, palette, &menus, focus, false);
    GdiFlush();
    BitBlt(dc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
    SelectObject(mem, old);
    DeleteObject(bmp);
    DeleteDC(mem);
    EndPaint(hwnd, &ps);
}

unsafe fn draw_dock(mem: HDC, r: RECT, dpi: u32, palette: dock::Palette, menus: &[dock::MenuItem], focus: usize, labels: bool) {
    let bg = CreateSolidBrush(palette.background);
    let edge = CreatePen(PS_SOLID, 1, palette.border);
    let old_brush = SelectObject(mem, bg);
    let old_pen = SelectObject(mem, edge);
    RoundRect(mem, 2, 2, r.right - 2, r.bottom - 2, 24, 24);
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
        draw_icon(mem, dock::icon_shapes(item.tab), x-size/2, y-size/2, size, palette.foreground, palette.accent);
        if focus == i {
            let pen=CreatePen(PS_SOLID,1,palette.accent); let op=SelectObject(mem,pen); let ob=SelectObject(mem,GetStockObject(NULL_BRUSH as i32));
            Ellipse(mem,x-size/2-sc(3),y-size/2-sc(3),x+size/2+sc(3),y+size/2+sc(3));
            SelectObject(mem,ob); SelectObject(mem,op); DeleteObject(pen);
        }
        let state = WINDOW_STATES.get_or_init(|| Mutex::new(std::collections::HashMap::new())).lock().unwrap().get(&item.tab).copied().unwrap_or(dock::WindowState::Closed);
        if dock::running_dot(state) { let dot=CreateSolidBrush(palette.accent); let ob=SelectObject(mem,dot); Ellipse(mem,x-sc(3),y+size/2+sc(2),x+sc(3),y+size/2+sc(8)); SelectObject(mem,ob); DeleteObject(dot); }
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
    SelectObject(mem,old_brush); SelectObject(mem,old_pen); DeleteObject(bg); DeleteObject(edge);
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
        let mut bi: BITMAPINFO=std::mem::zeroed(); bi.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32; bi.bmiHeader.biWidth=width; bi.bmiHeader.biHeight=-height; bi.bmiHeader.biPlanes=1; bi.bmiHeader.biBitCount=32; bi.bmiHeader.biCompression=BI_RGB;
        let dc=CreateCompatibleDC(ptr::null_mut()); let mut bits: *mut core::ffi::c_void=ptr::null_mut(); let bmp=CreateDIBSection(dc,&bi,DIB_RGB_COLORS,&mut bits,ptr::null_mut(),0);
        if bmp.is_null() || bits.is_null(){DeleteDC(dc);return Err("메모리 비트맵 생성 실패".into());}
        let old=SelectObject(dc,bmp); let r=RECT{left:0,top:0,right:width,bottom:height};
        let p=resolved_palette(hc,Some(dark));
        WINDOW_STATES.get_or_init(||Mutex::new(std::collections::HashMap::new())).lock().unwrap().insert(0,dock::WindowState::OpenFocused);
        let all=dock::menu_items(&[],true); draw_dock(dc,r,dpi,p,&all,0,true); GdiFlush();
        let px=std::slice::from_raw_parts(bits as *const u8,(width*height*4) as usize); let mut rgb=Vec::with_capacity((width*height*3) as usize);
        for q in px.chunks_exact(4){rgb.extend_from_slice(&[q[2],q[1],q[0]]);}
        let mut raw=Vec::with_capacity(rgb.len()+height as usize); for row in rgb.chunks_exact(width as usize*3){raw.push(0);raw.extend_from_slice(row);}
        let mut z=vec![0x78,0x01]; let mut blocks=raw.chunks(65535).peekable(); while let Some(b)=blocks.next(){z.push(u8::from(blocks.peek().is_none()));z.extend_from_slice(&(b.len() as u16).to_le_bytes());z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());z.extend_from_slice(b);}
        let(mut a,mut b)=(1u32,0u32);for q in &raw{a=(a+*q as u32)%65521;b=(b+a)%65521;}z.extend_from_slice(&((b<<16)|a).to_be_bytes());
        let mut out=b"\x89PNG\r\n\x1a\n".to_vec(); let mut add=|kind:&[u8;4],data:&[u8]|{out.extend_from_slice(&(data.len() as u32).to_be_bytes());out.extend_from_slice(kind);out.extend_from_slice(data);let mut crc=0xffff_ffffu32;for q in kind.iter().chain(data){crc^=*q as u32;for _ in 0..8{crc=if crc&1!=0{0xedb88320^(crc>>1)}else{crc>>1};}}out.extend_from_slice(&(!crc).to_be_bytes());};
        let mut ih=Vec::new();ih.extend_from_slice(&(width as u32).to_be_bytes());ih.extend_from_slice(&(height as u32).to_be_bytes());ih.extend_from_slice(&[8,2,0,0,0]);add(b"IHDR",&ih);add(b"IDAT",&z);add(b"IEND",&[]);
        let suffix=if hc{"hc"}else if dark{"dark"}else{"light"};let path=dir.join(format!("dock_{suffix}_{dpi}.png"));std::fs::write(&path,out).map_err(|e|e.to_string())?;
        SelectObject(dc,old);DeleteObject(bmp);DeleteDC(dc);println!("{}",path.file_name().unwrap_or_default().to_string_lossy());
    } Ok(())
}
fn visible_menus() -> Vec<dock::MenuItem> { let hidden = hidden_tabs(); dock::menu_items(&hidden.iter().copied().collect::<Vec<_>>(), false) }
fn setting_dark() -> bool {
    let Some(appdata)=std::env::var_os("APPDATA") else{return ui_theme::system_dark()};
    let path=std::path::PathBuf::from(appdata).join("HwpAutoDocFit").join("settings.json");
    let mode=std::fs::read_to_string(path).ok().and_then(|s|serde_json::from_str::<serde_json::Value>(&s).ok()).and_then(|v|v.get("theme_mode").and_then(|x|x.as_str()).map(str::to_owned));
    match mode.as_deref(){Some("dark")=>true,Some("light")=>false,_=>ui_theme::system_dark()}
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
    unsafe { InvalidateRect(hwnd, ptr::null(), 0); }
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
