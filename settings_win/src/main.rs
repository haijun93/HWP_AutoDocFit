#![windows_subsystem = "windows"]
//! `settings_win [--settings 파일]` — 앱 설정 창(좌측 묶음 트리 + 우측 항목 목록 + 검색 + 저장/되돌리기).
//! `settings_win --capture-dir <폴더> [--dark|--hc] [--dpi N] [--group <묶음>] [--search 글] [--demo-edits] [--settings 파일]`
//!   — 창 없이 PNG 를 쓰고 끝난다(설정 파일은 읽기만 한다). `--capture-set` 은 라이트·다크·150%·검색 상태를 한 번에 쓴다.
use settings_win::{capture, layout, model::Model, paint::Ui, schema::GROUPS, window};
use ui_theme::Kind;

struct Args {
    capture_dir: Option<String>,
    kind: Kind,
    dpi: u32,
    group: Option<String>,
    search: Option<String>,
    demo_edits: bool,
    capture_set: bool,
    settings: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { capture_dir: None, kind: Kind::Light, dpi: 96, group: None, search: None, demo_edits: false, capture_set: false, settings: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--capture-dir" => a.capture_dir = Some(it.next().ok_or("--capture-dir 에 폴더가 필요합니다")?),
            "--dark" => a.kind = Kind::Dark,
            "--hc" => a.kind = Kind::HighContrast,
            "--dpi" => a.dpi = it.next().and_then(|v| v.parse().ok()).ok_or("--dpi 에 숫자가 필요합니다")?,
            "--group" => a.group = Some(it.next().ok_or("--group 에 묶음 이름이 필요합니다")?),
            "--search" => a.search = Some(it.next().ok_or("--search 에 글이 필요합니다")?),
            "--demo-edits" => a.demo_edits = true,
            "--capture-set" => a.capture_set = true,
            "--settings" => a.settings = Some(it.next().ok_or("--settings 에 파일이 필요합니다")?),
            other => return Err(format!("알 수 없는 인자: {other}")),
        }
    }
    Ok(a)
}

fn group_by_name(name: &str) -> Result<usize, String> {
    GROUPS
        .iter()
        .position(|g| g.id == name || g.title == name)
        .ok_or_else(|| format!("알 수 없는 묶음: {name} (id: {})", GROUPS.iter().map(|g| g.id).collect::<Vec<_>>().join(", ")))
}

fn load(a: &Args) -> Result<Model, String> {
    let path = match &a.settings {
        Some(p) => std::path::PathBuf::from(p),
        None => docfit_config::settings_path(),
    };
    Model::load(&path)
}

fn run() -> Result<String, String> {
    let a = parse_args()?;
    let Some(dir) = a.capture_dir.clone() else {
        window::run(load(&a)?)?;
        return Ok(String::new());
    };
    let dir = std::path::Path::new(&dir);
    let mut written = Vec::new();
    // (태그, 테마, dpi, 묶음, 검색)
    let mut jobs: Vec<(String, Kind, u32, Option<String>, Option<String>)> = Vec::new();
    if a.capture_set {
        let g = a.group.clone();
        jobs.push(("group".into(), Kind::Light, 96, g.clone(), None));
        jobs.push(("group".into(), Kind::Dark, 96, g.clone(), None));
        jobs.push(("group".into(), Kind::Light, 144, g.clone(), None));
        jobs.push(("search".into(), Kind::Light, 96, None, Some(a.search.clone().unwrap_or_else(|| "간격".into()))));
        jobs.push(("search".into(), Kind::Dark, 144, None, Some(a.search.clone().unwrap_or_else(|| "간격".into()))));
        jobs.push(("owners".into(), Kind::Light, 96, Some("program".into()), None));
        jobs.push(("internal".into(), Kind::Light, 96, Some("internal".into()), None));
        jobs.push(("font".into(), Kind::Light, 96, Some("font".into()), None));
        jobs.push(("group".into(), Kind::HighContrast, 96, g, None));
    } else {
        jobs.push((if a.search.is_some() { "search" } else { "group" }.into(), a.kind, a.dpi, a.group.clone(), a.search.clone()));
    }
    for (tag, kind, dpi, group, search) in jobs {
        let mut model = load(&a)?;
        if a.demo_edits || a.capture_set {
            capture::apply_demo_edits(&mut model);
        }
        model.group = match group {
            Some(g) => group_by_name(&g)?,
            None => 2, // 문장·줄 정리: 숫자·체크가 많은 묶음
        };
        if let Some(q) = search {
            model.set_search(&q);
        }
        let mut ui = Ui::default();
        ui.focus_row = 1;
        let path = capture::capture_to(dir, &tag, &model, &ui, kind, dpi)?;
        written.push(path.display().to_string());
    }
    let _ = layout::DEFAULT_W;
    Ok(written.join("\n"))
}

fn main() {
    // GUI 하위 시스템이라 콘솔이 없다. 결과·오류는 SETTINGS_WIN_OUT 파일에도 적는다.
    let result = run();
    let line = match &result {
        Ok(s) => format!("ok {s}"),
        Err(e) => format!("error {e}"),
    };
    if let Some(path) = std::env::var_os("SETTINGS_WIN_OUT") {
        let _ = std::fs::write(path, &line);
    }
    if let Err(e) = result {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                settings_win::paint::wide(&e).as_ptr(),
                settings_win::paint::wide("설정").as_ptr(),
                0x10,
            );
        }
        std::process::exit(1);
    }
}
