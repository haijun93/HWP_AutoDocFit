mod common;
use common::*;
use outliner_core::DropPlace;
use outliner_win::{bullet::{crumbs, Pointer}, edit::Editor, paint::{Metrics, scale}, view::View, window::{Harness, WinHost}};
use std::sync::Mutex;
use ui_theme::Kind;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static LOCK: Mutex<()> = Mutex::new(());
const MD: &str = "- 부모\r\n  - TODO 자식\r\n    id:: child\r\n    - 손자\r\n- 다른 부모\r\n  - 다른 자식\r\n- 마지막";

fn editor() -> Editor<Fake> { Editor::new(View::from_markdown(MD).unwrap(), Fake::default(), None, MD.into()) }
fn harness(dpi: u32, kind: Kind) -> Harness {
    let ed = Editor::new(View::from_markdown(MD).unwrap(), WinHost::new(), None, MD.into());
    Harness::new(ed, "합성 아웃라이너", kind, dpi, scale(780, dpi), scale(520, dpi)).unwrap()
}

#[test]
fn zoom_keeps_only_the_subtree_rebases_depth_and_back_restores_selection() {
    let mut ed = editor();
    ed.view.set_selected(3, 10);
    let before = ed.markdown();
    let id = ed.view.doc.blocks[1].id;
    assert!(ed.focus(Some(id)));
    assert_eq!(ed.view.rows.iter().map(|r| r.block).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(ed.view.rows.iter().map(|r| r.depth).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(ed.view.ancestors(), [0, 1]);
    assert_eq!(ed.markdown(), before);
    assert!(!ed.dirty() && !ed.can_undo());
    assert!(!ed.focus(Some(999)));
    assert!(ed.view.zoom_back(10));
    assert_eq!(ed.view.selected, 3);
    assert_eq!(ed.view.rows.len(), 6);
}

#[test]
fn folding_in_zoom_preserves_the_root_and_original_markdown() {
    let mut ed = editor();
    let id = ed.view.doc.blocks[1].id;
    ed.focus(Some(id));
    assert!(ed.view.set_fold(true, 10));
    assert_eq!(ed.view.rows.len(), 1);
    assert!(ed.view.rows[0].folded);
    assert_eq!(ed.markdown(), MD);
    ed.view.set_fold(false, 10);
    assert_eq!(ed.view.rows.len(), 2);
    ed.view.reveal_block(4, 10);
    assert!(ed.view.zoom.is_none());
    assert_eq!(ed.view.rows[ed.view.selected].block, 4);
}

#[test]
fn dropping_typed_text_children_and_metadata_is_exactly_one_undo_step() {
    let mut ed = editor();
    ed.view.set_selected(1, 10);
    ed.start_edit_selected();
    ed.host.type_text("새 한글😀");
    let source = ed.view.doc.blocks[1].id;
    let target = ed.view.doc.blocks[3].id;
    assert!(ed.drop_subtree(source, target, DropPlace::Child, 10));
    let after = ed.markdown();
    assert!(after.contains("새 한글😀"));
    let i = ed.view.doc.blocks.iter().position(|b| b.id == source).unwrap();
    assert_eq!(ed.view.doc.blocks[i + 1].text, "손자");
    assert_eq!(ed.view.doc.blocks[i].properties.get("id").map(String::as_str), Some("child"));
    // 이동 후 재편집도 순번이 아닌 기존 ID를 써야 한다.
    ed.start_edit_selected();
    ed.host.type_text("다시 편집");
    ed.finish_edit();
    assert!(ed.markdown().contains("다시 편집"));
    ed.history_step(true);
    assert_eq!(ed.markdown(), after);
    ed.history_step(true);
    assert_eq!(ed.markdown(), MD);
    assert!(!ed.can_undo());
    ed.history_step(false);
    assert_eq!(ed.markdown(), after);
}

#[test]
fn invalid_drop_keeps_pending_edit_document_and_history() {
    let mut ed = editor();
    ed.start_edit_selected();
    ed.host.type_text("아직 입력 중");
    let (a, b) = (ed.view.doc.blocks[0].id, ed.view.doc.blocks[1].id);
    assert!(!ed.drop_subtree(a, b, DropPlace::Child, 10));
    assert_eq!(ed.markdown(), MD);
    assert_eq!(ed.host.text, "아직 입력 중");
    assert!(ed.editing() && !ed.can_undo());
}

#[test]
fn pointer_threshold_invalid_targets_and_three_drop_zones() {
    for dpi in [96, 144, 192] {
        let view = View::from_markdown(MD).unwrap();
        let m = Metrics::new(dpi);
        let source = view.doc.blocks[1].id;
        let mut pointer = Pointer::Armed { source, x: 0, y: 0 };
        pointer.motion(&view, &m, 1, 1, scale(780, dpi), scale(520, dpi), (4, 4));
        assert!(matches!(pointer, Pointer::Armed { .. }));
        let y = m.header_h + view.slot_of(3) as i32 * m.row_h;
        for (dy, place) in [(1, DropPlace::Before), (m.row_h / 2, DropPlace::Child), (m.row_h - 1, DropPlace::After)] {
            pointer.motion(&view, &m, m.text_x(0) + 20, y + dy, scale(780, dpi), scale(520, dpi), (4, 4));
            assert_eq!(pointer.hint().unwrap().place, place);
        }
        pointer.motion(&view, &m, 100, m.header_h + m.row_h * 2 + 1, scale(780, dpi), scale(520, dpi), (4, 4));
        assert!(pointer.hint().is_none(), "하위로 놓기 금지");
        pointer.motion(&view, &m, -1, y, scale(780, dpi), scale(520, dpi), (4, 4));
        assert!(pointer.hint().is_none());
    }
}

#[test]
fn breadcrumb_keeps_back_page_and_parent_reachable_in_a_narrow_view() {
    let mut ed = editor();
    let id = ed.view.doc.blocks[2].id;
    ed.focus(Some(id));
    for dpi in [96, 144, 192] {
        let width = scale(220, dpi);
        let path = crumbs(&ed.view, dpi, width);
        assert_eq!(path[2].root, Some(ed.view.doc.blocks[1].id));
        assert!(path.iter().all(|c| c.rect.l >= 0 && c.rect.r <= width && c.rect.w() > 0));
    }
}

#[test]
fn real_bullet_and_fold_have_distinct_hit_targets_and_zoom_back_key() {
    let _guard = LOCK.lock().unwrap();
    let mut h = harness(96, Kind::Light);
    h.layout();
    let m = Metrics::new(96);
    let y = m.header_h + m.row_h / 2;
    h.click_at(m.marker_x(0), y, false);
    assert_eq!(h.editor().view.rows.len(), 4);
    h.click_at(m.marker_x(0), y, false);
    assert_eq!(h.editor().view.rows.len(), 6);
    h.pointer_message(WM_LBUTTONDOWN, m.bullet_x(0), y);
    assert!(matches!(h.pointer_state(), Pointer::Armed { .. }));
    h.pointer_message(WM_LBUTTONUP, m.bullet_x(0), y);
    assert_eq!(h.editor().view.rows.len(), 3);
    assert!(h.list_key_modifiers(0xDB, true, false, false));
    assert_eq!(h.editor().view.rows.len(), 6);
    assert_eq!(h.editor().markdown(), MD);
}

#[test]
fn real_drag_move_undo_escape_capture_loss_and_outside_release() {
    let _guard = LOCK.lock().unwrap();
    let mut h = harness(96, Kind::Dark);
    h.layout();
    let m = Metrics::new(96);
    let (x, y) = (m.bullet_x(1), m.header_h + m.row_h + m.row_h / 2);
    let (tx, ty) = (m.text_x(0) + 30, m.header_h + m.row_h * 3 + m.row_h / 2);
    for cancel in [WM_CANCELMODE, WM_CAPTURECHANGED, 0, 1] {
        h.pointer_message(WM_LBUTTONDOWN, x, y);
        h.pointer_message(WM_MOUSEMOVE, tx, ty);
        assert!(h.pointer_state().dragging());
        match cancel {
            0 => { h.list_key(0x1B); }
            1 => { h.pointer_message(WM_LBUTTONUP, -10, ty); }
            msg => h.pointer_message(msg, 0, 0),
        }
        h.pointer_message(WM_LBUTTONUP, tx, ty);
        assert_eq!(h.editor().markdown(), MD);
        assert!(!h.editor().can_undo());
    }
    h.pointer_message(WM_LBUTTONDOWN, x, y);
    h.pointer_message(WM_MOUSEMOVE, tx, ty);
    h.pointer_message(WM_LBUTTONUP, tx, ty);
    assert_ne!(h.editor().markdown(), MD);
    h.bullet_action(6);
    assert_eq!(h.editor().markdown(), MD);
    assert!(!h.editor().can_undo());
    h.editor().view.set_selected(1, 10);
    h.bullet_action(5);
    assert_eq!(h.editor().view.doc.blocks.len(), 4);
    h.bullet_action(6);
    assert_eq!(h.editor().markdown(), MD);
}

#[test]
fn real_capture_zoom_drag_and_fold_in_three_themes_two_dpis() {
    let _guard = LOCK.lock().unwrap();
    let destination = std::env::var_os("OUTLINEUI2_CAPTURE_DIR").map(std::path::PathBuf::from);
    for (tag, kind) in [("light", Kind::Light), ("dark", Kind::Dark), ("hc", Kind::HighContrast)] {
        for dpi in [96, 192] {
            let mut h = harness(dpi, kind);
            h.layout();
            let m = Metrics::new(dpi);
            let x = m.bullet_x(1);
            let y = m.header_h + m.row_h + m.row_h / 2;
            h.pointer_message(WM_LBUTTONDOWN, x, y);
            h.pointer_message(WM_MOUSEMOVE, m.text_x(0) + scale(30, dpi), m.header_h + m.row_h * 3 + m.row_h / 2);
            assert!(h.pointer_state().hint().is_some());
            let drag = h.render().unwrap();
            h.pointer_message(WM_CANCELMODE, 0, 0);
            h.editor().view.set_selected(3, 10);
            h.bullet_action(3);
            let fold = h.render().unwrap();
            h.editor().view.set_selected(1, 10);
            h.bullet_action(1);
            let zoom = h.render().unwrap();
            assert_ne!(drag.bgra, zoom.bgra);
            assert_ne!(fold.bgra, zoom.bgra);
            if let Some(dir) = &destination {
                std::fs::create_dir_all(dir).unwrap();
                for (name, shot) in [("drag", drag), ("fold", fold), ("zoom", zoom)] {
                    outliner_win::png::write(&dir.join(format!("{name}_{tag}_{dpi}.png")), shot.width, shot.height, &shot.bgra).unwrap();
                }
            }
        }
    }
}

#[test]
fn real_drag_autoscrolls_at_the_edge_without_changing_selection_or_document() {
    let _guard = LOCK.lock().unwrap();
    let md = (0..70).map(|i| format!("- 합성 {i}\n")).collect::<String>();
    let ed = Editor::new(View::from_markdown(&md).unwrap(), WinHost::new(), None, md.clone());
    let mut h = Harness::new(ed, "긴 합성 목록", Kind::Light, 96, 780, 300).unwrap();
    h.layout();
    let m = Metrics::new(96);
    h.pointer_message(WM_LBUTTONDOWN, m.bullet_x(0), m.header_h + m.row_h / 2);
    h.pointer_message(WM_MOUSEMOVE, m.text_x(0), 290);
    h.drag_tick();
    h.drag_tick();
    assert!(h.editor().view.first >= 2);
    assert_eq!(h.editor().view.selected, 0);
    h.pointer_message(WM_CANCELMODE, 0, 0);
    assert_eq!(h.editor().markdown(), md);
}

#[test]
fn zoom_edit_undo_clears_reparsed_ids_instead_of_focusing_another_block() {
    let mut ed = editor();
    let id = ed.view.doc.blocks[1].id;
    ed.focus(Some(id));
    ed.start_edit_selected();
    ed.host.type_text("줌 중 편집");
    assert!(ed.finish_edit());
    assert_eq!(ed.view.zoom, Some(id));
    assert!(ed.history_step(true));
    assert!(ed.view.zoom.is_none());
    assert!(!ed.view.can_zoom_back());
    assert_eq!(ed.markdown(), MD);
}

#[test]
fn real_ancestor_path_parent_page_and_back_use_the_painted_targets() {
    let _guard = LOCK.lock().unwrap();
    let mut h = harness(96, Kind::Light);
    let id = h.editor().view.doc.blocks[2].id;
    h.editor().focus(Some(id));
    h.layout();
    let path = crumbs(&h.editor().view, 96, 780);
    let parent = &path[2];
    h.click_at((parent.rect.l + parent.rect.r) / 2, parent.rect.b / 2, false);
    assert_eq!(h.editor().view.zoom, parent.root);
    let page = &path[1];
    h.click_at((page.rect.l + page.rect.r) / 2, page.rect.b / 2, false);
    assert!(h.editor().view.zoom.is_none());
    assert_eq!(h.editor().markdown(), MD);
    assert!(h.list_key_modifiers(0xDB, true, false, false));
    assert_eq!(h.editor().view.zoom, parent.root);
}

struct AsyncHost(Fake);
impl outliner_win::edit::EditHost for AsyncHost {
    fn read_text(&mut self) -> String { self.0.text.clone() }
    fn selection(&mut self) -> (usize, usize) { self.0.sel }
    fn set_text(&mut self, s: &str, caret: usize) { self.0.text = s.into(); self.0.sel = (caret, caret); }
    fn composing(&mut self) -> bool { self.0.composing }
    fn candidate_open(&mut self) -> bool { false }
    fn request_complete(&mut self) { self.0.completes += 1; }
}

#[test]
fn asynchronous_ime_completion_blocks_zoom_and_drop_until_the_text_is_ready() {
    let mut ed = Editor::new(View::from_markdown(MD).unwrap(), AsyncHost(Fake::default()), None, MD.into());
    ed.start_edit_selected();
    ed.host.0.composing = true;
    ed.host.0.text = "조합 글자".into();
    let (a, b) = (ed.view.doc.blocks[0].id, ed.view.doc.blocks[3].id);
    assert!(!ed.focus(Some(a)));
    assert!(!ed.drop_subtree(a, b, DropPlace::After, 10));
    assert_eq!(ed.markdown(), MD);
    assert!(!ed.can_undo() && ed.editing());
    assert_eq!(ed.host.0.completes, 2);
    ed.host.0.composing = false;
    assert!(ed.drop_subtree(a, b, DropPlace::After, 10));
    assert!(ed.markdown().contains("조합 글자"));
    ed.history_step(true);
    assert_eq!(ed.markdown(), MD);
}
