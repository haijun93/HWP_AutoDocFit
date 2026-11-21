//! 캡처 모드: 창을 만들지 않고 `paint::draw` 를 메모리 DC에 그려 화소/PNG 로 돌려준다
//! (`ktm --capture-dir` 와 같은 방식).
use crate::paint::draw;
use crate::view::View;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use ui_theme::{Kind, Palette};
use windows_sys::Win32::Graphics::Gdi::*;

pub struct Shot {
    pub width: usize,
    pub height: usize,
    /// 위에서 아래로의 BGRA.
    pub bgra: Vec<u8>,
}

/// 지정한 팔레트로 `view` 를 그려 화소를 돌려준다.
pub fn render(view: &View, pal: &Palette, width: i32, height: i32, dpi: u32, title: &str) -> Result<Shot, String> {
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bm = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(mem, bm);
        draw(mem, width, height, view, pal, dpi, title);
        SelectObject(mem, old);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let got = GetDIBits(screen, bm, 0, height as u32, pixels.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        DeleteObject(bm);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        if got == 0 {
            return Err("캡처 실패(GetDIBits)".into());
        }
        Ok(Shot { width: width as usize, height: height as usize, bgra: pixels })
    }
}

pub fn palette_of(kind: Kind) -> Palette {
    Palette::of(kind, &ui_theme::sys_colors())
}

pub fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Light => "light",
        Kind::Dark => "dark",
        Kind::HighContrast => "highcontrast",
    }
}

/// `dir` 에 `outliner_<테마>_<dpi>dpi.png` 를 쓰고 경로를 돌려준다.
pub fn capture_to(dir: &Path, view: &View, kind: Kind, dpi: u32, title: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (w, h) = (crate::paint::scale(720, dpi), crate::paint::scale(520, dpi));
    let shot = render(view, &palette_of(kind), w, h, dpi, title)?;
    let path = dir.join(format!("outliner_{}_{}dpi.png", kind_name(kind), dpi));
    crate::png::write(&path, shot.width, shot.height, &shot.bgra)?;
    Ok(path)
}

/// 편집 중 상태 캡처 3장(EDIT 겹침·캐럿 / 분할 직후 / TODO 줄의 EDIT 위치)을 `dir` 에 쓴다.
/// 실제 (보이지 않는) 창과 실제 EDIT 를 만들어 EDIT 에 Enter 키를 보내 분할한 결과를 그린다.
/// 문서는 메모리에서만 바뀌고 어디에도 저장하지 않는다.
pub fn capture_edit_set(dir: &Path, markdown: &str, kind: Kind, dpi: u32, title: &str) -> Result<Vec<PathBuf>, String> {
    use crate::edit::{EditHost, Editor, VK_RETURN, utf16_len};
    use crate::window::{Harness, WinHost};
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let view = View::from_markdown(markdown)?;
    let base = outliner_core::serialize(&view.doc);
    let editor = Editor::new(view, WinHost::new(), None, base);
    let (w, h) = (crate::paint::scale(720, dpi), crate::paint::scale(520, dpi));
    let mut hn = Harness::new(editor, title, kind, dpi, w, h)?;
    let name = |tag: &str| dir.join(format!("outliner_{tag}_{}_{}dpi.png", kind_name(kind), dpi));
    let mut out = Vec::new();
    let mut shot = |hn: &mut Harness, tag: &str| -> Result<(), String> {
        hn.layout();
        let s = hn.render()?;
        let path = name(tag);
        crate::png::write(&path, s.width, s.height, &s.bgra)?;
        out.push(path);
        Ok(())
    };
    // 1) "예산 편성 [[예산 지침]]" 줄(보이는 줄 2번)을 편집, 캐럿을 "예산 편성" 뒤에.
    hn.editor().view.set_selected(2, 17);
    hn.editor().start_edit_selected();
    let text = hn.editor().host.read_text();
    let caret = text.find(" [[").map(|b| utf16_len(&text[..b])).unwrap_or(0);
    hn.editor().host.set_text(&text, caret);
    shot(&mut hn, "edit")?;
    // 2) Enter → 캐럿 위치에서 분할(실제 EDIT 서브클래스 경유).
    hn.send_key(VK_RETURN);
    shot(&mut hn, "split")?;
    // 3) TODO 줄(체크 칸이 있어 글자 시작이 밀린 줄)을 편집.
    let todo = hn.editor().view.rows.iter().position(|r| r.status.as_deref() == Some("TODO")).ok_or("TODO 줄이 없습니다")?;
    let block = hn.editor().view.rows[todo].block;
    hn.editor().submit(crate::edit::Cmd::Goto(block), 0, false, false);
    shot(&mut hn, "edit_todo")?;
    Ok(out)
}

/// 캡처·시험용 시연 데이터 폴더(작은 `.md` 페이지 6개)를 `dir` 에 쓴다. 링크로 서로 가리키므로 역링크가 보인다.
pub fn seed_demo_pages(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let pages: [(&str, &str); 6] = [
        ("예산 지침", "- 2027년 예산 편성 지침 #예산\n  - 부서별 요구서 수합 기한: 12월 5일\n  - 신규 사업은 [[회의록]] 에서 협의\n- TODO 지침 개정안 검토\n- DONE 부서 안내 메일 발송\n"),
        ("회의록", "- 2026-11-20 예산 협의 회의\n  - [[예산 지침]] 변경 사항 공유\n  - 다음 일정은 [[2026_11_21]] 일지에 기록\n- TODO 회의 결과 보고서 작성\n"),
        ("주간 업무", "- 이번 주 할 일\n  - TODO [[예산 지침]] 부서 회신 정리\n  - 민원 처리 현황 점검\n- 다음 주 계획\n"),
        ("업무 매뉴얼", "- 결재 절차\n  - 기안 → 협조 → 결재\n- 문서 보관 규정\n"),
        ("참고 자료", "- 예산 편성 관련 법령 목록\n- 지난해 편성 결과 요약\n"),
        ("2026_11_21", "- 오늘 할 일\n  - TODO [[예산 지침]] 검토\n  - DONE 회의 자료 인쇄\n  - 점심 약속 확인\n- 메모\n  - 예산 관련 문의는 [[회의록]] 참고\n"),
    ];
    for (name, body) in pages {
        std::fs::write(dir.join(format!("{name}.md")), body).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn demo_clock() -> crate::shell::Now {
    crate::shell::Now { date_iso: "2026-11-21".into(), stamp: "20261121-120000".into() }
}

/// 페이지 모드 캡처 4장: 페이지 목록·역링크 / 검색 결과 / `[[` 자동완성 팝업 / 다른 페이지의 역링크.
/// `data` 는 시연 폴더(없으면 만든다). 실제 (보이지 않는) 창·실제 EDIT 로 그린다. 문서는 저장하지 않는다.
pub fn capture_shell_set(dir: &Path, data: &Path, kind: Kind, dpi: u32) -> Result<Vec<PathBuf>, String> {
    use crate::edit::EditHost;
    use crate::pages::PageStore;
    use crate::shell::Shell;
    use crate::window::{Harness, WinHost};
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if !data.join("2026_11_21.md").exists() {
        seed_demo_pages(data)?;
    }
    let store = PageStore::new(data, Some(dir.join("recent_demo.json")));
    let mut shell = Shell::with_store(WinHost::new(), store, demo_clock)?;
    shell.start()?;
    for p in ["예산 지침", "회의록", "2026_11_21"] {
        shell.open_page(p)?; // 최근 페이지 목록을 채운다
    }
    let (w, h) = (crate::paint::scale(1000, dpi), crate::paint::scale(600, dpi));
    let mut hn = Harness::new_shell(shell, "아웃라이너", kind, dpi, w, h)?;
    let name = |tag: &str| dir.join(format!("outliner_{tag}_{}_{}dpi.png", kind_name(kind), dpi));
    let mut out = Vec::new();
    let mut shot = |hn: &mut Harness, tag: &str| -> Result<(), String> {
        hn.layout();
        let s = hn.render()?;
        let path = name(tag);
        crate::png::write(&path, s.width, s.height, &s.bgra)?;
        out.push(path);
        Ok(())
    };
    // 1) 오늘 일지 열림: 왼쪽 페이지 목록(최근·전체)·오른쪽 역링크(회의록이 이 일지를 가리킴).
    hn.shell().open_page("2026_11_21")?;
    hn.editor().view.set_selected(1, 15);
    shot(&mut hn, "shell_pages")?;
    // 2) 검색 "예산": 페이지 이름·여러 페이지의 블록 결과.
    hn.type_search("예산");
    shot(&mut hn, "shell_search")?;
    // 3) 검색 닫고 블록 편집 중 `[[` 입력 → 자동완성 팝업.
    hn.type_search("");
    let row = hn.editor().view.rows.iter().position(|r| r.text.contains("점심")).ok_or("점심 줄이 없습니다")?;
    hn.editor().view.set_selected(row, 15);
    hn.editor().start_edit_selected();
    let t = "점심 약속 확인 [[";
    hn.editor().host.set_text(t, t.encode_utf16().count());
    shot(&mut hn, "shell_popup")?;
    // 4) 다른 페이지로 이동: 예산 지침 — 역링크 3개 페이지.
    hn.editor().submit(crate::edit::Cmd::Cancel, 0, false, false);
    hn.shell().open_page("예산 지침")?;
    hn.editor().view.set_selected(0, 15);
    shot(&mut hn, "shell_backlinks")?;
    Ok(out)
}
