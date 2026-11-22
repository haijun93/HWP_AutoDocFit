//! 화면 캡처 모드(`ktm --capture-dir <폴더> [--dpi 96|120|144|192] [--scenario <이름>|all]`).
//!
//! 전역 단축키·트레이·단일 인스턴스 뮤텍스·검색 스레드를 만들지 않고, 고정(가짜) 결과로 창을 장면별로
//! 만들어 PNG로 저장한 뒤 끝난다. 같은 `draw` 함수를 메모리 DC에 그리므로 화면에 창이 보이거나
//! 가려지는 것과 무관하다. 장면 뒤에는 실제 창 프로시저에 가짜 클릭을 보내 단추가 작동하는지
//! 확인한 결과를 `capture-report.txt`에 적는다(수동 점검 도구이며 `cargo test`에는 들어가지 않는다).
use super::*;
use std::path::Path;

const SCENARIOS: &[&str] = &[
    "01_empty",
    "02_empty_history",
    "03_results",
    "04_related_chips",
    "05_date_7d",
    "06_date_30d",
    "07_ext_pdf",
    "08_options_open",
    "09_autocomplete",
    "10_error",
    "11_searching",
    "12_stats",
    "13_stats_history",
    "14_settings",
    "15_sidebar",
    "16_grid",
    "17_collapsed",
    "18_no_results",
    "19_within_open",
    "20_status_text",
    "21_mixed_names",
    "22_date_today",
    "23_date_year",
    "24_error_incomplete",
];

fn rows() -> Vec<Row> {
    use tm_state::ResultKind::*;
    let mk = |i: u64, name: &str, parent: &str, snippet: &str, location: &str, kinds: Vec<tm_state::ResultKind>| Row {
        doc_id: i,
        path: format!("{parent}\\{name}"),
        name: name.into(),
        parent: parent.into(),
        snippet: snippet.into(),
        location: location.into(),
        match_kinds: kinds,
        date: 20261100 + i as i64,
        rank: i as usize,
        snippet_tried: true,
        preview: snippet.into(),
        ..Row::default()
    };
    vec![
        mk(1, "2026년 예산 편성 계획.hwp", "C:\\문서\\기획", "", "", vec![Names]),
        mk(2, "예산 집행 지침.hwpx", "C:\\문서\\재정", "2026년도 예산 집행은 분기별 계획에 따라 추진한다.", "3쪽", vec![Hierarchy, Body]),
        mk(3, "예산안 검토보고서.pdf", "D:\\보고\\2026", "예산 편성 기준과 우선순위를 검토한 결과 다음과 같다.", "문단 4", vec![Body]),
        mk(4, "사업비 정산표.xlsx", "D:\\회계", "예산 대비 집행률 82.4%", "시트1 12행", vec![Names, Body]),
        mk(5, "공사 계약 변경 기안문.docx", "C:\\문서\\계약", "계약금액 조정에 따른 예산 반영 필요", "5쪽", vec![Body]),
        mk(6, "업무 메모.txt", "C:\\문서\\메모", "예산 관련 메모", "2행", vec![Body]),
        mk(7, "아주 아주 긴 파일 이름이 들어간 사업 추진 계획서 최종 수정본 v3.hwp", "C:\\문서\\아주\\깊은\\폴더\\경로\\예시", "긴 이름의 파일에서도 글자가 겹치지 않아야 한다", "7쪽", vec![Hierarchy]),
    ]
}

/// 한글·영문·숫자가 섞인 이름과 경로 장면용 고정 자료.
fn mixed_rows() -> Vec<Row> {
    use tm_state::ResultKind::*;
    let mk = |i: u64, name: &str, parent: &str, snippet: &str| Row {
        doc_id: i,
        path: format!("{parent}\\{name}"),
        name: name.into(),
        parent: parent.into(),
        snippet: snippet.into(),
        location: "1쪽".into(),
        match_kinds: vec![Body, Names],
        date: 20261100 + i as i64,
        rank: i as usize,
        snippet_tried: true,
        preview: snippet.into(),
        ..Row::default()
    };
    vec![
        mk(1, "FY2026_budget_plan_v2 최종본(수정).xlsx", "D:\\Reports\\2026 예산\\Q4", "Budget 항목별 집행액 and 예산 잔액 합계 1,234,567원"),
        mk(2, "Meeting_Minutes_2026-11-19 회의록.docx", "C:\\Users\\kim\\Documents\\회의", "다음 budget review 일정을 확정하고 예산 담당자에게 공유한다."),
        mk(3, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_abcdefghijklmnopqrstuvwxyz_no_space_long_name.pdf", "E:\\아카이브", "공백 없는 긴 영문 이름"),
        mk(4, "예산.hwp", "C:\\a", ""),
    ]
}

unsafe fn pump(ms: u32) {
    unsafe {
        let end = Instant::now() + std::time::Duration::from_millis(ms as u64);
        while Instant::now() < end {
            MsgWaitForMultipleObjects(0, null(), 0, 10, QS_ALLINPUT);
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

/// 한 장면의 상태를 처음부터 다시 만든다.
unsafe fn setup(h: HWND, name: &str) {
    unsafe {
        let ed = with(|u| u.edit);
        edit(|u| {
            u.state = State { phase: Phase::Visible, ..State::default() };
            u.rows.clear();
            u.terms.clear();
            u.top = 0;
            u.modal = None;
            u.modal_tab = 0;
            u.status_text.clear();
            u.hotkey_error.clear();
            u.within_open = false;
            u.timing = None;
        });
        let query = match name {
            "03_results" | "05_date_7d" | "06_date_30d" | "07_ext_pdf" | "08_options_open" | "09_autocomplete"
            | "12_stats" | "13_stats_history" | "14_settings" | "15_sidebar" | "16_grid" | "17_collapsed"
            | "19_within_open" | "20_status_text" | "10_error" | "11_searching" | "22_date_today" | "23_date_year"
            | "24_error_incomplete" => "예산",
            "21_mixed_names" => "budget 예산",
            "04_related_chips" => "예산; 공사 계약; 안전관리계획",
            "18_no_results" => "존재하지않는검색어",
            _ => "",
        };
        // 입력칸 글자를 바꾸면 EN_CHANGE가 상태를 건드리므로 먼저 쓰고, 아래에서 상태를 다시 덮어쓴다.
        SetWindowTextW(ed, wide(query).as_ptr());
        pump(30);
        let fixtures = rows();
        let result_phase = |u: &mut Ui| {
            u.state = State {
                phase: Phase::Results,
                query: query.to_string(),
                result_count: fixtures.len(),
                selected: Some(0),
                folder_counts: vec![("기획".into(), 3), ("재정".into(), 2), ("계약".into(), 2)],
                ..State::default()
            };
            u.rows = fixtures.clone();
            if name == "04_related_chips" {
                if let Some(row) = u.rows.get_mut(1) {
                    row.snippet = "2026년도 예산 집행은 공사 계약과 안전관리계획에 맞춰 추진한다.".into();
                    row.preview = row.snippet.clone();
                }
            }
            u.terms = vec!["예산".into()];
            u.timing = Some(Instant::now());
        };
        edit(|u| {
            u.state.recent_queries = vec!["예산".into(), "공사 계약".into(), "안전관리계획".into()];
            u.state.favorite_queries = vec![ktm::settings::FavoriteQuery::new("예산", "예산 업무")];
            u.state.top_topics = vec!["예산".into(), "공사".into(), "계약".into(), "계획".into(), "안전".into()];
            u.recent_query_times = vec![0, 0, 0];
            match name {
                "01_empty" => {
                    u.state.recent_queries.clear();
                    u.state.favorite_queries.clear();
                }
                "02_empty_history" => {}
                "03_results" => result_phase(u),
                "04_related_chips" => {
                    result_phase(u);
                    u.state.autocompletes.clear();
                }
                "05_date_7d" => {
                    result_phase(u);
                    u.state.date_filter = DateFilter::Last7Days;
                }
                "06_date_30d" => {
                    result_phase(u);
                    u.state.date_filter = DateFilter::Last30Days;
                }
                "07_ext_pdf" => {
                    result_phase(u);
                    u.state.selected_ext = Some("pdf".into());
                }
                "08_options_open" => {
                    result_phase(u);
                    u.state.options.options_open = true;
                }
                "09_autocomplete" => {
                    result_phase(u);
                    u.state.autocompletes = vec!["예산안".into(), "예산서".into(), "예산 편성".into()];
                }
                "10_error" => {
                    u.state.phase = Phase::Error;
                    u.state.query = query.into();
                    u.state.error = Some("검색 서비스(색인 서비스)에 연결할 수 없습니다. 서비스가 실행 중인지 확인해 주세요.".into());
                }
                "11_searching" => {
                    u.state.phase = Phase::Searching;
                    u.state.query = query.into();
                }
                "12_stats" => {
                    result_phase(u);
                    u.modal = Some(ktm::layout::ModalKind::Statistics);
                }
                "13_stats_history" => {
                    result_phase(u);
                    u.modal = Some(ktm::layout::ModalKind::Statistics);
                    u.modal_tab = 1;
                }
                "14_settings" => {
                    result_phase(u);
                    u.modal = Some(ktm::layout::ModalKind::Settings);
                }
                "15_sidebar" => {
                    result_phase(u);
                    u.state.options.sidebar_open = true;
                }
                "16_grid" => {
                    result_phase(u);
                    u.state.options.grid_view = true;
                }
                "17_collapsed" => {
                    result_phase(u);
                    u.state.options.collapsed_body = true;
                }
                "18_no_results" => {
                    result_phase(u);
                    u.rows.clear();
                    u.state.result_count = 0;
                    u.state.folder_counts.clear();
                    u.state.selected = None;
                }
                "19_within_open" => {
                    result_phase(u);
                    u.within_open = true;
                    u.state.options.within_query = "집행".into();
                }
                "20_status_text" => {
                    result_phase(u);
                    u.status_text = "표시 결과를 클립보드에 복사했습니다".into();
                }
                "21_mixed_names" => {
                    result_phase(u);
                    u.rows = mixed_rows();
                    u.state.result_count = u.rows.len();
                    u.terms = vec!["budget".into(), "예산".into()];
                }
                "22_date_today" => {
                    result_phase(u);
                    u.state.date_filter = DateFilter::Today;
                }
                "23_date_year" => {
                    result_phase(u);
                    u.state.date_filter = DateFilter::ThisYear;
                }
                "24_error_incomplete" => {
                    u.state.phase = Phase::Error;
                    u.state.query = query.into();
                    u.state.error = Some("색인이 아직 완료되지 않아 일부 폴더를 검색하지 못했습니다. 잠시 뒤 다시 시도해 주세요.".into());
                }
                _ => {}
            }
        });
        // 실제 앱처럼 사이드바가 열려 있으면 창 폭을 사이드바 폭만큼 넓힌다.
        let (bw, bh) = BASE_SIZE.with(|c| c.get());
        let extra = if with(|u| u.state.options.sidebar_open) { scale(h, ktm::layout::SIDEBAR_W) } else { 0 };
        SetWindowPos(h, null_mut(), -32000, -32000, bw + extra, bh, SWP_NOZORDER | SWP_NOACTIVATE);
        layout(h);
        pump(30);
    }
}

thread_local! {
    /// 사이드바가 닫힌 기본 창 크기(물리 화소). `run`이 정한다.
    static BASE_SIZE: std::cell::Cell<(i32, i32)> = const { std::cell::Cell::new((0, 0)) };
}

fn crc_table() -> [u32; 256] {
    std::array::from_fn(|n| {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        c
    })
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8], table: &[u32; 256]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut crc = 0xFFFF_FFFFu32;
    for b in kind.iter().chain(data.iter()) {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&(crc ^ 0xFFFF_FFFF).to_be_bytes());
}

/// 외부 크레이트 없이 압축 없는(저장 블록) PNG를 쓴다. `bgra`는 위에서 아래로의 32비트 화소.
fn write_png(path: &Path, width: i32, height: i32, bgra: &[u8]) -> Result<(), String> {
    let (w, h) = (width as usize, height as usize);
    let mut raw = Vec::with_capacity(h * (w * 3 + 1));
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let i = (y * w + x) * 4;
            raw.extend_from_slice(&[bgra[i + 2], bgra[i + 1], bgra[i]]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(chunk) = chunks.next() {
        z.push(if chunks.peek().is_none() { 1 } else { 0 });
        z.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let table = crc_table();
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr, &table);
    png_chunk(&mut out, b"IDAT", &z, &table);
    png_chunk(&mut out, b"IEND", &[], &table);
    std::fs::write(path, out).map_err(|e| e.to_string())
}

/// 지금 상태를 `draw`로 메모리 DC에 그리고(+입력칸), PNG로 저장한다.
unsafe fn snapshot(h: HWND, path: &Path) -> Result<(), String> {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bm = CreateCompatibleBitmap(screen, r.right, r.bottom);
        let old = SelectObject(mem, bm);
        draw(h, mem, r);
        // 입력칸과 결과 내 검색칸은 자식 창이라 draw에 없다: 같은 자리에 WM_PRINT로 합성한다.
        for (ed, visible) in [
            (with(|u| u.edit), true),
            (with(|u| u.within_edit), with(|u| u.within_open && u.modal.is_none())),
        ] {
            if !visible || ed.is_null() {
                continue;
            }
            let mut er: RECT = std::mem::zeroed();
            GetWindowRect(ed, &mut er);
            let mut pt = POINT { x: er.left, y: er.top };
            ScreenToClient(h, &mut pt);
            SetViewportOrgEx(mem, pt.x, pt.y, null_mut());
            SendMessageW(ed, WM_PRINT, mem as usize, (PRF_CLIENT | PRF_ERASEBKGND) as isize);
            SetViewportOrgEx(mem, 0, 0, null_mut());
        }
        SelectObject(mem, old);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = r.right;
        info.bmiHeader.biHeight = -r.bottom;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut pixels = vec![0u8; (r.right * r.bottom * 4) as usize];
        let got = GetDIBits(screen, bm, 0, r.bottom as u32, pixels.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        DeleteObject(bm);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        if got == 0 {
            return Err("캡처 실패".into());
        }
        write_png(path, r.right, r.bottom, &pixels)
    }
}

fn lparam(x: i32, y: i32) -> isize {
    ((y as u32 as isize) << 16) | (x as u32 as isize & 0xFFFF)
}

unsafe fn click(h: HWND, x: i32, y: i32) {
    unsafe {
        SendMessageW(h, WM_LBUTTONDOWN, 1, lparam(x, y));
        SendMessageW(h, WM_LBUTTONUP, 0, lparam(x, y));
    }
}

unsafe fn hit_code(h: HWND, x: i32, y: i32) -> isize {
    unsafe {
        let mut pt = POINT { x, y };
        ClientToScreen(h, &mut pt);
        SendMessageW(h, WM_NCHITTEST, 0, lparam(pt.x, pt.y))
    }
}

/// 실제 창 프로시저로 가짜 클릭·히트 테스트를 보내 단추가 작동하는지 확인한다.
unsafe fn interaction_checks(h: HWND) -> Vec<String> {
    unsafe {
        let mut log = Vec::new();
        let mut check = |name: &str, ok: bool, detail: String| {
            log.push(format!("{} {name}: {detail}", if ok { "PASS" } else { "FAIL" }));
        };
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        let head = scale(h, HEAD_H);
        let bw = scale(h, CAPTION_W);
        let chips = header_chip_rects(r.right, bw);
        let cy = head / 2;
        let mid = |c: (i32, i32)| (c.0 + c.1) / 2;
        // 히트 테스트: 칩과 캡션 단추는 클라이언트(HTCLIENT=1), 제목 줄 빈 곳은 캡션(HTCAPTION=2).
        for (name, x) in [("통계 칩", mid(chips[0])), ("설정 칩", mid(chips[1])), ("최소화", r.right - bw - bw / 2), ("닫기", r.right - bw / 2)] {
            let code = hit_code(h, x, cy);
            check(&format!("히트 {name}"), code == 1, format!("x={x} y={cy} code={code}"));
        }
        let code = hit_code(h, scale(h, 140), cy);
        check("히트 제목 줄 빈 곳", code == 2, format!("code={code}"));

        // 통계·설정 칩 클릭 → 해당 화면이 열린다.
        edit(|u| u.modal = None);
        click(h, mid(chips[0]), cy);
        check("통계 칩 클릭", with(|u| u.modal) == Some(ktm::layout::ModalKind::Statistics), format!("modal={:?}", with(|u| u.modal)));
        edit(|u| u.modal = None);
        click(h, mid(chips[1]), cy);
        check("설정 칩 클릭", with(|u| u.modal) == Some(ktm::layout::ModalKind::Settings), format!("modal={:?}", with(|u| u.modal)));
        edit(|u| u.modal = None);
        layout(h);

        // 기간 칩 클릭 → 기간 상태가 바뀌고 검색창 글자는 그대로다.
        let bands = with(|u| quick_layout(h, u));
        let pad = scale(h, 12);
        let side_w = { let full = scale(h, SIDEBAR_W); with(|u| ktm::layout::sidebar_width_px(u.state.options.sidebar_open, r.right, full)) };
        let fy = (bands.filters.top + bands.filters.bottom) / 2;
        let before = with(|u| u.state.query.clone());
        let mut fx = pad + scale(h, 34);
        for (lbl, df, w) in [
            ("전체", DateFilter::All, 36),
            ("오늘", DateFilter::Today, 36),
            ("7일간", DateFilter::Last7Days, 46),
            ("30일간", DateFilter::Last30Days, 52),
            ("올해", DateFilter::ThisYear, 36),
        ] {
            let cw = scale(h, w);
            click(h, fx + cw / 2, fy);
            let (got, query) = with(|u| (u.state.date_filter, u.state.query.clone()));
            check(
                &format!("기간 칩 {lbl}"),
                got == df && query == before && !query.contains("after:") && !query.contains("before:"),
                format!("date_filter={got:?} query={query:?}"),
            );
            fx += cw + scale(h, 3);
        }
        // 옵션 단추([옵션])를 누르면 팝업(모두 포함 등)이 열렸다 닫힌다.
        let spans = option_spans(pad + side_w, r.right - pad);
        let oy = bands.options.top + scale(h, OPTION_H / 4);
        let ox = (spans[6].0 + spans[6].1) / 2;
        let was = with(|u| u.state.options.options_open);
        click(h, ox, oy);
        let now = with(|u| u.state.options.options_open);
        check("[옵션] 단추", was != now, format!("{was} -> {now}"));
        if with(|u| u.state.options.options_open) {
            let bands2 = with(|u| quick_layout(h, u));
            let py = bands2.options.top + scale(h, OPTION_H / 2) + scale(h, 11);
            click(h, pad + side_w + scale(h, 2) + scale(h, 86) + scale(h, 40), py);
            let mode = with(|u| u.state.options.match_mode);
            check("옵션 팝업 '하나 이상'", mode == tm_state::MatchMode::AnyTerm, format!("{mode:?}"));
            click(h, ox, oy);
        }
        layout(h);
        log
    }
}

pub unsafe fn run(h: HWND, dir: &Path, dpi: i32, scenario: &str) -> Result<(), String> {
    unsafe {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        std::panic::set_hook(Box::new(|info| {
            eprintln!("{info}
{}", std::backtrace::Backtrace::force_capture());
        }));
        frame::set_dpi_override(dpi);
        create_fonts(h);
        // `--size 520x300` 처럼 논리 크기(96dpi 기준)를 주면 그 크기로 만든다(최소 창 크기 점검용).
        let (lw, lh) = std::env::args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|a| a[0] == "--size")
            .and_then(|a| a[1].split_once('x').and_then(|(x, y)| Some((x.parse::<i32>().ok()?, y.parse::<i32>().ok()?))))
            .unwrap_or((700, 510));
        let (w, ht) = (lw * dpi / 96, lh * dpi / 96);
        BASE_SIZE.with(|c| c.set((w, ht)));
        SetWindowPos(h, null_mut(), -32000, -32000, w, ht, SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        layout(h);
        pump(100);
        let names: Vec<&str> = if scenario == "all" {
            SCENARIOS.to_vec()
        } else {
            SCENARIOS.iter().copied().filter(|n| *n == scenario || n.ends_with(scenario)).collect()
        };
        if names.is_empty() {
            return Err(format!("알 수 없는 장면: {scenario}"));
        }
        let mut report = Vec::new();
        for name in &names {
            setup(h, name);
            let file = dir.join(format!("{name}_{dpi}dpi.png"));
            snapshot(h, &file)?;
            report.push(format!("장면 {name}: {}", file.display()));
        }
        setup(h, "03_results");
        report.extend(interaction_checks(h));
        std::fs::write(dir.join(format!("capture-report_{dpi}dpi.txt")), report.join("\n")).map_err(|e| e.to_string())?;
        Ok(())
    }
}
