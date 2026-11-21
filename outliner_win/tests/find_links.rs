//! 데이터 폴더 전체 검색·역링크(`find.rs`)와 `[[링크]]` 순수 함수(`links.rs`).
mod common;
use common::*;
use outliner_win::find::*;
use outliner_win::links::*;
use outliner_win::pages::PageStore;

fn demo() -> (PageStore, std::path::PathBuf) {
    let d = temp_dir("find");
    write(&d, "예산 지침.md", "- 2027년 예산 편성 지침\n  - 기한은 12월\n- TODO 개정안 검토\n");
    write(&d, "회의록.md", "- 예산 협의 회의\n  - [[예산 지침]] 변경 공유\n  - [[2026_11_21]] 일지에 기록\n");
    write(&d, "주간 업무.md", "- TODO [[예산 지침]] 회신 정리\n- [[예산 지침|지침]] 별칭 아님\n- [[예산 지침 초안]] 다른 페이지\n");
    write(&d, "2026_11_21.md", "- 오늘 [[예산지침]] 아님\n- 점심\n");
    write(&d, "참고.md", "- [[ 예산 지침 ]] 공백\n- [[ABC]] 대문자\n- [[abc]] 소문자\n");
    (PageStore::new(&d, None), d)
}

#[test]
fn search_all_finds_page_names_then_blocks_across_files() {
    let (st, _d) = demo();
    let hits = search_all(&st, "예산", None);
    // 페이지 이름 일치 1건(예산 지침)이 맨 앞.
    assert_eq!(hits[0], Hit { page: "예산 지침".into(), block: None, text: String::new() });
    let blocks: Vec<(&str, usize)> = hits.iter().filter_map(|h| h.block.map(|b| (h.page.as_str(), b))).collect();
    assert!(blocks.contains(&("예산 지침", 0)), "{blocks:?}");
    assert!(blocks.contains(&("회의록", 0)));
    assert!(blocks.contains(&("회의록", 1)));
    assert!(blocks.contains(&("주간 업무", 0)));
    // 블록 번호는 그 페이지 문서 안 순서 — 결과 본문이 실제 블록 글과 같다.
    let h = hits.iter().find(|h| h.page == "회의록" && h.block == Some(1)).unwrap();
    assert_eq!(h.text, "[[예산 지침]] 변경 공유");
}

#[test]
fn search_requires_two_chars_and_ignores_case() {
    let (st, _d) = demo();
    assert!(search_all(&st, "", None).is_empty());
    assert!(search_all(&st, "예", None).is_empty());
    assert!(search_all(&st, " 가 ", None).is_empty(), "공백을 뗀 글자 수로 센다");
    assert_eq!(search_all(&st, "abc", None).iter().filter(|h| h.block.is_some()).count(), 2, "대소문자 무시");
    assert!(search_all(&st, "없는말없는말", None).is_empty());
}

#[test]
fn search_uses_in_memory_text_for_the_current_page() {
    let (st, d) = demo();
    let mem = "- 메모리에만 있는 유일한낱말\n";
    let hits = search_all(&st, "유일한낱말", Some(("회의록", mem)));
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].page.as_str(), hits[0].block), ("회의록", Some(0)));
    assert!(read(&d, "회의록.md").contains("예산 협의"), "파일은 그대로");
    assert!(search_all(&st, "유일한낱말", None).is_empty());
}

#[test]
fn backlinks_all_collects_blocks_from_several_files_excluding_the_page_itself() {
    let (st, d) = demo();
    // 자기 자신을 가리키는 링크는 제외
    write(&d, "예산 지침.md", "- 자기 [[예산 지침]]\n  - 기한\n");
    let b = backlinks_all(&st, "예산 지침", None);
    let got: Vec<(&str, usize)> = b.iter().map(|x| (x.page.as_str(), x.block)).collect();
    assert_eq!(got, [("주간 업무", 0), ("참고", 0), ("회의록", 1)], "페이지 정렬 순: 주간 업무·참고·회의록. 별칭·다른 이름은 제외");
    assert_eq!(b[2].text, "[[예산 지침]] 변경 공유");
}

#[test]
fn backlinks_match_case_insensitively_and_journal_name_variants() {
    let (st, _d) = demo();
    let b = backlinks_all(&st, "abc", None);
    assert_eq!(b.iter().map(|x| x.block).collect::<Vec<_>>(), [1, 2], "ABC·abc 둘 다");
    let j = backlinks_all(&st, "2026-11-21", None);
    assert_eq!(j.len(), 1, "YYYY-MM-DD 로 물어도 [[2026_11_21]] 링크를 찾는다");
    assert_eq!(j[0].page, "회의록");
}

#[test]
fn backlinks_use_unsaved_current_page_text() {
    let (st, _d) = demo();
    let mem = "- 새로 쓴 [[업무 매뉴얼]] 링크\n";
    let b = backlinks_all(&st, "업무 매뉴얼", Some(("회의록", mem)));
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].page, "회의록");
    assert!(backlinks_all(&st, "업무 매뉴얼", None).is_empty());
}

// ---------- links.rs ----------

#[test]
fn link_context_detects_open_brackets_before_caret() {
    let c = link_context("오늘 [[예산", 7).unwrap();
    assert_eq!((c.start16, c.query.as_str(), c.caret16, c.end16), (3, "예산", 7, 7));
    assert_eq!(link_context("[[", 2).unwrap().query, "");
    assert!(link_context("[[예산]] 뒤", 8).is_none(), "이미 닫힌 링크 뒤");
    assert!(link_context("그냥 글", 3).is_none());
    assert!(link_context("[[예산", 1).is_none(), "캐럿이 [[ 앞쪽");
    assert!(link_context(&format!("[[{}", "가".repeat(61)), 63).is_none(), "너무 긴 질의");
    // 캐럿 바로 뒤의 ]] 는 치환 범위에 포함
    let c = link_context("[[예산]]", 4).unwrap();
    assert_eq!((c.query.as_str(), c.end16), ("예산", 6));
    // 서로게이트(이모지)가 앞에 있어도 UTF-16 위치가 맞다
    let t = "😀 [[예";
    let c = link_context(t, t.encode_utf16().count()).unwrap();
    assert_eq!(c.start16, 3);
}

fn pages() -> Vec<String> {
    ["회의록", "예산 지침", "예산 지침 초안", "참고 예산", "Alpha"].iter().map(|s| s.to_string()).collect()
}

#[test]
fn candidates_rank_prefix_matches_before_contains_and_cap_the_list() {
    let p = pages();
    assert_eq!(candidates(&p, &[], "예산", 8), ["예산 지침", "예산 지침 초안", "참고 예산"]);
    assert_eq!(candidates(&p, &[], "ALP", 8), ["Alpha"], "대소문자 무시");
    assert_eq!(candidates(&p, &[], "예산", 2).len(), 2);
    assert!(candidates(&p, &[], "없음", 8).is_empty());
    // 질의가 비면 최근 페이지가 먼저, 중복 없이
    let recent = vec!["참고 예산".to_string(), "없는 페이지".to_string()];
    let c = candidates(&p, &recent, "", 8);
    assert_eq!(c[0], "참고 예산");
    assert_eq!(c.len(), 5);
    assert!(!c.contains(&"없는 페이지".to_string()));
}

#[test]
fn apply_completion_replaces_query_closes_brackets_and_places_caret_after() {
    let t = "일정 [[예산 뒤";
    let c = link_context(t, 7).unwrap();
    let (new, caret) = apply_completion(t, &c, "예산 지침");
    assert_eq!(new, "일정 [[예산 지침]] 뒤");
    assert_eq!(caret, "일정 [[예산 지침]]".encode_utf16().count());
    // 이미 ]] 가 있으면 두 번 붙이지 않는다
    let t = "[[예산]] 끝";
    let c = link_context(t, 4).unwrap();
    assert_eq!(apply_completion(t, &c, "예산 지침").0, "[[예산 지침]] 끝");
    // 이모지 뒤에서도
    let t = "😀 [[예";
    let c = link_context(t, t.encode_utf16().count()).unwrap();
    assert_eq!(apply_completion(t, &c, "예산").0, "😀 [[예산]]");
}

#[test]
fn segments_and_link_at_split_text_into_plain_and_link_parts() {
    let segs = segments("앞 [[예산 지침]] 중간 [[ 회의록 ]] 끝 [[]] [[열림");
    let kinds: Vec<(&str, Option<&str>)> = segs.iter().map(|s| (s.text.as_str(), s.link.as_deref())).collect();
    assert_eq!(
        kinds,
        [("앞 ", None), ("[[예산 지침]]", Some("예산 지침")), (" 중간 ", None), ("[[ 회의록 ]]", Some("회의록")), (" 끝 [[]] [[열림", None)]
    );
    assert_eq!(segments("링크 없음").len(), 1);
    assert_eq!(segments("").len(), 0);
    let t = "앞 [[예산]] 뒤";
    assert_eq!(link_at(t, 2).as_deref(), Some("예산"), "[[ 의 첫 글자");
    assert_eq!(link_at(t, 7).as_deref(), Some("예산"), "]] 의 둘째");
    assert_eq!(link_at(t, 8), None);
    assert_eq!(link_at(t, 0), None);
    let e = "😀[[예]]";
    assert_eq!(link_at(e, 2).as_deref(), Some("예"));
    assert_eq!(link_at(e, 1), None, "서로게이트 둘째 조각은 링크 밖");
}
