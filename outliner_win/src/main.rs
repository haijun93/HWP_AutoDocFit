#![windows_subsystem = "windows"]
//! `outliner_win [문서.md]` — 읽기 전용 블록 목록 창.
//! `outliner_win --capture-dir <폴더> [--dark|--hc] [--dpi N] [--file 문서.md]` — 창 없이 PNG 를 쓰고 끝난다.
use outliner_win::{SAMPLE_MD, capture, view::View, window};
use ui_theme::Kind;

struct Args {
    capture_dir: Option<String>,
    kind: Kind,
    dpi: u32,
    file: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { capture_dir: None, kind: Kind::Light, dpi: 96, file: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--capture-dir" => a.capture_dir = Some(it.next().ok_or("--capture-dir 에 폴더가 필요합니다")?),
            "--dark" => a.kind = Kind::Dark,
            "--hc" => a.kind = Kind::HighContrast,
            "--dpi" => a.dpi = it.next().and_then(|v| v.parse().ok()).ok_or("--dpi 에 숫자가 필요합니다")?,
            "--file" => a.file = Some(it.next().ok_or("--file 에 경로가 필요합니다")?),
            other if !other.starts_with("--") && a.file.is_none() => a.file = Some(other.to_string()),
            other => return Err(format!("알 수 없는 인자: {other}")),
        }
    }
    Ok(a)
}

fn run() -> Result<String, String> {
    let args = parse_args()?;
    let (text, title) = match &args.file {
        Some(path) => (
            std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?,
            std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        ),
        None => (SAMPLE_MD.to_string(), "샘플 문서".to_string()),
    };
    let mut view = View::from_markdown(&text)?;
    match args.capture_dir {
        Some(dir) => {
            // 캡처에서는 선택 줄(두 번째 블록 아래)을 하나 보이게 해 선택 색도 확인한다.
            view.set_selected(2.min(view.rows.len().saturating_sub(1)), 18);
            let path = capture::capture_to(std::path::Path::new(&dir), &view, args.kind, args.dpi, &title)?;
            Ok(format!("{}", path.display()))
        }
        None => window::run(view, &title).map(|_| String::new()),
    }
}

fn main() {
    // GUI 하위 시스템이라 콘솔이 없다. 캡처 결과·오류는 OUTLINER_WIN_OUT 파일에도 적는다(없으면 오류만 대화상자).
    let result = run();
    let line = match &result {
        Ok(s) => format!("ok {s}"),
        Err(e) => format!("error {e}"),
    };
    if let Some(path) = std::env::var_os("OUTLINER_WIN_OUT") {
        let _ = std::fs::write(path, &line);
    }
    if let Err(e) = result {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                outliner_win::paint::wide(&e).as_ptr(),
                outliner_win::paint::wide("아웃라이너").as_ptr(),
                0x10,
            );
        }
        std::process::exit(1);
    }
}
