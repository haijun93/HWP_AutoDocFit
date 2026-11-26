use grid_core::{
    app::App,
    file_ops::{FileResult, Operation},
    items::Doc,
};

fn doc(name: &str) -> Doc {
    Doc {
        path: format!("/synthetic/{name}.pdf"),
        name: format!("{name}.pdf"),
        ext: "pdf".into(),
        size: 1,
        mtime: 0.0,
        cloud: false,
    }
}
fn app() -> App {
    let mut app = App::new(190.0);
    app.coll
        .replace((0..8).map(|i| doc(&format!("{i:02}"))).collect());
    app
}
fn names(app: &App) -> Vec<String> {
    app.selected_docs().iter().map(|d| d.name.clone()).collect()
}
fn click(app: &mut App, n: usize, ctrl: bool, shift: bool, right: bool) {
    let m = app.metrics(900, 600, 96);
    app.click_modified(&m, Some(n), ctrl, shift, right);
}

#[test]
fn click_ctrl_toggle_shift_replace_and_ctrl_shift_add_follow_visible_order() {
    let mut a = app();
    click(&mut a, 2, false, false, false);
    click(&mut a, 6, true, false, false);
    assert_eq!(names(&a), ["02.pdf", "06.pdf"]);
    click(&mut a, 2, true, false, false);
    assert_eq!(names(&a), ["06.pdf"]);
    click(&mut a, 4, false, true, false);
    assert_eq!(names(&a), ["02.pdf", "03.pdf", "04.pdf"]);
    click(&mut a, 7, true, true, false);
    assert_eq!(
        names(&a),
        ["02.pdf", "03.pdf", "04.pdf", "05.pdf", "06.pdf", "07.pdf"]
    );
}

#[test]
fn consecutive_shift_keeps_original_anchor_and_missing_anchor_selects_one() {
    let mut a = app();
    click(&mut a, 6, false, false, false);
    click(&mut a, 2, false, true, false);
    click(&mut a, 5, false, true, false);
    assert_eq!(names(&a), ["05.pdf", "06.pdf"]);
    a.clear_selection();
    click(&mut a, 4, false, true, false);
    assert_eq!(names(&a), ["04.pdf"]);
}

#[test]
fn right_click_inside_preserves_multi_selection_outside_replaces_and_never_opens() {
    let mut a = app();
    click(&mut a, 1, false, false, false);
    click(&mut a, 3, true, false, false);
    click(&mut a, 1, false, false, true);
    assert_eq!(names(&a), ["01.pdf", "03.pdf"]);
    assert!(a.clicked.is_none());
    assert!(a.enter_target().is_none());
    click(&mut a, 5, false, false, true);
    assert_eq!(names(&a), ["05.pdf"]);
}

#[test]
fn ctrl_toggle_last_item_off_has_no_painted_or_open_target_selection() {
    let mut a = app();
    click(&mut a, 3, false, false, false);
    click(&mut a, 3, true, false, false);
    assert!(a.selected.is_none());
    assert!(!a.is_selected(3));
    assert!(a.enter_target().is_none());
    assert!(a.clicked.is_none());
}

#[test]
fn select_all_uses_visible_search_results_and_clear_resets_anchor_and_double_click() {
    let mut a = app();
    a.coll.set_query("03");
    a.select_all();
    assert_eq!(names(&a), ["03.pdf"]);
    a.clear_selection();
    assert!(a.selected.is_none() && a.selection.anchor.is_none() && a.clicked.is_none());
}

#[test]
fn scan_insert_keeps_selected_paths_and_shift_anchor_after_reordering() {
    let mut a = app();
    click(&mut a, 2, false, false, false);
    click(&mut a, 4, true, false, false);
    a.extend_docs(vec![doc("-first")]);
    assert_eq!(names(&a), ["02.pdf", "04.pdf"]);
    assert_eq!(a.selected_doc().unwrap().name, "04.pdf");
    click(&mut a, 6, false, true, false);
    assert_eq!(names(&a), ["04.pdf", "05.pdf"]);
}

#[test]
fn successful_results_update_only_successes_and_late_scan_cannot_restore_retired_paths() {
    let mut a = app();
    a.select_all();
    let old = a.coll.get(0).unwrap().clone();
    let failed = a.coll.get(1).unwrap().clone();
    a.apply_file_results(
        Operation::Delete,
        &[
            FileResult {
                source: old.clone(),
                target: None,
                error: None,
            },
            FileResult {
                source: failed.clone(),
                target: None,
                error: Some("가짜 휴지통 실패".into()),
            },
        ],
        |_| true,
    );
    a.extend_docs(vec![old.clone()]);
    assert_eq!(a.coll.len(), 7);
    assert!(!a.selection.paths.contains(&old.path));
    assert!(a.selection.paths.contains(&failed.path));
}

#[test]
fn move_into_excluded_folder_removes_source_copy_keeps_source_and_selection() {
    let mut a = app();
    click(&mut a, 1, false, false, false);
    let source = a.coll.get(1).unwrap().clone();
    let target = doc("changed");
    let result = FileResult {
        source: source.clone(),
        target: Some(target.clone()),
        error: None,
    };
    a.apply_file_results(Operation::Copy, &[result.clone()], |_| true);
    assert!(a.selection.paths.contains(&source.path));
    assert!(!a.selection.paths.contains(&target.path));
    a.apply_file_results(Operation::Move, &[result], |_| false);
    assert!(!a.selection.paths.contains(&source.path));
}

#[test]
fn late_scan_does_not_duplicate_a_copy_added_by_file_results() {
    let mut a = app();
    let source = a.coll.get(1).unwrap().clone();
    let target = doc("copy");
    a.apply_file_results(
        Operation::Copy,
        &[FileResult {
            source,
            target: Some(target.clone()),
            error: None,
        }],
        |_| true,
    );
    assert_eq!(a.coll.len(), 9);
    a.extend_docs(vec![target]);
    assert_eq!(a.coll.len(), 9);
}

#[cfg(windows)]
#[test]
fn menu_shortcuts_and_file_drop_payload_keep_safety_boundaries() {
    use grid_core::native_ops::*;
    use std::path::PathBuf;
    assert!(!Action::Open.enabled(2, false));
    assert!(!Action::Rename.enabled(2, false));
    assert!(!Action::Delete.enabled(1, true));
    assert_eq!(shortcut(0x2e, false, true), Some(Action::Delete));
    assert_eq!(shortcut(0x43, true, false), Some(Action::Clipboard));
    assert_eq!(shortcut(0x71, false, false), Some(Action::Rename));
    let paths = [
        PathBuf::from("C:/synthetic/한글.pdf"),
        PathBuf::from("C:/synthetic/2.pdf"),
    ];
    let data = hdrop_payload(&paths);
    assert_eq!(u32::from_le_bytes(data[..4].try_into().unwrap()), 20);
    assert_eq!(u32::from_le_bytes(data[16..20].try_into().unwrap()), 1);
    let body: Vec<_> = data[20..]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(
        String::from_utf16(&body).unwrap(),
        "C:/synthetic/한글.pdf\0C:/synthetic/2.pdf\0\0"
    );
    assert!(!listed_destination(
        "C:/excluded/a.pdf",
        &["c:\\EXCLUDED".into()]
    ));
    assert!(listed_destination(
        "C:/excluded-neighbor/a.pdf",
        &["c:/excluded".into()]
    ));
}
