//! 표시 모델·검증·저장·검색·배치 시험(창 없이). 설정 파일은 모두 임시 폴더.
use serde_json::{Value, json};
use settings_win::layout::{self, Hit, Rect, Shape};
use settings_win::model::Model;
use settings_win::schema::{Control, GROUPS, ReadOnly};
use std::path::PathBuf;

fn temp_settings(body: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    if let Some(b) = body {
        std::fs::write(&path, b).unwrap();
    }
    (dir, path)
}

#[test]
fn key_to_control_mapping_covers_every_key() {
    let (_d, p) = temp_settings(None);
    let m = Model::load(&p).unwrap();
    let c = m.counts();
    // 항목 수는 소유자 스키마에서 계산한다(키가 늘어도 깨지지 않게 숫자를 고정하지 않는다).
    let schema = docfit_config::visible_schema();
    let roots: std::collections::HashSet<String> = schema.iter().map(|(k, _, _)| k.split('.').next().unwrap().to_string()).collect();
    let defaults = docfit_config::defaults();
    let internal_expected = defaults.as_object().unwrap().keys().filter(|k| !roots.contains(*k)).count();
    assert_eq!(m.items.len(), schema.len() + internal_expected);
    assert_eq!(c.internal, internal_expected);
    assert_eq!(c.unimplemented, 0, "화면에 보이는 모든 키에 컨트롤이 있어야 한다");
    assert_eq!(c.editable + c.multi_owner + c.internal + c.unimplemented, m.items.len());
    // 소유자가 둘 이상인 키는 설계 5-1 의 두 키뿐이고 읽기 전용이다.
    let mut multi: Vec<&str> = m.items.iter().filter(|i| matches!(i.control, Control::ReadOnly(ReadOnly::MultiOwner(_)))).map(|i| i.key.as_str()).collect();
    multi.sort();
    assert_eq!(multi, ["always_on_top", "mongttang_search"]);
    assert_eq!(c.multi_owner, multi.len());
    // 모든 최상위 키가 어느 항목에든 나타난다.
    for key in defaults.as_object().unwrap().keys() {
        assert!(m.items.iter().any(|i| i.key == *key || i.key.starts_with(&format!("{key}."))), "{key} 없음");
    }
    // 종류별 매핑(스키마 종류와 컨트롤이 어긋나지 않는다).
    for i in &m.items {
        match (&i.control, i.default.is_boolean()) {
            (Control::Check, b) => assert!(b, "{}", i.key),
            (Control::Number { .. } | Control::Text | Control::Choice(_) | Control::Color | Control::Font | Control::Folder, b) => assert!(!b, "{}", i.key),
            _ => {}
        }
    }
    let get = |k: &str| m.items[m.item_index(k).unwrap()].control.clone();
    assert_eq!(get("stdformat"), Control::Check);
    assert_eq!(get("retry_body"), Control::Number { integer: true, positive: false });
    assert_eq!(get("std_title_subtitle_pt"), Control::Number { integer: false, positive: true });
    assert_eq!(get("symbol_fonts.□.size"), Control::Number { integer: false, positive: true });
    assert_eq!(get("std_parspace_chapter"), Control::Number { integer: false, positive: false });
    assert!(matches!(get("search_engine"), Control::Choice(ref o) if o.len() == 2));
    assert_eq!(get("title_owner_text"), Control::Text);
    assert_eq!(get("label_symbols.□"), Control::Check);
    assert_eq!(get("color"), Control::Color);
    assert_eq!(get("hwp_font_folder"), Control::Folder);
    assert_eq!(get("paste_add_folder"), Control::Folder);
    for key in ["symbol_fonts.□.font", "symbol_fonts.ㅇ.font", "symbol_fonts.-.font", "symbol_fonts.※.font", "symbol_fonts.*.font", "symbol_fonts.•.font", "table_fonts.header.font", "table_fonts.body.font"] {
        assert_eq!(get(key), Control::Font, "{key}");
    }
    assert!(matches!(get("always_on_top"), Control::ReadOnly(ReadOnly::MultiOwner(_))));
    assert!(matches!(get("mongttang_search"), Control::ReadOnly(ReadOnly::MultiOwner(_))));
    assert!(matches!(get("theme_mode"), Control::ReadOnly(ReadOnly::Internal(_))));
    // 화면 방식 전환 키는 앱 설정 화면에 보이지 않는 내부 키다(Python EXCLUDED 와 같게).
    for key in ["settings_ui", "outliner_ui", "kclip_settings_mode"] {
        assert!(matches!(get(key), Control::ReadOnly(ReadOnly::Internal(_))), "{key}");
    }
}

#[test]
fn every_item_has_group_and_label() {
    let (_d, p) = temp_settings(None);
    let m = Model::load(&p).unwrap();
    for i in &m.items {
        assert!(i.group < GROUPS.len());
        assert!(!i.label.is_empty());
    }
    let total: usize = (0..GROUPS.len()).map(|g| m.group_count(g)).sum();
    assert_eq!(total, m.items.len());
}

#[test]
fn load_does_not_write_and_shows_defaults() {
    let (d, p) = temp_settings(None);
    let m = Model::load(&p).unwrap();
    assert!(!p.exists(), "읽기만 해야 한다");
    assert_eq!(m.current_text("retry_body"), "15");
    assert!(m.current_bool("stdformat"));
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
}

#[test]
fn validation_rejects_bad_values_with_reason_and_agrees_with_docfit_config() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    let cases = [
        ("retry_body", "abc"),
        ("retry_body", "1.5"),
        ("retry_body", "-1"),
        ("retry_body", "1001"),
        ("retry_body", ""),
        ("retry_body", "inf"),
        ("std_title_subtitle_pt", "0"),
        ("symbol_fonts.□.size", "0"),
        ("std_parspace_chapter", "nan"),
    ];
    for (key, text) in cases {
        m.set_text(key, text);
        let why = m.error_of(key).unwrap_or_else(|| panic!("{key}={text:?} 거부되어야 함"));
        assert!(!why.is_empty());
        assert!(docfit_config::validate_patch(&json!({ key: text })).is_err(), "{key}={text:?} docfit_config 도 거부해야 함");
        m.revert().unwrap();
    }
    for (key, text) in [("retry_body", "20"), ("std_parspace_chapter", "7.5"), ("std_title_subtitle_pt", "0.5"), ("retry_body", " 30 ")] {
        m.set_text(key, text);
        assert_eq!(m.error_of(key), None, "{key}={text:?}");
        assert!(docfit_config::validate_patch(&json!({ key: text.trim() })).is_ok());
    }
    m.set_text("title_owner_text", &"가".repeat(4097));
    assert!(m.error_of("title_owner_text").is_some());
    m.set_text("title_owner_text", "a\0b");
    assert!(m.error_of("title_owner_text").is_some());
    m.set_text("title_owner_text", "담당자 표시");
    assert_eq!(m.error_of("title_owner_text"), None);
}

#[test]
fn save_is_blocked_while_errors_exist() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.set_text("retry_body", "x");
    assert!(m.save().is_err());
    assert!(m.status.as_ref().unwrap().error);
    assert!(!p.exists(), "오류가 있으면 파일을 만들지 않는다");
}

#[test]
fn linespacing_min_must_not_exceed_max() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.set_text("linespacing_min", "250");
    assert!(m.error_of("linespacing_min").unwrap().contains("최소 줄 간격"));
    assert!(m.error_of("linespacing_max").is_some(), "짝 항목에도 같은 오류");
    assert!(m.save().is_err());
    m.set_text("linespacing_max", "300");
    assert_eq!(m.error_of("linespacing_min"), None);
    assert!(m.save().is_ok());
}

#[test]
fn save_roundtrip_preserves_unknown_keys_and_types() {
    let (_d, p) = temp_settings(Some(r#"{"custom_key":{"x":1},"retry_table":"9","hidden_tabs":[1,2]}"#));
    let mut m = Model::load(&p).unwrap();
    assert_eq!(m.current_text("retry_table"), "9");
    m.toggle("autoclose");
    m.set_text("retry_body", " 20 ");
    m.cycle("search_engine", 1);
    m.set_text("symbol_fonts.□.size", "18");
    m.toggle("label_symbols.□");
    assert_eq!(m.changed_count(), 5);
    assert_eq!(m.save(), Ok(5));
    assert_eq!(m.changed_count(), 0);
    let raw: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(raw["autoclose"], json!(true));
    assert_eq!(raw["retry_body"], json!("20")); // 숫자는 문자열로 저장
    assert_eq!(raw["search_engine"], json!("legacy"));
    assert_eq!(raw["symbol_fonts"]["□"]["size"], json!("18"));
    assert_eq!(raw["label_symbols"]["□"], json!(true));
    assert_eq!(raw["custom_key"], json!({"x": 1}), "모르는 키 보존");
    assert_eq!(raw["hidden_tabs"], json!([1, 2]));
    let m2 = Model::load(&p).unwrap();
    assert!(m2.current_bool("autoclose"));
    assert_eq!(m2.current_text("retry_body"), "20");
    assert_eq!(m2.current_text("search_engine"), "legacy");
    assert_eq!(m2.current_text("retry_table"), "9");
    assert!(
        std::fs::read_dir(p.parent().unwrap()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")),
        "임시 파일이 남지 않는다"
    );
}

#[test]
fn editing_back_to_saved_value_clears_change_and_revert_restores() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.toggle("autoclose");
    assert!(m.is_changed("autoclose"));
    m.toggle("autoclose");
    assert!(!m.is_changed("autoclose"));
    m.set_text("retry_body", "99");
    m.cycle("search_engine", 1);
    assert_eq!(m.changed_count(), 2);
    m.revert().unwrap();
    assert_eq!(m.changed_count(), 0);
    assert_eq!(m.current_text("retry_body"), "15");
    m.reset_to_default("retry_table");
    assert!(!m.is_changed("retry_table"), "기본값과 같으면 변경 아님");
    m.set_text("retry_table", "1");
    assert!(m.is_changed("retry_table"));
    m.reset_to_default("retry_table");
    assert!(!m.is_changed("retry_table"));
}

#[test]
fn step_stays_in_range() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    m.set_text("retry_body", "1000");
    m.step("retry_body", 1);
    assert_eq!(m.current_text("retry_body"), "1000");
    m.set_text("symbol_fonts.□.size", "1");
    m.step("symbol_fonts.□.size", -1);
    assert_eq!(m.current_text("symbol_fonts.□.size"), "1", "크기는 0보다 커야 한다");
    m.set_text("retry_body", "abc");
    m.step("retry_body", 1); // 저장 값(15)에서 시작
    assert_eq!(m.current_text("retry_body"), "16");
}

#[test]
fn revision_conflict_is_detected_and_edits_survive() {
    let (_d, p) = temp_settings(Some(r#"{"retry_table":"9"}"#));
    let mut a = Model::load(&p).unwrap();
    let mut b = Model::load(&p).unwrap();
    b.set_text("retry_table", "11");
    assert_eq!(b.save(), Ok(1));
    a.set_text("retry_body", "21");
    let err = a.save().unwrap_err();
    assert!(err.contains("다른 곳에서 설정이 바뀌었습니다"), "{err}");
    assert!(a.status.as_ref().unwrap().error);
    assert_eq!(a.changed_count(), 1, "충돌해도 편집은 남는다");
    let raw: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(raw["retry_table"], json!("11"));
    assert!(raw.get("retry_body").is_none(), "충돌 저장은 파일을 바꾸지 않는다");
    a.revert().unwrap();
    assert_eq!(a.current_text("retry_table"), "11");
    a.set_text("retry_body", "21");
    assert_eq!(a.save(), Ok(1));
}

#[test]
fn corrupt_file_is_an_error_not_a_silent_default() {
    let (_d, p) = temp_settings(Some("{ not json"));
    assert!(Model::load(&p).is_err());
}

#[test]
fn search_filter_two_chars_name_and_description() {
    let (_d, p) = temp_settings(None);
    let mut m = Model::load(&p).unwrap();
    let in_group = m.visible().len();
    assert_eq!(in_group, m.group_count(m.group));
    m.set_search("간");
    assert!(!m.search_active(), "1글자는 바로 찾지 않는다");
    assert_eq!(m.visible().len(), in_group);
    m.search_forced = true;
    assert!(m.search_active(), "Enter 로 1글자 확정");
    assert!(!m.visible().is_empty());
    m.set_search("간격");
    assert!(m.search_active());
    let names: Vec<String> = m.visible().iter().map(|i| m.items[*i].label.clone()).collect();
    assert!(names.iter().any(|n| n.contains("최소 줄 간격")), "{names:?}");
    // 안내문(설명)만 맞는 항목: "제목 표 모양" 은 std_title 의 안내 요약에만 있다.
    m.set_search("제목 표 모양");
    let keys: Vec<String> = m.visible().iter().map(|i| m.items[*i].key.clone()).collect();
    assert!(keys.contains(&"std_title".to_string()), "{keys:?}");
    m.set_search("없는검색어쿼리");
    assert!(m.visible().is_empty());
    m.set_search("굵게");
    let groups: std::collections::HashSet<usize> = m.visible().iter().map(|i| m.items[*i].group).collect();
    assert!(groups.len() >= 2, "묶음을 가로질러 찾는다");
    m.set_search("");
    assert!(!m.search_active());
    assert_eq!(m.visible().len(), in_group);
}

#[test]
fn layout_has_no_overlap_at_all_dpi_and_sizes() {
    for dpi in [96u32, 120, 144, 192] {
        for (w, h) in [(layout::DEFAULT_W, layout::DEFAULT_H), (layout::MIN_W, layout::MIN_H), (1400, 900)] {
            let (w, h) = (layout::scale(w, dpi), layout::scale(h, dpi));
            let l = layout::compute(w, h, dpi);
            let all = Rect::new(0, 0, w, h);
            for r in [l.header, l.tree, l.list, l.footer, l.search, l.save_btn, l.revert_btn] {
                assert!(r.inside(&all), "{r:?} 화면 밖 (dpi {dpi})");
            }
            assert!(!l.tree.overlaps(&l.list) && !l.header.overlaps(&l.tree) && !l.footer.overlaps(&l.list));
            assert!(!l.save_btn.overlaps(&l.revert_btn));
            assert!(l.search.inside(&l.header) && l.save_btn.inside(&l.footer) && l.revert_btn.inside(&l.footer));
            for i in 0..GROUPS.len() {
                let r = l.tree_row(i);
                assert!(r.inside(&l.tree), "묶음 {i} 가 트리 밖 (dpi {dpi}, {w}x{h})");
                if i > 0 {
                    assert!(!r.overlaps(&l.tree_row(i - 1)));
                }
            }
            for n in 0..3 {
                let row = l.row_rect(n, 0);
                assert!(!row.overlaps(&l.row_rect(n + 1, 0)));
                for shape in [Shape::Check, Shape::Number, Shape::Text, Shape::Choice, Shape::Color, Shape::Font, Shape::Folder, Shape::List, Shape::Hotkey, Shape::ReadOnly] {
                    let p = l.row_parts(row, shape);
                    for r in [p.dot, p.label, p.summary, p.extra, p.control, p.field] {
                        assert!(r.inside(&row), "{shape:?} {r:?} 가 줄 {row:?} 밖 (dpi {dpi})");
                    }
                    assert!(!p.label.overlaps(&p.summary) && !p.summary.overlaps(&p.extra) && !p.label.overlaps(&p.control));
                    assert!(!p.summary.overlaps(&p.control) && !p.dot.overlaps(&p.label));
                    assert!(p.label.width() > layout::scale(100, dpi), "글 영역이 너무 좁다: {} (dpi {dpi}, {w}x{h})", p.label.width());
                    if shape == Shape::Number {
                        assert!(p.minus.inside(&row) && p.plus.inside(&row));
                        assert!(!p.field.overlaps(&p.minus) && !p.minus.overlaps(&p.plus) && p.field.width() > layout::scale(40, dpi));
                    }
                    if matches!(shape, Shape::Font | Shape::Folder | Shape::List | Shape::Hotkey) {
                        assert!(p.plus.inside(&row) && p.field.inside(&row), "{shape:?} 단추/칸이 줄 밖 (dpi {dpi})");
                        assert!(!p.field.overlaps(&p.plus) && p.field.width() > layout::scale(100, dpi), "{shape:?} 입력칸이 좁다 (dpi {dpi}, {w}x{h})");
                    }
                    if shape == Shape::Color {
                        let sw: Vec<Rect> = (0..9).map(|i| l.swatch(p.control, i)).collect();
                        for (i, r) in sw.iter().enumerate() {
                            assert!(r.inside(&p.control) && r.width() > 0, "견본 {i} 가 컨트롤 밖 (dpi {dpi})");
                            if i > 0 {
                                assert!(!r.overlaps(&sw[i - 1]), "견본 겹침 (dpi {dpi})");
                            }
                        }
                    }
                    if shape == Shape::ReadOnly {
                        assert!(p.chip.inside(&row), "꼬리표가 줄 밖: {:?} {row:?}", p.chip);
                        // 꼬리표는 오른쪽 컨트롤 칸, 안내 글(extra)은 왼쪽 글 칸이라 가로로 겹치지 않는다.
                        assert!(p.chip.left >= p.extra.right, "꼬리표와 안내 글이 겹침 (dpi {dpi}, {w}x{h})");
                    }
                }
            }
        }
    }
}

#[test]
fn layout_hit_testing_and_scroll() {
    let l = layout::compute(1000, 680, 96);
    assert_eq!(l.hit(l.save_btn.left + 2, l.save_btn.top + 2, 0, 5), Hit::Save);
    assert_eq!(l.hit(l.revert_btn.left + 2, l.revert_btn.top + 2, 0, 5), Hit::Revert);
    assert_eq!(l.hit(l.search.left + 5, l.search.top + 5, 0, 5), Hit::Search);
    let t3 = l.tree_row(3);
    assert_eq!(l.hit(t3.left + 5, t3.top + 5, 0, 5), Hit::Tree(3));
    let r2 = l.row_rect(2, 0);
    assert_eq!(l.hit(r2.left + 5, r2.top + 5, 0, 5), Hit::Row(2));
    assert_eq!(l.hit(r2.left + 5, r2.top + 5, 0, 2), Hit::None);
    assert_eq!(l.max_scroll(3), 0);
    let max = l.max_scroll(34);
    assert!(max > 0);
    assert_eq!(l.clamp_scroll(10_000, 34), max);
    assert_eq!(l.clamp_scroll(-5, 34), 0);
    let s = l.scroll_to_show(33, 0, 34);
    assert!(s > 0 && l.row_rect(33, s).bottom <= l.list.bottom);
    let back = l.scroll_to_show(0, s, 34);
    assert_eq!(back, 0);
}

#[test]
fn overlay_panel_fits_inside_list_area_at_all_sizes() {
    for dpi in [96u32, 120, 144, 192] {
        for (w, h) in [(layout::DEFAULT_W, layout::DEFAULT_H), (layout::MIN_W, layout::MIN_H), (1400, 900)] {
            let (w, h) = (layout::scale(w, dpi), layout::scale(h, dpi));
            let l = layout::compute(w, h, dpi);
            let o = l.overlay();
            assert!(o.panel.inside(&l.list), "패널이 목록 영역 밖 (dpi {dpi}, {w}x{h})");
            for r in [o.title, o.input, o.add_btn, o.list, o.remove_btn, o.up_btn, o.down_btn, o.close_btn] {
                assert!(r.inside(&o.panel), "{r:?} 가 패널 밖 (dpi {dpi}, {w}x{h})");
            }
            assert!(!o.input.overlaps(&o.add_btn) && !o.list.overlaps(&o.input) && !o.list.overlaps(&o.close_btn));
            assert!(!o.remove_btn.overlaps(&o.up_btn) && !o.up_btn.overlaps(&o.down_btn) && !o.down_btn.overlaps(&o.close_btn));
            assert!(o.list.height() >= o.item_h * 3, "목록 줄이 3개도 안 보임 (dpi {dpi}, {w}x{h})");
            // 스크롤 계산
            assert_eq!(o.max_scroll(2), 0);
            let max = o.max_scroll(400);
            assert!(max > 0 && o.scroll_to_show(399, 0, 400) == max && o.scroll_to_show(0, max, 400) == 0);
        }
    }
}
