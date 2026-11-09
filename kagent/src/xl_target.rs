//! 엑셀 표 항목을 '목표 결과'로 붙이기: 저장한 엑셀 HTML을 앱 서비스 파이프(`docfit_app/kagent_app_service.py`)에
//! 보내 변환본(공백·빈 칸·열 너비·칸 여백 정리)을 받아 HTML + 글 두 형식만 클립보드에 올린다.
//!
//! 앱이 꺼져 있거나(파이프 없음) 늦거나(시간 제한) 바꿀 것이 없거나 응답이 이상하면 `None` — 호출 쪽은 저장한
//! 원래 형식 그대로 붙인다(폴백). 판단·요청·응답 해석은 순수 함수라 단위 시험하고, 파이프 입출력만 Win32다.

use crate::json::{self, Value};
use crate::util::{base64, unbase64};

/// Rust 변환이 Python 기준과 완전히 같아질 때만 활성화한다. 비교 도구가 별도 바이너리로
/// 결과를 검증하고 나서 이 상수를 true로 바꾼다. 현재 앱 파이프를 유지한다.
pub const DIRECT_RUST_ENABLED: bool = false;

/// 앱 응답을 기다리는 최대 시간(붙여넣기 지연 상한). 넘으면 원래 형식으로 붙인다.
pub const TIMEOUT_MS: u64 = 1500;
/// 파이프가 바쁠 때(앱이 다른 요청 처리 중) 한 번 기다리는 시간.
const BUSY_WAIT_MS: u32 = 300;
/// 응답 최대 크기(항목 하나 최대 50MB + base64 여유).
const MAX_RESPONSE: usize = 80 * 1024 * 1024;

/// 저장한 HTML이 엑셀에서 복사한 표인가(엑셀 표지 + 표 칸).
pub fn is_excel_html(html: &[u8]) -> bool {
    let head = &html[..html.len().min(65536)];
    let lower_head = head.to_ascii_lowercase();
    let marked = contains(&lower_head, b"urn:schemas-microsoft-com:office:excel") || contains(&lower_head, b"excel.sheet");
    marked && contains(&html.to_ascii_lowercase(), b"<td")
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// 보관한 형식 묶음에서 엑셀 표 HTML을 고른다.
pub fn excel_html(formats: &[(String, Vec<u8>)]) -> Option<&[u8]> {
    formats
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("HTML Format"))
        .map(|(_, b)| b.as_slice())
        .filter(|b| is_excel_html(b))
}

/// 변환을 시도할지(설정 켬 + 글 항목 + 엑셀 표 HTML 있음).
pub fn wants(enabled: bool, kind: &str, formats: &[(String, Vec<u8>)]) -> bool {
    enabled && kind == "text" && excel_html(formats).is_some()
}

pub fn build_request(html: &[u8], text: &str) -> String {
    let v = Value::obj(vec![
        ("cmd", "xl_target".into()),
        ("html", base64(html).into()),
        ("text", text.into()),
    ]);
    let mut line = json::to_string(&v);
    line.push('\n');
    line
}

/// 앱 응답 해석: 바뀐 HTML(끝 NUL 하나 붙임)과 글. 그 밖(바꿀 것 없음·오류·형식 이상)은 None.
pub fn parse_response(line: &str) -> Option<(Vec<u8>, String)> {
    let v = json::parse(line.trim_end())?;
    if v.bool("ok") != Some(true) || v.bool("changed") != Some(true) {
        return None;
    }
    let mut html = unbase64(v.str("html")?)?;
    while html.last() == Some(&0) {
        html.pop();
    }
    if html.is_empty() || !is_excel_html(&html) {
        return None;
    }
    html.push(0); // CF_HTML은 NUL로 끝나는 바이트열로 둔다(엑셀·브라우저와 같게).
    let text = v.str("text").unwrap_or("").to_string();
    Some((html, text))
}

/// 앱 서비스 파이프 이름(앱 `kagent_app_service.pipe_name`과 같은 규칙, 시험 인스턴스는 따로).
pub fn pipe_name() -> String {
    let suffix = crate::util::instance_suffix().unwrap_or_default();
    format!("\\\\.\\pipe\\HwpAutoDocFit.appsvc{suffix}.{}", crate::pipe::user_sid())
}

/// 저장한 항목을 목표 결과로 바꿔 달라고 앱에 묻는다. 앱이 없거나 늦으면 None(폴백).
pub fn convert(kind: &str, payload: &[u8], extra: &[u8], enabled: bool) -> Option<(Vec<u8>, String)> {
    let formats = crate::open_item::formats_from_extra(extra);
    if !wants(enabled, kind, &formats) {
        return None;
    }
    let html = excel_html(&formats)?;
    let request = build_request(html, &String::from_utf8_lossy(payload));
    let response = call_with_timeout(pipe_name(), request, TIMEOUT_MS)?;
    parse_response(&response)
}

/// 클립보드에 올리는 길 고르기: 변환본이 있으면 먼저 쓰고, 없거나 쓰기에 실패하면 원래 형식.
/// 반환: Some(true) = 목표 결과로 올림, Some(false) = 원래 형식으로 올림, None = 둘 다 실패.
pub fn write_choice<T>(converted: Option<T>, write_converted: impl FnOnce(&T) -> bool,
                       write_original: impl FnOnce() -> bool) -> Option<bool> {
    if let Some(c) = converted.as_ref() {
        if write_converted(c) {
            return Some(true);
        }
    }
    write_original().then_some(false)
}

/// 요청은 작업 스레드에서 하고 창 스레드는 최대 `timeout_ms`만 기다린다(앱이 멈춰도 붙여넣기는 계속).
pub fn call_with_timeout(name: String, request: String, timeout_ms: u64) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("kagent-xl-target".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            let _ = tx.send(call(&name, request.as_bytes()));
        })
        .ok()?;
    rx.recv_timeout(std::time::Duration::from_millis(timeout_ms)).ok().flatten()
}

fn call(name: &str, request: &[u8]) -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE, GetLastError,
                                         INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING, ReadFile, WriteFile};
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
    let wide = crate::util::wide(name);
    let open = || unsafe {
        CreateFileW(wide.as_ptr(), GENERIC_READ | GENERIC_WRITE, 0, std::ptr::null(), OPEN_EXISTING, 0,
                    std::ptr::null_mut())
    };
    let mut h = open();
    if h == INVALID_HANDLE_VALUE {
        // 파이프가 없으면(앱 꺼짐) 바로 포기, 바쁘면 한 번만 기다린다.
        if unsafe { GetLastError() } != ERROR_PIPE_BUSY || unsafe { WaitNamedPipeW(wide.as_ptr(), BUSY_WAIT_MS) } == 0 {
            return None;
        }
        h = open();
        if h == INVALID_HANDLE_VALUE {
            return None;
        }
    }
    let mut ok = true;
    let mut data = request;
    while !data.is_empty() {
        let mut n = 0u32;
        let chunk = data.len().min(1 << 20) as u32;
        if unsafe { WriteFile(h, data.as_ptr(), chunk, &mut n, std::ptr::null_mut()) } == 0 || n == 0 {
            ok = false;
            break;
        }
        data = &data[n as usize..];
    }
    let mut out = Vec::new();
    if ok {
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let mut n = 0u32;
            if unsafe { ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) } == 0 || n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
            if out.last() == Some(&b'\n') || out.len() > MAX_RESPONSE {
                break;
            }
        }
    }
    unsafe { CloseHandle(h) };
    if !ok || out.len() > MAX_RESPONSE {
        return None;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXCEL: &[u8] = b"Version:0.9\r\nStartHTML:0000000105\r\n<html xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\
<body><table><tr><td>  88,180 </td></tr></table></body></html>\0";

    fn formats(html: &[u8]) -> Vec<(String, Vec<u8>)> {
        vec![("HTML Format".into(), html.to_vec()), ("Biff12".into(), vec![1, 2, 3])]
    }

    #[test]
    fn detects_excel_tables_only() {
        assert!(is_excel_html(EXCEL));
        assert!(is_excel_html(b"<meta name=ProgId content=Excel.Sheet><table><TD>1</TD></table>"));
        assert!(!is_excel_html(b"<html><table><td>web</td></table></html>"), "웹 표는 대상 아님");
        assert!(!is_excel_html(b"<html xmlns:x=\"urn:schemas-microsoft-com:office:excel\"><p>no table</p></html>"));
    }

    #[test]
    fn branch_needs_setting_text_kind_and_excel_html() {
        let f = formats(EXCEL);
        assert!(wants(true, "text", &f));
        assert!(!wants(false, "text", &f), "설정을 끄면 원래 형식");
        assert!(!wants(true, "image", &f));
        assert!(!wants(true, "text", &formats(b"<table><td>web</td></table>")));
        assert!(!wants(true, "text", &[("Rich Text Format".into(), b"{\\rtf1}".to_vec())]));
        assert!(!wants(true, "text", &[]));
    }

    #[test]
    fn request_carries_html_bytes_and_text() {
        let line = build_request(EXCEL, "부서명\t목\r\n  88,180 \t");
        assert!(line.ends_with('\n'));
        let v = json::parse(line.trim_end()).unwrap();
        assert_eq!(v.str("cmd"), Some("xl_target"));
        assert_eq!(unbase64(v.str("html").unwrap()).unwrap(), EXCEL);
        assert_eq!(v.str("text"), Some("부서명\t목\r\n  88,180 \t"));
    }

    #[test]
    fn response_parsing_accepts_only_changed_excel_html() {
        let html = b"<html xmlns:x=\"urn:schemas-microsoft-com:office:excel\"><table><td>88,180 </td></table></html>";
        let good = format!("{{\"ok\":true,\"changed\":true,\"html\":\"{}\",\"text\":\"부서명\\t88,180\"}}\n", base64(html));
        let (out, text) = parse_response(&good).unwrap();
        assert_eq!(&out[..out.len() - 1], &html[..]);
        assert_eq!(out.last(), Some(&0), "끝 NUL 하나");
        assert_eq!(text, "부서명\t88,180");
        // 앱이 NUL을 붙여 보내도 하나만 남긴다.
        let mut with_nul = html.to_vec();
        with_nul.extend_from_slice(&[0, 0]);
        let r = format!("{{\"ok\":true,\"changed\":true,\"html\":\"{}\"}}", base64(&with_nul));
        let (out, text) = parse_response(&r).unwrap();
        assert_eq!(out.iter().filter(|b| **b == 0).count(), 1);
        assert_eq!(text, "");
        // 폴백: 바꿀 것 없음·오류·깨진 base64·엑셀 아님·깨진 JSON
        for bad in [
            "{\"ok\":true,\"changed\":false,\"reason\":\"바꿀 것 없음\"}".to_string(),
            "{\"ok\":false,\"message\":\"x\"}".to_string(),
            "{\"ok\":true,\"changed\":true,\"html\":\"@@@\"}".to_string(),
            format!("{{\"ok\":true,\"changed\":true,\"html\":\"{}\"}}", base64(b"<table><td>x</td></table>")),
            format!("{{\"ok\":true,\"changed\":true,\"html\":\"{}\"}}", base64(b"\0\0")),
            "{\"ok\":true,".to_string(),
            String::new(),
        ] {
            assert!(parse_response(&bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn write_choice_prefers_target_and_falls_back_to_original() {
        let order = std::cell::RefCell::new(Vec::new());
        let target = |ok: bool| { let o = &order; move |_: &i32| { o.borrow_mut().push("target"); ok } };
        let orig = |ok: bool| { let o = &order; move || { o.borrow_mut().push("orig"); ok } };
        assert_eq!(write_choice(Some(1), target(true), orig(true)), Some(true));
        assert_eq!(*order.borrow(), ["target"], "변환본을 쓰면 원래 형식은 쓰지 않는다");
        order.borrow_mut().clear();
        assert_eq!(write_choice(Some(1), target(false), orig(true)), Some(false));
        assert_eq!(*order.borrow(), ["target", "orig"], "변환본 쓰기 실패 → 원래 형식");
        assert_eq!(write_choice(None::<i32>, |_| panic!("변환본 없음"), || true), Some(false));
        assert_eq!(write_choice(None::<i32>, |_| true, || false), None);
    }

    #[test]
    fn app_off_falls_back_quickly() {
        // 없는 파이프(앱 꺼짐)는 시간 제한까지 기다리지 않고 바로 None.
        let start = std::time::Instant::now();
        let name = format!("\\\\.\\pipe\\HwpAutoDocFit.appsvc.test-missing.{}", std::process::id());
        assert!(call_with_timeout(name, "{\"cmd\":\"ping\"}\n".into(), TIMEOUT_MS).is_none());
        assert!(start.elapsed() < std::time::Duration::from_millis(TIMEOUT_MS), "{:?}", start.elapsed());
    }

    /// 시험용 가짜 앱: 한 번 연결을 받아 `delay_ms` 뒤 `reply`를 보낸다.
    fn fake_app(name: &str, reply: &'static str, delay_ms: u64) -> std::thread::JoinHandle<Option<String>> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile};
        use windows_sys::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_WAIT};
        let wide = crate::util::wide(name);
        let h = unsafe { CreateNamedPipeW(wide.as_ptr(), PIPE_ACCESS_DUPLEX, PIPE_WAIT, 1, 65536, 65536, 0, std::ptr::null()) };
        assert_ne!(h, INVALID_HANDLE_VALUE);
        let h = h as usize;
        std::thread::spawn(move || {
            let h = h as windows_sys::Win32::Foundation::HANDLE;
            unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) };
            let mut buf = vec![0u8; 1 << 16];
            let mut got = Vec::new();
            loop {
                let mut n = 0u32;
                if unsafe { ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) } == 0 || n == 0 { break; }
                got.extend_from_slice(&buf[..n as usize]);
                if got.last() == Some(&b'\n') { break; }
            }
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            let mut n = 0u32;
            unsafe {
                WriteFile(h, reply.as_ptr(), reply.len() as u32, &mut n, std::ptr::null_mut());
                FlushFileBuffers(h);
                DisconnectNamedPipe(h);
                CloseHandle(h);
            }
            String::from_utf8(got).ok()
        })
    }

    #[test]
    fn app_reply_is_used_and_slow_app_times_out() {
        let html = b"<html xmlns:x=\"urn:schemas-microsoft-com:office:excel\"><table><td>88,180 </td></table></html>";
        let reply: &'static str = Box::leak(format!("{{\"ok\":true,\"changed\":true,\"html\":\"{}\",\"text\":\"88,180\"}}\n", base64(html)).into_boxed_str());
        // 정상: 요청이 그대로 가고 응답을 해석한다.
        let name = format!("\\\\.\\pipe\\HwpAutoDocFit.appsvc.test-ok.{}", std::process::id());
        let server = fake_app(&name, reply, 0);
        let request = build_request(EXCEL, "  88,180 ");
        let line = call_with_timeout(name, request.clone(), TIMEOUT_MS).expect("응답");
        assert_eq!(server.join().unwrap().as_deref(), Some(request.as_str()));
        let (out, text) = parse_response(&line).unwrap();
        assert_eq!(&out[..out.len() - 1], &html[..]);
        assert_eq!(text, "88,180");
        // 느린 앱: 시간 제한을 넘기면 기다리지 않고 None(원래 형식으로 붙임).
        let name = format!("\\\\.\\pipe\\HwpAutoDocFit.appsvc.test-slow.{}", std::process::id());
        let server = fake_app(&name, reply, 800);
        let start = std::time::Instant::now();
        assert!(call_with_timeout(name, request, 200).is_none());
        assert!(start.elapsed() < std::time::Duration::from_millis(700), "{:?}", start.elapsed());
        let _ = server.join();
    }

    /// 앱(Python) 서비스와 실제로 주고받기: `XL_E2E_PIPE`에 앱 서비스 파이프 이름을 주고
    /// `cargo test e2e_with_python_app -- --ignored`로 돌린다(tests/fixtures 합성 엑셀 HTML 사용).
    #[test]
    #[ignore]
    fn e2e_with_python_app() {
        let name = std::env::var("XL_E2E_PIPE").expect("XL_E2E_PIPE");
        let html = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/xlpaste_accounting.cf_html")).unwrap();
        assert!(is_excel_html(&html));
        let line = call_with_timeout(name, build_request(&html, "관광정책과\t                    88,180 "), TIMEOUT_MS)
            .expect("앱 응답");
        let (out, text) = parse_response(&line).expect("목표 결과");
        assert_ne!(&out[..out.len() - 1], &html[..]);
        assert!(contains(&out, b"88,180"));
        assert_eq!(text, "관광정책과\t88,180 ");
    }

    #[test]
    fn convert_skips_non_excel_items_without_touching_the_pipe() {
        // 엑셀 표가 아니거나 설정이 꺼져 있으면 파이프를 열지 않는다(pipe_name 호출 전 반환).
        let extra = crate::clip::pack_formats(&formats(EXCEL));
        assert!(convert("text", b"x", &extra, false).is_none());
        assert!(convert("image", b"x", &extra, true).is_none());
        let web = crate::clip::pack_formats(&formats(b"<table><td>web</td></table>"));
        assert!(convert("text", b"x", &web, true).is_none());
    }
}
