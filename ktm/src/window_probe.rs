//! 격리 창 검사/단 한 번의 측정에서만 쓰는 어댑터. 일반 상주 경로에는 타이머가 없다.
use super::*;
use windows_sys::Win32::System::{ProcessStatus::*, Threading::*};
unsafe fn pump_until(limit_ms: u64, done: impl Fn() -> bool) -> Result<(), String> {
    unsafe {
        let limit = Instant::now() + std::time::Duration::from_millis(limit_ms);
        while !done() {
            if Instant::now() > limit {
                return Err("격리 창 대기 초과".into());
            }
            MsgWaitForMultipleObjects(0, null(), 0, 50, QS_ALLINPUT);
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                let t = Instant::now();
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
                // 첫 입력 구간 진단(MG3): 한 메시지 처리에 10ms 넘게 걸리면 메시지 번호와 함께 남긴다.
                let took = ms(t);
                if took > 10.0 {
                    log_line(serde_json::json!({"metric":"slow_dispatch_ms","value":took,"msg":msg.message,"to_edit":msg.hwnd == with(|u| u.edit)}).to_string());
                }
            }
        }
        Ok(())
    }
}
pub unsafe fn wait_results(h: HWND) -> Result<(), String> {
    unsafe {
        pump_until(5000, || with(|u| u.state.phase) != Phase::Searching)?;
        UpdateWindow(h);
        if with(|u| u.state.phase) != Phase::Results {
            return Err(with(|u| format!("검색 실패: {:?}", u.state.error)));
        }
        Ok(())
    }
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
/// 검색어를 넣고 결과를 그릴 때까지(입력 → 결과 그리기).
unsafe fn type_query(h: HWND, text: &str) -> Result<f64, String> {
    unsafe {
        let start = Instant::now();
        SetWindowTextW(with(|u| u.edit), wide(text).as_ptr());
        wait_results(h)?;
        Ok(ms(start))
    }
}
unsafe fn switch(h: HWND, tab: Tab) -> Result<f64, String> {
    unsafe {
        let start = Instant::now();
        event(h, Event::SwitchTab(tab));
        wait_results(h)?;
        Ok(ms(start))
    }
}
unsafe fn show(h: HWND) -> f64 {
    unsafe {
        let start = Instant::now();
        SendMessageW(h, WM_HOTKEY, tray::HOTKEY_ID as usize, 0);
        ms(start)
    }
}
pub unsafe fn smoke(h: HWND, output: &std::path::Path, measure: bool) -> Result<(), String> {
    unsafe {
        let boot = Instant::now();
        pump_until(20000, || with(|u| u.warmed))?;
        let warm_wait_ms = ms(boot);
        let show_first_ms = show(h);
        // KTM_PROBE_SETTLE_MS: 표시 뒤 사람이 치기 시작할 때까지의 틈(기본 0 = 표시 직후 바로 입력).
        let settle = std::env::var("KTM_PROBE_SETTLE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(0u64);
        if settle > 0 {
            let _ = pump_until(settle, || false);
        }
        let first_ms = type_query(h, "예산")?;
        let count = with(|u| u.rows.len());
        if count < 6 {
            return Err("합성 검사 자료는 6건 이상 필요합니다".into());
        }
        let snippets = with(|u| u.rows.iter().take(ktm::backend::SNIPPET_PREFETCH).filter(|r| !r.snippet.is_empty()).count());
        let first_path = with(|u| u.rows[0].path.clone());
        ktm::open::target(&first_path, false)?;
        ktm::open::target(&first_path, true)?;
        event(h, Event::MoveSelection(9));
        if with(|u| u.top) == 0 {
            return Err("선택 스크롤 실패".into());
        }
        // 스크롤로 새로 보인 줄의 원문 찾기 응답을 받는다.
        let _ = pump_until(500, || with(|u| u.rows.get(9).is_some_and(|r| !r.snippet.is_empty())));
        let scrolled_snippet = with(|u| u.rows.get(9).is_some_and(|r| !r.snippet.is_empty()));
        UpdateWindow(h);
        capture(h, &output.with_extension("bmp"))?;
        let second_ms = type_query(h, "추진 계획")?;
        let hierarchy_ms = switch(h, Tab::Hierarchy)?;
        if with(|u| u.state.query.clone()) != "추진 계획" {
            return Err("탭 검색어 공유 실패".into());
        }
        let hier_sorted = with(|u| u.rows.windows(2).all(|w| w[0].level <= w[1].level));
        let hierarchy_retype_ms = type_query(h, "예산")?;
        let names_ms = match switch(h, Tab::Names) {
            Ok(v) => Some(v),
            Err(e) => {
                log_text("names_error", &e);
                None
            }
        };
        // 파일명 색인이 있으면(서비스가 names_build를 마친 경우) 파일 이름에 든 낱말로 한 번 더.
        let names_query_ms = if names_ms.is_some() { type_query(h, "합성문서").ok() } else { None };
        let names_rows = with(|u| u.rows.len());
        if names_query_ms.is_some() {
            type_query(h, "예산")?;
        }
        let body_back_ms = switch(h, Tab::Body)?;
        SetWindowTextW(with(|u| u.edit), wide("예").as_ptr());
        if with(|u| u.state.phase) != Phase::Typing {
            return Err("1글자 자동검색 금지 실패".into());
        }
        let start = Instant::now();
        event(h, Event::Submit);
        wait_results(h)?;
        let one_char_ms = ms(start);
        event(h, Event::Hide);
        if IsWindowVisible(h) != 0 {
            return Err("Esc 숨김 실패".into());
        }
        let hidden_mem = memory();
        let show_second_ms = show(h);
        event(h, Event::Hide);
        let report = serde_json::json!({"ok":true,"after_hide_private_working_set_bytes":hidden_mem.2,"results":count,"warm_wait_ms":warm_wait_ms,
            "show_first_ms":show_first_ms,"show_second_ms":show_second_ms,
            "first_result_ms":first_ms,"settle_ms":settle,"second_query_ms":second_ms,"hierarchy_switch_ms":hierarchy_ms,
            "hierarchy_retype_ms":hierarchy_retype_ms,"hierarchy_sorted":hier_sorted,
            "names_switch_ms":names_ms,"names_query_ms":names_query_ms,"names_rows":names_rows,"body_back_ms":body_back_ms,"one_char_ms":one_char_ms,
            "snippets_prefetched":snippets,"scrolled_snippet":scrolled_snippet,
            "after_hide_private_bytes":hidden_mem.0,"after_hide_working_set_bytes":hidden_mem.1,
            "checks":["registered isolated hotkey","warm before show","show","Korean EDIT change","results","snippets from paragraph table","selection scroll","shared query tabs","hierarchy top-N order","names tab","one-character Enter","file/folder target","hide"]});
        std::fs::write(output, report.to_string()).map_err(|e| e.to_string())?;
        if measure {
            let output = output.to_path_buf();
            let handle = h as usize;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let before = cpu();
                let mem = memory();
                let start = Instant::now();
                std::thread::sleep(std::time::Duration::from_secs(60));
                let after = cpu();
                let mem_end = memory();
                let elapsed = start.elapsed().as_secs_f64();
                let data = serde_json::json!({"idle_seconds":elapsed,"idle_cpu_ms":(after-before) as f64/10000.0,
                    "idle_cpu_percent_one_core":(after-before) as f64/100000.0/elapsed,
                    "private_bytes":mem.0,"working_set_bytes":mem.1,
                    "private_bytes_end":mem_end.0,"working_set_bytes_end":mem_end.1,
                    "private_working_set_bytes":mem.2,"private_working_set_bytes_end":mem_end.2});
                let _ = std::fs::write(output.with_extension("idle.json"), data.to_string());
                PostMessageW(handle as HWND, WM_DESTROY, 0, 0);
            });
        } else {
            DestroyWindow(h);
        }
        Ok(())
    }
}
fn log_text(name: &str, value: &str) {
    log_line(serde_json::json!({"metric":name,"text":value}).to_string());
}
unsafe fn cpu() -> u64 {
    unsafe {
        let mut c = std::mem::zeroed();
        let mut e = std::mem::zeroed();
        let mut k: FILETIME = std::mem::zeroed();
        let mut u: FILETIME = std::mem::zeroed();
        GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u);
        ((k.dwHighDateTime as u64) << 32 | k.dwLowDateTime as u64) + ((u.dwHighDateTime as u64) << 32 | u.dwLowDateTime as u64)
    }
}
/// (개인 커밋, 작업 집합, 개인 작업 집합 = 작업 관리자 '메모리' 열).
fn memory() -> (usize, usize, usize) {
    unsafe {
        let mut m: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
        m.cb = std::mem::size_of_val(&m) as u32;
        GetProcessMemoryInfo(GetCurrentProcess(), (&mut m as *mut PROCESS_MEMORY_COUNTERS_EX2).cast(), m.cb);
        (m.PrivateUsage, m.WorkingSetSize, m.PrivateWorkingSetSize)
    }
}
unsafe fn capture(h: HWND, path: &std::path::Path) -> Result<(), String> {
    unsafe {
        let dc = GetDC(h);
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        let mem = CreateCompatibleDC(dc);
        let bm = CreateCompatibleBitmap(dc, r.right, r.bottom);
        let old = SelectObject(mem, bm);
        BitBlt(mem, 0, 0, r.right, r.bottom, dc, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = r.right;
        info.bmiHeader.biHeight = -r.bottom;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut pixels = vec![0u8; (r.right * r.bottom * 4) as usize];
        let got = GetDIBits(dc, bm, 0, r.bottom as u32, pixels.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        DeleteObject(bm);
        DeleteDC(mem);
        ReleaseDC(h, dc);
        if got == 0 {
            return Err("캡처 실패".into());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&((54 + pixels.len()) as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&54u32.to_le_bytes());
        let header = std::slice::from_raw_parts((&info.bmiHeader as *const BITMAPINFOHEADER).cast::<u8>(), 40);
        out.extend_from_slice(header);
        out.extend_from_slice(&pixels);
        std::fs::write(path, out).map_err(|e| e.to_string())
    }
}
