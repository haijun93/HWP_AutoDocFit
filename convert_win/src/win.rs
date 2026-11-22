use crate::logic::{self, Event, FileResult, FileStatus, Hit, Layout, Model, TARGETS};
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf, ptr::null_mut, sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex}};
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

struct State { model: Model, results: Arc<Mutex<Vec<FileResult>>>, cancel: Option<Arc<AtomicBool>>, worker_done: bool, close_requested:bool }
impl Default for State { fn default()->Self { Self { model:Model::default(), results:Arc::new(Mutex::new(Vec::new())), cancel:None, worker_done:true, close_requested:false } } }
const WM_TIMER: u32 = 0x0113;
const WM_APP: u32 = 0x8000;
const WM_WORKER_DONE: u32 = WM_APP + 1;
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
        let selected = i == st.model.target;
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
    let results = st.results.lock().unwrap();
    let head = if st.worker_done { st.model.header() } else { format!("변환 진행 중  ·  {}/{}", results.iter().filter(|x| matches!(x.status,FileStatus::Succeeded|FileStatus::Failed(_)|FileStatus::Cancelled)).count(), results.len()) };
    text(hdc, 28, 198, &head, color(45, 55, 70));
    fill(hdc, lay.list_rect(), color(255, 255, 255));
    if results.is_empty() {
        text(hdc, 48, 250, "여기에 파일을 놓으세요", color(130, 140, 154));
    } else {
        for (i, item) in results.iter().take(lay.visible_rows()).enumerate() {
            let name=item.source.file_name().unwrap_or_default().to_string_lossy();
            let (mark,c)=match &item.status { FileStatus::Waiting=>("대기",color(100,110,125)), FileStatus::Running=>("진행",color(20,95,190)), FileStatus::Succeeded=>("완료",color(20,130,70)), FileStatus::Failed(_)=>("실패",color(190,45,45)), FileStatus::Cancelled=>("취소",color(130,100,40)) };
            text(hdc, 48, logic::ROW_TOP + i as i32 * logic::ROW_STEP, &format!("{mark}  {name}"), c);
        }
        if results.len()>lay.visible_rows() {
            text(hdc,48,logic::ROW_TOP+lay.visible_rows() as i32*logic::ROW_STEP,&format!("외 {}개",results.len()-lay.visible_rows()),color(130,140,154));
        }
    }
    drop(results);
    let cr=lay.cancel_rect(); fill(hdc,cr,color(225,229,235)); text(hdc,cr.left+24,cr.top+15,"취소",color(45,55,70));
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
fn conversion_temps(dir:&std::path::Path)->std::collections::HashSet<PathBuf> {
    std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e|e.path()).filter(|p| {
        let n=p.file_name().unwrap_or_default().to_string_lossy();
        n.starts_with("docfit_pdf_") || n.starts_with("docfit_hwp_") || n.starts_with(".docfit_")
    }).collect()
}
fn remove_new_conversion_temps(dir:&std::path::Path,before:&std::collections::HashSet<PathBuf>) {
    for p in conversion_temps(dir).difference(before) { let _=std::fs::remove_file(p); }
}
fn start_conversion(hwnd: HWND, s: &mut State) {
    if !s.worker_done { return; }
    if s.model.files.is_empty() {
        message(hwnd, "먼저 파일을 끌어다 놓으세요.", 0x40);
        return;
    }
    let exe = logic::resolve_engine(
        std::env::var_os("DOCFIT_APP_EXE").map(PathBuf::from),
        std::env::current_exe().ok(),
        |p| p.is_file(),
    );
    match exe {
        Some(exe) => {
            let files=s.model.files.clone(); let format=s.model.format().to_string();
            *s.results.lock().unwrap()=logic::results(&files);
            let shared=s.results.clone(); let cancel=Arc::new(AtomicBool::new(false)); s.cancel=Some(cancel.clone()); s.worker_done=false;
            let hwnd_num=hwnd as usize;
            std::thread::spawn(move || {
                for (i,file) in files.iter().enumerate() {
                    if cancel.load(Ordering::Acquire) { let mut r=shared.lock().unwrap(); logic::transition(&mut r,Event::Cancel(i)); break; }
                    { let mut r=shared.lock().unwrap(); logic::transition(&mut r,Event::Start(i)); }
                    let output_dir=file.parent().unwrap_or_else(||std::path::Path::new("."));
                    let temps_before=conversion_temps(output_dir);
                    let args=logic::engine_file_args(file,&format);
                    let mut child=match std::process::Command::new(&exe).args(args).spawn() {
                        Ok(c)=>c, Err(e)=>{let mut r=shared.lock().unwrap(); if let Some(x)=r.get_mut(i){x.status=FileStatus::Failed(e.to_string())}; continue}
                    };
                    loop {
                        if cancel.load(Ordering::Acquire) { let _=child.kill(); let _=child.wait(); remove_new_conversion_temps(output_dir,&temps_before); let mut r=shared.lock().unwrap(); logic::transition(&mut r,Event::Cancel(i)); break; }
                        match child.try_wait() { Ok(Some(status))=>{let mut r=shared.lock().unwrap(); logic::transition(&mut r,if status.success(){Event::Success(i)}else{Event::Failure(i)}); break}, Ok(None)=>std::thread::sleep(std::time::Duration::from_millis(100)), Err(e)=>{let _=child.kill();let mut r=shared.lock().unwrap(); if let Some(x)=r.get_mut(i){x.status=FileStatus::Failed(e.to_string())};break} }
                    }
                    if cancel.load(Ordering::Acquire) { break; }
                }
                unsafe { PostMessageW(hwnd_num as HWND, WM_WORKER_DONE, 0, 0); }
            });
            unsafe { SetTimer(hwnd,1,100,None); }
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
                state(hwnd).model.add_dropped(paths, |p| p.is_file());
                InvalidateRect(hwnd, null_mut(), 1);
                0
            }
            WM_LBUTTONUP => {
                let x = (lp & 0xffff) as i16 as i32;
                let y = ((lp >> 16) & 0xffff) as i16 as i32;
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                match Layout::new(r.right, r.bottom).hit(x, y) {
                    Hit::Target(i) => state(hwnd).model.select(i),
                    Hit::Start => start_conversion(hwnd, state(hwnd)),
                    Hit::Cancel => if let Some(c)=&state(hwnd).cancel { c.store(true,Ordering::Release); },
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
            WM_TIMER => { InvalidateRect(hwnd,null_mut(),1); 0 }
            WM_CLOSE => { let st=state(hwnd); if st.worker_done { DestroyWindow(hwnd); } else { st.close_requested=true; if let Some(c)=&st.cancel { c.store(true,Ordering::Release); } } 0 }
            WM_WORKER_DONE => { let st=state(hwnd); st.worker_done=true; st.cancel=None; KillTimer(hwnd,1); if st.close_requested { DestroyWindow(hwnd); } else { InvalidateRect(hwnd,null_mut(),1); } 0 }
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
        let state=State { model:st.clone(), results:Arc::new(Mutex::new(logic::results(&st.files))), cancel:None, worker_done:true, close_requested:false };
        draw(hdc, 1050, 650, &state);
        st.files.len()
    })?;
    grid_core::png::write(path, shot.width, shot.height, &shot.bgra)?;
    Ok(path.to_path_buf())
}

pub fn capture_status(path:&std::path::Path, mode:&str)->Result<PathBuf,String> {
    let model=logic::sample_model(); let mut items=logic::results(&model.files);
    let (done,active)=match mode { "progress"=>(0,0), "complete"=>(3,3), "failure"=>(1,1), _=>return Err("상태는 progress, complete, failure 중 하나여야 합니다".into()) };
    for i in 0..done { logic::transition(&mut items,Event::Start(i)); logic::transition(&mut items,Event::Success(i)); }
    if mode=="progress" { logic::transition(&mut items,Event::Start(active)); }
    if mode=="failure" { logic::transition(&mut items,Event::Start(active)); logic::transition(&mut items,Event::Failure(active)); }
    if let Some(parent)=path.parent(){std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;}
    let st=State { model, results:Arc::new(Mutex::new(items)), cancel:None, worker_done:mode!="progress", close_requested:false };
    let shot=grid_core::gdi::render(1050,650,|hdc|{draw(hdc,1050,650,&st);3})?;
    grid_core::png::write(path,shot.width,shot.height,&shot.bgra)?; Ok(path.to_path_buf())
}
