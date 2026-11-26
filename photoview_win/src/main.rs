#![windows_subsystem = "windows"]
//! `photoview_win [--settings 파일] [--klib klib.exe] [--synthetic N] [--no-scan] [--photo-db 파일]` — 사진뷰 창.
//!   `--photo-db`: 즐겨찾기·태그 DB(기본은 앱 화면과 같은 파일). 원본 사진 편집은 하지 않고 사본만 만든다.
//! `photoview_win --capture-dir <폴더> --scan-root <폴더> --klib <klib.exe> --settings <파일> [--panel] [--panel-add <폴더>] [--dark] [--dpi N]`
//!   — 합성 모델 대신 **실제 `klib scan`** 결과(제외 폴더는 설정에서 읽어 `--exclude` 로 전달)와 제외 폴더 편집 패널을 그린다(PHOTOWIN2).
//! `photoview_win --capture-dir <폴더> [--dark] [--dpi N] [--size WxH] [--scene 이름] [--settings 파일]`
//!   — 창 없이 PNG 를 쓰고 끝난다. `--capture-set` 은 모든 장면을 라이트·다크·150% 로 한 번에 쓴다.
//!   결과 줄(장면·파일·그린 칸 수)은 `PHOTOVIEW_WIN_OUT` 파일에도 적는다(GUI 하위 시스템이라 콘솔이 없다).
use photoview_win::capture::{self, Size};
use photoview_win::synth::{SCENES, scene};
use photoview_win::{settings, window};
use ui_theme::Kind;

#[derive(Default)]
struct Args {
    capture_dir: Option<String>,
    dark: bool,
    dpi: Option<u32>,
    size: Option<String>,
    scene: Option<String>,
    capture_set: bool,
    settings: Option<String>,
    klib: Option<String>,
    synthetic: Option<usize>,
    no_scan: bool,
    scan_root: Option<String>,
    panel: bool,
    panel_add: Option<String>,
    photo_db: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = |what: &str| it.next().ok_or(format!("{arg} 에 {what} 이(가) 필요합니다"));
        match arg.as_str() {
            "--capture-dir" => a.capture_dir = Some(val("폴더")?),
            "--dark" => a.dark = true,
            "--dpi" => a.dpi = Some(val("숫자")?.parse().map_err(|_| "--dpi 에 숫자가 필요합니다".to_string())?),
            "--size" => a.size = Some(val("WxH")?),
            "--scene" => a.scene = Some(val("장면 이름")?),
            "--capture-set" => a.capture_set = true,
            "--settings" => a.settings = Some(val("파일")?),
            "--klib" => a.klib = Some(val("파일")?),
            "--synthetic" => a.synthetic = Some(val("개수")?.parse().map_err(|_| "--synthetic 에 숫자가 필요합니다".to_string())?),
            "--no-scan" => a.no_scan = true,
            "--photo-db" => a.photo_db = Some(val("파일")?),
            "--scan-root" => a.scan_root = Some(val("폴더")?),
            "--panel" => a.panel = true,
            "--panel-add" => {
                a.panel = true;
                a.panel_add = Some(val("폴더")?);
            }
            other => return Err(format!("알 수 없는 인자: {other}")),
        }
    }
    Ok(a)
}

fn run() -> Result<String, String> {
    let a = parse_args()?;
    let Some(dir) = a.capture_dir.clone() else {
        window::run(window::Options {
            settings: a.settings.map(Into::into),
            klib: a.klib.map(Into::into),
            synthetic: a.synthetic,
            no_scan: a.no_scan,
            photo_db: a.photo_db.map(Into::into),
        })?;
        return Ok(String::new());
    };
    let size = match &a.size {
        Some(s) => capture::parse_size(s)?,
        None => Size::default(),
    };
    // 설정 파일은 --settings 를 줄 때만 읽는다(기본 캡처는 이 PC 설정과 무관하게 같은 결과).
    let thumb = match &a.settings {
        Some(p) => settings::thumb_size(&settings::read_text(std::path::Path::new(p))),
        None => settings::DEFAULT_PHOTO_THUMB,
    };
    if a.scan_root.is_some() || a.panel {
        return capture_real(&a, &dir, size, thumb);
    }
    let mut jobs: Vec<(String, Kind, u32)> = Vec::new();
    if a.capture_set {
        for s in SCENES {
            jobs.push((s.into(), Kind::Light, 96));
        }
        for s in ["list", "scanning", "virtual"] {
            jobs.push((s.into(), Kind::Dark, 96));
        }
        for s in ["list", "scanning", "virtual"] {
            jobs.push((s.into(), Kind::Light, 144));
        }
    } else {
        let kind = if a.dark { Kind::Dark } else { Kind::Light };
        jobs.push((a.scene.clone().unwrap_or_else(|| "list".into()), kind, a.dpi.unwrap_or(96)));
    }
    let mut lines = Vec::new();
    for (name, kind, dpi) in jobs {
        let (w, h) = (photoview_win::layout::scale(size.0, dpi), photoview_win::layout::scale(size.1, dpi));
        let app = scene(&name, w, h, dpi, thumb)?;
        let (path, drawn) = capture::capture_to(std::path::Path::new(&dir), &name, &app, kind, dpi, size)?;
        lines.push(format!("{} (항목 {}개 중 {}칸 그림)", path.display(), app.coll.len(), drawn));
    }
    Ok(lines.join("\n"))
}

/// 실제 `klib scan` 결과(와 선택적으로 제외 폴더 패널)를 그려 PNG 로 쓴다.
fn capture_real(a: &Args, dir: &str, size: Size, _thumb: f64) -> Result<String, String> {
    use photoview_win::model::{ScanState, ScanView};
    let settings_path = a.settings.as_ref().map(std::path::PathBuf::from).ok_or("--scan-root/--panel 에는 --settings 가 필요합니다")?;
    let loaded = settings::load(&settings_path);
    let kind = if a.dark { Kind::Dark } else { Kind::Light };
    let dpi = a.dpi.unwrap_or(96);
    let (w, h) = (photoview_win::layout::scale(size.0, dpi), photoview_win::layout::scale(size.1, dpi));
    let mut app = photoview_win::synth::new_app(loaded.thumb);
    app.excluded_count = loaded.excluded.len();
    app.tz_offset = photoview_win::sys::tz_offset_secs();
    let mut tag = "panel".to_string();
    if let Some(root) = &a.scan_root {
        let exe = a.klib.as_ref().map(std::path::PathBuf::from).or_else(photoview_win::scan::find_klib).ok_or("klib 실행 파일을 찾지 못했습니다(--klib 또는 KLIB_EXE)")?;
        let started = std::time::Instant::now();
        let got = photoview_win::scan::scan_blocking(&exe, &[std::path::PathBuf::from(root)], &loaded.excluded, std::time::Duration::from_secs(60))?;
        app.engine = "klib scan".into();
        app.coll.replace(got.docs);
        app.scan = ScanView { state: ScanState::Complete, visited: got.visited, folder: String::new(), elapsed_secs: started.elapsed().as_secs() };
        let m = app.metrics(w, h, dpi);
        app.select(&m, Some(0));
        tag = "realscan".into();
    }
    let mut panel = if a.panel { Some(photoview_win::panel::ExcludePanel::open(&settings_path)) } else { None };
    // `--panel-add <폴더>`: 패널에서 [폴더 추가] 로 고른 것과 같은 저장 경로(`ExcludePanel::add`)를 실제로 실행한 뒤 그린다(설정 파일에 쓴다 — 임시 파일로만 쓸 것).
    if let (Some(p), Some(folder)) = (panel.as_mut(), a.panel_add.as_deref()) {
        p.add(folder);
    }
    if panel.is_some() && a.scan_root.is_some() {
        tag = "realscan_panel".into();
    }
    let mut thumbs = std::collections::HashMap::new();
    let metrics = app.metrics(w, h, dpi);
    for i in metrics.visible_range(app.scroll, app.coll.len()) {
        if let Some(doc) = app.coll.get(i) {
            if let Ok(bitmap) = grid_core::shell_thumb::thumbnail(std::path::Path::new(&doc.path), 256) {
                thumbs.insert(doc.path.clone(), bitmap);
            }
        }
    }
    let (path, drawn) = capture::capture_with_shell_thumbs(std::path::Path::new(dir), &tag, &app, panel.as_ref(), kind, dpi, size, &thumbs)?;
    Ok(format!("{} (실제 스캔 항목 {}개, 그린 칸 {}, 제외 폴더 {}개)", path.display(), app.coll.len(), drawn, loaded.excluded.len()))
}

fn main() {
    let result = run();
    let line = match &result {
        Ok(s) => format!("ok {s}"),
        Err(e) => format!("error {e}"),
    };
    if let Some(path) = std::env::var_os("PHOTOVIEW_WIN_OUT") {
        let _ = std::fs::write(path, &line);
    }
    if let Err(e) = result {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                photoview_win::sys::wide(&e).as_ptr(),
                photoview_win::sys::wide("사진뷰").as_ptr(),
                0x10,
            );
        }
        std::process::exit(1);
    }
}
