//! 복잡한 컨트롤(색·폴더·글꼴·목록·단축키)과 저장 충돌 시험(창 없이). 설정 파일은 모두 임시 폴더.
//! 기대값을 키 개수 같은 숫자로 고정하지 않는다(소유자 스키마에서 계산).
use serde_json::{Value, json};
use settings_win::controls::{self, COLORS};
use settings_win::model::Model;
use settings_win::schema::Control;
use std::path::PathBuf;

fn temp_settings(body: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    if let Some(b) = body {
        std::fs::write(&path, b).unwrap();
    }
    (dir, path)
}

fn raw(p: &PathBuf) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

// ---------- 색 ----------

#[test]
fn colors_match_the_app_list_and_cycle_wraps() {
    let ids: Vec<&str> = COLORS.iter().map(|c| c.0).collect();
    assert_eq!(ids, ["red", "orange", "green", "blue", "skyblue", "purple", "magenta", "brown", "gray"]);
    assert_eq!(controls::color_label("blue"), "파랑");
    assert_eq!(controls::color_label("zzz"), "zzz");
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    assert_eq!(m.current_text("color"), "blue");
    m.cycle_color("color", 1);
    assert_eq!(m.current_text("color"), "skyblue");
    assert!(m.is_changed("color"));
    m.cycle_color("color", -1);
    assert!(!m.is_changed("color"), "원래 값으로 돌아오면 변경 표시가 사라진다");
    m.set_text("color", "gray");
    m.cycle_color("color", 1);
    assert_eq!(m.current_text("color"), "red", "끝에서 처음으로");
    // 목록에 없는 저장 값에서도 순환이 시작된다.
    m.set_text("color", "#123456");
    m.cycle_color("color", 1);
    assert_eq!(m.current_text("color"), "red");
}

// ---------- 단축키 ----------

#[test]
fn hotkey_text_is_validated_and_normalized() {
    assert_eq!(controls::normalize_hotkey("ctrl+alt+space"), Ok("Ctrl+Alt+Space".into()));
    assert_eq!(controls::normalize_hotkey("Win + shift + f"), Ok("Shift+Win+F".into()));
    assert_eq!(controls::normalize_hotkey("Alt+f12"), Ok("Alt+F12".into()));
    assert_eq!(controls::normalize_hotkey("  "), Ok(String::new()), "비우면 '없음'");
    for bad in ["A", "Ctrl+", "Ctrl+Ctrl+K", "Ctrl+F25", "Ctrl+F0", "Ctrl+Enter", "Foo+K", "Ctrl+AB", "Ctrl+Alt"] {
        assert!(controls::normalize_hotkey(bad).is_err(), "{bad:?} 는 거부해야 함");
    }
}

#[test]
fn hotkey_capture_follows_modifier_rules() {
    // 조합 키만 눌린 동안은 계속 기다린다.
    for vk in [0x10, 0x11, 0x12, 0x5B, 0xA0, 0xA2] {
        assert!(controls::hotkey_from_press(true, false, false, false, vk).is_none(), "{vk:#x}");
    }
    assert_eq!(controls::hotkey_from_press(true, true, false, false, 0x4B), Some(Ok("Ctrl+Alt+K".into())));
    assert_eq!(controls::hotkey_from_press(false, false, true, true, 0x20), Some(Ok("Shift+Win+Space".into())));
    assert_eq!(controls::hotkey_from_press(true, false, false, false, 0x70 + 4), Some(Ok("Ctrl+F5".into())));
    assert!(matches!(controls::hotkey_from_press(false, false, false, false, 0x41), Some(Err(_))), "조합 키 없이는 거부");
    assert!(matches!(controls::hotkey_from_press(true, false, false, false, 0x0D), Some(Err(_))), "Enter 는 지원하지 않는다");
    // 입력으로 얻은 글은 다시 검사해도 그대로다.
    let got = controls::hotkey_from_press(true, true, true, false, 0x5A).unwrap().unwrap();
    assert_eq!(controls::normalize_hotkey(&got), Ok(got));
}

#[test]
fn hotkey_demo_item_validates_and_can_be_cleared() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.add_demo_items();
    assert!(matches!(m.items[m.item_index("demo.hotkey").unwrap()].control, Control::Hotkey));
    m.set_text("demo.hotkey", "Ctrl+Q");
    assert_eq!(m.error_of("demo.hotkey"), None);
    m.set_text("demo.hotkey", "Q");
    assert!(m.error_of("demo.hotkey").unwrap().contains("함께"));
    m.set_text("demo.hotkey", "");
    assert_eq!(m.error_of("demo.hotkey"), None, "비워 두기는 허용");
}

// ---------- 목록 ----------

#[test]
fn list_add_remove_move_with_checks() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.add_demo_items();
    let key = "demo.excluded_apps";
    assert!(m.current_list(key).is_empty());
    assert!(!m.is_changed(key));
    assert_eq!(m.list_add(key, "  a.exe "), Ok(0));
    assert_eq!(m.list_add(key, "b.exe"), Ok(1));
    assert_eq!(m.list_add(key, "C.exe"), Ok(2));
    assert!(m.is_changed(key));
    assert_eq!(m.current_list(key), ["a.exe", "b.exe", "C.exe"], "앞뒤 공백은 지운다");
    assert!(m.list_add(key, "c.EXE").unwrap_err().contains("이미"), "대소문자 무시 중복");
    assert!(m.list_add(key, "   ").is_err());
    assert!(m.list_add(key, "a\0b").is_err());
    assert!(m.list_add(key, &"x".repeat(1001)).is_err());
    assert_eq!(m.list_move(key, 2, -1), Some(1));
    assert_eq!(m.current_list(key), ["a.exe", "C.exe", "b.exe"]);
    assert_eq!(m.list_move(key, 0, -1), None, "맨 위는 더 못 올린다");
    assert_eq!(m.list_move(key, 2, 1), None, "맨 아래는 더 못 내린다");
    assert!(m.list_remove(key, 1));
    assert!(!m.list_remove(key, 5));
    assert_eq!(m.current_list(key), ["a.exe", "b.exe"]);
    assert!(m.list_remove(key, 0) && m.list_remove(key, 0));
    assert!(!m.is_changed(key), "다시 비면 변경 표시가 사라진다");
}

#[test]
fn list_respects_its_maximum() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.add_demo_items();
    let key = "demo.excluded_apps";
    let max = match m.items[m.item_index(key).unwrap()].control {
        Control::List { max } => max,
        _ => panic!(),
    };
    for i in 0..max {
        m.list_add(key, &format!("app{i}.exe")).unwrap();
    }
    assert!(m.list_add(key, "one-more.exe").unwrap_err().contains("최대"));
    assert_eq!(m.current_list(key).len(), max);
}

// ---------- 폴더 ----------

#[test]
fn folder_text_rules_and_missing_folder_warning() {
    assert_eq!(controls::explain_folder(""), None);
    assert_eq!(controls::explain_folder(r"C:\Windows\Fonts"), None);
    assert_eq!(controls::explain_folder(r"\\server\share\문서"), None);
    for bad in ["C:\\a<b", "C:\\a|b", "C:\\a?b", "C:\\a*b", "C:\\a\"b", "D:\\x:y"] {
        assert!(controls::explain_folder(bad).is_some(), "{bad:?}");
    }
    assert!(controls::explain_folder(&"가".repeat(261)).is_some());
    let (d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.set_text("paste_add_folder", d.path().to_str().unwrap());
    assert_eq!(m.error_of("paste_add_folder"), None);
    assert_eq!(m.warning_of("paste_add_folder"), None, "있는 폴더는 경고 없음");
    let missing = d.path().join("없는폴더");
    m.set_text("paste_add_folder", missing.to_str().unwrap());
    assert_eq!(m.error_of("paste_add_folder"), None, "없는 폴더는 저장을 막지 않는다");
    assert!(m.warning_of("paste_add_folder").unwrap().contains("없습니다"));
    m.set_text("paste_add_folder", "C:\\x*y");
    assert!(m.error_of("paste_add_folder").is_some());
    assert_eq!(m.warning_of("paste_add_folder"), None, "오류가 있으면 경고는 따로 내지 않는다");
    m.set_text("paste_add_folder", "");
    assert_eq!(m.warning_of("paste_add_folder"), None, "비우면 자동(경고 없음)");
}

// ---------- 글꼴 ----------

#[test]
fn font_name_rules_and_list_filter() {
    assert!(controls::explain_font("").is_some());
    assert!(controls::explain_font("   ").is_some());
    assert!(controls::explain_font(&"가".repeat(65)).is_some());
    assert!(controls::explain_font("한컴\n돋움").is_some());
    assert_eq!(controls::explain_font("한컴돋움"), None);
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.fonts = vec!["Arial".into(), "Malgun Gothic".into(), "맑은 고딕".into()];
    assert_eq!(m.font_choices("").len(), 3);
    assert_eq!(m.font_choices("GOTHIC"), ["Malgun Gothic"], "대소문자 무시");
    assert_eq!(m.font_choices("고딕"), ["맑은 고딕"]);
    assert!(m.font_choices("없는글꼴").is_empty());
    m.set_text("symbol_fonts.□.font", "맑은 고딕");
    assert_eq!(m.error_of("symbol_fonts.□.font"), None);
    assert_eq!(m.warning_of("symbol_fonts.□.font"), None);
    m.set_text("symbol_fonts.□.font", "한컴돋움");
    assert_eq!(m.error_of("symbol_fonts.□.font"), None, "직접 입력은 허용");
    assert!(m.warning_of("symbol_fonts.□.font").unwrap().contains("설치"), "목록에 없으면 알림만");
    m.set_text("symbol_fonts.□.font", "");
    assert!(m.error_of("symbol_fonts.□.font").is_some(), "빈 글꼴 이름은 오류");
}

#[cfg(windows)]
#[test]
fn installed_font_enumeration_is_sorted_and_unique() {
    let fonts = controls::installed_fonts();
    assert!(!fonts.is_empty(), "Windows 에는 글꼴이 있다");
    assert!(fonts.iter().all(|f| !f.starts_with('@') && !f.is_empty()));
    let mut keys: Vec<String> = fonts.iter().map(|f| f.to_lowercase()).collect();
    let sorted = {
        let mut k = keys.clone();
        k.sort();
        k
    };
    assert_eq!(keys, sorted, "정렬되어 있다");
    keys.dedup();
    assert_eq!(keys.len(), fonts.len(), "중복 없음");
}

// ---------- 새로 구현된 11개 키의 저장 왕복 ----------

#[test]
fn every_formerly_unimplemented_key_saves_and_reloads() {
    let (d, p) = temp_settings(None);
    let folder = d.path().to_str().unwrap().to_string();
    let edits: Vec<(&str, String)> = vec![
        ("color", "purple".into()),
        ("hwp_font_folder", folder.clone()),
        ("paste_add_folder", folder.clone()),
        ("symbol_fonts.□.font", "맑은 고딕".into()),
        ("symbol_fonts.ㅇ.font", "Malgun Gothic".into()),
        ("symbol_fonts.-.font", "바탕체".into()),
        ("symbol_fonts.※.font", "굴림체".into()),
        ("symbol_fonts.*.font", "굴림".into()),
        ("symbol_fonts.•.font", "바탕".into()),
        ("table_fonts.header.font", "돋움".into()),
        ("table_fonts.body.font", "궁서".into()),
    ];
    let mut m = Model::load(&p).unwrap();
    for (k, v) in &edits {
        m.set_text(k, v);
        assert_eq!(m.error_of(k), None, "{k}");
    }
    assert_eq!(m.save(), Ok(edits.len()));
    let reread = Model::load(&p).unwrap();
    for (k, v) in &edits {
        assert_eq!(&reread.current_text(k), v, "{k}");
    }
    let r = raw(&p);
    assert_eq!(r["color"], json!("purple"));
    assert_eq!(r["symbol_fonts"]["□"]["font"], json!("맑은 고딕"));
    assert_eq!(reread.current_text("symbol_fonts.□.size"), "17", "같은 묶음의 다른 값은 기본값 그대로(병합)");
    assert_eq!(r["table_fonts"]["body"]["font"], json!("궁서"));
    // docfit_config 의 검증도 통과한 것이다.
    for (k, v) in &edits {
        assert!(docfit_config::validate_patch(&json!({ *k: v })).is_ok(), "{k}");
    }
}

// ---------- 저장 충돌(앱이 설정을 쓰는 동안 새 창이 저장) ----------

#[test]
fn save_is_refused_when_the_app_wrote_the_file_meanwhile() {
    let (_d, p) = temp_settings(Some(r#"{"retry_table":"9","custom":{"keep":true}}"#));
    let mut window = Model::load(&p).unwrap(); // 새 창이 읽은 시점
    window.set_text("color", "red");
    window.set_text("hwp_font_folder", r"C:\Windows");
    // 그 사이 앱(Python)이 같은 파일을 쓴다: 같은 잠금·원자 교체 경로(docfit_config)로 서식 값을 바꾼다.
    docfit_config::set(&p, &json!({ "retry_table": "12" })).unwrap();
    let after_app = std::fs::read(&p).unwrap();
    let err = window.save().unwrap_err();
    assert!(err.contains("다시 불러") || err.contains("되돌리기"), "다시 불러오기를 안내해야 함: {err}");
    assert!(err.contains("다른 곳에서 설정이 바뀌었습니다"), "{err}");
    assert!(window.status.as_ref().unwrap().error);
    assert_eq!(std::fs::read(&p).unwrap(), after_app, "충돌이면 파일을 한 바이트도 바꾸지 않는다");
    assert_eq!(window.changed_count(), 2, "내가 편집한 값은 화면에 남는다");
    // [되돌리기]로 다시 불러오면 앱이 쓴 값이 보이고, 다시 편집해 저장할 수 있다.
    window.revert().unwrap();
    assert_eq!(window.current_text("retry_table"), "12");
    assert_eq!(window.changed_count(), 0);
    window.set_text("color", "red");
    assert_eq!(window.save(), Ok(1));
    let r = raw(&p);
    assert_eq!(r["retry_table"], json!("12"), "앱이 쓴 값이 유지된다");
    assert_eq!(r["color"], json!("red"));
    assert_eq!(r["custom"]["keep"], json!(true), "모르는 키도 보존");
}

#[test]
fn concurrent_writers_never_lose_either_save() {
    // 앱 쪽 저장과 창 저장을 여러 번 번갈아 시도한다. 충돌은 거부되어야 하고, 성공한 저장은 모두 파일에 남아야 한다.
    let (_d, p) = temp_settings(Some("{}"));
    let path = p.clone();
    let app = std::thread::spawn(move || {
        for i in 0..20 {
            docfit_config::set(&path, &json!({ "retry_body": format!("{}", 10 + i) })).unwrap();
        }
    });
    let mut accepted = 0;
    for i in 0..20 {
        let mut w = Model::load(&p).unwrap();
        w.set_text("retry_table", &format!("{}", 100 + i));
        match w.save() {
            Ok(_) => accepted += 1,
            Err(e) => assert!(e.contains("다른 곳에서 설정이 바뀌었습니다"), "{e}"),
        }
    }
    app.join().unwrap();
    let r = raw(&p);
    assert_eq!(r["retry_body"], json!("29"), "앱의 마지막 저장은 사라지지 않는다");
    assert!(accepted >= 1 || r.get("retry_table").is_none());
    assert!(docfit_config::read_json(&p).is_ok(), "파일은 항상 읽을 수 있는 JSON");
}

// ---------- 실제 목록 키(SETTINGSWIN4) ----------

/// 소유자 스키마에서 '목록' 종류인 키를 전부 찾는다(개수를 고정하지 않는다).
fn schema_list_keys() -> Vec<String> {
    docfit_config::visible_schema().into_iter().filter(|(_, kind, _)| kind == "list").map(|(k, _, _)| k).collect()
}

#[test]
fn every_list_kind_key_in_the_schema_gets_a_list_control_in_a_real_group() {
    let (_d, p) = temp_settings(None);
    let m = Model::load(&p).unwrap();
    let keys = schema_list_keys();
    assert!(!keys.is_empty(), "스키마에 목록 키가 하나도 없다");
    for key in &keys {
        let item = &m.items[m.item_index(key).unwrap_or_else(|| panic!("{key} 가 설정 창 항목에 없다"))];
        assert!(matches!(item.control, Control::List { .. }), "{key} 는 목록 컨트롤이어야 한다");
        assert_ne!(item.group, settings_win::schema::group_index("internal"), "{key} 가 내부(화면 밖) 묶음에 들어갔다");
        assert!(!item.label.is_empty() && item.label != *key, "{key} 의 화면 이름이 없다");
    }
}

#[test]
fn hidden_tabs_list_stores_sorted_integers_and_refuses_required_menus() {
    let (_d, p) = temp_settings(Some(r#"{"hidden_tabs":[4,2],"keep":{"a":1}}"#));
    let mut m = Model::load(&p).unwrap();
    let key = "hidden_tabs";
    assert_eq!(m.current_list(key), ["4", "2"], "저장 값 그대로 보인다");
    assert!(m.list_add(key, "12").unwrap_err().contains("항상"));
    assert!(m.list_add(key, "13").is_err());
    assert!(m.list_add(key, "0").is_err() && m.list_add(key, "16").is_err());
    assert!(m.list_add(key, "abc").is_err() && m.list_add(key, "").is_err());
    assert!(m.list_add(key, "4").unwrap_err().contains("이미"));
    assert_eq!(m.list_add(key, "3"), Ok(1), "정렬된 자리");
    assert_eq!(m.list_move(key, 0, 1), None, "번호 목록은 순서를 바꾸지 않는다");
    assert!(m.save().is_ok());
    let saved = raw(&p);
    assert_eq!(saved["hidden_tabs"], json!([2, 3, 4]), "오름차순 정수 배열(Python clean_hidden_tabs 와 같은 모양)");
    assert_eq!(saved["keep"], json!({"a": 1}));
}

#[test]
fn library_excluded_folders_list_checks_paths_and_ignores_trailing_separator() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    let key = "library_excluded_folders";
    assert_eq!(m.list_add(key, r"  D:\자료\임시\ "), Ok(0));
    assert!(m.list_add(key, r"d:\자료\임시").unwrap_err().contains("이미"), "끝 구분자·대소문자 무시 중복");
    assert!(m.list_add(key, r"D:\a|b").is_err(), "예약 문자");
    assert_eq!(m.list_add(key, r"E:\"), Ok(1), "드라이브 루트는 그대로");
    assert_eq!(m.list_move(key, 1, -1), Some(0), "폴더 목록은 순서를 바꿀 수 있다");
    assert!(m.save().is_ok());
    assert_eq!(raw(&p)[key], json!([r"E:\", r"D:\자료\임시"]));
    let again = Model::load(&p).unwrap();
    assert_eq!(again.current_list(key), [r"E:\", r"D:\자료\임시"]);
}

#[test]
fn list_keys_default_is_empty_and_reset_to_default_clears() {
    let (_d, p) = temp_settings(Some(r#"{"hidden_tabs":[5]}"#));
    let mut m = Model::load(&p).unwrap();
    for key in schema_list_keys() {
        let default = m.items[m.item_index(&key).unwrap()].default.clone();
        assert_eq!(default, json!([]), "{key}: 기본값은 빈 목록(없을 때 동작과 같음)");
    }
    m.reset_to_default("hidden_tabs");
    assert!(m.is_changed("hidden_tabs"));
    assert!(m.save().is_ok());
    assert_eq!(raw(&p)["hidden_tabs"], json!([]));
}

#[test]
fn menu_number_entries_show_their_menu_name_and_other_lists_do_not() {
    use settings_win::schema::{list_entry_label, MENU_NAMES};
    assert_eq!(list_entry_label("hidden_tabs", "3"), "3  다이어그램");
    assert_eq!(list_entry_label("hidden_tabs", "99"), "99", "이름 없는 번호는 그대로");
    assert_eq!(list_entry_label("library_excluded_folders", "3"), "3");
    // 숨길 수 있는 번호는 모두 이름이 있어야 한다(번호 범위는 docfit_config 규칙에서 계산).
    for n in (0..=20).filter(|n| docfit_config::is_hideable_menu(*n)) {
        assert!(MENU_NAMES.get(n as usize).is_some_and(|s| !s.is_empty()), "{n} 번 메뉴 이름이 없다");
    }
}
