use crate::logic::{self, Event, FileResult, FileStatus, Hit, Layout, Model, TARGETS};
use std::{ffi::OsString, os::windows::ffi::{OsStringExt, OsStrExt}, path::PathBuf, ptr::null_mut, sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex}};
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
const WM_OPEN_REQUEST: u32 = WM_APP + 2;
const PIPE_NAME: &str = r"\\.\pipe\HwpAutoDocFit.convert";
struct OpenRequest { target: String, paths: Vec<PathBuf> }
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn open_path(path:&std::path::Path, folder:bool) {
    let Some(candidate)=logic::validated_open_path(path,folder,|p|p.is_file(),|p|p.is_dir()) else { return; };
    let Ok(valid)=candidate.canonicalize() else { return; };
    let target:Vec<u16>=valid.as_os_str().encode_wide().chain(Some(0)).collect();
    let verb=wide("open");
    unsafe { windows_sys::Win32::UI::Shell::ShellExecuteW(null_mut(),verb.as_ptr(),target.as_ptr(),null_mut(),null_mut(),1); }
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
    let head = if st.worker_done { st.model.header() } else { format!("변환 진행 중  ·  {}/{}", results.iter().filter(|x| matches!(x.status,FileStatus::Succeeded(_)|FileStatus::Failed(_)|FileStatus::Cancelled)).count(), results.len()) };
    text(hdc, 28, 198, &head, color(45, 55, 70));
    fill(hdc, lay.list_rect(), color(255, 255, 255));
    if results.is_empty() {
        text(hdc, 48, 250, "여기에 파일을 놓으세요", color(130, 140, 154));
    } else {
        for (i, item) in results.iter().take(lay.visible_rows()).enumerate() {
            let name=item.source.file_name().unwrap_or_default().to_string_lossy();
            let (mark,c,detail)=match &item.status { FileStatus::Waiting=>("대기",color(100,110,125),String::new()), FileStatus::Running=>("진행",color(20,95,190),String::new()), FileStatus::Succeeded(_)=>("완료",color(20,130,70),String::new()), FileStatus::Failed(reason)=>("실패",color(190,45,45),reason.clone()), FileStatus::Cancelled=>("취소",color(130,100,40),String::new()) };
            text(hdc, 48, logic::ROW_TOP + i as i32 * logic::ROW_STEP, &format!("{mark}  {name}  {detail}"), c);
        }
        if results.len()>lay.visible_rows() {
            text(hdc,48,logic::ROW_TOP+lay.visible_rows() as i32*logic::ROW_STEP,&format!("외 {}개",results.len()-lay.visible_rows()),color(130,140,154));
        }
    }
    drop(results);
    let cr=lay.cancel_rect(); fill(hdc,cr,color(225,229,235)); text(hdc,cr.left+24,cr.top+15,"취소",color(45,55,70));
    let fr=lay.folder_rect(); fill(hdc,fr,color(225,229,235)); text(hdc,fr.left+12,fr.top+15,"폴더 열기",color(45,55,70));
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
                    let mut child=match std::process::Command::new(&exe).args(args).stderr(std::process::Stdio::piped()).spawn() {
                        Ok(c)=>c, Err(e)=>{let mut r=shared.lock().unwrap(); if let Some(x)=r.get_mut(i){x.status=FileStatus::Failed(e.to_string())}; continue}
                    };
                    loop {
                        if cancel.load(Ordering::Acquire) { let _=child.kill(); let _=child.wait(); remove_new_conversion_temps(output_dir,&temps_before); let mut r=shared.lock().unwrap(); logic::transition(&mut r,Event::Cancel(i)); break; }
                        match child.try_wait() { Ok(Some(status))=>{let output=child.wait_with_output().ok(); let mut r=shared.lock().unwrap(); if status.success() { if let Some(x)=r.get_mut(i) { x.status=FileStatus::Succeeded(logic::output_path_for(file,&format).unwrap_or_else(||file.clone())); } } else if let Some(x)=r.get_mut(i) { x.status=FileStatus::Failed(logic::failure_reason(status.code(),&output.map(|o|String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default())); } break}, Ok(None)=>std::thread::sleep(std::time::Duration::from_millis(100)), Err(e)=>{let _=child.kill();let mut r=shared.lock().unwrap(); if let Some(x)=r.get_mut(i){x.status=FileStatus::Failed(e.to_string())};break} }
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
                    Hit::OpenFolder => { let st=state(hwnd); if let Some(item)=st.results.lock().unwrap().iter().find(|x|matches!(x.status,FileStatus::Succeeded(_))) { if let FileStatus::Succeeded(p)=&item.status { open_path(p,true); } } },
                    Hit::None => {}
                }
                InvalidateRect(hwnd, null_mut(), 1);
                0
            }
            WM_LBUTTONDBLCLK => {
                let y=((lp>>16)&0xffff) as i16 as i32;
                if y>=logic::ROW_TOP { let i=((y-logic::ROW_TOP)/logic::ROW_STEP) as usize; if let Some(item)=state(hwnd).results.lock().unwrap().get(i) { if let FileStatus::Succeeded(path)=&item.status { open_path(path,false); } } }
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
            WM_OPEN_REQUEST => { let req=Box::from_raw(wp as *mut OpenRequest); let st=state(hwnd); st.model.add_dropped(req.paths,|p|p.is_file()); if let Some(i)=TARGETS.iter().position(|(_,v)|*v==req.target) { st.model.select(i); } SetForegroundWindow(hwnd); InvalidateRect(hwnd,null_mut(),1); 0 }
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

fn pipe_server(hwnd: usize) {
    use windows_sys::Win32::{Foundation::*,Storage::FileSystem::*,System::Pipes::*};
    loop { unsafe {
        let name=wide(PIPE_NAME);
        let pipe=CreateNamedPipeW(name.as_ptr(), PIPE_ACCESS_DUPLEX, PIPE_TYPE_BYTE|PIPE_READMODE_BYTE|PIPE_WAIT, 1, 4096, 1_048_576, 0, null_mut());
        if pipe.is_null() || pipe as isize == -1 { std::thread::sleep(std::time::Duration::from_millis(250)); continue; }
        let connected=ConnectNamedPipe(pipe,null_mut())!=0 || GetLastError()==ERROR_PIPE_CONNECTED;
        if connected {
            let mut buf=vec![0u8;1_048_577]; let mut read=0;
            if ReadFile(pipe,buf.as_mut_ptr(),buf.len() as u32,&mut read,null_mut())!=0 {
                let data=&buf[..read as usize];
                let result=logic::decode_open_request(data.split(|b|*b==b'\n').next().unwrap_or(data));
                let (response, request)=match result {
                    Ok((target,paths))=>{ let valid:Vec<_>=paths.into_iter().filter(|p|p.is_file()).collect(); if valid.is_empty(){(r#"{"ok":false,"error":"no_supported_files"}"#.to_string(),None)}else{(format!(r#"{{"ok":true,"added":{}}}"#,valid.len()),Some(OpenRequest{target,paths:valid}))} },
                    Err(e)=>(format!(r#"{{"ok":false,"error":"{e}"}}"#),None),
                };
                if let Some(req)=request { PostMessageW(hwnd as HWND,WM_OPEN_REQUEST,Box::into_raw(Box::new(req)) as WPARAM,0); }
                let response=response+"\n"; let mut written=0; let _=WriteFile(pipe,response.as_ptr(),response.len() as u32,&mut written,null_mut());
            }
        }
        DisconnectNamedPipe(pipe); CloseHandle(pipe);
    } }
}

fn forward_to_instance(args:&[String]) {
    use windows_sys::Win32::{Foundation::*,Storage::FileSystem::*};
    if args.is_empty(){return;}
    let target="pdf";
    let paths:Vec<_>=args.iter().map(|p|p.as_str()).collect();
    let payload=serde_json::json!({"cmd":"convert_open","target":target,"paths":paths}).to_string()+"\n";
    unsafe {
        let name=wide(PIPE_NAME);
        for _ in 0..40 { let h=CreateFileW(name.as_ptr(),GENERIC_READ|GENERIC_WRITE,0,null_mut(),OPEN_EXISTING,0,null_mut()); if !h.is_null() && h as isize != -1 { let mut written=0; let _=WriteFile(h,payload.as_ptr(),payload.len() as u32,&mut written,null_mut()); CloseHandle(h); return; } std::thread::sleep(std::time::Duration::from_millis(50)); }
    }
}
pub fn run(args:Vec<String>) -> Result<(), String> {
    unsafe {
        let mutex_name=wide("Local\\HwpAutoDocFit.ConvertWin");
        let mutex=windows_sys::Win32::System::Threading::CreateMutexW(std::ptr::null(),0,mutex_name.as_ptr());
        if mutex.is_null(){return Err("단일 인스턴스 뮤텍스를 만들지 못했습니다".into());}
        if GetLastError()==ERROR_ALREADY_EXISTS { CloseHandle(mutex); forward_to_instance(&args); let cls=wide("ConvertWinStage1"); let existing=FindWindowW(cls.as_ptr(),std::ptr::null()); if !existing.is_null(){SetForegroundWindow(existing);} return Ok(()); }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(std::ptr::null());
        let cls = wide("ConvertWinStage1");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
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
            CloseHandle(mutex);
            return Err("창을 만들지 못했습니다".into());
        }
        let hwnd_num=hwnd as usize;
        std::thread::spawn(move||pipe_server(hwnd_num));
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, null_mut(), 0, 0) > 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
        CloseHandle(mutex);
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
