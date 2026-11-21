//! 화면 저장 시험 모드: `kvf.exe --capture-dir <폴더> [--dpi 96|120|144|192] [--scenario <이름>|all] [--dark]`.
//! 단축키·트레이·단일 인스턴스 뮤텍스·창을 만들지 않고, 메모리 안의 가짜 자료로 실제와 같은 그리기 함수(vf_paint)를
//! 메모리 비트맵에 그려 PNG로 저장한 뒤 끝난다. 사용자 폴더·자료함·설정은 읽지도 쓰지도 않는다.
use crate::{
    vf_data::{BoxRow, Item},
    vf_layout::Layout,
    vf_paint::{self, Scene},
    vf_store::VirtualItem,
    vf_ui::{self, Chrome},
};
use windows_sys::Win32::Graphics::Gdi::*;

pub struct Case {
    pub name: &'static str,
    pub w: i32,
    pub h: i32,
    pub layout: Layout,
    pub boxes: Vec<BoxRow>,
    pub items: Vec<Item>,
    pub filtered: Vec<i64>,
    pub virt: Vec<Vec<VirtualItem>>,
    pub top: usize,
    pub query: String,
    pub message: String,
    pub selected: Option<i64>,
    pub truncated: bool,
    pub last_operation: String,
    pub drag_target: Option<String>,
    pub ctx_space: Option<String>,
    pub sticky: bool,
}

pub const SCENARIOS: [&str; 20] = [
    "one_list", "two_list", "three_list", "four_list", "one_icons", "four_icons", "mixed_views", "virtual_rows", "drag_target", "loading",
    "failed", "empty_box", "no_match", "search", "truncated_status", "space_tab", "min_size_four", "min_size_message", "wide_grid", "long_names",
];

fn boxes() -> Vec<BoxRow> {
    ["오늘의 업무", "2026년 마포구 주요업무 추진계획 검토 자료 모음", "빈 상자"]
        .iter()
        .enumerate()
        .map(|(i, n)| BoxRow { id: i as i64 + 1, name: (*n).into(), description: String::new(), created_at: 1.0, updated_at: 1.0 })
        .collect()
}
fn items(long: bool) -> Vec<Item> {
    let names = [
        "한글 보고서 초안.hwp",
        "예산 집행 현황.xlsx",
        "https://www.mapo.go.kr/site/main/index",
        "회의 자료",
        "2026년도 지방보조금 교부 결정 및 정산 지침 개정안 검토 의견서(최종).hwpx",
        "사진 001.jpg",
        "붙임 1. 추진 일정.pdf",
        "민원 처리 결과 회신.docx",
        "참고 문헌 목록.txt",
        "계약 서류 모음",
        "발표 자료.pptx",
        "메모.txt",
    ];
    let spaces = ["core", "basis", "reference", "temp"];
    names
        .iter()
        .enumerate()
        .map(|(i, n)| Item {
            id: i as i64 + 1,
            box_id: 1,
            kind: if n.starts_with("http") { "link" } else if n.contains('.') { "file" } else { "folder" }.into(),
            source_path: String::new(),
            origin_path: String::new(),
            managed_path: String::new(),
            display_name: if long && i % 3 == 0 { format!("{n} {n}") } else { (*n).into() },
            space: spaces[i % 4].into(),
            created_at: i as f64,
        })
        .collect()
}
fn virt(panels: usize) -> Vec<Vec<VirtualItem>> {
    (0..panels)
        .map(|p| {
            (0..3)
                .map(|k| VirtualItem {
                    id: format!("v{p}{k}"),
                    location: format!("p{}", p + 1),
                    source_ref: format!("C:\\가짜\\폴더{p}\\참조 파일 {k}.hwp"),
                    display_name: format!("참조 파일 {k} (영역 {})", p + 1),
                    created_at: k as i64,
                })
                .collect()
        })
        .collect()
}

pub fn case(name: &str) -> Option<Case> {
    let all = items(false);
    let ids: Vec<i64> = all.iter().map(|i| i.id).collect();
    let mut c = Case {
        name: SCENARIOS.iter().find(|n| **n == name)?,
        w: 600,
        h: 420,
        layout: Layout::for_count(1),
        boxes: boxes(),
        items: all,
        filtered: ids,
        virt: virt(0),
        top: 0,
        query: String::new(),
        message: String::new(),
        selected: Some(3),
        truncated: false,
        last_operation: String::new(),
        drag_target: None,
        ctx_space: None,
        sticky: false,
    };
    let icons = |l: &mut Layout| l.panels.iter_mut().for_each(|p| p.view = "icons".into());
    match name {
        "one_list" => {}
        "two_list" => c.layout = Layout::for_count(2),
        "three_list" => {
            c.layout = Layout::for_count(3);
            c.w = 800;
            c.h = 560;
        }
        "four_list" => {
            c.layout = Layout::for_count(4);
            c.w = 900;
            c.h = 640;
        }
        "one_icons" => icons(&mut c.layout),
        "four_icons" => {
            c.layout = Layout::for_count(4);
            icons(&mut c.layout);
            c.w = 900;
            c.h = 680;
        }
        "mixed_views" => {
            c.layout = Layout::for_count(4);
            c.layout.panels[1].view = "icons".into();
            c.layout.panels[3].view = "icons".into();
            c.w = 900;
            c.h = 640;
        }
        "virtual_rows" => {
            c.layout = Layout::for_count(2);
            c.virt = virt(2);
            c.layout.panels[0].selection = vec!["v01".into()];
            c.selected = None;
            c.last_operation = "복제됨(참조 파일 1 (영역 1) 2)".into();
            c.w = 760;
        }
        "drag_target" => {
            c.layout = Layout::for_count(2);
            c.virt = virt(2);
            c.layout.panels[0].selection = vec!["v00".into(), "v01".into()];
            c.drag_target = Some("p2".into());
            c.selected = None;
            c.w = 760;
        }
        "loading" => c.message = "불러오는 중…".into(),
        "failed" => c.message = "자료함이 사용 중이거나 잠겨 있습니다. 잠시 후 F5를 누르세요 · F5로 다시".into(),
        "empty_box" => {
            c.message = "항목이 없습니다. 다른 상자를 고르거나 앱에서 추가한 뒤 F5로 다시 불러오세요".into();
            c.filtered.clear();
            c.selected = None;
            c.layout = Layout::for_count(2);
            c.virt = virt(2);
        }
        "no_match" => {
            c.message = "표시할 항목이 없습니다. 검색어를 지우거나 F5로 새로고침하세요".into();
            c.filtered.clear();
            c.selected = None;
            c.query = "없는 낱말".into();
        }
        "search" => {
            c.query = "보고서".into();
            c.filtered = vec![1, 5, 8];
            c.selected = Some(5);
            c.layout = Layout::for_count(2);
        }
        "truncated_status" => {
            c.truncated = true;
            c.last_operation = "실제 파일 복사 완료".into();
            c.layout = Layout::for_count(3);
            c.w = 700;
            c.h = 520;
        }
        "space_tab" => {
            c.ctx_space = Some("core".into());
            c.filtered = vec![1, 5, 9];
            c.sticky = true;
        }
        "min_size_four" => {
            c.layout = Layout::for_count(4);
            c.w = vf_ui::MIN_W;
            c.h = vf_ui::MIN_H;
            c.virt = virt(4);
        }
        "min_size_message" => {
            c.layout = Layout::for_count(4);
            c.w = vf_ui::MIN_W;
            c.h = vf_ui::MIN_H + 40;
            c.message = "불러오는 중…".into();
            c.query = "한글".into();
            c.truncated = true;
            c.last_operation = "5개 외부 항목 추가".into();
        }
        "wide_grid" => {
            c.layout = Layout::for_count(4);
            c.w = 1500;
            c.h = 800;
            c.virt = virt(4);
        }
        "long_names" => {
            c.items = items(true);
            c.layout = Layout::for_count(2);
            c.w = 640;
        }
        _ => return None,
    }
    Some(c)
}

/// 한 장면을 지정한 배율로 그려 (너비, 높이, 위에서 아래 RGB 바이트)를 돌려준다.
pub fn render(c: &Case, dpi: i32) -> Result<(i32, i32, Vec<u8>), String> {
    let (w, h) = (vf_ui::sc(dpi, c.w), vf_ui::sc(dpi, c.h));
    unsafe {
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = w;
        info.bmiHeader.biHeight = -h;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mem = CreateCompatibleDC(std::ptr::null_mut());
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if bmp.is_null() || bits.is_null() {
            DeleteDC(mem);
            return Err("비트맵 생성 실패".into());
        }
        let old = SelectObject(mem, bmp);
        let sc = Scene {
            dpi,
            title: vf_paint::window_title(c.boxes.first().map(|b| b.name.as_str()).unwrap_or("오늘의 업무")),
            sticky: c.sticky,
            layout: &c.layout,
            boxes: &c.boxes,
            items: &c.items,
            filtered: &c.filtered,
            virt: &c.virt,
            top: c.top,
            query: c.query.clone(),
            message: c.message.clone(),
            selected_id: c.selected,
            truncated: c.truncated,
            last_operation: &c.last_operation,
            drag_target: c.drag_target.as_deref(),
            context_box: Some(1),
            context_space: c.ctx_space.as_deref(),
        };
        let ch = Chrome::compute(dpi, w, h, !c.query.is_empty(), c.truncated, !c.message.is_empty(), &c.layout);
        vf_paint::draw(mem, &sc, &ch);
        let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for px in src.chunks_exact(4) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        Ok((w, h, rgb))
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    !data.iter().fold(!0u32, |c, b| table[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8))
}
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for d in data {
        a = (a + *d as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
/// 압축 없는(저장 블록) PNG. 외부 크레이트 없이 쓴다.
pub fn png(w: i32, h: i32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgb.len() + h as usize);
    for row in rgb.chunks_exact(w as usize * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

/// 명령줄 처리. 성공하면 저장한 파일 수를 표준 출력에 한 줄로 적는다.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut dir = None;
    let mut dpi = 96;
    let mut scenario = "all".to_string();
    let mut dark = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--capture-dir" => dir = Some(it.next().ok_or("--capture-dir 값 없음")?.clone()),
            "--dpi" => dpi = it.next().ok_or("--dpi 값 없음")?.parse::<i32>().map_err(|_| "잘못된 --dpi")?,
            "--scenario" => scenario = it.next().ok_or("--scenario 값 없음")?.clone(),
            "--dark" => dark = true,
            other => return Err(format!("알 수 없는 인자: {other}")),
        }
    }
    if ![96, 120, 144, 192].contains(&dpi) {
        return Err("--dpi는 96·120·144·192 중 하나".into());
    }
    let dir = std::path::PathBuf::from(dir.ok_or("--capture-dir 필요")?);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if dark {
        ui_theme::refresh(ui_theme::ThemePref::Dark);
    } else {
        ui_theme::refresh(ui_theme::ThemePref::Light);
    }
    let names: Vec<&str> = if scenario == "all" { SCENARIOS.to_vec() } else { vec![scenario.as_str()] };
    let mut saved = 0;
    for name in names {
        let c = case(name).ok_or_else(|| format!("알 수 없는 장면: {name}"))?;
        let (w, h, rgb) = render(&c, dpi)?;
        let path = dir.join(format!("{name}_{dpi}{}.png", if dark { "_dark" } else { "" }));
        std::fs::write(&path, png(w, h, &rgb)).map_err(|e| e.to_string())?;
        saved += 1;
    }
    println!("{saved}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_scenario_exists_and_renders_at_each_scale() {
        for dpi in [96, 120, 144, 192] {
            for name in SCENARIOS {
                let c = case(name).expect(name);
                let (w, h, rgb) = render(&c, dpi).expect(name);
                assert_eq!(rgb.len(), (w * h * 3) as usize);
                // 바탕만 칠해진 빈 그림이 아니어야 한다.
                assert!(rgb.windows(3).any(|p| p != &rgb[..3]), "{name} 빈 그림");
            }
        }
    }
    #[test]
    fn scenario_sizes_respect_minimum_window() {
        for name in SCENARIOS {
            let c = case(name).unwrap();
            assert!(c.w >= vf_ui::MIN_W && c.h >= vf_ui::MIN_H, "{name}");
        }
        assert!(case("없는 장면").is_none());
    }
    #[test]
    fn png_has_valid_signature_and_chunks() {
        let data = png(2, 2, &[255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]);
        assert_eq!(&data[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert!(data.windows(4).any(|w| w == b"IDAT") && data.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
        assert_eq!(crc32(b"IEND"), 0xAE426082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E60398);
    }
    #[test]
    fn args_are_validated() {
        assert!(run(&["--dpi".into(), "100".into(), "--capture-dir".into(), "x".into()]).is_err());
        assert!(run(&["--bogus".into()]).is_err());
        assert!(run(&["--dpi".into(), "96".into()]).is_err());
    }
}
