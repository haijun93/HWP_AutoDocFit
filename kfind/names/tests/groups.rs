//! QMS2PIPE1: `;` 묶음 교집합(파일명). 합성 레코드만 쓴다(이 PC 경로·사용자 파일 없음).
use std::collections::BTreeSet;

use kfind_names::query::{hits, parse_groups, search};
use kfind_names::source::ChangeKind;
use kfind_names::{Builder, Change, Names, Options, Query, Searcher, Sort, parse};

fn build() -> Names {
    let mut b = Builder::new();
    let v = b.add_volume("C:", 5);
    b.push(v, 10, 5, "업무", 0x10);
    b.push(v, 11, 10, "2026", 0x10);
    b.push(v, 12, 5, "자료", 0x10);
    b.push(v, 20, 11, "2026 공사 계약서.hwp", 0);
    b.push(v, 21, 11, "2026 공사 설계.pdf", 0);
    b.push(v, 22, 11, "2025 공사 계약서.hwp", 0);
    b.push(v, 23, 11, "예산 집행 계획.hwp", 0);
    b.push(v, 24, 12, "2026 공사 계약서 사본.hwp", 0);
    b.push(v, 25, 12, "Report 2026 Final.pdf", 0);
    b.push(v, 26, 12, "report draft.pdf", 0);
    b.push(v, 27, 12, "2026 보고서 Report.hwp", 0);
    b.push(v, 28, 12, "무관.txt", 0);
    b.finish()
}

fn all(n: &Names, qs: &[Query], opts: &Options) -> (usize, Vec<String>) {
    let v = n.view();
    let mut s = Searcher::new();
    let r = s.search_groups(&v, n.generation(), qs, opts).unwrap();
    (r.total, hits(&v, &r.top).into_iter().map(|h| format!("{}|{}", h.dir, h.name)).collect())
}

/// 기준: 묶음마다 따로 전부 찾아 이름 집합을 교집합.
fn oracle(n: &Names, qs: &[Query]) -> BTreeSet<String> {
    let v = n.view();
    let o = Options { limit: 100_000, ..Default::default() };
    let mut acc: Option<BTreeSet<String>> = None;
    for q in qs {
        let r = search(&v, q, &o).unwrap();
        let set: BTreeSet<String> = hits(&v, &r.top).into_iter().map(|h| format!("{}|{}", h.dir, h.name)).collect();
        acc = Some(match acc {
            None => set,
            Some(a) => a.intersection(&set).cloned().collect(),
        });
    }
    acc.unwrap_or_default()
}

fn check(n: &Names, text: &str) -> usize {
    let qs = parse_groups(text);
    let (total, got) = all(n, &qs, &Options { limit: 100_000, ..Default::default() });
    let want = oracle(n, &qs);
    assert_eq!(total, want.len(), "{text}");
    assert_eq!(got.iter().cloned().collect::<BTreeSet<_>>(), want, "{text}");
    total
}

#[test]
fn one_group_identical_to_plain_search() {
    let n = build();
    let v = n.view();
    for t in ["2026", "공사 계약", "report", "ext:hwp;pdf 2026", "path:업무 -예산", "folder:", "", "ext:pdf"] {
        for sort in [Sort::Relevance, Sort::Name, Sort::Path] {
            let o = Options { limit: 3, sort, ..Default::default() };
            let a = search(&v, &parse(t), &o).unwrap();
            let mut s = Searcher::new();
            let b = s.search_groups(&v, n.generation(), &parse_groups(t), &o).unwrap();
            assert_eq!((a.total, a.top), (b.total, b.top), "{t} {sort:?}");
        }
    }
}

#[test]
fn two_and_three_groups_intersect() {
    let n = build();
    assert_eq!(check(&n, "2026; 공사 계약"), 2); // 업무\2026 + 자료 의 "2026 공사 계약서(.hwp/사본)"
    assert_eq!(check(&n, "2026; 공사; 계약"), 2);
    assert_eq!(check(&n, "2026; 공사; ext:pdf"), 1);
    assert_eq!(check(&n, "2026; 공사; 계약; 사본"), 1);
    assert_eq!(check(&n, "공사; 예산"), 0);
    assert_eq!(check(&n, "2026;2025"), 0);
}

#[test]
fn path_and_exclusion_conditions_apply_per_group() {
    let n = build();
    // 두 번째 묶음의 path: 조건이 후보 확인에서도 지켜져야 한다.
    assert_eq!(check(&n, "공사; path:업무"), 3);
    assert_eq!(check(&n, "공사; path:자료"), 1);
    assert_eq!(check(&n, "path:자료; 공사"), 1);
    assert_eq!(check(&n, "공사; -2025"), 3);
    assert_eq!(check(&n, "계약; folder:"), 0);
}

#[test]
fn empty_groups_are_ignored_and_all_empty_is_plain() {
    let n = build();
    assert_eq!(check(&n, "공사;"), check(&n, "공사"));
    assert_eq!(check(&n, ";공사;;"), check(&n, "공사"));
    assert_eq!(parse_groups(" ; ;").len(), 1);
    assert!(parse_groups(" ; ;")[0].is_empty());
}

#[test]
fn korean_english_mixed_and_ext_semicolon_kept() {
    let n = build();
    assert_eq!(check(&n, "report; 2026"), 2);
    assert_eq!(check(&n, "REPORT;보고서"), 1);
    // ext: 안의 ; 는 묶음 구분이 아니다.
    assert_eq!(check(&n, "2026 ext:hwp;pdf"), 5);
    assert_eq!(parse_groups("2026 ext:hwp;pdf").len(), 1);
    assert_eq!(check(&n, "2026 ext:hwp;pdf; 공사"), 3);
}

#[test]
fn appended_entries_are_checked_with_all_conditions() {
    let mut n = build();
    n.apply(&Change { vol: 0, rec: 40, parent: 10, name: "2026 공사 추가.hwp".into(), attrs: 0, kind: ChangeKind::Upsert, time: 1 });
    n.apply(&Change { vol: 0, rec: 41, parent: 12, name: "2026 공사 추가본.hwp".into(), attrs: 0, kind: ChangeKind::Upsert, time: 1 });
    assert_eq!(check(&n, "2026; 공사 추가"), 2);
    assert_eq!(check(&n, "추가; path:업무"), 1);
    assert_eq!(check(&n, "추가; path:자료"), 1);
    assert_eq!(check(&n, "공사; path:업무"), 4);
}

#[test]
fn old_json_request_text_without_separator_is_unchanged() {
    // 구버전 요청에는 `;` 묶음 개념이 없다. `;` 없는 text 는 한 묶음으로 예전과 같은 결과.
    let n = build();
    let v = n.view();
    for t in ["공사 계약", "2026 ext:hwp"] {
        let a = search(&v, &parse(t), &Options::default()).unwrap();
        let mut s = Searcher::new();
        let b = s.search_groups(&v, n.generation(), &parse_groups(t), &Options::default()).unwrap();
        assert_eq!((a.total, a.top), (b.total, b.top));
    }
}
