//! 실제 (화면 밖) 창·실제 EDIT 를 거치는 시험: 서브클래스 키 처리, EDIT 겹침 위치, 캡처 화소.
use outliner_win::capture::{capture_edit_set, kind_name, palette_of};
use outliner_win::edit::*;
use outliner_win::paint::Metrics;
use outliner_win::view::View;
use outliner_win::window::{Harness, WinHost};
use outliner_win::{SAMPLE_MD, png};
use ui_theme::Kind;

fn harness(md: &str, kind: Kind) -> Harness {
    let view = View::from_markdown(md).unwrap();
    let base = outliner_core::serialize(&view.doc);
    Harness::new(Editor::new(view, WinHost::new(), None, base), "시험", kind, 96, 720, 520).unwrap()
}

#[test]
fn edit_rect_overlaps_the_block_row_exactly() {
    let m = Metrics::new(96);
    // 깊이 1, 첫 줄(slot 0): 글자 x = text_x(1) = 60, EDIT 왼쪽 = 글자 x - 4.
    let r = m.edit_rect(96, 1, false, 0, 720);
    assert_eq!((r.left, r.top, r.right, r.bottom), (56, 36 + 3, 720 - 12, 36 + 28 - 3));
    assert!(r.top > m.header_h && r.bottom < m.header_h + m.row_h, "줄 띠 안에 들어간다");
    // TODO 줄은 체크 칸 폭(2*7+8=22)만큼 오른쪽.
    assert_eq!(m.edit_rect(96, 1, true, 0, 720).left, r.left + 22);
    // 192 dpi 는 정확히 2배.
    let r2 = Metrics::new(192).edit_rect(192, 1, false, 0, 1440);
    assert_eq!((r2.left, r2.top), (r.left * 2, r.top * 2));
}

#[test]
fn real_edit_control_receives_keys_through_the_subclass() {
    let mut h = harness("- 가나다라\n- 둘째\n", Kind::Light);
    h.editor().view.set_selected(0, 17);
    assert!(h.editor().start_edit_selected());
    h.editor().host.set_text("가나다라", 2);
    h.layout();
    assert_eq!(h.editor().host.selection(), (2, 2));
    // 실제 EDIT 의 WM_KEYDOWN(Enter) → 서브클래스 → Editor::key → 분할.
    h.send_key(VK_RETURN);
    let texts: Vec<String> = h.editor().view.doc.blocks.iter().map(|b| b.text.clone()).collect();
    assert_eq!(texts, ["가나", "다라", "둘째"]);
    assert_eq!(h.editor().editing_block(), Some(1));
    assert_eq!(h.editor().host.read_text(), "다라", "EDIT 가 새 블록의 글로 바뀜");
    // Tab = 들여쓰기, 실제 EDIT 글은 그대로.
    h.send_key(VK_TAB);
    assert_eq!(h.editor().markdown(), "- 가나\n - 다라\n- 둘째\n");
    // 맨 앞(캐럿 0)에서 Backspace → 앞 블록과 합치기.
    h.editor().host.set_text("다라", 0);
    h.send_key(VK_BACK);
    assert_eq!(h.editor().view.doc.blocks.len(), 2);
    assert_eq!(h.editor().view.doc.blocks[0].text, "가나다라");
    // Esc 는 글을 확정하고 블록을 선택한다.
    h.send_key(VK_ESCAPE);
    assert!(!h.editor().editing());
}

fn px(shot: &outliner_win::capture::Shot, x: usize, y: usize) -> [u8; 3] {
    let i = (y * shot.width + x) * 4;
    [shot.bgra[i + 2], shot.bgra[i + 1], shot.bgra[i]]
}

#[test]
fn capture_shows_edit_box_over_the_row_with_text_in_light_and_dark() {
    let dir = std::env::temp_dir().join(format!("outliner_win_editcap_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for kind in [Kind::Light, Kind::Dark] {
        let files = capture_edit_set(&dir, SAMPLE_MD, kind, 96, "샘플 문서").unwrap();
        assert_eq!(files.len(), 3);
        let pal = palette_of(kind);
        let m = Metrics::new(96);
        let render = |name: &str| {
            let bytes = std::fs::read(dir.join(format!("outliner_{name}_{}_96dpi.png", kind_name(kind)))).unwrap();
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            assert!(bytes.len() > 720 * 520 * 3);
            bytes
        };
        let (edit, split, todo) = (render("edit"), render("split"), render("edit_todo"));
        assert_ne!(edit, split, "분할 뒤에는 그림이 달라져야 한다");
        assert_ne!(edit, todo);
        // 화소 검사는 edit 그림을 다시 그려서(같은 경로) 한다.
        let mut h = harness(SAMPLE_MD, kind);
        h.editor().view.set_selected(2, 17);
        h.editor().start_edit_selected();
        h.layout();
        let shot = h.render().unwrap();
        // 선택 줄(보이는 줄 2번)의 EDIT 상자: 안쪽 배경은 팔레트 bg 색, 마젠타(미그림 표시)는 없다.
        let er = m.edit_rect(96, 1, false, 2, 720);
        let bg = [(pal.bg & 0xFF) as u8, ((pal.bg >> 8) & 0xFF) as u8, ((pal.bg >> 16) & 0xFF) as u8];
        assert_eq!(px(&shot, (er.right - 20) as usize, ((er.top + er.bottom) / 2) as usize), bg, "{kind:?}: 상자 안쪽 배경");
        let mut ink = 0;
        for y in er.top..er.bottom {
            for x in er.left..er.right {
                let p = px(&shot, x as usize, y as usize);
                assert_ne!(p, [255, 0, 255], "{kind:?}: EDIT 가 그려지지 않은 화소(마젠타)");
                if p != bg {
                    ink += 1;
                }
            }
        }
        assert!(ink > 150, "{kind:?}: 상자 안에 글자가 그려져야 함(비배경 화소 {ink})");
        // 상자 밖(바로 아래 줄)에는 원래 목록 글자가 그대로 있다.
        let below = m.edit_rect(96, 2, true, 3, 720);
        let mut below_ink = 0;
        for y in below.top..below.bottom {
            for x in 80..400 {
                if px(&shot, x, y as usize) != bg {
                    below_ink += 1;
                }
            }
        }
        assert!(below_ink > 100, "{kind:?}: 아래 줄 글자 유지({below_ink})");
        // 테두리(accent)가 상자를 감싼다.
        let a = pal.accent;
        let acc = [(a & 0xFF) as u8, ((a >> 8) & 0xFF) as u8, ((a >> 16) & 0xFF) as u8];
        assert_eq!(px(&shot, (er.left + 40) as usize, (er.top - 1) as usize), acc, "{kind:?}: 위 테두리");
        let _ = png::encode;
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn capture_cli_edit_mode_writes_three_extra_pngs() {
    let exe = env!("CARGO_BIN_EXE_outliner_win");
    let dir = std::env::temp_dir().join(format!("outliner_win_editcli_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let st = std::process::Command::new(exe).arg("--capture-dir").arg(&dir).arg("--capture-edit").arg("--dark").status().unwrap();
    assert!(st.success());
    for tag in ["edit", "split", "edit_todo"] {
        assert!(dir.join(format!("outliner_{tag}_dark_96dpi.png")).exists(), "{tag}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn real_multiline_edit_keeps_offsets_and_swallows_backspace_char() {
    let view = View::from_markdown("- 앞\n- 첫줄\n  둘째\n").unwrap();
    let base = outliner_core::serialize(&view.doc);
    let editor = Editor::new(view, WinHost::new(), None, base);
    let mut h = Harness::new(editor, "multiline", Kind::Light, 96, 720, 400).unwrap();
    h.editor().view.set_selected(1, 10);
    assert!(h.editor().start_edit_selected());
    h.layout();
    assert_eq!(h.editor().host.read_text(), "첫줄\n둘째");
    assert_eq!(h.editor().host.selection(), (5, 5), "줄바꿈은 한 글자로 센다");
    h.editor().host.set_text("첫줄\n둘째", 3);
    assert_eq!(h.editor().host.selection(), (3, 3));
    // 둘째 줄 맨 앞의 Backspace 는 EDIT 안 처리, 블록 맨 앞은 합치기.
    h.editor().host.set_text("첫줄\n둘째", 0);
    h.send_key(VK_BACK);
    assert_eq!(h.editor().markdown(), "- 앞첫줄\n  둘째\n");
    // 뒤따르는 WM_CHAR(8)는 삼켜서 이음매 앞 글자가 지워지지 않는다.
    h.send_char(8);
    assert_eq!(h.editor().host.read_text(), "앞첫줄\n둘째");
    h.send_char(b'(' as u16);
    assert_eq!(h.editor().host.read_text(), "앞(첫줄\n둘째", "글 중간의 ( 는 짝을 만들지 않는다");
}
