//! 격리 창 검사/단 한 번의 측정에서만 쓰는 어댑터. 일반 상주 경로에는 타이머가 없다.
use super::*;
use windows_sys::Win32::System::{ProcessStatus::*, Threading::*};
pub unsafe fn wait_results(h:HWND)->Result<(),String>{unsafe{
    let limit=Instant::now()+std::time::Duration::from_secs(5);
    while with(|u|u.state.phase)==Phase::Searching {
        if Instant::now()>limit{return Err("격리 창 검색 대기 초과".into());}
        MsgWaitForMultipleObjects(0,null(),0,50,QS_ALLINPUT);
        let mut msg:MSG=std::mem::zeroed();while PeekMessageW(&mut msg,null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
    }
    UpdateWindow(h);
    if with(|u|u.state.phase)!=Phase::Results{return Err(with(|u|format!("검색 실패: {:?}",u.state.error)));}Ok(())
}}
pub unsafe fn smoke(h:HWND,output:&std::path::Path,measure:bool)->Result<(),String>{unsafe{
    SendMessageW(h,WM_HOTKEY,tray::HOTKEY_ID as usize,0);
    let start=Instant::now();let ed=with(|u|u.edit);SetWindowTextW(ed,wide("예산").as_ptr());wait_results(h)?;
    let first_ms=start.elapsed().as_secs_f64()*1000.0;
    let count=with(|u|u.rows.len());if count<6{return Err("합성 검사 자료는 6건 이상 필요합니다".into());}
    let first_path=with(|u|u.rows[0].path.clone());ktm::open::target(&first_path,false)?;ktm::open::target(&first_path,true)?;
    event(h,Event::MoveSelection(6));if with(|u|u.top)==0{return Err("선택 스크롤 실패".into());}
    UpdateWindow(h);capture(h,&output.with_extension("bmp"))?;
    event(h,Event::SwitchTab(Tab::Hierarchy));wait_results(h)?;if with(|u|u.state.query.clone())!="예산"{return Err("탭 검색어 공유 실패".into());}
    event(h,Event::SwitchTab(Tab::Names));if with(|u|u.state.phase)==Phase::Searching{return Err("파일명 탭 비활성 실패".into());}
    event(h,Event::SwitchTab(Tab::Body));wait_results(h)?;
    SetWindowTextW(ed,wide("예").as_ptr());if with(|u|u.state.phase)!=Phase::Typing{return Err("1글자 자동검색 금지 실패".into());}event(h,Event::Submit);wait_results(h)?;
    event(h,Event::Hide);if IsWindowVisible(h)!=0{return Err("Esc 숨김 실패".into());}
    let report=serde_json::json!({"ok":true,"results":count,"first_result_ms":first_ms,"checks":["registered isolated hotkey","show","Korean EDIT change","results","selection scroll","shared query tabs","disabled names","one-character Enter","file/folder target","hide"]});
    std::fs::write(output,report.to_string()).map_err(|e|e.to_string())?;
    if measure {
        let output=output.to_path_buf();let handle=h as usize;
        std::thread::spawn(move||{std::thread::sleep(std::time::Duration::from_secs(2));let before=cpu();let mem=memory();let start=Instant::now();std::thread::sleep(std::time::Duration::from_secs(60));let after=cpu();let elapsed=start.elapsed().as_secs_f64();let data=serde_json::json!({"idle_seconds":elapsed,"idle_cpu_ms":(after-before) as f64/10000.0,"idle_cpu_percent_one_core":(after-before) as f64/100000.0/elapsed,"private_bytes":mem.0,"working_set_bytes":mem.1});let _=std::fs::write(output.with_extension("idle.json"),data.to_string());PostMessageW(handle as HWND,WM_DESTROY,0,0);});
    } else {DestroyWindow(h);}
    Ok(())
}}
unsafe fn cpu()->u64{unsafe{let mut c=std::mem::zeroed();let mut e=std::mem::zeroed();let mut k:FILETIME=std::mem::zeroed();let mut u:FILETIME=std::mem::zeroed();GetProcessTimes(GetCurrentProcess(),&mut c,&mut e,&mut k,&mut u);((k.dwHighDateTime as u64)<<32|k.dwLowDateTime as u64)+((u.dwHighDateTime as u64)<<32|u.dwLowDateTime as u64)}}
unsafe fn memory()->(usize,usize){unsafe{let mut m:PROCESS_MEMORY_COUNTERS_EX=std::mem::zeroed();GetProcessMemoryInfo(GetCurrentProcess(),(&mut m as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),std::mem::size_of_val(&m)as u32);(m.PrivateUsage,m.WorkingSetSize)}}
unsafe fn capture(h:HWND,path:&std::path::Path)->Result<(),String>{unsafe{
    let dc=GetDC(h);let mut r:RECT=std::mem::zeroed();GetClientRect(h,&mut r);let mem=CreateCompatibleDC(dc);let bm=CreateCompatibleBitmap(dc,r.right,r.bottom);let old=SelectObject(mem,bm);BitBlt(mem,0,0,r.right,r.bottom,dc,0,0,SRCCOPY);SelectObject(mem,old);
    let mut info:BITMAPINFO=std::mem::zeroed();info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>()as u32;info.bmiHeader.biWidth=r.right;info.bmiHeader.biHeight= -r.bottom;info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;info.bmiHeader.biCompression=BI_RGB;
    let mut pixels=vec![0u8;(r.right*r.bottom*4)as usize];let got=GetDIBits(dc,bm,0,r.bottom as u32,pixels.as_mut_ptr().cast(),&mut info,DIB_RGB_COLORS);DeleteObject(bm);DeleteDC(mem);ReleaseDC(h,dc);if got==0{return Err("캡처 실패".into());}
    let mut out=Vec::new();out.extend_from_slice(b"BM");out.extend_from_slice(&((54+pixels.len())as u32).to_le_bytes());out.extend_from_slice(&[0;4]);out.extend_from_slice(&54u32.to_le_bytes());let header=std::slice::from_raw_parts((&info.bmiHeader as *const BITMAPINFOHEADER).cast::<u8>(),40);out.extend_from_slice(header);out.extend_from_slice(&pixels);std::fs::write(path,out).map_err(|e|e.to_string())
}}
