#![windows_subsystem = "windows"]
//! `settings_win [--settings 파일]` — 앱 설정 창(좌측 묶음 트리 + 우측 항목 목록 + 검색 + 저장/되돌리기).
//! `settings_win --capture-dir <폴더> [--dark|--hc] [--dpi N] [--group <묶음>] [--search 글] [--demo-edits] [--settings 파일]`
//!   — 창 없이 PNG 를 쓰고 끝난다(설정 파일은 읽기만 한다). `--capture-set` 은 라이트·다크·150%·검색 상태를 한 번에,
//!   `--capture-controls` 는 색·폴더·글꼴·목록·단축키 컨트롤 장면을 한 번에 쓴다.
//!   개별 장면: `--focus-key <키>`(그 줄로 스크롤) · `--overlay fonts|list|hotkey`(패널·입력 대기 상태; fonts·list 는 --focus-key 의 항목) · `--demo-controls`(시연 항목 덧붙임).
use settings_win::paint::{Overlay, Ui};
use settings_win::{capture, layout, model::Model, schema::GROUPS, window};
use ui_theme::Kind;

#[derive(Default)]
struct Args {
    capture_dir: Option<String>,
    dark: bool,
    hc: bool,
    dpi: Option<u32>,
    group: Option<String>,
    search: Option<String>,
    demo_edits: bool,
    demo_controls: bool,
    capture_set: bool,
    capture_controls: bool,
    focus_key: Option<String>,
    overlay: Option<String>,
    settings: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--capture-dir" => a.capture_dir = Some(it.next().ok_or("--capture-dir 에 폴더가 필요합니다")?),
            "--dark" => a.dark = true,
            "--hc" => a.hc = true,
            "--dpi" => a.dpi = Some(it.next().and_then(|v| v.parse().ok()).ok_or("--dpi 에 숫자가 필요합니다")?),
            "--group" => a.group = Some(it.next().ok_or("--group 에 묶음 이름이 필요합니다")?),
            "--search" => a.search = Some(it.next().ok_or("--search 에 글이 필요합니다")?),
            "--focus-key" => a.focus_key = Some(it.next().ok_or("--focus-key 에 키가 필요합니다")?),
            "--overlay" => a.overlay = Some(it.next().ok_or("--overlay 에 fonts|list|hotkey 가 필요합니다")?),
            "--demo-edits" => a.demo_edits = true,
            "--demo-controls" => a.demo_controls = true,
            "--capture-set" => a.capture_set = true,
            "--capture-controls" => a.capture_controls = true,
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

/// 한 장면: 태그·테마·dpi·묶음·검색·강조 줄·겹침 패널·시연 항목.
struct Job {
    tag: String,
    kind: Kind,
    dpi: u32,
    group: Option<String>,
    search: Option<String>,
    focus_key: Option<String>,
    overlay: Option<String>,
    demo_controls: bool,
}

fn job(tag: &str, kind: Kind, dpi: u32, group: Option<&str>) -> Job {
    Job { tag: tag.into(), kind, dpi, group: group.map(String::from), search: None, focus_key: None, overlay: None, demo_controls: false }
}

fn scene(tag: &str, kind: Kind, dpi: u32, group: &str, focus: &str, overlay: Option<&str>, demo: bool) -> Job {
    let mut j = job(tag, kind, dpi, Some(group));
    j.focus_key = Some(focus.into());
    j.overlay = overlay.map(String::from);
    j.demo_controls = demo;
    j
}

fn jobs_of(a: &Args) -> Vec<Job> {
    let mut jobs = Vec::new();
    let three = [(Kind::Light, 96), (Kind::Dark, 96), (Kind::Light, 144)];
    if a.capture_controls {
        for (k, d) in three {
            jobs.push(scene("color", k, d, "sentence", "color", None, false));
        }
        for (k, d) in three {
            jobs.push(scene("folder", k, d, "program", "hwp_font_folder", None, true));
        }
        // 붙여넣은 글 저장 폴더(결과·기록 묶음): 없는 폴더라 알림이 붙는다.
        for (k, d) in [(Kind::Light, 96), (Kind::Dark, 96)] {
            jobs.push(scene("folderwarn", k, d, "result", "paste_add_folder", None, false));
        }
        for (k, d) in [(Kind::Light, 96), (Kind::Dark, 96)] {
            jobs.push(scene("listhotkey", k, d, "program", "demo.excluded_apps", None, true));
        }
        jobs.push(scene("fontrows", Kind::Light, 96, "font", "symbol_fonts.□.font", None, false));
        for (k, d) in three {
            jobs.push(scene("fontpick", k, d, "font", "symbol_fonts.ㅇ.font", Some("fonts"), false));
        }
        for (k, d) in three {
            jobs.push(scene("listedit", k, d, "program", "demo.excluded_apps", Some("list"), true));
        }
        jobs.push(scene("hotkeywait", Kind::Light, 96, "program", "demo.hotkey", Some("hotkey"), true));
    } else if a.capture_set {
        let g = a.group.as_deref();
        jobs.push(job("group", Kind::Light, 96, g));
        jobs.push(job("group", Kind::Dark, 96, g));
        jobs.push(job("group", Kind::Light, 144, g));
        for (kind, dpi) in [(Kind::Light, 96), (Kind::Dark, 144)] {
            let mut j = job("search", kind, dpi, None);
            j.search = Some(a.search.clone().unwrap_or_else(|| "간격".into()));
            jobs.push(j);
        }
        jobs.push(job("owners", Kind::Light, 96, Some("program")));
        jobs.push(job("internal", Kind::Light, 96, Some("internal")));
        jobs.push(job("font", Kind::Light, 96, Some("font")));
        jobs.push(job("group", Kind::HighContrast, 96, g));
    } else {
        let kind = if a.hc { Kind::HighContrast } else if a.dark { Kind::Dark } else { Kind::Light };
        let mut j = job(if a.search.is_some() { "search" } else { "group" }, kind, a.dpi.unwrap_or(96), a.group.as_deref());
        j.search = a.search.clone();
        j.focus_key = a.focus_key.clone();
        j.overlay = a.overlay.clone();
        j.demo_controls = a.demo_controls;
        jobs.push(j);
    }
    jobs
}

fn run() -> Result<String, String> {
    let a = parse_args()?;
    let Some(dir) = a.capture_dir.clone() else {
        let path = match &a.settings {
            Some(p) => std::path::PathBuf::from(p),
            None => docfit_config::settings_path(),
        };
        if window::activate_existing_instance(&path.display().to_string()) {
            return Ok("이미 열려 있는 설정 창을 앞으로 가져왔습니다.".into());
        }
        window::run(load(&a)?)?;
        return Ok(String::new());
    };
    let dir = std::path::Path::new(&dir);
    let mut written = Vec::new();
    for j in jobs_of(&a) {
        let mut model = load(&a)?;
        if j.demo_controls {
            model.add_demo_items();
        }
        if a.demo_edits || a.capture_set {
            capture::apply_demo_edits(&mut model);
        }
        if a.capture_controls || j.demo_controls || j.overlay.is_some() {
            capture::apply_control_demo_edits(&mut model);
        }
        model.group = match &j.group {
            Some(g) => group_by_name(g)?,
            None => 2, // 문장·줄 정리: 숫자·체크가 많은 묶음
        };
        if let Some(q) = &j.search {
            model.set_search(q);
        }
        let mut ui = Ui::default();
        ui.focus_row = 1;
        let (w, h) = (layout::scale(layout::DEFAULT_W, j.dpi), layout::scale(layout::DEFAULT_H, j.dpi));
        if let Some(key) = &j.focus_key {
            let visible = model.visible();
            let n = visible.iter().position(|i| model.items[*i].key == *key).ok_or_else(|| format!("이 묶음에 {key} 항목이 없습니다"))?;
            ui.focus_row = n;
            ui.scroll = layout::compute(w, h, j.dpi).scroll_to_show(n, 0, visible.len());
        }
        if let Some(kind) = &j.overlay {
            let key = match kind.as_str() {
                "fonts" => j.focus_key.clone().ok_or("--overlay fonts 에는 --focus-key 가 필요합니다")?,
                "list" => j.focus_key.clone().unwrap_or_else(|| "demo.excluded_apps".to_string()),
                "hotkey" => "demo.hotkey".to_string(),
                other => return Err(format!("알 수 없는 --overlay: {other}")),
            };
            ui.overlay = Some(match kind.as_str() {
                "fonts" => {
                    let names = model.font_choices("");
                    let cur = model.current_text(&key).to_lowercase();
                    let sel = names.iter().position(|n| n.to_lowercase() == cur).unwrap_or(0);
                    let scroll = layout::compute(w, h, j.dpi).overlay().scroll_to_show(sel, 0, names.len());
                    Overlay::Fonts { key, filter: String::new(), sel, scroll }
                }
                "list" => Overlay::ListEdit { key, sel: Some(1), input: String::new(), scroll: 0, note: None },
                _ => Overlay::HotkeyWait { key, note: None },
            });
        }
        let path = capture::capture_to(dir, &j.tag, &model, &ui, j.kind, j.dpi)?;
        written.push(path.display().to_string());
    }
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
