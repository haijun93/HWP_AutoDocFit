//! 빠른 목록의 그림 미리보기 창(사용자 요청 2026-11-13): 그림 항목을 마우스로 가리키거나 키보드로 고르면
//! 축소 스냅샷을 커서(또는 고른 줄) 옆에 띄우고, 벗어나면 숨긴다.
//!
//! 미리보기 상태 기계(`step`, 순수 함수 — 창·캐시와 떨어져 시험한다):
//! | 지금 \ 사건 | 가리킴 없음/그림 아님 | 그림 t 가리킴(캐시 있음) | 그림 t 가리킴(캐시 없음) | 썸네일 준비(t) | 숨기기 |
//! |---|---|---|---|---|---|
//! | `Hidden` | 그대로 | `Showing(t)`·보이기 | `Waiting(t)`·만들기 요청 | 무시(늦게 온 결과) | 그대로 |
//! | `Waiting(u)` | `Hidden`·숨기기 | u==t: 그대로 / 아니면 `Showing(t)`·보이기 | u==t: 그대로 / 아니면 `Waiting(t)`·요청 | u==t: 성공 `Showing`·보이기, 실패 `Hidden` / 아니면 무시 | `Hidden` |
//! | `Showing(u)` | `Hidden`·숨기기 | u==t: 위치만 옮김 / 아니면 `Showing(t)`·보이기 | u==t: 옮김 / 아니면 `Waiting(t)`·숨기고 요청 | 무시 | `Hidden`·숨기기 |
//! 썸네일은 작업 스레드 하나가 만든다(필요할 때 처음 만들고, 밀린 요청은 마지막 것만 처리).
//! 창은 포커스·마우스를 가져가지 않는다(WS_EX_NOACTIVATE·WS_EX_TRANSPARENT) — 빠른 목록의 키보드·마우스 그대로.

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::category::Category;
use crate::store::Summary;
use crate::util::wide;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub id: i64,
    pub size: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Hidden,
    Waiting(Target),
    Showing(Target),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// 마우스·키보드가 가리키는 항목(그림이 아니면 None)과 그 썸네일이 메모리 캐시에 있는지.
    Point { target: Option<Target>, cached: bool },
    /// 작업 스레드가 썸네일을 만들었다(실패면 ok=false).
    Ready { id: i64, ok: bool },
    Hide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Request(Target),
    Show(Target),
    Move,
    Hide,
}

pub fn step(state: State, ev: Event) -> (State, Action) {
    use Action::*;
    use State::*;
    match ev {
        Event::Hide | Event::Point { target: None, .. } => {
            (Hidden, if state == Hidden { Nothing } else { Hide })
        }
        Event::Point { target: Some(t), cached } => match state {
            Showing(u) if u == t => (state, Move),
            Waiting(u) if u == t => (state, Nothing),
            _ if cached => (Showing(t), Show(t)),
            _ => (Waiting(t), Request(t)),
        },
        Event::Ready { id, ok } => match state {
            Waiting(t) if t.id == id => if ok { (Showing(t), Show(t)) } else { (Hidden, Hide) },
            _ => (state, Nothing),
        },
    }
}

/// 미리보기할 그림 항목인지: 저장한 그림, 또는 그림 파일이 든 파일 항목(화면캡처 폴더·탐색기 복사).
pub fn target_of(item: &Summary) -> Option<Target> {
    let image = item.kind == "image" || (item.kind == "files" && item.cats.contains(Category::Image));
    image.then_some(Target { id: item.id, size: item.size })
}

/// 창 위치: 기준점 오른쪽(`right`+간격)에 두고, 넘치면 왼쪽(`left`−간격−폭)으로. 세로는 작업 영역 안으로.
pub fn place(right: i32, left: i32, top: i32, w: i32, h: i32, work: (i32, i32, i32, i32), gap: i32) -> (i32, i32) {
    let (wl, wt, wr, wb) = work;
    let mut x = right + gap;
    if x + w > wr {
        x = left - gap - w;
    }
    let x = x.min(wr - w).max(wl);
    let y = top.min(wb - h).max(wt);
    (x, y)
}

/// 기준 위치: 마우스 커서(커서 아래 오른쪽) 또는 키보드로 고른 줄(빠른 목록 창 옆).
#[derive(Clone, Copy)]
pub enum Anchor {
    Cursor(POINT),
    /// 고른 줄의 화면 y, 빠른 목록 창의 화면 왼쪽·오른쪽 x.
    Row { y: i32, left: i32, right: i32 },
}

struct Shared {
    state: State,
    anchor: Option<Anchor>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { state: State::Hidden, anchor: None });
static HWND_PREVIEW: AtomicIsize = AtomicIsize::new(0);
static WORKER: Mutex<Option<Sender<Target>>> = Mutex::new(None);
const CLASS: &str = "HwpAutoDocFit.KAgentPreview";
const WM_READY: u32 = WM_APP + 40;
const PAD: i32 = 4;
const GAP: i32 = 16;

fn hwnd() -> HWND {
    HWND_PREVIEW.load(Ordering::Relaxed) as HWND
}

pub fn register(instance: HINSTANCE) {
    unsafe {
        let class = wide(CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED;
        let h = CreateWindowExW(ex, class.as_ptr(), wide("그림 미리보기").as_ptr(), WS_POPUP, 0, 0, 1, 1,
                                std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
        if !h.is_null() {
            // WS_EX_TRANSPARENT로 마우스가 지나가게 하려면 계층 창이어야 한다(불투명 255).
            SetLayeredWindowAttributes(h, 0, 255, LWA_ALPHA);
        }
        HWND_PREVIEW.store(h as isize, Ordering::Relaxed);
    }
}

/// 마우스·키보드가 가리키는 항목이 바뀌거나 움직일 때 부른다(창 스레드).
pub fn point(target: Option<Target>, anchor: Anchor) {
    let cached = target.is_some_and(|t| crate::thumb::is_cached(t.id, t.size));
    let action = {
        let mut sh = SHARED.lock().unwrap();
        sh.anchor = Some(anchor);
        let (next, action) = step(sh.state, Event::Point { target, cached });
        sh.state = next;
        action
    };
    run(action);
}

pub fn hide() {
    let action = {
        let mut sh = SHARED.lock().unwrap();
        let (next, action) = step(sh.state, Event::Hide);
        sh.state = next;
        action
    };
    run(action);
}

fn run(action: Action) {
    match action {
        Action::Nothing => {}
        Action::Hide => unsafe {
            ShowWindow(hwnd(), SW_HIDE);
        },
        Action::Request(t) => {
            unsafe { ShowWindow(hwnd(), SW_HIDE) };
            request(t);
        }
        Action::Show(t) => show(t),
        Action::Move => {
            if let Some(t) = match SHARED.lock().unwrap().state { State::Showing(t) => Some(t), _ => None } {
                show(t);
            }
        }
    }
}

fn request(t: Target) {
    let mut guard = WORKER.lock().unwrap();
    if guard.as_ref().is_none_or(|tx| tx.send(t).is_err()) {
        let (tx, rx) = std::sync::mpsc::channel::<Target>();
        let spawned = std::thread::Builder::new().name("kagent-thumb".into()).spawn(move || {
            while let Ok(mut t) = rx.recv() {
                // 밀린 요청은 마지막 것만(마우스로 여러 줄을 빠르게 지나간 경우).
                while let Ok(next) = rx.try_recv() {
                    t = next;
                }
                let ok = crate::thumb::get(t.id, t.size).is_some();
                unsafe { PostMessageW(hwnd(), WM_READY, t.id as usize, ok as isize) };
            }
        });
        if spawned.is_ok() && tx.send(t).is_ok() {
            *guard = Some(tx);
        }
    }
}

fn show(t: Target) {
    let Some(thumb) = crate::thumb::cached(t.id, t.size) else { return };
    let Some(anchor) = SHARED.lock().unwrap().anchor else { return };
    let h = hwnd();
    unsafe {
        let (w, ht) = (thumb.w as i32 + PAD * 2, thumb.h as i32 + PAD * 2);
        let (right, left, top, probe) = match anchor {
            Anchor::Cursor(p) => (p.x + 4, p.x - 4, p.y + GAP, p),
            Anchor::Row { y, left, right } => (right, left, y, POINT { x: right, y }),
        };
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromPoint(probe, MONITOR_DEFAULTTONEAREST), &mut mi);
        let wa = mi.rcWork;
        let (x, y) = place(right, left, top, w, ht, (wa.left, wa.top, wa.right, wa.bottom), GAP / 2);
        SetWindowPos(h, HWND_TOPMOST, x, y, w, ht, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        InvalidateRect(h, std::ptr::null(), 0);
    }
}

unsafe fn paint(h: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(h, &mut ps);
        if dc.is_null() {
            return;
        }
        let mut rc: RECT = std::mem::zeroed();
        GetClientRect(h, &mut rc);
        let bg = CreateSolidBrush(0x00FFFFFF);
        FillRect(dc, &rc, bg);
        DeleteObject(bg);
        let border = CreateSolidBrush(0x00B4B4B4);
        FrameRect(dc, &rc, border);
        DeleteObject(border);
        let shown = match SHARED.lock().unwrap().state { State::Showing(t) => Some(t), _ => None };
        if let Some(thumb) = shown.and_then(|t| crate::thumb::cached(t.id, t.size)) {
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: thumb.w as i32,
                biHeight: -(thumb.h as i32), biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..std::mem::zeroed()
            };
            SetDIBitsToDevice(dc, PAD, PAD, thumb.w, thumb.h, 0, 0, 0, thumb.h, thumb.bgra.as_ptr() as _, &bi,
                              DIB_RGB_COLORS);
        }
        EndPaint(h, &ps);
    }
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                paint(h);
                0
            }
            WM_ERASEBKGND => 1,
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            WM_NCHITTEST => HTTRANSPARENT as LRESULT,
            WM_READY => {
                let action = {
                    let mut sh = SHARED.lock().unwrap();
                    let (next, action) = step(sh.state, Event::Ready { id: wp as i64, ok: lp != 0 });
                    sh.state = next;
                    action
                };
                run(action);
                0
            }
            _ => DefWindowProcW(h, msg, wp, lp),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;
    use State::*;

    const A: Target = Target { id: 1, size: 10 };
    const B: Target = Target { id: 2, size: 20 };

    fn at(t: Option<Target>, cached: bool) -> Event {
        Event::Point { target: t, cached }
    }

    #[test]
    fn hover_uncached_image_requests_then_shows_when_ready() {
        let (s, a) = step(Hidden, at(Some(A), false));
        assert_eq!((s, a), (Waiting(A), Request(A)));
        let (s, a) = step(s, Event::Ready { id: 1, ok: true });
        assert_eq!((s, a), (Showing(A), Show(A)));
        // 같은 항목 위에서 움직이면 위치만 옮긴다.
        assert_eq!(step(s, at(Some(A), true)), (Showing(A), Move));
    }

    #[test]
    fn cached_image_shows_immediately_and_leaving_hides() {
        let (s, a) = step(Hidden, at(Some(A), true));
        assert_eq!((s, a), (Showing(A), Show(A)));
        assert_eq!(step(s, at(None, false)), (Hidden, Hide));
        assert_eq!(step(s, Event::Hide), (Hidden, Hide));
        assert_eq!(step(Hidden, Event::Hide), (Hidden, Nothing));
        assert_eq!(step(Hidden, at(None, false)), (Hidden, Nothing));
    }

    #[test]
    fn late_or_failed_results_never_show_the_wrong_picture() {
        // A를 기다리는 중 B로 옮김 → A 결과는 무시, B 결과만 보인다.
        let (s, _) = step(Hidden, at(Some(A), false));
        let (s, a) = step(s, at(Some(B), false));
        assert_eq!((s, a), (Waiting(B), Request(B)));
        assert_eq!(step(s, Event::Ready { id: 1, ok: true }), (Waiting(B), Nothing));
        assert_eq!(step(s, Event::Ready { id: 2, ok: false }), (Hidden, Hide));
        // 숨긴 뒤 도착한 결과도 무시.
        assert_eq!(step(Hidden, Event::Ready { id: 2, ok: true }), (Hidden, Nothing));
        // 보이는 중에 다른 그림(캐시 없음)으로 → 기다림(창은 숨기고 요청).
        assert_eq!(step(Showing(A), at(Some(B), false)), (Waiting(B), Request(B)));
        assert_eq!(step(Showing(A), at(Some(B), true)), (Showing(B), Show(B)));
        // 같은 그림을 기다리는 중 다시 가리켜도 요청을 되풀이하지 않는다.
        assert_eq!(step(Waiting(A), at(Some(A), false)), (Waiting(A), Nothing));
    }

    #[test]
    fn only_image_items_are_targets() {
        let mut item = Summary { id: 5, kind: "image".into(), size: 99, ..Default::default() };
        assert_eq!(target_of(&item), Some(Target { id: 5, size: 99 }));
        item.kind = "text".into();
        assert_eq!(target_of(&item), None);
        item.kind = "files".into();
        assert_eq!(target_of(&item), None);
        item.cats = crate::category::CategorySet::of(&[Category::All, Category::File, Category::Image]);
        assert_eq!(target_of(&item), Some(Target { id: 5, size: 99 }));
    }

    #[test]
    fn placement_prefers_right_then_left_and_stays_on_screen() {
        let work = (0, 0, 1000, 800);
        assert_eq!(place(100, 100, 50, 200, 100, work, 8), (108, 50));
        // 오른쪽에 자리가 없으면 왼쪽 기준점 왼편.
        assert_eq!(place(900, 600, 50, 200, 100, work, 8), (392, 50));
        // 아래로 넘치면 작업 영역 안으로 올린다.
        assert_eq!(place(100, 100, 780, 200, 100, work, 8), (108, 700));
        // 양쪽 다 모자라면 화면 안에 붙인다.
        assert_eq!(place(950, 20, 0, 300, 100, work, 8), (0, 0));
    }
}
