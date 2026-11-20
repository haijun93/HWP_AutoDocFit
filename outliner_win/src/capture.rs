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
