#![windows_subsystem = "windows"]
//! `docview_win [--settings 파일] [--klib klib.exe] [--synthetic N] [--no-scan]` — 문서뷰 창(읽기 전용 격자).
//! `docview_win --capture-dir <폴더> [--dark] [--dpi N] [--size WxH] [--scene 이름] [--settings 파일]`
//!   — 창 없이 PNG 를 쓰고 끝난다. `--capture-set` 은 모든 장면을 라이트·다크·150% 로 한 번에 쓴다.
//!   결과 줄(장면·파일·그린 칸 수)은 `DOCVIEW_WIN_OUT` 파일에도 적는다(GUI 하위 시스템이라 콘솔이 없다).
use docview_win::capture::{self, Size};
use docview_win::synth::{SCENES, scene};
use docview_win::{settings, window};
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
        None => settings::DEFAULT_DOC_THUMB,
    };
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
        let (w, h) = (docview_win::layout::scale(size.0, dpi), docview_win::layout::scale(size.1, dpi));
        let app = scene(&name, w, h, dpi, thumb)?;
        let (path, drawn) = capture::capture_to(std::path::Path::new(&dir), &name, &app, kind, dpi, size)?;
        lines.push(format!("{} (항목 {}개 중 {}칸 그림)", path.display(), app.coll.len(), drawn));
    }
    Ok(lines.join("\n"))
}

fn main() {
    let result = run();
    let line = match &result {
        Ok(s) => format!("ok {s}"),
        Err(e) => format!("error {e}"),
    };
    if let Some(path) = std::env::var_os("DOCVIEW_WIN_OUT") {
        let _ = std::fs::write(path, &line);
    }
    if let Err(e) = result {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                docview_win::sys::wide(&e).as_ptr(),
                docview_win::sys::wide("문서뷰").as_ptr(),
                0x10,
            );
        }
        std::process::exit(1);
    }
}
