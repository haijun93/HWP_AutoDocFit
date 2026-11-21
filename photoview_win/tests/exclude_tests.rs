//! 앱 설정 읽기·제외 폴더 편집(PHOTOWIN2): `docfit_config` 경유 읽기/저장, revision 충돌, 패널 배치. 임시 설정 파일만 쓴다.
use photoview_win::layout::scale;
use photoview_win::panel::{ExcludePanel, Hit, PanelLayout, Tone};
use photoview_win::settings::{self, SaveError};
use serde_json::json;
use std::path::PathBuf;

fn temp_settings(body: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.json");
    if !body.is_empty() {
        std::fs::write(&p, body).unwrap();
    }
    (dir, p)
}

fn file_json(p: &PathBuf) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

// ---------- 읽기 ----------

#[test]
fn load_reads_thumb_size_and_excluded_folders_through_docfit_config() {
    let (_d, p) = temp_settings(r#"{"ui_thumb_sizes":{"photo_album":150,"doc_grid":120},"library_excluded_folders":["D:\\b","C:\\a","c:/A/"],"photoview_ui":"native"}"#);
    let l = settings::load(&p);
    assert_eq!(l.thumb, 150.0);
    assert_eq!(l.excluded, [r"D:\b", r"C:\a"], "대소문자·구분자 무시 중복 제거");
    assert_eq!(l.ui, "native");
    assert_eq!(l.error, None);
    assert!(!l.revision.is_empty());
}

#[test]
fn load_falls_back_to_defaults_for_missing_or_broken_file_without_writing() {
    let (d, p) = temp_settings("");
    let l = settings::load(&p);
    assert_eq!((l.thumb, l.excluded.len(), l.ui, l.error.is_none()), (160.0, 0, "flutter", true));
    assert!(!p.exists(), "읽기만으로 설정 파일을 만들지 않는다");
    std::fs::write(&p, "{ 깨짐").unwrap();
    let l = settings::load(&p);
    assert_eq!((l.thumb, l.excluded.len(), l.ui), (160.0, 0, "flutter"));
    assert!(l.error.unwrap().contains("기본값"), "깨진 파일은 이유와 함께 기본값");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ 깨짐", "깨진 파일도 그대로 둔다");
    drop(d);
}

#[test]
fn photoview_ui_key_is_part_of_the_app_defaults_owned_by_docfit_config() {
    assert_eq!(docfit_config::defaults()["photoview_ui"], "flutter");
    // 설정 창이 글 입력칸으로 보여 주면 안 되는 내부 키.
    assert!(docfit_config::visible_schema().iter().all(|(k, _, _)| k != "photoview_ui"));
}

#[test]
fn folder_key_matches_the_python_rules() {
    assert_eq!(settings::folder_key(r"C:\Work\"), settings::folder_key("c:/work"));
    assert_eq!(settings::folder_key(r" D:\x "), r"d:\x");
    assert_eq!(settings::folder_key(r"C:\"), r"c:\", "드라이브 루트의 끝 구분자는 남긴다");
}

// ---------- 저장 ----------

#[test]
fn panel_add_and_remove_save_a_sorted_list_and_keep_every_other_key() {
    let (_d, p) = temp_settings(r#"{"theme_mode":"dark","custom_unknown":{"x":1},"ui_thumb_sizes":{"photo_album":170},"library_excluded_folders":["D:\\Zeta"]}"#);
    let mut panel = ExcludePanel::open(&p);
    assert_eq!(panel.items, [r"D:\Zeta"]);
    assert!(panel.add(r"C:\Alpha"));
    assert!(panel.add(r"E:\beta\"));
    assert_eq!(panel.items, [r"C:\Alpha", r"D:\Zeta", r"E:\beta\"], "Python 과 같은 대소문자 무시 정렬");
    let saved = file_json(&p);
    assert_eq!(saved["library_excluded_folders"], json!([r"C:\Alpha", r"D:\Zeta", r"E:\beta\"]));
    assert_eq!(saved["theme_mode"], "dark");
    assert_eq!(saved["custom_unknown"], json!({"x": 1}), "모르는 키 보존");
    assert_eq!(saved["ui_thumb_sizes"]["photo_album"], 170);
    assert!(panel.changed);

    // 중복(대소문자·구분자·끝 구분자 달라도)은 저장하지 않는다.
    let before = std::fs::read_to_string(&p).unwrap();
    assert!(!panel.add("c:/alpha/"));
    assert_eq!(panel.tone, Tone::Info);
    assert!(panel.message.contains("이미"));
    assert!(!panel.add("   "));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), before, "거절한 추가는 파일을 바꾸지 않는다");

    // 삭제.
    panel.select(Some(1));
    assert!(panel.remove_selected());
    assert_eq!(file_json(&p)["library_excluded_folders"], json!([r"C:\Alpha", r"E:\beta\"]));
    assert_eq!(panel.selected, Some(1), "다음 줄로 선택이 이어진다");
    assert!(panel.remove_selected());
    assert!(panel.remove_selected());
    assert!(panel.items.is_empty() && panel.selected.is_none());
    assert!(!panel.remove_selected(), "선택이 없으면 아무 일 없음");
    assert_eq!(file_json(&p)["library_excluded_folders"], json!([]));
    // 새 창이 같은 파일을 다시 읽어도 같은 상태.
    assert!(settings::load(&p).excluded.is_empty());
}

#[test]
fn panel_creates_the_settings_file_when_the_app_has_none() {
    let (_d, p) = temp_settings("");
    let mut panel = ExcludePanel::open(&p);
    assert!(panel.add(r"C:\새 폴더"));
    assert_eq!(settings::load(&p).excluded, [r"C:\새 폴더"]);
}

#[test]
fn save_is_refused_when_the_app_wrote_the_file_meanwhile_and_nothing_is_lost() {
    let (_d, p) = temp_settings(r#"{"library_excluded_folders":["D:\\one"],"search_engine":"kfind"}"#);
    let mut panel = ExcludePanel::open(&p);
    // 그 사이 앱(Python)이 같은 파일을 쓴다.
    docfit_config::set(&p, &json!({"library_excluded_folders": [r"D:\one", r"X:\from_app"], "search_engine": "legacy"})).unwrap();
    let after_app = std::fs::read_to_string(&p).unwrap();
    assert!(!panel.add(r"C:\mine"), "오래된 revision 으로는 저장하지 않는다");
    assert_eq!(panel.tone, Tone::Error);
    assert!(panel.message.contains("다른 곳에서 설정이 바뀌어"), "{}", panel.message);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), after_app, "거절한 저장은 파일을 한 바이트도 바꾸지 않는다");
    assert_eq!(panel.items, [r"D:\one", r"X:\from_app"], "패널은 최신 목록을 다시 불러온다");
    assert!(!panel.changed);
    // 다시 시도하면 앱이 쓴 값과 함께 저장된다.
    assert!(panel.add(r"C:\mine"));
    let saved = file_json(&p);
    assert_eq!(saved["library_excluded_folders"], json!([r"C:\mine", r"D:\one", r"X:\from_app"]));
    assert_eq!(saved["search_engine"], "legacy", "앱이 쓴 다른 설정이 보존된다");
}

#[test]
fn save_excluded_reports_conflict_kind_and_validation_errors() {
    let (_d, p) = temp_settings(r#"{"search_engine":"kfind"}"#);
    let rev = settings::load(&p).revision;
    assert_eq!(settings::save_excluded(&p, &[r"C:\a".into()], "오래된 revision"), Err(SaveError::Conflict));
    // docfit_config 의 목록 검증(대소문자 무시 중복)은 그대로 적용된다.
    let dup = settings::save_excluded(&p, &[r"C:\a".into(), r"c:\A".into()], &rev);
    assert!(matches!(dup, Err(SaveError::Failed(_))), "{dup:?}");
    let ok = settings::save_excluded(&p, &[r"C:\a".into()], &rev).unwrap();
    assert_ne!(ok, rev, "저장하면 revision 이 바뀐다");
    assert_eq!(ok, settings::load(&p).revision);
}

#[test]
fn many_exclusions_scroll_and_keyboard_selection_stay_in_view() {
    let (_d, p) = temp_settings("");
    let mut panel = ExcludePanel::open(&p);
    for i in 0..30 {
        assert!(panel.add(&format!(r"D:\폴더{i:02}")));
    }
    let rows = 6;
    panel.select(None);
    panel.step(1, rows);
    assert_eq!(panel.selected, Some(0));
    for _ in 0..20 {
        panel.step(1, rows);
    }
    let sel = panel.selected.unwrap();
    assert!(sel >= panel.scroll && sel < panel.scroll + rows, "선택 줄이 보이는 범위 안: sel={sel} scroll={}", panel.scroll);
    panel.step(-100, rows);
    assert_eq!((panel.selected, panel.scroll), (Some(0), 0));
    panel.wheel(1000, rows);
    assert_eq!(panel.scroll, 30 - rows, "휠은 끝을 넘지 않는다");
    panel.wheel(-1000, rows);
    assert_eq!(panel.scroll, 0);
}

// ---------- 배치 ----------

#[test]
fn panel_layout_fits_inside_every_window_size_and_dpi() {
    let (_d, p) = temp_settings("");
    let panel = ExcludePanel::open(&p);
    for dpi in [96u32, 120, 144, 192] {
        for (lw, lh) in [(820, 480), (1100, 720), (1920, 1080)] {
            let (w, h) = (scale(lw, dpi), scale(lh, dpi));
            let l = PanelLayout::new(w, h, dpi);
            let win = photoview_win::layout::Rect::new(0, 0, w, h);
            for (name, r) in [("dialog", l.dialog), ("title", l.title), ("hint", l.hint), ("list", l.list), ("message", l.message), ("add", l.add), ("remove", l.remove), ("close", l.close)] {
                assert!(r.left >= win.left && r.right <= win.right && r.top >= win.top && r.bottom <= win.bottom, "{name} 이 창 밖: {r:?} ({w}x{h} @{dpi})");
                assert!(r.width() > 0 && r.height() > 0, "{name} 크기 0 ({w}x{h} @{dpi})");
            }
            for (name, r) in [("title", l.title), ("hint", l.hint), ("list", l.list), ("message", l.message), ("add", l.add), ("remove", l.remove), ("close", l.close)] {
                assert!(r.left >= l.dialog.left && r.right <= l.dialog.right && r.top >= l.dialog.top && r.bottom <= l.dialog.bottom, "{name} 이 상자 밖 ({w}x{h} @{dpi})");
            }
            // 서로 겹치지 않는다.
            let parts = [("title", l.title), ("hint", l.hint), ("list", l.list), ("message", l.message), ("add", l.add), ("remove", l.remove), ("close", l.close)];
            for i in 0..parts.len() {
                for j in i + 1..parts.len() {
                    assert!(!parts[i].1.overlaps(&parts[j].1), "{} 와 {} 겹침 ({w}x{h} @{dpi}): {:?} {:?}", parts[i].0, parts[j].0, parts[i].1, parts[j].1);
                }
            }
            assert!(l.visible_rows() >= 2, "목록에 최소 2줄 ({w}x{h} @{dpi})");
            assert_eq!(l.hit(l.add.left + 2, l.add.top + 2, &panel), Hit::Add);
            assert_eq!(l.hit(l.remove.left + 2, l.remove.top + 2, &panel), Hit::Remove);
            assert_eq!(l.hit(l.close.left + 2, l.close.top + 2, &panel), Hit::Close);
            assert_eq!(l.hit(0, 0, &panel), Hit::Outside, "모달: 상자 밖");
        }
    }
}

#[test]
fn panel_hit_testing_maps_rows_with_scroll() {
    let (_d, p) = temp_settings("");
    let mut panel = ExcludePanel::open(&p);
    for i in 0..12 {
        panel.add(&format!(r"D:\f{i:02}"));
    }
    let l = PanelLayout::new(1100, 720, 96);
    panel.scroll = 3;
    let r0 = l.row_rect(0);
    assert_eq!(l.hit(r0.left + 10, r0.top + 5, &panel), Hit::Row(3));
    let r2 = l.row_rect(2);
    assert_eq!(l.hit(r2.left + 10, r2.top + 5, &panel), Hit::Row(5));
    let short = ExcludePanel::open(&temp_settings("").1);
    assert_eq!(l.hit(r0.left + 10, r0.top + 5, &short), Hit::Inside, "빈 목록의 빈 칸");
}
