//! 창 없이 시험하는 부분: 격자 배치·가상화·키보드 이동·스캔 줄 파서·스캔 취소·설정 읽기·검색/정렬.
use docview_win::layout::{Metrics, Toolbar};
use docview_win::model::{App, Collection, Doc, Nav, ScanState, SortKey, format_date, format_size, group_digits};
use docview_win::scan::{self, Event, ScanJob, parse_line};
use docview_win::settings;
use docview_win::synth::{synthetic_docs, to_json_line};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn metrics(w: i32, h: i32, dpi: u32, thumb: f64) -> Metrics {
    Metrics::new(w, h, dpi, thumb, false)
}

// ---------- 배치·가상화 ----------

#[test]
fn grid_columns_follow_flutter_max_cross_axis_rule() {
    // 1100 폭: 가용 = 1100-12(스크롤바)-32(여백) = 1056, 칸 최대 190+간격 14 → ceil(1056/204)=6칸.
    let m = metrics(1100, 720, 96, 190.0);
    assert_eq!(m.cols, 6);
    assert_eq!(m.card_w, (1056 - 14 * 5) / 6);
    assert_eq!(m.card_h, (m.card_w as f64 / 0.74).round() as i32);
    // 칸이 최대 크기를 넘지 않는다.
    assert!(m.card_w <= 190);
    // 100~240 범위 밖 값은 맞춰진다.
    assert_eq!(metrics(1100, 720, 96, 999.0).cols, metrics(1100, 720, 96, 240.0).cols);
    assert_eq!(metrics(1100, 720, 96, 1.0).cols, metrics(1100, 720, 96, 100.0).cols);
}

#[test]
fn bigger_thumb_means_fewer_columns() {
    let small = metrics(1100, 720, 96, 100.0).cols;
    let big = metrics(1100, 720, 96, 240.0).cols;
    assert!(small > big, "{small} > {big}");
}

#[test]
fn dpi_scales_card_but_not_column_count_much() {
    let a = metrics(1100, 720, 96, 190.0);
    let b = metrics(1650, 1080, 144, 190.0);
    assert_eq!(a.cols, b.cols);
    assert_eq!(b.card_w, a.card_w * 3 / 2);
}

#[test]
fn virtualization_only_visits_visible_rows_for_tens_of_thousands() {
    let m = metrics(1100, 720, 96, 190.0);
    let n = 50_000;
    let total_rows = m.rows(n);
    assert_eq!(total_rows, 50_000_usize.div_ceil(m.cols));
    // 맨 위.
    let r = m.visible_range(0, n);
    assert_eq!(r.start, 0);
    assert!(r.len() <= m.cols * 4, "한 화면에 {}칸", r.len());
    // 한가운데·끝.
    for scroll in [m.max_scroll(n) / 2, m.max_scroll(n)] {
        let r = m.visible_range(scroll, n);
        assert!(!r.is_empty() && r.len() <= m.cols * 4);
        // 보이는 모든 칸은 실제로 화면과 겹치고, 범위 밖 칸은 겹치지 않는다.
        for i in r.clone() {
            assert!(m.item_rect(i, scroll).overlaps(&m.viewport), "항목 {i} 가 화면 밖");
        }
        if r.start > 0 {
            assert!(!m.item_rect(r.start - 1, scroll).overlaps(&m.viewport));
        }
        if r.end < n {
            assert!(!m.item_rect(r.end, scroll).overlaps(&m.viewport));
        }
    }
    assert_eq!(m.visible_range(m.max_scroll(n), n).end, n);
}

#[test]
fn visible_range_empty_and_short_lists() {
    let m = metrics(1100, 720, 96, 190.0);
    assert!(m.visible_range(0, 0).is_empty());
    assert_eq!(m.max_scroll(0), 0);
    assert_eq!(m.visible_range(0, 3), 0..3);
    assert_eq!(m.max_scroll(3), 0);
}

#[test]
fn hit_test_matches_item_rects_and_misses_gaps() {
    let m = metrics(1100, 720, 96, 190.0);
    let n = 1000;
    for scroll in [0, 777, m.max_scroll(n)] {
        for i in m.visible_range(scroll, n) {
            let r = m.item_rect(i, scroll);
            assert_eq!(m.hit((r.left + r.right) / 2, (r.top + r.bottom) / 2, scroll, n), if m.viewport.contains((r.left + r.right) / 2, (r.top + r.bottom) / 2) { Some(i) } else { None });
        }
    }
    let r0 = m.item_rect(0, 0);
    assert_eq!(m.hit(r0.right + 3, r0.top + 5, 0, n), None, "칸 사이 간격");
    assert_eq!(m.hit(2, r0.top + 5, 0, n), None, "바깥 여백");
    assert_eq!(m.hit(r0.left + 5, 5, 0, n), None, "도구줄 위");
    // 마지막 줄의 빈 칸.
    let n2 = m.cols + 1;
    let blank = m.item_rect(m.cols + 1, 0);
    assert_eq!(m.hit(blank.left + 4, blank.top + 4, 0, n2), None);
}

#[test]
fn scroll_clamp_and_show() {
    let m = metrics(1100, 720, 96, 190.0);
    let n = 400;
    assert!(m.max_scroll(n) > 0);
    let last = n - 1;
    let s = m.scroll_to_show(last, 0, n);
    assert_eq!(s, m.max_scroll(n));
    let r = m.item_rect(last, s);
    assert!(r.bottom <= m.viewport.bottom && r.top >= m.viewport.top);
    // 이미 보이면 그대로.
    assert_eq!(m.scroll_to_show(0, 0, n), 0);
    // 위로 되돌아오면 맨 위.
    assert_eq!(m.scroll_to_show(0, s, n), 0);
    let mut app = App::new(190.0);
    app.coll.replace(synthetic_docs(n, 1));
    app.scroll_by(&m, 10_000_000);
    assert_eq!(app.scroll, m.max_scroll(n));
    app.scroll_by(&m, -10_000_000);
    assert_eq!(app.scroll, 0);
}

#[test]
fn scrollbar_thumb_round_trip() {
    let m = metrics(1100, 720, 96, 190.0);
    let n = 5000;
    assert!(m.scrollbar_thumb(0, 3).is_none());
    for s in [0, m.max_scroll(n) / 3, m.max_scroll(n)] {
        let t = m.scrollbar_thumb(s, n).unwrap();
        assert!(t.top >= m.viewport.top && t.bottom <= m.viewport.bottom);
        let back = m.scroll_from_thumb_top(t.top, n);
        assert!((back - s).abs() <= m.max_scroll(n) / (m.viewport.height() / 2), "{back} vs {s}");
    }
}

// ---------- 키보드 이동 ----------

#[test]
fn keyboard_navigation() {
    let m = metrics(1100, 720, 96, 190.0);
    let c = m.cols;
    let n = 20;
    assert_eq!(m.navigate(None, Nav::Right, n), Some(0));
    assert_eq!(m.navigate(None, Nav::End, n), Some(n - 1));
    assert_eq!(m.navigate(Some(0), Nav::Left, n), Some(0));
    assert_eq!(m.navigate(Some(0), Nav::Up, n), Some(0));
    assert_eq!(m.navigate(Some(2), Nav::Right, n), Some(3));
    assert_eq!(m.navigate(Some(2), Nav::Down, n), Some(2 + c));
    assert_eq!(m.navigate(Some(2 + c), Nav::Up, n), Some(2));
    // 아래 줄에 같은 칸이 없으면 마지막 칸으로.
    assert_eq!(m.navigate(Some(n - 1 - (n - 1) % c - c + (c - 1)), Nav::Down, n).map(|v| v < n), Some(true));
    assert_eq!(m.navigate(Some(n - 1), Nav::Right, n), Some(n - 1));
    assert_eq!(m.navigate(Some(n - 1), Nav::Down, n), Some(n - 1));
    assert_eq!(m.navigate(Some(7), Nav::Home, n), Some(0));
    assert_eq!(m.navigate(Some(7), Nav::End, n), Some(n - 1));
    assert_eq!(m.navigate(Some(0), Nav::PageDown, n), Some((m.rows_per_page() * c).min(n - 1)));
    assert_eq!(m.navigate(Some(n - 1), Nav::PageUp, n), Some((n - 1).saturating_sub(m.rows_per_page() * c)));
    assert_eq!(m.navigate(Some(0), Nav::Down, 0), None);
}

#[test]
fn selection_scrolls_into_view() {
    let m = metrics(1100, 720, 96, 190.0);
    let mut app = App::new(190.0);
    app.coll.replace(synthetic_docs(400, 1));
    app.select(&m, Some(0));
    for _ in 0..30 {
        app.nav(&m, Nav::Down);
    }
    let i = app.selected.unwrap();
    assert_eq!(i, 30 * m.cols);
    let r = m.item_rect(i, app.scroll);
    assert!(r.top >= m.viewport.top && r.bottom <= m.viewport.bottom, "선택 칸이 화면 안에 있어야 한다");
    app.nav(&m, Nav::Home);
    assert_eq!((app.selected, app.scroll), (Some(0), 0));
    app.select(&m, Some(10_000));
    assert_eq!(app.selected, None, "범위 밖 선택은 무시");
}

#[test]
fn thumb_size_change_keeps_selected_item_visible() {
    let (w, h, dpi) = (1100, 720, 96);
    let mk = |t: f64| Metrics::new(w, h, dpi, t, false);
    let mut app = App::new(190.0);
    app.coll.replace(synthetic_docs(2000, 1));
    app.select(&mk(190.0), Some(900));
    for t in [100.0, 240.0, 150.0] {
        app.set_thumb(t, mk);
        assert_eq!(app.thumb, t);
        let m = mk(t);
        let r = m.item_rect(900, app.scroll);
        assert!(r.top >= m.viewport.top && r.bottom <= m.viewport.bottom, "크기 {t}: 선택 칸이 화면 밖");
    }
    app.set_thumb(9999.0, mk);
    assert_eq!(app.thumb, 240.0);
}

#[test]
fn slider_maps_range_100_to_240() {
    let tb = Toolbar::new(1100, 96);
    assert_eq!(tb.slider_value(tb.slider_x(100.0)), 100.0);
    assert_eq!(tb.slider_value(tb.slider_x(240.0)), 240.0);
    assert_eq!(tb.slider_value(tb.slider_x(190.0)), 190.0);
    assert_eq!(tb.slider_value(-500), 100.0);
    assert_eq!(tb.slider_value(99_999), 240.0);
    assert!(tb.slider_x(100.0) < tb.slider_x(240.0));
    // 도구줄 칸이 서로 겹치지 않고 폭 안에 있다(최소 폭 MIN_W=820 포함).
    for w in [820, 900, 1100, 1920] {
        let t = Toolbar::new(w, 96);
        let parts = [t.search, t.sort, t.count, t.slider_label, t.slider, t.exclude, t.action];
        for (i, a) in parts.iter().enumerate() {
            assert!(a.left >= 0 && a.right <= w, "폭 {w}: {i}번 칸이 밖");
            for b in &parts[i + 1..] {
                assert!(!a.overlaps(b), "폭 {w}: 도구줄 칸 겹침 {a:?} {b:?}");
            }
        }
    }
}

// ---------- 모델: 검색·정렬·표시 글자 ----------

fn doc(name: &str, ext: &str, size: u64, mtime: f64) -> Doc {
    Doc { path: format!(r"D:\{name}"), name: name.into(), ext: ext.into(), size, mtime, cloud: false }
}

#[test]
fn search_and_sort() {
    let mut c = Collection::new(vec![doc("b 예산.hwp", "hwp", 10, 3.0), doc("A 계획.pdf", "pdf", 30, 1.0), doc("c 예산 계획.xlsx", "xlsx", 20, 2.0)]);
    let names = |c: &Collection| (0..c.len()).map(|i| c.get(i).unwrap().name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&c), ["A 계획.pdf", "b 예산.hwp", "c 예산 계획.xlsx"], "이름순은 대소문자 무시");
    c.set_sort(SortKey::Size, true);
    assert_eq!(c.get(0).unwrap().size, 30);
    c.set_sort(SortKey::Modified, false);
    assert_eq!(c.get(0).unwrap().mtime, 1.0);
    c.set_sort(SortKey::Ext, false);
    assert_eq!(c.get(0).unwrap().ext, "hwp");
    c.set_query("예산");
    assert_eq!(c.len(), 2);
    c.set_query("예산 계획");
    assert_eq!(c.len(), 1, "낱말 모두 포함");
    c.set_query("XLSX");
    assert_eq!(c.len(), 1, "확장자도 검색, 대소문자 무시");
    c.set_query("없는말");
    assert!(c.is_empty());
    assert_eq!(c.total(), 3);
    c.set_query("");
    assert_eq!(c.len(), 3);
    c.cycle_sort();
    assert_eq!(c.sort, SortKey::Ext.next());
}

#[test]
fn selection_follows_document_when_scan_adds_items() {
    let m = metrics(1100, 720, 96, 190.0);
    let mut app = App::new(190.0);
    app.coll.replace(vec![doc("m.hwp", "hwp", 1, 1.0), doc("z.hwp", "hwp", 1, 1.0)]);
    app.select(&m, Some(1));
    assert_eq!(app.selected_doc().unwrap().name, "z.hwp");
    app.extend_docs(vec![doc("a.hwp", "hwp", 1, 1.0), doc("n.hwp", "hwp", 1, 1.0)]);
    assert_eq!(app.selected_doc().unwrap().name, "z.hwp");
    assert_eq!(app.selected, Some(3));
}

#[test]
fn display_strings_match_flutter_rules() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(1023), "1023 B");
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(1536), "1.5 KB");
    assert_eq!(format_size(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
    assert_eq!(format_date(0.0, 0), "1970-01-01");
    assert_eq!(format_date(1_700_000_000.0, 0), "2023-11-14");
    assert_eq!(format_date(1_700_000_000.0, 9 * 3600), "2023-11-15");
    assert_eq!(format_date(951_782_400.0, 0), "2000-02-29");
    assert_eq!(group_digits(0), "0");
    assert_eq!(group_digits(1234), "1,234");
    assert_eq!(group_digits(50_000), "50,000");
    assert_eq!(group_digits(1_234_567), "1,234,567");
    assert_eq!(docview_win::model::ext_color("PDF"), docview_win::model::ext_color("pdf"));
    assert_ne!(docview_win::model::ext_color("hwp"), docview_win::model::ext_color("xls"));
    assert_eq!(docview_win::model::badge_text(""), "DOC");
    assert_eq!(docview_win::model::badge_text("hwpx"), "HWPX");
}

// ---------- klib scan 줄 파서 ----------

const LINE: &str = r#"{"path":"D:\\업무\\기획\\2026년 업무계획(안).hwp","name":"2026년 업무계획(안).hwp","ext":"hwp","size":12345,"mtime":1763800000.5,"mtime_ns":1763800000500000000,"cloud":false}"#;

#[test]
fn parses_klib_scan_line() {
    let d = parse_line(LINE).unwrap();
    assert_eq!(d.path, r"D:\업무\기획\2026년 업무계획(안).hwp");
    assert_eq!(d.name, "2026년 업무계획(안).hwp");
    assert_eq!(d.ext, "hwp");
    assert_eq!(d.size, 12345);
    assert_eq!(d.mtime, 1763800000.5);
    assert!(!d.cloud);
    assert_eq!(scan::parent_folder(&d.path), r"D:\업무\기획");
}

#[test]
fn parser_handles_escapes_unicode_and_cloud() {
    let l = r#"{"path":"C:\\a\"b\u0041\ud83d\ude00.pdf","name":"x\ty.pdf","ext":".PDF","size":0,"mtime":0,"cloud":true,"extra":[1,{"k":null}]}"#;
    let d = parse_line(l).unwrap();
    assert_eq!(d.path, "C:\\a\"bA😀.pdf");
    assert_eq!(d.name, "x\ty.pdf");
    assert_eq!(d.ext, "pdf", "점·대문자 정규화");
    assert!(d.cloud);
}

#[test]
fn parser_rejects_invalid_rows_like_python() {
    for bad in [
        "",
        "not json",
        "[1,2]",
        r#"{"path":"a","name":"b","ext":"hwp","size":1,"mtime":1}"#,           // cloud 없음
        r#"{"path":"a","name":"b","ext":"hwp","size":"1","mtime":1,"cloud":false}"#, // size 형식
        r#"{"path":"a","name":"b","ext":"hwp","size":-1,"mtime":1,"cloud":false}"#,
        r#"{"path":"","name":"b","ext":"hwp","size":1,"mtime":1,"cloud":false}"#,
        r#"{"path":"a","name":"b","ext":"hwp","size":1,"mtime":1,"cloud":1}"#,
        r#"{"path":"a","name":"b","ext":"hwp","size":1,"mtime":1,"cloud":false} x"#,
        r#"{"path":"a\"#,
    ] {
        assert!(parse_line(bad).is_err(), "받아들이면 안 됨: {bad}");
    }
}

#[test]
fn synthetic_lines_round_trip_through_parser() {
    for d in synthetic_docs(200, 5) {
        let back = parse_line(&to_json_line(&d)).unwrap();
        assert_eq!((back.path, back.name, back.ext, back.size, back.cloud), (d.path, d.name, d.ext, d.size, d.cloud));
    }
}

#[test]
fn klib_arguments_match_python_adapter() {
    let a = scan::klib_args(&[r"C:\".into(), r"D:\".into()], &[r"D:\skip".to_string()]);
    assert_eq!(a, ["scan", "--roots", r"C:\", r"D:\", "--ext", "hwp,hwpx,xls,xlsx,ppt,pptx,pdf", "--json-lines", "--exclude", r"D:\skip"]);
    assert!(!scan::klib_args(&[r"C:\".into()], &[]).contains(&"--exclude".to_string()));
}

// ---------- 비동기 스캔·취소 ----------

fn wait_finished(job: &ScanJob, limit: Duration) -> (Vec<Doc>, usize, Option<bool>, Option<String>) {
    let (mut docs, mut visited, mut fin, mut err) = (Vec::new(), 0, None, None);
    let t = Instant::now();
    while t.elapsed() < limit && fin.is_none() && err.is_none() {
        match job.rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Event::Batch { docs: d, visited: v, .. }) => {
                docs.extend(d);
                visited = v;
            }
            Ok(Event::Finished { cancelled }) => fin = Some(cancelled),
            Ok(Event::Failed(e)) => err = Some(e),
            Err(_) => {}
        }
    }
    (docs, visited, fin, err)
}

#[test]
fn scan_job_streams_batches_and_finishes() {
    let docs = synthetic_docs(1234, 9);
    let lines: Vec<io::Result<String>> = docs.iter().map(|d| Ok(to_json_line(d))).collect();
    let woke = Arc::new(AtomicUsize::new(0));
    let w = woke.clone();
    let mut job = ScanJob::from_lines(lines.into_iter(), None, Arc::new(move || {
        w.fetch_add(1, Ordering::SeqCst);
    }));
    let (got, visited, fin, err) = wait_finished(&job, Duration::from_secs(10));
    job.join();
    assert_eq!(err, None);
    assert_eq!(fin, Some(false));
    assert_eq!(got.len(), 1234);
    assert_eq!(visited, 1234);
    assert_eq!(got[0].name, docs[0].name);
    assert!(woke.load(Ordering::SeqCst) >= 3, "배치마다 창을 깨운다(1234줄은 500줄 단위로 최소 3번)");
}

#[test]
fn scan_job_reports_bad_line_as_failure() {
    let lines = vec![Ok(to_json_line(&synthetic_docs(1, 1)[0])), Ok("garbage".to_string()), Ok(to_json_line(&synthetic_docs(1, 2)[0]))];
    let mut job = ScanJob::from_lines(lines.into_iter(), None, Arc::new(|| {}));
    let (_, _, fin, err) = wait_finished(&job, Duration::from_secs(10));
    job.join();
    assert_eq!(fin, None);
    assert!(err.unwrap().contains("잘못된 스캔 줄"));
}

#[test]
fn scan_job_reports_read_error_as_failure() {
    let lines = vec![Err(io::Error::other("깨짐"))];
    let mut job = ScanJob::from_lines(lines.into_iter(), None, Arc::new(|| {}));
    let (_, _, _, err) = wait_finished(&job, Duration::from_secs(10));
    job.join();
    assert!(err.unwrap().contains("깨짐"));
}

#[test]
fn cancel_stops_an_endless_scan_quickly() {
    let produced = Arc::new(AtomicUsize::new(0));
    let p = produced.clone();
    let endless = (0u64..).map(move |i| {
        std::thread::sleep(Duration::from_millis(1));
        p.fetch_add(1, Ordering::SeqCst);
        Ok(to_json_line(&synthetic_docs(1, i)[0]))
    });
    let killed = Arc::new(AtomicUsize::new(0));
    let k = killed.clone();
    let mut job = ScanJob::from_lines(endless, Some(Box::new(move || {
        k.fetch_add(1, Ordering::SeqCst);
    })), Arc::new(|| {}));
    // 첫 배치(80ms 시간 배치)가 올 때까지 기다린다.
    let t = Instant::now();
    let first = loop {
        if let Ok(Event::Batch { visited, .. }) = job.rx.recv_timeout(Duration::from_millis(50)) {
            break visited;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "진행 이벤트가 오지 않음");
    };
    assert!(first > 0);
    let before = Instant::now();
    job.cancel();
    assert!(job.is_cancelled());
    assert_eq!(killed.load(Ordering::SeqCst), 1, "취소가 소스(자식 프로세스)를 깨운다");
    let (_, _, fin, err) = wait_finished(&job, Duration::from_secs(5));
    job.join();
    assert_eq!((fin, err), (Some(true), None));
    assert!(before.elapsed() < Duration::from_secs(2), "취소 후 즉시 끝나야 한다");
    let after = produced.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(produced.load(Ordering::SeqCst), after, "취소 뒤 더 읽지 않는다");
}

#[test]
fn app_state_machine_for_scan_view() {
    let mut app = App::new(190.0);
    assert!(!app.scan.busy());
    app.scan.state = ScanState::Running;
    assert!(app.scan.busy());
    assert!(!app.banner_shown(), "비어 있으면 띠 대신 가운데 안내");
    app.coll.replace(synthetic_docs(3, 1));
    assert!(app.banner_shown());
    app.scan.state = ScanState::Cancelling;
    assert!(app.scan.busy());
    app.scan.state = ScanState::Cancelled;
    assert!(!app.scan.busy() && !app.banner_shown());
    // 띠가 있으면 격자 시작 y 가 그만큼 내려간다.
    assert!(Metrics::new(1100, 720, 96, 190.0, true).viewport.top > Metrics::new(1100, 720, 96, 190.0, false).viewport.top);
}

#[cfg(windows)]
#[test]
fn real_klib_scan_if_available() {
    let Some(exe) = scan::find_klib() else {
        eprintln!("klib.exe 없음: 실제 klib 스캔 시험 건너뜀");
        return;
    };
    let dir = std::env::temp_dir().join(format!("docview_win_scan_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("하위 폴더")).unwrap();
    for f in ["가.hwp", "나.PDF", "하위 폴더/다.xlsx", "무시.txt"] {
        std::fs::write(dir.join(f), b"x").unwrap();
    }
    let mut job = ScanJob::start_klib(&exe, &[dir.clone()], &[], Arc::new(|| {})).unwrap();
    let (docs, _, fin, err) = wait_finished(&job, Duration::from_secs(30));
    job.join();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(err, None);
    assert_eq!(fin, Some(false));
    let mut names: Vec<_> = docs.iter().map(|d| d.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["가.hwp", "나.PDF", "다.xlsx"]);
}

// ---------- 설정 읽기 ----------

#[test]
fn thumb_size_setting_rules_match_python() {
    assert_eq!(settings::thumb_size("{}"), 190.0);
    assert_eq!(settings::thumb_size(""), 190.0, "깨진 파일은 기본값");
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"doc_grid":150}}"#), 150.0);
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"doc_grid":150.5,"photo_album":120}}"#), 150.5);
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"doc_grid":50}}"#), 100.0, "최소로 맞춤");
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"doc_grid":9999}}"#), 240.0, "최대로 맞춤");
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"photo_album":120}}"#), 190.0, "다른 화면 값은 쓰지 않는다");
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"doc_grid":"150"}}"#), 190.0);
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":[1,2]}"#), 190.0);
    assert_eq!(settings::thumb_size("\u{feff}{\"ui_thumb_sizes\":{\"doc_grid\":222}}"), 222.0, "BOM 허용");
    assert_eq!((settings::MIN_THUMB, settings::MAX_THUMB, settings::DEFAULT_DOC_THUMB), (100.0, 240.0, 190.0));
}

#[test]
fn excluded_folders_and_ui_choice() {
    let t = r#"{"library_excluded_folders":["D:\\a"," D:\\A\\ ","","C:/x","c:\\x",3,"E:\\b"],"docview_ui":"native"}"#;
    assert_eq!(settings::excluded_folders(t), [r"D:\a", "C:/x", r"E:\b"], "빈 값·숫자 제외, 대소문자·구분자 무시 중복 제거");
    assert!(settings::excluded_folders("{}").is_empty());
    assert!(settings::excluded_folders("깨짐").is_empty());
    assert_eq!(settings::docview_ui(t), "native");
    assert_eq!(settings::docview_ui("{}"), "flutter");
    assert_eq!(settings::docview_ui(r#"{"docview_ui":"weird"}"#), "flutter");
    assert_eq!(settings::docview_ui("깨짐"), "flutter");
}

#[test]
fn settings_file_is_read_only() {
    let dir = std::env::temp_dir().join(format!("docview_win_settings_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("settings.json");
    let body = "{\r\n  \"ui_thumb_sizes\": {\"doc_grid\": 123}\r\n}\r\n";
    std::fs::write(&p, body).unwrap();
    let before = std::fs::metadata(&p).unwrap().modified().unwrap();
    let text = settings::read_text(&p);
    assert_eq!(settings::thumb_size(&text), 123.0, "CRLF 줄바꿈도 읽는다");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), body);
    assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), before);
    assert_eq!(settings::read_text(&dir.join("없음.json")), "{}");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(settings::settings_path().ends_with("settings.json"));
}

#[test]
fn json_parser_basics() {
    use docview_win::json::{Value, parse};
    assert_eq!(parse("[1, 2.5, -3e2, true, null, \"x\"]").unwrap().as_array().unwrap().len(), 6);
    assert_eq!(parse("{\"a\":{\"b\":[]}}").unwrap().get("a").unwrap().get("b"), Some(&Value::Arr(vec![])));
    assert!(parse("{\"a\":1,}").is_err());
    assert!(parse("[1 2]").is_err());
    assert!(parse(&"[".repeat(100)).is_err(), "너무 깊은 중첩 거절");
    assert!(parse("\"\\ud800\"").is_err(), "짝 없는 서로게이트");
}

// ---------- 자식 프로세스 경로(가짜 klib 배치 파일) ----------

#[cfg(windows)]
fn fake_klib(name: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("docview_win_fake_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("klib.bat");
    std::fs::write(&exe, format!("@echo off\r\n{body}\r\n")).unwrap();
    (dir, exe)
}

#[cfg(windows)]
#[test]
fn child_process_output_is_parsed_and_exit_code_checked() {
    let l1 = r#"{"path":"D:\\a.hwp","name":"a.hwp","ext":"hwp","size":10,"mtime":1.5,"mtime_ns":1,"cloud":false}"#;
    let l2 = r#"{"path":"D:\\b.pdf","name":"b.pdf","ext":"pdf","size":20,"mtime":2.5,"mtime_ns":2,"cloud":true}"#;
    // 정상 종료.
    let (dir, exe) = fake_klib("ok", &format!("echo {l1}\r\necho {l2}"));
    let mut job = ScanJob::start_klib(&exe, &[r"D:\".into()], &[], Arc::new(|| {})).unwrap();
    let (docs, visited, fin, err) = wait_finished(&job, Duration::from_secs(20));
    job.join();
    assert_eq!((err, fin, visited), (None, Some(false), 2));
    assert_eq!(docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["a.hwp", "b.pdf"]);
    assert!(docs[1].cloud);
    let _ = std::fs::remove_dir_all(&dir);
    // 0이 아닌 종료 코드는 실패.
    let (dir, exe) = fake_klib("fail", &format!("echo {l1}\r\nexit /b 3"));
    let mut job = ScanJob::start_klib(&exe, &[r"D:\".into()], &[], Arc::new(|| {})).unwrap();
    let (_, _, fin, err) = wait_finished(&job, Duration::from_secs(20));
    job.join();
    assert_eq!(fin, None);
    assert!(err.unwrap().contains("종료 코드"));
    let _ = std::fs::remove_dir_all(&dir);
    // 실행 파일이 없으면 시작 단계에서 오류.
    assert!(ScanJob::start_klib(std::path::Path::new(r"Z:\없는\klib.exe"), &[r"D:\".into()], &[], Arc::new(|| {})).is_err());
}

#[cfg(windows)]
#[test]
fn cancel_kills_the_child_process() {
    let l1 = r#"{"path":"D:\\a.hwp","name":"a.hwp","ext":"hwp","size":10,"mtime":1.5,"mtime_ns":1,"cloud":false}"#;
    // 한 줄 낸 뒤 30초 멈춘다(취소하지 않으면 시험이 30초 걸린다).
    let (dir, exe) = fake_klib("hang", &format!("echo {l1}\r\nping -n 30 127.0.0.1 >nul"));
    let mut job = ScanJob::start_klib(&exe, &[r"D:\".into()], &[], Arc::new(|| {})).unwrap();
    let t = Instant::now();
    loop {
        if let Ok(Event::Batch { visited, .. }) = job.rx.recv_timeout(Duration::from_millis(50)) {
            assert_eq!(visited, 1);
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(20), "첫 줄이 오지 않음");
    }
    let before = Instant::now();
    job.cancel();
    let (_, _, fin, err) = wait_finished(&job, Duration::from_secs(10));
    job.join();
    assert_eq!((fin, err), (Some(true), None));
    assert!(before.elapsed() < Duration::from_secs(5), "자식을 죽이면 곧 끝난다: {:?}", before.elapsed());
    let _ = std::fs::remove_dir_all(&dir);
}
