use crate::logic::{self, Event, FileResult, FileStatus, Hit, Layout, Model, ALL_TARGETS};
use std::{ffi::OsString, os::windows::ffi::{OsStringExt, OsStrExt}, path::PathBuf, ptr::null_mut, sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex}};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::*,
        Input::KeyboardAndMouse::{GetAsyncKeyState, SetFocus, VK_DOWN, VK_MENU, VK_UP},
        Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
        WindowsAndMessaging::*,
    },
};

struct State { model: Model, results: Arc<Mutex<Vec<FileResult>>>, cancel: Option<Arc<AtomicBool>>, worker_done: bool, close_requested:bool, output_dir:Option<PathBuf>, scroll:usize, pending:Vec<PathBuf> }
impl Default for State { fn default()->Self { Self { model:Model::default(), results:Arc::new(Mutex::new(Vec::new())), cancel:None, worker_done:true, close_requested:false, output_dir:None, scroll:0, pending:Vec::new() } } }
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
    if path.is_dir() {native_tasks::window::open_path(path);return;}
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
    for (i, (label, _)) in ALL_TARGETS.iter().enumerate() {
        let r = logic::format_rect(i);
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
    let head = if st.worker_done { st.model.header() } else { format!("변환 진행 중  ·  {}/{}", results.iter().filter(|x| matches!(x.status,FileStatus::Succeeded(_)|FileStatus::Failed(_)|FileStatus::Skipped(_)|FileStatus::Cancelled)).count(), results.len()) };
    text(hdc, 28, 198, &head, color(45, 55, 70));
    fill(hdc, lay.list_rect(), color(255, 255, 255));
    if results.is_empty() {
        text(hdc, 48, 250, "여기에 파일을 놓으세요", color(130, 140, 154));
    } else {
        for (i, item) in results.iter().enumerate().skip(st.scroll).take(lay.visible_rows()) {
            let name=item.source.file_name().unwrap_or_default().to_string_lossy();
            let (mark,c,detail)=match &item.status { FileStatus::Waiting=>("대기",color(100,110,125),String::new()), FileStatus::Running=>("진행",color(20,95,190),String::new()), FileStatus::Succeeded(p)=>("완료",color(20,130,70),p.file_name().unwrap_or_default().to_string_lossy().into_owned()), FileStatus::Failed(reason)=>("실패",color(190,45,45),reason.clone()), FileStatus::Skipped(reason)=>("건너뜀",color(100,110,125),reason.clone()), FileStatus::Cancelled=>("취소",color(130,100,40),String::new()) };
            let row_y=logic::ROW_TOP + (i-st.scroll) as i32 * logic::ROW_STEP;
            if st.worker_done && st.model.selected_file==i {
                fill(hdc,logic::Rect{left:34,top:row_y-4,right:w-34,bottom:row_y+20},color(225,239,255));
            }
            text(hdc, 48, row_y, &format!("{mark}  {name}  {detail}"), c);
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
    let destination=st.output_dir.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||"원본 파일 옆".into());
    text(hdc,28,h-96,&format!("저장 위치: {destination} · 결과를 두 번 누르면 엽니다. 순서: Alt+↑/↓"),color(78,88,104));
}
/// 끌어 놓기·다른 창에서 보낸 파일·명령줄을 같은 길로 넣고, 대기 중이면 화면 목록도 함께 바꾼다.
unsafe fn add_files(hwnd: HWND, paths: Vec<PathBuf>) {
    unsafe {
        let st = state(hwnd);
        if !st.worker_done {st.pending.extend(paths);return;}
        if st.model.add_dropped(paths, |p| p.is_file()) > 0 && st.worker_done {
            *st.results.lock().unwrap() = logic::results(&st.model.files);
        }
        InvalidateRect(hwnd, null_mut(), 1);
    }
}

/// 닫기 확인 창이 떠 있다(겹쳐 묻지 않는다).
static CONFIRMING_CLOSE: AtomicBool = AtomicBool::new(false);

/// 닫기 상태: 대기 → 바로 닫음. 변환 중 → 묻기 → (계속) 변환 중 | (중단) 취소 중 → 작업이 끝나면 닫음.
/// 취소 중의 닫기는 아무것도 하지 않는다.
unsafe fn close_request(hwnd: HWND) {
    unsafe {
        let st = state(hwnd);
        if st.worker_done {
            DestroyWindow(hwnd);
            return;
        }
        if st.close_requested || CONFIRMING_CLOSE.swap(true, Ordering::AcqRel) {
            return;
        }
        let body = wide("변환이 진행 중입니다. 중단하고 닫을까요?
이미 만든 결과 파일은 남습니다.");
        let title = wide("몽땅파일변환");
        let answer = MessageBoxW(hwnd, body.as_ptr(), title.as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2);
        CONFIRMING_CLOSE.store(false, Ordering::Release);
        if answer != IDYES {
            return;
        }
        // 묻는 동안 변환이 끝났을 수 있다.
        let st = state(hwnd);
        if st.worker_done {
            DestroyWindow(hwnd);
        } else {
            st.close_requested = true;
            if let Some(c) = &st.cancel {
                c.store(true, Ordering::Release);
            }
        }
    }
}

fn message(hwnd: HWND, body: &str, flags: u32) {
    let p = wide(body);
    let t = wide("몽땅파일변환");
    unsafe {
        MessageBoxW(hwnd, p.as_ptr(), t.as_ptr(), flags);
    }
}
fn start_conversion(hwnd: HWND, s: &mut State) {
    if !s.worker_done { return; }
    if s.model.files.is_empty() {message(hwnd,"먼저 파일을 추가해 주세요.",MB_ICONINFORMATION);return;}
    let mb=unsafe {let mut b=[0u16;32];let n=GetWindowTextW(GetDlgItem(hwnd,204),b.as_mut_ptr(),32);String::from_utf16_lossy(&b[..n.max(0) as usize])}.parse::<f64>();
    let Ok(mb)=mb else {message(hwnd,"PDF 목표 크기에 숫자를 입력해 주세요.",MB_ICONERROR);return;};
    if !(1.0..=1000.0).contains(&mb) {message(hwnd,"PDF 목표 크기는 1~1000 MB입니다.",MB_ICONERROR);return;}
    let method=unsafe {SendMessageW(GetDlgItem(hwnd,205),0x147,0,0)};
    let method=["auto","preserve_text","image_pdf"].get(method.max(0) as usize).copied().unwrap_or("auto");
    let files=s.model.files.clone();let format=s.model.format().to_string();let out=s.output_dir.clone();
    *s.results.lock().unwrap()=logic::results(&files);
    let shared=s.results.clone();let cancel=Arc::new(AtomicBool::new(false));s.cancel=Some(cancel.clone());s.worker_done=false;
    let window=hwnd as usize;
    std::thread::spawn(move|| {
        let outcome=(||->Result<(),String>{
            let mut client=native_tasks::Client::start(&native_tasks::data_dir())?;
            let first=client.call(&serde_json::json!({"op":"convert","payload":{"paths":files,"target":format,"output_dir":out,"pdf_target_mb":mb,"pdf_compress_method":method}}))?;
            if first["ok"]==false {return Err(first["message"].as_str().unwrap_or("변환 시작 실패").into());}
            let id=first["job_id"].as_str().ok_or("작업 번호가 없습니다.")?;
            for i in 0..files.len(){logic::transition(&mut shared.lock().unwrap(),Event::Start(i));}
            let mut cancel_sent=false;
            loop {
                if cancel.load(Ordering::Acquire)&&!cancel_sent {client.call(&serde_json::json!({"op":"cancel","payload":{"job_id":id}}))?;cancel_sent=true;}
                let result=client.call(&serde_json::json!({"op":"job_status","payload":{"job_id":id}}))?;
                if result["ok"]==false {return Err(result["message"].as_str().unwrap_or("변환 상태 오류").into());}
                if matches!(result["state"].as_str(),Some("done"|"failed"|"cancelled")) {crate::task::finish(&mut shared.lock().unwrap(),&result);break;}
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            Ok(())
        })();
        if let Err(e)=outcome {for item in shared.lock().unwrap().iter_mut(){if !matches!(item.status,FileStatus::Succeeded(_)){item.status=FileStatus::Failed(e.clone());}}}
        unsafe {PostMessageW(window as HWND,WM_WORKER_DONE,0,0);}
    });
    unsafe {SetTimer(hwnd,1,100,None);}
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
                for (id,title,x,width) in [(201,"파일 추가",650,90),(202,"선택 제거",745,90),(203,"비우기",840,90),(206,"저장 폴더",28,110),(207,"원본 옆 저장",145,110)] {
                    let y=if id>=206 {8}else{42};
                    CreateWindowExW(0,wide("BUTTON").as_ptr(),wide(title).as_ptr(),WS_CHILD|WS_VISIBLE|WS_TABSTOP,x,y,width,28,hwnd,id as _,GetModuleHandleW(std::ptr::null()),std::ptr::null());
                }
                CreateWindowExW(WS_EX_CLIENTEDGE,wide("EDIT").as_ptr(),wide("10").as_ptr(),WS_CHILD|WS_VISIBLE|WS_TABSTOP|ES_AUTOHSCROLL as u32,780,8,50,26,hwnd,204 as _,GetModuleHandleW(std::ptr::null()),std::ptr::null());
                CreateWindowExW(0,wide("STATIC").as_ptr(),wide("목표 MB").as_ptr(),WS_CHILD|WS_VISIBLE,670,12,105,24,hwnd,std::ptr::null_mut(),GetModuleHandleW(std::ptr::null()),std::ptr::null());
                let method=CreateWindowExW(0,wide("COMBOBOX").as_ptr(),wide("").as_ptr(),WS_CHILD|WS_VISIBLE|WS_TABSTOP|CBS_DROPDOWNLIST as u32,840,8,160,200,hwnd,205 as _,GetModuleHandleW(std::ptr::null()),std::ptr::null());
                for label in ["자동 압축","글자층 유지","이미지 PDF"] {SendMessageW(method,0x143,0,wide(label).as_ptr() as isize);}SendMessageW(method,0x14e,0,0);
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
                add_files(hwnd, paths);
                0
            }
            WM_LBUTTONUP => {
                SetFocus(hwnd);
                let x = (lp & 0xffff) as i16 as i32;
                let y = ((lp >> 16) & 0xffff) as i16 as i32;
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let st=state(hwnd);
                if st.worker_done && y>=logic::ROW_TOP && y<r.bottom-104 {
                    let i=st.scroll+((y-logic::ROW_TOP)/logic::ROW_STEP) as usize;
                    if i<st.model.files.len() { st.model.selected_file=i; }
                }
                if st.worker_done {if let Some(i)=(0..ALL_TARGETS.len()).find(|i|logic::format_rect(*i).contains(x,y)){st.model.select_output(i);}}
                match Layout::new(r.right, r.bottom).hit(x, y) {
                    Hit::Target(_) => {},
                    Hit::Start => start_conversion(hwnd, state(hwnd)),
                    Hit::Cancel => if let Some(c)=&state(hwnd).cancel { c.store(true,Ordering::Release); },
                    Hit::OpenFolder => { let st=state(hwnd); if let Some(item)=st.results.lock().unwrap().iter().find(|x|matches!(x.status,FileStatus::Succeeded(_))) { if let FileStatus::Succeeded(p)=&item.status { open_path(p,true); } } },
                    Hit::None => {}
                }
                InvalidateRect(hwnd, null_mut(), 1);
                0
            }
            WM_KEYDOWN => {
                let st=state(hwnd);
                if st.worker_done && st.model.format()=="pdfm" && (wp==VK_UP as usize || wp==VK_DOWN as usize) {
                    let alt=(GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000)!=0;
                    if alt { let delta=if wp==VK_UP as usize {-1} else {1}; let i=st.model.selected_file; st.model.move_file(i,delta); *st.results.lock().unwrap()=logic::results(&st.model.files); InvalidateRect(hwnd,null_mut(),1); }
                }
                0
            }
            WM_LBUTTONDBLCLK => {
                let y=((lp>>16)&0xffff) as i16 as i32;
                if y>=logic::ROW_TOP { let st=state(hwnd);let i=st.scroll+((y-logic::ROW_TOP)/logic::ROW_STEP) as usize; if let Some(item)=st.results.lock().unwrap().get(i) { if let FileStatus::Succeeded(path)=&item.status { open_path(path,false); } } }
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
            WM_COMMAND => {
                let st=state(hwnd);
                if st.worker_done {match wp&0xffff {
                    201=>add_files(hwnd,native_tasks::window::pick_files(hwnd)),
                    202=>{st.model.remove_selected();st.scroll=st.scroll.min(st.model.files.len().saturating_sub(1));*st.results.lock().unwrap()=logic::results(&st.model.files);},
                    203=>{st.model.files.clear();st.scroll=0;*st.results.lock().unwrap()=Vec::new();},
                    206=>{if let Some(folder)=native_tasks::window::pick_folder(hwnd){st.output_dir=Some(folder);}},
                    207=>st.output_dir=None,_=>{}
                }}
                InvalidateRect(hwnd,null_mut(),1);0
            }
            WM_SIZE=>{let mut r:RECT=std::mem::zeroed();GetClientRect(hwnd,&mut r);for (id,x) in [(206,28),(207,145)]{SetWindowPos(GetDlgItem(hwnd,id),null_mut(),x,r.bottom-70,110,28,SWP_NOZORDER|SWP_NOACTIVATE);}0}
            WM_MOUSEWHEEL=>{let st=state(hwnd);let delta=((wp>>16)&0xffff) as i16;let mut r:RECT=std::mem::zeroed();GetClientRect(hwnd,&mut r);let max=st.model.files.len().saturating_sub(Layout::new(r.right,r.bottom).visible_rows());st.scroll=if delta<0{(st.scroll+3).min(max)}else{st.scroll.saturating_sub(3)};InvalidateRect(hwnd,null_mut(),1);0}
            WM_GETMINMAXINFO=>{let info=&mut *(lp as *mut MINMAXINFO);info.ptMinTrackSize=POINT{x:1050,y:650};0}
            WM_TIMER => { InvalidateRect(hwnd,null_mut(),0); 0 }
            WM_CLOSE => { close_request(hwnd); 0 }
            WM_WORKER_DONE => { let st=state(hwnd); st.worker_done=true; st.cancel=None; KillTimer(hwnd,1); let pending=std::mem::take(&mut st.pending); if !pending.is_empty(){add_files(hwnd,pending);} if state(hwnd).close_requested { DestroyWindow(hwnd); } else { InvalidateRect(hwnd,null_mut(),1); } 0 }
            WM_OPEN_REQUEST => { let req=Box::from_raw(wp as *mut OpenRequest); add_files(hwnd, req.paths); let st=state(hwnd); if st.worker_done { if let Some(i)=ALL_TARGETS.iter().position(|(_,v)|*v==req.target) { st.model.select_output(i); } } SetForegroundWindow(hwnd); InvalidateRect(hwnd,null_mut(),1); 0 }
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
        native_tasks::window::apply_saved_topmost(hwnd,&native_tasks::data_dir().join("settings.json"));
        // 처음 뜨는 창도 명령줄의 파일을 받는다(이미 떠 있는 창에는 forward_to_instance 가 넘긴다).
        if !args.is_empty() {
            add_files(hwnd, args.iter().map(PathBuf::from).collect());
        }
        let hwnd_num=hwnd as usize;
        std::thread::spawn(move||pipe_server(hwnd_num));
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd,&m)==0 {
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
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
        let state=State { model:st.clone(), results:Arc::new(Mutex::new(logic::results(&st.files))), cancel:None, worker_done:true, close_requested:false, output_dir:None, scroll:0, pending:Vec::new() };
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
    let st=State { model, results:Arc::new(Mutex::new(items)), cancel:None, worker_done:mode!="progress", close_requested:false, output_dir:None, scroll:0, pending:Vec::new() };
    let shot=grid_core::gdi::render(1050,650,|hdc|{draw(hdc,1050,650,&st);3})?;
    grid_core::png::write(path,shot.width,shot.height,&shot.bgra)?; Ok(path.to_path_buf())
}
