//! 사이드바 실행 파일: 앱을 켜면 이것만 뜨고, 메뉴를 누르면 그 메뉴의 창이 따로 열린다.
//! 판단은 `dock::sidebar`(순수)에 있고 여기는 Win32 경계다. 사이드바를 닫아도 띄운 창은 남는다.
//!
//! 인자: `--settings <settings.json>` `--resources <Rust 창 폴더>` `--app <HWP_AutoDocFit.exe>`
//! `--python <python.exe> --source-root <소스 폴더>`(개발) `--state-file <상태 JSON>`
//! `--capture <png> [--dpi N] [--height N]`(창 없이 그림만 저장)
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("sidebar is a Windows application");
}

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(windows)]
mod win {
    #![allow(unsafe_op_in_unsafe_fn)]
    use dock::sidebar::{self, Click, Entry, Env, Identity, MenuState};
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command};
    use std::ptr::{null, null_mut};
    use std::time::SystemTime;
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetTickCount64, Threading::CreateMutexW},
        UI::{
            Controls::{DRAWITEMSTRUCT, ODS_FOCUS, ODS_SELECTED},
            HiDpi::*,
            Input::KeyboardAndMouse::GetFocus,
            WindowsAndMessaging::*,
        },
    };

    static ICON_FONT: &[u8] = include_bytes!("../../assets/MaterialIconsDock.otf");
    const CLASS: &str = "DocFitSidebar";
    const ID_PIN: usize = 100;
    const ID_CLOSE: usize = 101;
    const ID_MENU_BASE: usize = 200;
    const TIMER_POLL: usize = 1;
    /// 다른 실행(두 번째 사이드바)이 이미 떠 있는 사이드바를 보이게 할 때 보내는 메시지.
    const WM_SHOW_SIDEBAR: u32 = WM_APP + 1;
    /// 단추를 다시 만드는 일은 그 단추의 누름 처리가 끝난 뒤에 한다.
    const WM_REBUILD: u32 = WM_APP + 2;

    struct Config {
        env: Env,
        state_file: Option<PathBuf>,
    }

    #[derive(Default)]
    struct Prefs {
        hidden: BTreeSet<u8>,
        developer: bool,
        auto_hide: bool,
        dark: bool,
    }

    struct State {
        config: Config,
        prefs: Prefs,
        stamp: Option<(SystemTime, u64)>,
        entries: Vec<Entry>,
        menus: BTreeMap<u8, MenuState>,
        children: BTreeMap<u8, Child>,
        buttons: Vec<HWND>,
        hover: HWND,
        visible: bool,
        ticks: u32,
        notice: Option<String>,
        written: String,
    }

    thread_local!(static STATE: RefCell<Option<State>> = const { RefCell::new(None) });

    /// 재진입(MessageBox 의 안쪽 메시지 루프 등)에서는 아무것도 하지 않는다.
    fn with<T>(f: impl FnOnce(&mut State) -> T) -> Option<T> {
        STATE.with(|s| s.try_borrow_mut().ok().and_then(|mut s| s.as_mut().map(f)))
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn scale(v: i32, dpi: u32) -> i32 {
        ((v as i64 * dpi as i64 + 48) / 96) as i32
    }

    fn read_prefs(settings: &Path) -> Prefs {
        let text = std::fs::read_to_string(settings).unwrap_or_default();
        let theme = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("theme_mode").and_then(|x| x.as_str()).map(str::to_owned));
        Prefs {
            hidden: dock::parse_hidden_tabs(&text),
            developer: sidebar::parse_developer_mode(&text),
            auto_hide: dock::parse_auto_hide(&text),
            dark: match theme.as_deref() {
                Some("dark") => true,
                Some("light") => false,
                _ => ui_theme::system_dark(),
            },
        }
    }

    fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
        let meta = std::fs::metadata(path).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    fn palette(dark: bool) -> dock::Palette {
        let sys = ui_theme::sys_colors();
        let base = if dark { ui_theme::Palette::DARK } else { ui_theme::Palette::LIGHT };
        let normal = dock::Palette { foreground: base.text, accent: base.accent, background: base.bg, border: base.border };
        dock::palette_for_mode(normal, ui_theme::high_contrast_on(), sys.window, sys.window_text, sys.highlight,
                               sys.highlight_text)
    }

    // ---- 그리기 ----

    unsafe fn load_icon_font() {
        // 프로세스 전용 등록. 실패해도 이름 글자로 메뉴를 쓸 수 있다.
        let mut count = 0u32;
        AddFontMemResourceEx(ICON_FONT.as_ptr() as *const _, ICON_FONT.len() as u32, null(), &mut count);
    }

    unsafe fn text(dc: HDC, s: &str, rect: &mut RECT, face: &str, height: i32, color: u32, flags: u32) {
        let font = CreateFontW(-height, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32,
                               OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32,
                               (DEFAULT_PITCH | FF_DONTCARE) as u32, wide(face).as_ptr());
        let old = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        let w = wide(s);
        DrawTextW(dc, w.as_ptr(), -1, rect, flags | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        SelectObject(dc, old);
        DeleteObject(font);
    }

    unsafe fn fill(dc: HDC, rect: &RECT, color: u32) {
        let brush = CreateSolidBrush(color);
        FillRect(dc, rect, brush);
        DeleteObject(brush);
    }

    fn icon_of(id: u8) -> (String, &'static str) {
        match dock::icon_glyph(id) {
            Some(c) => (c.to_string(), "Material Icons"),
            // 설정 톱니는 독 글꼴 서브셋에 없어 Windows 기본 아이콘 글꼴을 쓴다.
            None => ("\u{E713}".into(), "Segoe MDL2 Assets"),
        }
    }

    unsafe fn draw_row(dc: HDC, rect: RECT, dpi: u32, p: dock::Palette, id: u8, state: MenuState, hot: bool, focus: bool) {
        fill(dc, &rect, p.background);
        let mut inner = RECT { left: rect.left + scale(6, dpi), right: rect.right - scale(6, dpi), ..rect };
        if hot {
            fill(dc, &inner, p.border);
        }
        if state != MenuState::Closed {
            // 열림: 채운 막대, 여는 중: 짧은 막대.
            let h = if state == MenuState::Open { (rect.bottom - rect.top) / 2 } else { scale(6, dpi) };
            let mid = (rect.top + rect.bottom) / 2;
            fill(dc, &RECT { left: rect.left + scale(1, dpi), top: mid - h / 2, right: rect.left + scale(4, dpi), bottom: mid + h / 2 }, p.accent);
        }
        let (glyph, face) = icon_of(id);
        let mut icon = RECT { left: inner.left + scale(4, dpi), right: inner.left + scale(26, dpi), ..inner };
        text(dc, &glyph, &mut icon, face, scale(18, dpi), p.foreground, DT_CENTER);
        let mut name = RECT { left: icon.right + scale(8, dpi), ..inner };
        let suffix = if matches!(state, MenuState::Launching { .. }) { " …" } else { "" };
        text(dc, &format!("{}{suffix}", sidebar::label(id)), &mut name, "맑은 고딕", scale(13, dpi), p.foreground,
             DT_LEFT | DT_END_ELLIPSIS);
        if focus {
            inner.top += 1;
            inner.bottom -= 1;
            let brush = CreateSolidBrush(p.accent);
            FrameRect(dc, &inner, brush);
            DeleteObject(brush);
        }
    }

    unsafe fn draw_header_button(dc: HDC, rect: RECT, dpi: u32, p: dock::Palette, id: usize, on: bool, hot: bool) {
        fill(dc, &rect, if hot { p.border } else { p.background });
        let glyph = match (id, on) {
            (ID_PIN, true) => "\u{E77A}",  // 자동 숨김 켜짐(고정 풀림)
            (ID_PIN, false) => "\u{E718}", // 고정
            _ => "\u{E8BB}",               // 닫기
        };
        let mut r = rect;
        text(dc, glyph, &mut r, "Segoe MDL2 Assets", scale(11, dpi), p.foreground, DT_CENTER);
    }

    unsafe fn draw_background(dc: HDC, client: RECT, dpi: u32, p: dock::Palette, entries: &[Entry], rows: &[(i32, i32)]) {
        fill(dc, &client, p.background);
        let mut title = RECT { left: scale(14, dpi), top: 0, right: client.right - scale(64, dpi), bottom: scale(sidebar::HEADER, dpi) };
        text(dc, "DocFit", &mut title, "맑은 고딕", scale(13, dpi), p.accent, DT_LEFT);
        for i in 1..rows.len() {
            if entries[i - 1].group != entries[i].group {
                let y = scale(rows[i].0, dpi) - scale(sidebar::GAP, dpi) / 2;
                fill(dc, &RECT { left: scale(12, dpi), top: y, right: client.right - scale(12, dpi), bottom: y + 1 }, p.border);
            }
        }
        fill(dc, &RECT { left: client.right - 1, ..client }, p.border);
    }

    // ---- 창 구성 ----

    unsafe fn work_area(hwnd: HWND) -> (RECT, RECT) {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut mi);
        (mi.rcWork, mi.rcMonitor)
    }

    unsafe fn button(parent: HWND, id: usize, name: &str) -> HWND {
        CreateWindowExW(0, wide("BUTTON").as_ptr(), wide(name).as_ptr(),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32, 0, 0, 1, 1, parent, id as HMENU,
                        GetModuleHandleW(null()), null())
    }

    /// 설정을 반영해 메뉴 단추를 다시 만든다. 단추 이름은 화면 읽기 프로그램이 읽는다.
    unsafe fn rebuild(hwnd: HWND) {
        let old = with(|s| std::mem::take(&mut s.buttons)).unwrap_or_default();
        for b in old {
            DestroyWindow(b);
        }
        let Some((entries, auto_hide)) = with(|s| {
            s.entries = sidebar::entries(&s.prefs.hidden, s.prefs.developer);
            s.hover = null_mut();
            (s.entries.clone(), s.prefs.auto_hide)
        }) else { return };
        let mut buttons = vec![
            button(hwnd, ID_PIN, if auto_hide { "자동 숨김 끄기" } else { "자동 숨김 켜기" }),
            button(hwnd, ID_CLOSE, "사이드바 닫기"),
        ];
        buttons.extend(entries.iter().map(|e| button(hwnd, ID_MENU_BASE + e.id as usize, sidebar::label(e.id))));
        with(|s| s.buttons = buttons);
        layout(hwnd);
    }

    unsafe fn layout(hwnd: HWND) {
        let dpi = GetDpiForWindow(hwnd).max(96);
        let (work, _) = work_area(hwnd);
        let (width, height) = (scale(sidebar::WIDTH, dpi), work.bottom - work.top);
        SetWindowPos(hwnd, HWND_TOPMOST, work.left, work.top, width, height, SWP_NOACTIVATE);
        let Some((entries, buttons)) = with(|s| (s.entries.clone(), s.buttons.clone())) else { return };
        if buttons.len() != entries.len() + 2 {
            return;
        }
        let logical = (height as i64 * 96 / dpi as i64) as i32;
        let rows = sidebar::rows(&entries, logical);
        let side = scale(26, dpi);
        let top = (scale(sidebar::HEADER, dpi) - side) / 2;
        MoveWindow(buttons[0], width - side * 2 - scale(8, dpi), top, side, side, 1);
        MoveWindow(buttons[1], width - side - scale(6, dpi), top, side, side, 1);
        for (b, (y0, y1)) in buttons[2..].iter().zip(rows) {
            MoveWindow(*b, 0, scale(y0, dpi), width - 1, scale(y1, dpi) - scale(y0, dpi), 1);
        }
        InvalidateRect(hwnd, null(), 1);
    }

    unsafe fn paint(hwnd: HWND) {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let dpi = GetDpiForWindow(hwnd).max(96);
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut client);
        with(|s| {
            let rows = sidebar::rows(&s.entries, (client.bottom as i64 * 96 / dpi as i64) as i32);
            draw_background(dc, client, dpi, palette(s.prefs.dark), &s.entries, &rows);
        });
        EndPaint(hwnd, &ps);
    }

    unsafe fn draw_item(item: &DRAWITEMSTRUCT) {
        let dpi = GetDpiForWindow(item.hwndItem).max(96);
        with(|s| {
            let p = palette(s.prefs.dark);
            let id = item.CtlID as usize;
            let hot = s.hover == item.hwndItem || item.itemState & ODS_SELECTED != 0;
            if id >= ID_MENU_BASE {
                let menu = (id - ID_MENU_BASE) as u8;
                let state = s.menus.get(&menu).copied().unwrap_or_default();
                draw_row(item.hDC, item.rcItem, dpi, p, menu, state, hot, item.itemState & ODS_FOCUS != 0);
            } else {
                draw_header_button(item.hDC, item.rcItem, dpi, p, id, s.prefs.auto_hide, hot);
            }
        });
    }

    // ---- 창 찾기·열기 ----

    unsafe fn window_text(f: unsafe extern "system" fn(HWND, *mut u16, i32) -> i32, hwnd: HWND) -> String {
        let mut buffer = [0u16; 256];
        let n = f(hwnd, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&buffer[..n])
    }

    struct Seen {
        hwnd: HWND,
        class: String,
        title: String,
    }

    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
        if IsWindowVisible(hwnd) != 0 {
            let list = &mut *(data as *mut Vec<Seen>);
            list.push(Seen { hwnd, class: window_text(GetClassNameW, hwnd), title: window_text(GetWindowTextW, hwnd) });
        }
        1
    }

    unsafe fn visible_windows() -> Vec<Seen> {
        let mut list: Vec<Seen> = Vec::new();
        EnumWindows(Some(collect), &mut list as *mut _ as LPARAM);
        list
    }

    fn find(windows: &[Seen], id: u8) -> Option<HWND> {
        sidebar::identities(id).iter().find_map(|identity| {
            windows.iter().find(|w| match identity {
                Identity::Class(c) => w.class == *c,
                Identity::Title(t) => &w.title == t,
            }).map(|w| w.hwnd)
        })
    }

    unsafe fn activate(hwnd: HWND) {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }

    fn start(spawn: &sidebar::Spawn) -> std::io::Result<Child> {
        let mut command = Command::new(&spawn.program);
        command.args(&spawn.args).envs(spawn.envs.iter().map(|(k, v)| (*k, v)));
        if let Some(cwd) = &spawn.cwd {
            command.current_dir(cwd);
        }
        command.spawn()
    }

    unsafe fn open(hwnd: HWND, id: u8) {
        let windows = visible_windows();
        let found = find(&windows, id);
        let error = with(|s| {
            let state = s.menus.get(&id).copied().unwrap_or_default();
            match sidebar::on_click(state, found.is_some()) {
                Click::Wait => None,
                Click::Activate => {
                    s.menus.insert(id, MenuState::Open);
                    None
                }
                Click::Spawn => match sidebar::spawn_for(id, &s.config.env, &|p| p.is_file()) {
                    Ok(spawn) => match start(&spawn) {
                        Ok(child) => {
                            AllowSetForegroundWindow(child.id());
                            s.children.insert(id, child);
                            s.menus.insert(id, MenuState::Launching { since_ms: GetTickCount64() });
                            None
                        }
                        Err(e) => Some(format!("{} 창을 열지 못했습니다.\n{e}", sidebar::label(id))),
                    },
                    Err(message) => Some(message),
                },
            }
        }).flatten();
        if let Some(target) = found {
            activate(target);
        }
        SetTimer(hwnd, TIMER_POLL, 100, None);
        refresh(hwnd);
        if let Some(message) = error {
            warn(hwnd, &message);
        }
    }

    unsafe fn warn(hwnd: HWND, message: &str) {
        MessageBoxW(hwnd, wide(message).as_ptr(), wide("DocFit 사이드바").as_ptr(), MB_OK | MB_ICONWARNING);
    }

    unsafe fn refresh(hwnd: HWND) {
        write_state();
        let buttons = with(|s| s.buttons.clone()).unwrap_or_default();
        for b in buttons {
            InvalidateRect(b, null(), 0);
        }
        InvalidateRect(hwnd, null(), 0);
    }

    /// 에이전트·시험이 읽는 상태 파일(`--state-file`). 바뀔 때만 쓴다.
    fn write_state() {
        with(|s| {
            let Some(path) = &s.config.state_file else { return };
            let menus: serde_json::Map<String, serde_json::Value> = s.entries.iter()
                .map(|e| (e.id.to_string(), s.menus.get(&e.id).copied().unwrap_or_default().name().into()))
                .collect();
            let body = serde_json::json!({
                "visible": s.visible, "auto_hide": s.prefs.auto_hide, "menus": menus, "notice": s.notice,
            }).to_string();
            if body != s.written {
                let temporary = path.with_extension("tmp");
                if std::fs::write(&temporary, &body).and_then(|_| std::fs::rename(&temporary, path)).is_ok() {
                    s.written = body;
                }
            }
        });
    }

    // ---- 주기 점검 ----

    unsafe fn auto_hide_input(hwnd: HWND, auto_hide: bool) -> sidebar::HideInput {
        let (work, monitor) = work_area(hwnd);
        let dpi = GetDpiForWindow(hwnd).max(96);
        let own = dock::Rect { left: work.left, top: work.top, right: work.left + scale(sidebar::WIDTH, dpi), bottom: work.bottom };
        let screen = dock::Rect { left: monitor.left, top: monitor.top, right: monitor.right, bottom: monitor.bottom };
        let mut point = POINT { x: i32::MIN, y: i32::MIN };
        GetCursorPos(&mut point);
        let foreground = GetForegroundWindow();
        let mut input = sidebar::HideInput {
            auto_hide,
            pointer_over: point.x >= own.left && point.x < own.right && point.y >= own.top && point.y < own.bottom,
            pointer_on_edge: dock::pointer_on_reveal_edge((point.x, point.y), screen, dock::Edge::Left, 2),
            focused: !foreground.is_null() && (foreground == hwnd || GetWindow(foreground, GW_OWNER) == hwnd),
            ..Default::default()
        };
        // 바탕화면·작업 표시줄은 화면을 덮어도 작업 창이 아니다.
        let class = if foreground.is_null() { String::new() } else { window_text(GetClassNameW, foreground) };
        if !input.focused && !matches!(class.as_str(), "" | "Progman" | "WorkerW" | "Shell_TrayWnd") {
            let mut r: RECT = std::mem::zeroed();
            if GetWindowRect(foreground, &mut r) != 0 && IsIconic(foreground) == 0 {
                input.covers = dock::foreground_covers_dock(
                    dock::Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }, own);
                input.fullscreen = r.left <= monitor.left && r.top <= monitor.top && r.right >= monitor.right
                    && r.bottom >= monitor.bottom;
            }
        }
        input
    }

    unsafe fn poll(hwnd: HWND) {
        let Some((ticks, auto_hide, launching, visible)) = with(|s| {
            s.ticks = s.ticks.wrapping_add(1);
            let launching = s.menus.values().any(|m| matches!(m, MenuState::Launching { .. }));
            (s.ticks, s.prefs.auto_hide, launching, s.visible)
        }) else { return };

        let show = sidebar::next_visible(visible, auto_hide_input(hwnd, auto_hide));
        if show != visible {
            with(|s| s.visible = show);
            if show {
                layout(hwnd);
            }
            ShowWindow(hwnd, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
            write_state();
        }

        // 포인터가 사이드바를 벗어나면 강조를 지운다.
        let mut point: POINT = std::mem::zeroed();
        GetCursorPos(&mut point);
        let hover = with(|s| s.hover).unwrap_or(null_mut());
        if !hover.is_null() {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(hover, &mut r);
            if point.x < r.left || point.x >= r.right || point.y < r.top || point.y >= r.bottom {
                with(|s| s.hover = null_mut());
                InvalidateRect(hover, null(), 0);
            }
        }

        // 빠른 박자(150ms)에서 창 훑기는 여는 중일 때만 매번, 그 밖에는 약 1초마다.
        if !launching && ticks % 7 != 0 {
            return;
        }
        let current = with(|s| stamp(&s.config.env.settings)).flatten();
        if with(|s| s.stamp != current).unwrap_or(false) {
            let changed = with(|s| {
                s.stamp = current;
                let next = read_prefs(&s.config.env.settings);
                let menu = next.hidden != s.prefs.hidden || next.developer != s.prefs.developer
                    || next.auto_hide != s.prefs.auto_hide || next.dark != s.prefs.dark;
                s.prefs = next;
                menu
            }).unwrap_or(false);
            if changed {
                rebuild(hwnd);
                write_state();
            }
        }
        let windows = visible_windows();
        let now = GetTickCount64();
        let (changed, failure) = with(|s| {
            let mut changed = false;
            let mut failure = None;
            for e in s.entries.clone() {
                let state = s.menus.get(&e.id).copied().unwrap_or_default();
                let exit = s.children.get_mut(&e.id)
                    .and_then(|c| c.try_wait().ok().flatten())
                    .map(|status| status.code().unwrap_or(-1));
                let (next, failed) = sidebar::observe(state, find(&windows, e.id).is_some(), exit, now);
                if exit.is_some() {
                    s.children.remove(&e.id);
                }
                if next != state {
                    s.menus.insert(e.id, next);
                    changed = true;
                }
                if let Some(f) = failed {
                    failure = Some(match f {
                        sidebar::Failure::Exited(code) => format!("{} 창이 열리지 않고 끝났습니다(종료 코드 {code}).", sidebar::label(e.id)),
                        sidebar::Failure::Timeout => format!("{} 창이 열리지 않습니다.", sidebar::label(e.id)),
                    });
                }
            }
            if failure.is_some() {
                s.notice = failure.clone();
            }
            (changed, failure)
        }).unwrap_or((false, None));
        if changed {
            refresh(hwnd);
        }
        if let Some(message) = failure {
            warn(hwnd, &message);
        }
    }

    unsafe fn toggle_auto_hide(hwnd: HWND) {
        let Some((path, next)) = with(|s| (s.config.env.settings.clone(), !s.prefs.auto_hide)) else { return };
        // 설정 소유자(docfit_config)로 저장한다: 앱과 같은 잠금·원자 교체, 다른 키는 보존.
        match docfit_config::set(&path, &serde_json::json!({ "dock_auto_hide": next })) {
            Ok(_) => {
                with(|s| {
                    s.prefs.auto_hide = next;
                    s.stamp = stamp(&path);
                });
                PostMessageW(hwnd, WM_REBUILD, 0, 0);
            }
            Err(e) => warn(hwnd, &format!("설정을 저장하지 못했습니다.\n{}", e.message)),
        }
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match msg {
            WM_PAINT => {
                paint(hwnd);
                0
            }
            WM_DRAWITEM => {
                draw_item(&*(l as *const DRAWITEMSTRUCT));
                1
            }
            WM_COMMAND => {
                let id = w & 0xFFFF;
                match id {
                    ID_PIN => toggle_auto_hide(hwnd),
                    ID_CLOSE => {
                        DestroyWindow(hwnd);
                    }
                    // Enter 는 초점 있는 단추를 누른다.
                    id if id == IDOK as usize => {
                        let focus = GetFocus();
                        if !focus.is_null() && GetParent(focus) == hwnd {
                            PostMessageW(focus, BM_CLICK, 0, 0);
                        }
                    }
                    // IDCANCEL(Esc)은 닫지 않는다: 사이드바는 계속 떠 있다.
                    id if id >= ID_MENU_BASE => open(hwnd, (id - ID_MENU_BASE) as u8),
                    _ => {}
                }
                0
            }
            WM_SETCURSOR => {
                let over = w as HWND;
                let previous = with(|s| std::mem::replace(&mut s.hover, if over == hwnd { null_mut() } else { over }));
                if let Some(previous) = previous {
                    if previous != over {
                        InvalidateRect(over, null(), 0);
                        if !previous.is_null() {
                            InvalidateRect(previous, null(), 0);
                        }
                    }
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_TIMER => {
                poll(hwnd);
                SetTimer(hwnd, TIMER_POLL, 150, None);
                0
            }
            WM_SHOW_SIDEBAR => {
                with(|s| s.visible = true);
                layout(hwnd);
                ShowWindow(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
                write_state();
                0
            }
            WM_REBUILD => {
                rebuild(hwnd);
                write_state();
                0
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                layout(hwnd);
                0
            }
            WM_DESTROY => {
                // 띄운 창은 독립 프로세스라 그대로 둔다.
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }

    // ---- 시작 ----

    struct Args {
        config: Config,
        capture: Option<PathBuf>,
        dpi: u32,
        height: i32,
    }

    fn parse(args: impl Iterator<Item = std::ffi::OsString>) -> Result<Args, String> {
        let mut values: BTreeMap<String, std::ffi::OsString> = BTreeMap::new();
        let mut args = args;
        while let Some(name) = args.next() {
            let name = name.to_string_lossy().into_owned();
            if !["--settings", "--resources", "--app", "--python", "--source-root", "--state-file", "--capture", "--dpi",
                 "--height"].contains(&name.as_str()) {
                return Err(format!("알 수 없는 인자: {name}"));
            }
            let value = args.next().ok_or_else(|| format!("{name} 값이 없습니다"))?;
            values.insert(name, value);
        }
        let number = |key: &str, default: i32| -> Result<i32, String> {
            match values.get(key) {
                None => Ok(default),
                Some(v) => v.to_string_lossy().parse().map_err(|_| format!("잘못된 {key}")),
            }
        };
        let dpi = number("--dpi", 96)?.clamp(96, 384) as u32;
        let height = number("--height", 1040)?.clamp(300, 4000);
        let path = |key: &str| values.get(key).map(PathBuf::from);
        let source = match (path("--python"), path("--source-root")) {
            (Some(python), Some(root)) => Some((python, root)),
            (None, None) => None,
            _ => return Err("--python 과 --source-root 는 함께 지정합니다".into()),
        };
        let resources = path("--resources")
            .or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)))
            .ok_or("실행 위치를 알 수 없습니다")?;
        let settings = path("--settings").unwrap_or_else(docfit_config::settings_path);
        Ok(Args {
            config: Config {
                env: Env { resources, settings, app_override: path("--app"), source },
                state_file: path("--state-file"),
            },
            capture: path("--capture"),
            dpi,
            height,
        })
    }

    /// 같은 설정 파일의 사이드바는 하나만 뜬다(시험용 설정은 사용자의 사이드바와 섞이지 않는다).
    fn instance_key(settings: &Path) -> String {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for b in settings.to_string_lossy().to_lowercase().bytes() {
            hash = (hash ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        format!("{hash:016x}")
    }

    unsafe fn capture(args: &Args, out: &Path) -> Result<(), String> {
        let prefs = read_prefs(&args.config.env.settings);
        let dpi = args.dpi;
        let entries = sidebar::entries(&prefs.hidden, prefs.developer);
        let (w, h) = (scale(sidebar::WIDTH, dpi), scale(args.height, dpi));
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = w;
        info.bmiHeader.biHeight = -h;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        let dc = CreateCompatibleDC(null_mut());
        let mut bits: *mut core::ffi::c_void = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            DeleteDC(dc);
            return Err("메모리 비트맵 생성 실패".into());
        }
        let old = SelectObject(dc, bitmap);
        let p = palette(prefs.dark);
        let rows = sidebar::rows(&entries, args.height);
        draw_background(dc, RECT { left: 0, top: 0, right: w, bottom: h }, dpi, p, &entries, &rows);
        for (i, (e, (y0, y1))) in entries.iter().zip(&rows).enumerate() {
            let rect = RECT { left: 0, top: scale(*y0, dpi), right: w - 1, bottom: scale(*y1, dpi) };
            let state = match i { 0 => MenuState::Open, 1 => MenuState::Launching { since_ms: 0 }, _ => MenuState::Closed };
            draw_row(dc, rect, dpi, p, e.id, state, i == 2, false);
        }
        let side = scale(26, dpi);
        let top = (scale(sidebar::HEADER, dpi) - side) / 2;
        for (id, left) in [(ID_PIN, w - side * 2 - scale(8, dpi)), (ID_CLOSE, w - side - scale(6, dpi))] {
            let rect = RECT { left, top, right: left + side, bottom: top + side };
            draw_header_button(dc, rect, dpi, p, id, prefs.auto_hide, false);
        }
        GdiFlush();
        let pixels = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        let rgb: Vec<u8> = pixels.chunks_exact(4).flat_map(|q| [q[2], q[1], q[0]]).collect();
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        std::fs::write(out, dock::encode_png(w as u32, h as u32, 3, &rgb)).map_err(|e| e.to_string())
    }

    pub fn run() {
        unsafe {
            let args = match parse(std::env::args_os().skip(1)) {
                Ok(args) => args,
                Err(message) => {
                    warn(null_mut(), &message);
                    std::process::exit(2);
                }
            };
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            load_icon_font();
            if let Some(out) = args.capture.clone() {
                std::process::exit(if capture(&args, &out).is_ok() { 0 } else { 1 });
            }
            let key = instance_key(&args.config.env.settings);
            let title = format!("DocFit 사이드바 {key}");
            let mutex = CreateMutexW(null(), 0, wide(&format!("Local\\HwpAutoDocFit.Sidebar.{key}")).as_ptr());
            if mutex.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
                // 먼저 뜬 쪽이 창을 만드는 중일 수 있어 잠깐 기다린다.
                for _ in 0..40 {
                    let existing = FindWindowW(wide(CLASS).as_ptr(), wide(&title).as_ptr());
                    if !existing.is_null() {
                        let mut pid = 0;
                        GetWindowThreadProcessId(existing, &mut pid);
                        AllowSetForegroundWindow(pid);
                        PostMessageW(existing, WM_SHOW_SIDEBAR, 0, 0);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                return;
            }
            let instance = GetModuleHandleW(null());
            let class = wide(CLASS);
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(proc),
                hInstance: instance,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_CONTROLPARENT, class.as_ptr(),
                                       wide(&title).as_ptr(), WS_POPUP | WS_CLIPCHILDREN, 0, 0, 1, 1, null_mut(),
                                       null_mut(), instance, null());
            if hwnd.is_null() {
                return;
            }
            let prefs = read_prefs(&args.config.env.settings);
            let stamp = stamp(&args.config.env.settings);
            STATE.with(|s| {
                *s.borrow_mut() = Some(State {
                    config: args.config,
                    prefs,
                    stamp,
                    entries: Vec::new(),
                    menus: BTreeMap::new(),
                    children: BTreeMap::new(),
                    buttons: Vec::new(),
                    hover: null_mut(),
                    visible: true,
                    ticks: 0,
                    notice: None,
                    written: String::new(),
                });
            });
            rebuild(hwnd);
            ShowWindow(hwnd, SW_SHOW);
            write_state();
            SetTimer(hwnd, TIMER_POLL, 150, None);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                // Tab·화살표 키 이동과 Enter·Space 누름.
                if IsDialogMessageW(hwnd, &msg) == 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}
