//! 페이지 저장소(`pages.rs`): 만들기·이름 바꾸기(링크 갱신)·삭제(.trash)·일지·최근. 임시 폴더만 쓴다.
mod common;
use common::*;
use outliner_win::pages::*;

fn store(tag: &str) -> (PageStore, std::path::PathBuf) {
    let d = temp_dir(tag);
    (PageStore::new(&d, Some(d.join("recent.json"))), d)
}

#[test]
fn create_makes_dash_file_and_rejects_duplicates_and_bad_names() {
    let (st, d) = store("create");
    assert_eq!(st.create("새 페이지").unwrap(), "새 페이지");
    assert_eq!(read(&d, "새 페이지.md"), "- \n");
    assert_eq!(st.create("새 페이지.md").unwrap_err(), "같은 이름의 페이지가 이미 있습니다");
    for bad in ["", "  ", ".", "..", "a/b", "a\\b", "a:b", "a?b", "a|b", ".숨김", "끝점.", "x\ny"] {
        assert!(st.create(bad).is_err(), "{bad:?} 는 거부돼야 한다");
    }
    assert!(st.create(&"가".repeat(101)).is_err());
    assert_eq!(st.list(), vec!["새 페이지".to_string()]);
}

#[test]
fn list_sorts_normal_pages_first_then_journals_newest_first_and_skips_subfolders() {
    let (st, d) = store("list");
    for n in ["나", "가", "2026_11_20", "2026_11_21", "2026-11-19", "Zeta"] {
        write(&d, &format!("{n}.md"), "- x\n");
    }
    std::fs::create_dir_all(d.join(".trash")).unwrap();
    write(&d.join(".trash"), "묻힘.20261121.md", "- x\n");
    write(&d, "메모.txt", "x");
    assert_eq!(st.list(), ["Zeta", "가", "나", "2026_11_21", "2026_11_20", "2026-11-19"]);
}

#[test]
fn ensure_page_fills_empty_file_and_creates_missing() {
    let (st, d) = store("ensure");
    write(&d, "빈.md", "");
    st.ensure_page("빈").unwrap();
    assert_eq!(read(&d, "빈.md"), "- \n");
    st.ensure_page("없던").unwrap();
    assert!(d.join("없던.md").is_file());
    write(&d, "있던.md", "- 내용\n");
    st.ensure_page("있던").unwrap();
    assert_eq!(read(&d, "있던.md"), "- 내용\n");
}

#[test]
fn rename_moves_file_and_updates_links_in_all_pages_but_not_code_or_other_names() {
    let (st, d) = store("rename");
    write(&d, "옛.md", "- 자기 자신 [[옛]] 링크\n");
    write(&d, "a.md", "- 참조 [[옛]] 와 [[옛 이름]] 그리고 `[[옛]]` 코드\n- [[옛]]\n");
    write(&d, "b.md", "- 관련 없음\n");
    let n = st.rename("옛", "새").unwrap();
    assert_eq!(n, 2, "링크를 고친 파일은 자기 자신 + a.md");
    assert!(!d.join("옛.md").exists());
    assert_eq!(read(&d, "새.md"), "- 자기 자신 [[새]] 링크\n");
    assert_eq!(read(&d, "a.md"), "- 참조 [[새]] 와 [[옛 이름]] 그리고 `[[옛]]` 코드\n- [[새]]\n");
    assert_eq!(read(&d, "b.md"), "- 관련 없음\n");
}

#[test]
fn rename_errors_leave_everything_untouched() {
    let (st, d) = store("rename_err");
    write(&d, "가.md", "- [[나]]\n");
    write(&d, "나.md", "- 본문\n");
    assert_eq!(st.rename("가", "나").unwrap_err(), "같은 이름의 페이지가 이미 있습니다");
    assert_eq!(st.rename("없음", "다").unwrap_err(), "페이지를 찾을 수 없습니다");
    assert_eq!(st.rename("가", "가").unwrap_err(), "같은 이름입니다");
    assert!(st.rename("가", "a/b").is_err());
    assert_eq!(read(&d, "가.md"), "- [[나]]\n");
    assert_eq!(read(&d, "나.md"), "- 본문\n");
}

#[test]
fn rename_only_changing_letter_case_is_allowed() {
    let (st, d) = store("rename_case");
    write(&d, "abc.md", "- 본문\n");
    st.rename("abc", "ABC").unwrap();
    let names: Vec<String> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(names.contains(&"ABC.md".to_string()), "{names:?}");
}

#[test]
fn delete_moves_to_trash_and_restore_brings_it_back() {
    let (st, d) = store("delete");
    write(&d, "지울.md", "- 소중한 내용\n");
    let moved = st.delete("지울", "20261121-101500").unwrap();
    assert!(!d.join("지울.md").exists());
    assert!(moved.starts_with(d.join(".trash")), "{moved:?}");
    assert_eq!(std::fs::read_to_string(&moved).unwrap(), "- 소중한 내용\n", "내용은 그대로 보존");
    assert!(st.list().is_empty(), ".trash 는 페이지 목록에 안 나온다");
    // 같은 이름을 같은 시각에 또 지워도 덮어쓰지 않는다.
    write(&d, "지울.md", "- 두 번째\n");
    let moved2 = st.delete("지울", "20261121-101500").unwrap();
    assert_ne!(moved, moved2);
    assert_eq!(st.list_trash().len(), 2);
    // 복원: 원래 이름이 비어 있을 때만.
    let first = moved.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(st.restore(&first).unwrap(), "지울");
    assert_eq!(read(&d, "지울.md"), "- 소중한 내용\n");
    let second = moved2.file_name().unwrap().to_string_lossy().into_owned();
    assert!(st.restore(&second).unwrap_err().contains("이미 있어"));
    assert!(st.restore("../x.md").is_err());
    assert!(st.delete("없음", "s").is_err());
}

#[test]
fn journal_today_creates_underscore_name_and_converts_flutter_dash_name() {
    let (st, d) = store("journal");
    assert_eq!(st.journal_today("2026-11-21").unwrap(), "2026_11_21");
    assert_eq!(read(&d, "2026_11_21.md"), "- \n");
    // Flutter 가 만든 옛 이름: 내용 보존 + 이름 정리 + 링크 갱신
    write(&d, "2026-11-22.md", "- 어제 한 일\n");
    write(&d, "메모.md", "- 보라 [[2026-11-22]]\n");
    assert_eq!(st.journal_today("2026-11-22").unwrap(), "2026_11_22");
    assert!(!d.join("2026-11-22.md").exists());
    assert_eq!(read(&d, "2026_11_22.md"), "- 어제 한 일\n");
    assert_eq!(read(&d, "메모.md"), "- 보라 [[2026_11_22]]\n");
    // 두 이름이 다 있으면 건드리지 않는다.
    write(&d, "2026-11-23.md", "- 옛\n");
    write(&d, "2026_11_23.md", "- 새\n");
    assert_eq!(st.normalize_journal("2026-11-23").unwrap(), "2026-11-23");
    assert!(d.join("2026-11-23.md").exists());
    assert!(st.journal_today("2026-13-40").is_err());
}

#[test]
fn recent_list_round_trips_filters_missing_pages_and_caps_at_20() {
    let (st, d) = store("recent");
    let names: Vec<String> = (0..25).map(|i| format!("p{i:02}")).collect();
    for n in &names {
        write(&d, &format!("{n}.md"), "- x\n");
    }
    st.save_recent(&names);
    let got = st.load_recent(&names);
    assert_eq!(got.len(), 20);
    assert_eq!(got[0], "p00");
    let some = vec!["p01".to_string(), "p02".to_string()];
    assert_eq!(st.load_recent(&some), some, "없는 페이지는 걸러진다");
    std::fs::write(d.join("recent.json"), "깨진 json").unwrap();
    assert!(st.load_recent(&names).is_empty());
    let nowhere = PageStore::new(&d, None);
    nowhere.save_recent(&names);
    assert!(nowhere.load_recent(&names).is_empty());
}
