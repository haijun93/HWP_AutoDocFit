//! 사진뷰 순수 부분 시험(창 없음): 격자 배치·가상화, 스캔 줄 파서, 비동기 스캔·취소, 설정 읽기, 합성 모델.
use grid_core::layout::{GridSpec, Metrics, Toolbar};
use photoview_win::model::{App, Collection, Doc, Nav, ScanState, SortKey, badge_text, format_date, format_size, photo_color};
use photoview_win::scan::{self, Event, PHOTO_EXTENSIONS, ScanJob, parse_line};
use photoview_win::settings;
use photoview_win::synth::{new_app, synthetic_photos, to_json_line};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn photo_metrics(w: i32, h: i32, dpi: u32, thumb: f64) -> Metrics {
    Metrics::with_spec(GridSpec::PHOTO, w, h, dpi, thumb, false)
}

// ---------- 배치·가상화 ----------

#[test]
fn photo_grid_uses_flutter_photo_rule() {
    // Flutter 사진 격자: maxCrossAxisExtent=썸네일 크기, 간격 10, 가로세로비 0.8, 바깥 여백 16.
    let m = photo_metrics(1100, 720, 96, 160.0);
    assert_eq!((m.gap, m.pad), (10, 16));
    // 가용 폭 = 1100 - 스크롤바 12 - 여백 32 = 1056, 칸 하나 최대 160+10 → ceil(1056/170) = 7열.
    assert_eq!(m.cols, 7);
    assert_eq!(m.card_w, (1056 - 10 * 6) / 7);
    assert_eq!(m.card_h, (m.card_w as f64 / 0.8).round() as i32);
    // 문서뷰 규칙과는 달라야 한다(간격·비율).
    let d = Metrics::new(1100, 720, 96, 160.0, false);
    assert_ne!((m.gap, m.card_h), (d.gap, d.card_h));
}

#[test]
fn smaller_thumbs_give_more_columns_and_dpi_scales() {
    let wide = |t: f64| photo_metrics(1100, 720, 96, t).cols;
    assert!(wide(100.0) > wide(160.0) && wide(160.0) > wide(240.0));
    let (a, b) = (photo_metrics(1100, 720, 96, 160.0), photo_metrics(1650, 1080, 144, 160.0));
    assert_eq!(a.cols, b.cols, "150% 에서도 같은 열 수");
    assert_eq!(b.gap, 15);
}

#[test]
fn fifty_thousand_photos_only_visible_cells() {
    let m = photo_metrics(1100, 720, 96, 160.0);
    let n = 50_000;
    let max = m.max_scroll(n);
    assert!(max > 0);
    for scroll in [0, 1, 777, max / 2, max - 1, max] {
        let r = m.visible_range(scroll, n);
        assert!(r.len() <= 4 * m.cols * 2, "scroll {scroll}: {}칸", r.len());
        // 범위 안의 칸은 모두 화면 영역과 겹치고, 바로 앞뒤 칸은 겹치지 않는다.
        for i in r.clone() {
            assert!(m.item_rect(i, scroll).overlaps(&m.viewport), "scroll {scroll} 칸 {i}");
        }
        if r.start >= m.cols {
            assert!(!m.item_rect(r.start - m.cols, scroll).overlaps(&m.viewport), "앞 줄이 범위 밖이어야 한다");
        }
        if r.end + m.cols <= n {
            assert!(!m.item_rect(r.end + m.cols - 1, scroll).overlaps(&m.viewport), "뒤 줄이 범위 밖이어야 한다");
        }
    }
    assert_eq!(m.visible_range(max, n).end, n, "맨 끝에서는 마지막 사진까지 포함");
    assert_eq!(m.visible_range(0, 0), 0..0);
    assert_eq!(m.visible_range(0, 3), 0..3);
}

#[test]
fn hit_test_and_scroll_to_show() {
    let m = photo_metrics(1100, 720, 96, 160.0);
    let n = 500;
    let r = m.item_rect(10, 0);
    assert_eq!(m.hit(r.left + 3, r.top + 3, 0, n), Some(10));
    assert_eq!(m.hit(r.right + 2, r.top + 3, 0, n), None, "칸 사이 틈");
    assert_eq!(m.hit(m.viewport.right - 3, r.top + 3, 0, n), None, "스크롤바 위는 칸이 아니다");
    for i in [0usize, 99, 250, 499] {
        let s = m.scroll_to_show(i, 0, n);
        let rc = m.item_rect(i, s);
        assert!(rc.top >= m.viewport.top && rc.bottom <= m.viewport.bottom, "{i} 가 완전히 보여야 한다");
    }
    // 스크롤바 손잡이 왕복.
    let t = m.scrollbar_thumb(1234, n).unwrap();
    let back = m.scroll_from_thumb_top(t.top, n);
    assert!((back - 1234).abs() <= m.content_height(n) / m.viewport.height() + 2, "왕복 오차 {back}");
    assert!(m.scrollbar_thumb(0, 3).is_none(), "내용이 짧으면 손잡이 없음");
}

#[test]
fn keyboard_navigation_over_the_grid() {
    let m = photo_metrics(1100, 720, 96, 160.0);
    let n = 100;
    let c = m.cols;
    assert_eq!(m.navigate(None, Nav::Right, n), Some(0));
    assert_eq!(m.navigate(None, Nav::End, n), Some(n - 1));
    assert_eq!(m.navigate(Some(5), Nav::Right, n), Some(6));
    assert_eq!(m.navigate(Some(5), Nav::Down, n), Some(5 + c));
    assert_eq!(m.navigate(Some(5 + c), Nav::Up, n), Some(5));
    assert_eq!(m.navigate(Some(0), Nav::Left, n), Some(0));
    assert_eq!(m.navigate(Some(n - 1), Nav::Right, n), Some(n - 1));
    assert_eq!(m.navigate(Some(50), Nav::Home, n), Some(0));
    assert_eq!(m.navigate(Some(50), Nav::End, n), Some(n - 1));
    assert!(m.navigate(Some(0), Nav::PageDown, n).unwrap() > c);
    assert_eq!(m.navigate(Some(0), Nav::Down, 0), None);
    // 아래 줄이 마지막 줄에 못 미치면 마지막 칸으로.
    let last_row_start = ((n - 1) / c) * c;
    assert_eq!(m.navigate(Some(last_row_start - c), Nav::Down, n), Some((last_row_start - c + c).min(n - 1)));
    // App 은 선택 칸이 항상 보이게 스크롤한다.
    let mut app = new_app(160.0);
    app.coll.replace(synthetic_photos(3000, 5));
    let m = app.metrics(1100, 720, 96);
    for _ in 0..40 {
        app.nav(&m, Nav::Down);
    }
    let sel = app.selected.unwrap();
    let rc = m.item_rect(sel, app.scroll);
    assert!(rc.top >= m.viewport.top && rc.bottom <= m.viewport.bottom);
    app.nav(&m, Nav::End);
    assert_eq!(app.selected, Some(2999));
    assert_eq!(app.scroll, m.max_scroll(3000));
}

#[test]
fn slider_range_and_selection_stays_visible_when_resized() {
    let tb = Toolbar::new(1100, 96);
    assert_eq!(tb.slider_value(tb.slider_x(100.0)), 100.0);
    assert_eq!(tb.slider_value(tb.slider_x(240.0)), 240.0);
    assert!((tb.slider_value(tb.slider_x(160.0)) - 160.0).abs() <= 2.0, "중간 값은 픽셀 단위로 왕복(±2)");
    assert_eq!(tb.slider_value(-500), 100.0, "왼쪽 끝으로 맞춤");
    assert_eq!(tb.slider_value(99_999), 240.0, "오른쪽 끝으로 맞춤");
    for w in [820, 1100, 1920] {
        let t = Toolbar::new(w, 96);
        let parts = [t.search, t.sort, t.count, t.slider_label, t.slider, t.exclude, t.action];
        for (i, a) in parts.iter().enumerate() {
            for b in &parts[i + 1..] {
                assert!(!a.overlaps(b), "폭 {w}: 도구줄 칸이 겹친다 {a:?} {b:?}");
            }
        }
    }
    let mut app = new_app(160.0);
    app.coll.replace(synthetic_photos(4000, 6));
    let make = |t: f64| photo_metrics(1100, 720, 96, t);
    let m = make(160.0);
    app.select(&m, Some(2500));
    app.set_thumb(100.0, make);
    let m2 = make(app.thumb);
    let rc = m2.item_rect(2500, app.scroll);
    assert!(rc.top >= m2.viewport.top && rc.bottom <= m2.viewport.bottom, "크기를 바꿔도 선택 칸이 보인다");
    app.set_thumb(9999.0, make);
    assert_eq!(app.thumb, 240.0);
    app.set_thumb(f64::NAN, make);
    assert_eq!(app.thumb, 160.0, "사진뷰 기본값 160");
}

// ---------- 모델 ----------

#[test]
fn synthetic_photos_are_unique_deterministic_and_newest_first() {
    let a = synthetic_photos(2000, 11);
    assert_eq!(a, synthetic_photos(2000, 11));
    let mut names: Vec<_> = a.iter().map(|d| d.path.clone()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 2000);
    assert!(a.iter().all(|d| PHOTO_EXTENSIONS.contains(&d.ext.as_str())));
    let mut app = new_app(160.0);
    app.coll.replace(a);
    assert_eq!((app.coll.sort, app.coll.descending), (SortKey::Modified, true));
    let t: Vec<f64> = (0..app.coll.len()).map(|i| app.coll.get(i).unwrap().mtime).collect();
    assert!(t.windows(2).all(|w| w[0] >= w[1]), "최신 사진이 먼저");
}

#[test]
fn search_and_sort() {
    let docs = vec![
        Doc { path: r"D:\a\IMG_1.JPG".into(), name: "IMG_1.JPG".into(), ext: "jpg".into(), size: 300, mtime: 10.0, cloud: false },
        Doc { path: r"D:\a\현장 점검 2.png".into(), name: "현장 점검 2.png".into(), ext: "png".into(), size: 100, mtime: 30.0, cloud: false },
        Doc { path: r"D:\b\준공 사진.heic".into(), name: "준공 사진.heic".into(), ext: "heic".into(), size: 200, mtime: 20.0, cloud: true },
    ];
    let mut c = Collection::new(docs);
    c.set_sort(SortKey::Name, false);
    assert_eq!(c.get(0).unwrap().name, "IMG_1.JPG");
    c.set_sort(SortKey::Modified, true);
    assert_eq!(c.get(0).unwrap().name, "현장 점검 2.png");
    c.set_sort(SortKey::Size, false);
    assert_eq!(c.get(0).unwrap().size, 100);
    c.set_query("img");
    assert_eq!(c.len(), 1);
    c.set_query("  점검   png ");
    assert_eq!(c.len(), 1, "낱말 모두 포함(이름·확장자)");
    c.set_query("heic 준공");
    assert_eq!(c.len(), 1);
    c.set_query("없는말");
    assert!(c.is_empty() && c.total() == 3);
}

#[test]
fn display_strings_and_colors() {
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(2048), "2.0 KB");
    assert_eq!(format_size(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
    assert_eq!(format_date(0.0, 0), "1970-01-01");
    assert_eq!(format_date(1_764_000_000.0, 9 * 3600), "2025-11-25");
    assert_eq!(badge_text("heic"), "HEIC");
    assert_eq!(badge_text(""), "IMG");
    assert_ne!(photo_color("jpg"), photo_color("png"));
    assert_eq!(photo_color("jpg"), photo_color("JPEG"));
    assert_eq!(photo_color("zzz"), photo_color("bmp"));
}

#[test]
fn app_scan_banner_rules() {
    let mut app = new_app(160.0);
    app.scan.state = ScanState::Running;
    assert!(!app.banner_shown(), "비어 있으면 띠 대신 가운데 안내");
    app.coll.replace(synthetic_photos(3, 1));
    assert!(app.banner_shown());
    app.scan.state = ScanState::Cancelled;
    assert!(!app.banner_shown());
    let a: App = new_app(160.0);
    assert!(a.metrics(1100, 720, 96).cols >= 1 && a.spec == GridSpec::PHOTO);
}

#[test]
fn selection_follows_the_same_photo_while_scanning() {
    let mut app = new_app(160.0);
    app.coll.replace(synthetic_photos(300, 4));
    let m = app.metrics(1100, 720, 96);
    app.select(&m, Some(20));
    let path = app.selected_doc().unwrap().path.clone();
    // 더 최신 사진 여러 장이 들어와 순서가 밀려도 같은 사진에 머문다.
    let mut more = synthetic_photos(50, 99);
    for d in &mut more {
        d.mtime += 1e9;
        d.path = format!("{}_new", d.path);
    }
    app.extend_docs(more);
    assert_eq!(app.selected_doc().unwrap().path, path);
    assert_eq!(app.selected, Some(70));
}

// ---------- klib 스캔 줄 파서 ----------

#[test]
fn parses_klib_photo_lines() {
    let d = parse_line(r#"{"path":"D:\\사진\\IMG_0001.JPG","name":"IMG_0001.JPG","ext":".JPG","size":2048,"mtime":1700000000.5,"mtime_ns":1700000000500000000,"cloud":true}"#).unwrap();
    assert_eq!((d.name.as_str(), d.ext.as_str(), d.size, d.cloud), ("IMG_0001.JPG", "jpg", 2048, true));
    assert_eq!(d.path, r"D:\사진\IMG_0001.JPG");
    assert_eq!(scan::parent_folder(&d.path), r"D:\사진");
    // 합성 줄 왕복.
    for p in synthetic_photos(200, 3) {
        assert_eq!(parse_line(&to_json_line(&p)).unwrap(), p);
    }
    // 이스케이프·서로게이트(이모지 파일 이름).
    let e = parse_line(r#"{"path":"D:\\a\\\ud83d\udcf7 \"여행\".png","name":"\ud83d\udcf7 \"여행\".png","ext":"png","size":1,"mtime":1,"mtime_ns":1,"cloud":false}"#).unwrap();
    assert_eq!(e.name, "📷 \"여행\".png");
}

#[test]
fn rejects_bad_scan_lines() {
    let bad = [
        "",
        "[]",
        "{",
        r#"{"path":"a","name":"a","ext":"jpg","size":1,"mtime":1}"#,
        r#"{"path":"a","name":"a","ext":"jpg","size":-1,"mtime":1,"cloud":false}"#,
        r#"{"path":"a","name":"a","ext":"jpg","size":"1","mtime":1,"cloud":false}"#,
        r#"{"path":"a","name":"a","ext":"jpg","size":1,"mtime":1,"cloud":"no"}"#,
        r#"{"path":"","name":"a","ext":"jpg","size":1,"mtime":1,"cloud":false}"#,
        r#"{"path":"a","name":"","ext":"jpg","size":1,"mtime":1,"cloud":false}"#,
        r#"{"path":1,"name":"a","ext":"jpg","size":1,"mtime":1,"cloud":false}"#,
    ];
    for b in bad {
        assert!(parse_line(b).is_err(), "받아들이면 안 되는 줄: {b}");
    }
}

#[test]
fn klib_args_use_photo_extensions() {
    let a = scan::klib_args(&[r"C:\".into(), r"D:\".into()], &[r"D:\skip".to_string()]);
    assert_eq!(a[..4], ["scan", "--roots", r"C:\", r"D:\"]);
    let ext_at = a.iter().position(|x| x == "--ext").unwrap();
    assert_eq!(a[ext_at + 1], "jpg,jpeg,png,webp,bmp,tif,tiff,heic");
    assert!(!a[ext_at + 1].contains("hwp") && !a[ext_at + 1].contains("pdf"));
    assert!(a.contains(&"--json-lines".to_string()));
    assert_eq!(a[a.len() - 2..], ["--exclude", r"D:\skip"]);
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
    let photos = synthetic_photos(1234, 9);
    let lines: Vec<io::Result<String>> = photos.iter().map(|d| Ok(to_json_line(d))).collect();
    let woke = Arc::new(AtomicUsize::new(0));
    let w = woke.clone();
    let mut job = ScanJob::from_lines(lines.into_iter(), None, Arc::new(move || {
        w.fetch_add(1, Ordering::Relaxed);
    }));
    let (got, visited, fin, err) = wait_finished(&job, Duration::from_secs(10));
    job.join();
    assert_eq!((err, fin, visited), (None, Some(false), 1234));
    assert_eq!(got, photos);
    assert!(woke.load(Ordering::Relaxed) >= 2, "배치마다 창을 깨운다");
}

#[test]
fn bad_line_fails_but_keeps_earlier_photos() {
    let mut lines: Vec<io::Result<String>> = synthetic_photos(5, 1).iter().map(|d| Ok(to_json_line(d))).collect();
    lines.push(Ok("{깨짐".into()));
    lines.push(Ok(to_json_line(&synthetic_photos(1, 2)[0])));
    let mut job = ScanJob::from_lines(lines.into_iter(), None, Arc::new(|| {}));
    let (got, _, fin, err) = wait_finished(&job, Duration::from_secs(10));
    job.join();
    assert_eq!(got.len(), 5, "오류 앞의 사진은 먼저 전달");
    assert_eq!(fin, None);
    assert!(err.unwrap().contains("잘못된 스캔 줄"));
    // 읽기 오류.
    let lines = vec![Ok(to_json_line(&synthetic_photos(1, 2)[0])), Err(io::Error::other("디스크 오류"))];
    let mut job = ScanJob::from_lines(lines.into_iter(), None, Arc::new(|| {}));
    let (_, _, _, err) = wait_finished(&job, Duration::from_secs(10));
    job.join();
    assert!(err.unwrap().contains("디스크 오류"));
}

#[test]
fn sparse_lines_still_report_progress() {
    // 줄이 드물게(200ms 간격) 와도 배치가 80ms 마다 비워져 진행이 멈추지 않는다.
    let photos = synthetic_photos(4, 8);
    let iter = photos.into_iter().map(|d| {
        std::thread::sleep(Duration::from_millis(200));
        Ok(to_json_line(&d))
    });
    let mut job = ScanJob::from_lines(iter, None, Arc::new(|| {}));
    let t = Instant::now();
    let mut batches = 0;
    let mut first_batch_at = None;
    while t.elapsed() < Duration::from_secs(10) {
        match job.rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Event::Batch { .. }) => {
                batches += 1;
                first_batch_at.get_or_insert(t.elapsed());
            }
            Ok(Event::Finished { .. }) => break,
            _ => {}
        }
    }
    job.join();
    assert_eq!(batches, 4, "줄마다 바로 보인다");
    assert!(first_batch_at.unwrap() < Duration::from_millis(600));
}

#[test]
fn cancel_stops_an_endless_scan_quickly() {
    let n = Arc::new(AtomicUsize::new(0));
    let c = n.clone();
    let endless = std::iter::from_fn(move || {
        let i = c.fetch_add(1, Ordering::Relaxed);
        std::thread::sleep(Duration::from_micros(200));
        Some(Ok(to_json_line(&synthetic_photos(1, i as u64)[0])))
    });
    let mut job = ScanJob::from_lines(endless, Some(Box::new(|| {})), Arc::new(|| {}));
    let t = Instant::now();
    loop {
        if let Ok(Event::Batch { visited, .. }) = job.rx.recv_timeout(Duration::from_millis(50)) {
            if visited > 50 {
                break;
            }
        }
        assert!(t.elapsed() < Duration::from_secs(10));
    }
    let before = Instant::now();
    job.cancel();
    assert!(job.is_cancelled());
    let (_, _, fin, err) = wait_finished(&job, Duration::from_secs(5));
    job.join();
    assert_eq!((fin, err), (Some(true), None));
    assert!(before.elapsed() < Duration::from_secs(1), "취소 후 1초 안에 끝: {:?}", before.elapsed());
    let after = n.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(200));
    assert!(n.load(Ordering::Relaxed) <= after + 2, "취소 뒤에는 더 읽지 않는다");
}

#[cfg(windows)]
fn fake_klib(name: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("photoview_win_fake_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("klib.bat");
    std::fs::write(&exe, format!("@echo off\r\n{body}\r\n")).unwrap();
    (dir, exe)
}

#[cfg(windows)]
#[test]
fn child_process_output_is_parsed_and_exit_code_checked() {
    let l1 = r#"{"path":"D:\\a.jpg","name":"a.jpg","ext":"jpg","size":10,"mtime":1.5,"mtime_ns":1,"cloud":false}"#;
    let l2 = r#"{"path":"D:\\b.heic","name":"b.heic","ext":"heic","size":20,"mtime":2.5,"mtime_ns":2,"cloud":true}"#;
    let (dir, exe) = fake_klib("ok", &format!("echo {l1}\r\necho {l2}"));
    let mut job = ScanJob::start_klib(&exe, &[r"D:\".into()], &[], Arc::new(|| {})).unwrap();
    let (docs, visited, fin, err) = wait_finished(&job, Duration::from_secs(20));
    job.join();
    assert_eq!((err, fin, visited), (None, Some(false), 2));
    assert_eq!(docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["a.jpg", "b.heic"]);
    assert!(docs[1].cloud);
    let _ = std::fs::remove_dir_all(&dir);
    let (dir, exe) = fake_klib("fail", &format!("echo {l1}\r\nexit /b 3"));
    let mut job = ScanJob::start_klib(&exe, &[r"D:\".into()], &[], Arc::new(|| {})).unwrap();
    let (_, _, fin, err) = wait_finished(&job, Duration::from_secs(20));
    job.join();
    assert_eq!(fin, None);
    assert!(err.unwrap().contains("종료 코드"));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ScanJob::start_klib(std::path::Path::new(r"Z:\없는\klib.exe"), &[r"D:\".into()], &[], Arc::new(|| {})).is_err());
}

#[cfg(windows)]
#[test]
fn cancel_kills_the_child_process() {
    let l1 = r#"{"path":"D:\\a.jpg","name":"a.jpg","ext":"jpg","size":10,"mtime":1.5,"mtime_ns":1,"cloud":false}"#;
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

#[cfg(windows)]
#[test]
fn real_klib_scan_if_available() {
    let Some(exe) = scan::find_klib() else {
        eprintln!("klib.exe 없음: 실제 klib 사진 스캔 시험 건너뜀");
        return;
    };
    let dir = std::env::temp_dir().join(format!("photoview_win_scan_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("하위 폴더")).unwrap();
    for f in ["가.jpg", "나.PNG", "하위 폴더/다.heic", "무시.txt", "문서.hwp"] {
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
    assert_eq!(names, ["가.jpg", "나.PNG", "다.heic"]);
}

// ---------- 설정 읽기 ----------

#[test]
fn thumb_size_setting_rules_match_python() {
    assert_eq!(settings::thumb_size("{}"), 160.0);
    assert_eq!(settings::thumb_size(""), 160.0, "깨진 파일은 기본값");
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"photo_album":120}}"#), 120.0);
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"photo_album":150.5,"doc_grid":200}}"#), 150.5);
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"photo_album":50}}"#), 100.0, "최소로 맞춤");
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"photo_album":9999}}"#), 240.0, "최대로 맞춤");
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"doc_grid":120}}"#), 160.0, "문서뷰 값은 쓰지 않는다");
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":{"photo_album":"150"}}"#), 160.0);
    assert_eq!(settings::thumb_size(r#"{"ui_thumb_sizes":[1,2]}"#), 160.0);
    assert_eq!(settings::thumb_size("\u{feff}{\"ui_thumb_sizes\":{\"photo_album\":222}}"), 222.0, "BOM 허용");
    assert_eq!((settings::MIN_THUMB, settings::MAX_THUMB, settings::DEFAULT_PHOTO_THUMB), (100.0, 240.0, 160.0));
}

#[test]
fn excluded_folders_and_ui_choice() {
    let t = r#"{"library_excluded_folders":["D:\\a"," D:\\A\\ ","","C:/x","c:\\x",3,"E:\\b"],"photoview_ui":"native","docview_ui":"native"}"#;
    assert_eq!(settings::excluded_folders(t), [r"D:\a", "C:/x", r"E:\b"], "빈 값·숫자 제외, 대소문자·구분자 무시 중복 제거");
    assert!(settings::excluded_folders("{}").is_empty());
    assert!(settings::excluded_folders("깨짐").is_empty());
    assert_eq!(settings::photoview_ui(t), "native");
    assert_eq!(settings::photoview_ui(r#"{"docview_ui":"native"}"#), "flutter", "문서뷰 키는 사진뷰와 무관");
    assert_eq!(settings::photoview_ui("{}"), "flutter");
    assert_eq!(settings::photoview_ui(r#"{"photoview_ui":"weird"}"#), "flutter");
    assert_eq!(settings::photoview_ui("깨짐"), "flutter");
}

#[test]
fn settings_file_is_read_only() {
    let dir = std::env::temp_dir().join(format!("photoview_win_settings_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("settings.json");
    let body = "{\r\n  \"ui_thumb_sizes\": {\"photo_album\": 123}\r\n}\r\n";
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
