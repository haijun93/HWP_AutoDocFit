//! MG2(Q-F2·Q-F3): 크기·날짜 순(모르는 값은 맨 뒤), 글자 그대로 일치 우선, names.seg 왕복·USN 시각 반영.
use kfind_names::query::{hits, search};
use kfind_names::source::ChangeKind;
use kfind_names::{Builder, Change, NamesSeg, Options, Sort, parse, seg};

const U: u64 = u64::MAX;
const T: i64 = i64::MIN;

fn build() -> kfind_names::Names {
    let mut b = Builder::new();
    let v = b.add_volume("C:", 5);
    b.push_meta(v, 10, 5, "자료", 0x10, U, 300);
    b.push_meta(v, 11, 10, "예산_집행.hwp", 0, 500, 100);
    b.push_meta(v, 12, 10, "예산집행.hwp", 0, 9000, 400);
    b.push_meta(v, 13, 10, "예산 집행 계획.pdf", 0, 20, 200);
    b.push_meta(v, 14, 10, "예산집행_모름.txt", 0, U, T);
    b.push(v, 15, 10, "예산집행-옛.hwp", 0);
    b.finish()
}

fn names(v: &kfind_names::View, text: &str, sort: Sort) -> Vec<String> {
    let q = parse(text);
    let r = search(v, &q, &Options { limit: 100, sort, ..Default::default() }).unwrap();
    hits(v, &r.top).into_iter().map(|h| h.name).collect()
}

#[test]
fn size_and_date_order_unknown_last() {
    let n = build();
    let v = n.view();
    assert_eq!(
        names(&v, "예산집행", Sort::Size),
        ["예산집행.hwp", "예산_집행.hwp", "예산 집행 계획.pdf", "예산집행_모름.txt", "예산집행-옛.hwp"]
    );
    assert_eq!(
        names(&v, "예산집행", Sort::Date),
        ["예산집행.hwp", "예산 집행 계획.pdf", "예산_집행.hwp", "예산집행_모름.txt", "예산집행-옛.hwp"]
    );
    let q = parse("예산_집행");
    let r = search(&v, &q, &Options { limit: 1, sort: Sort::Size, ..Default::default() }).unwrap();
    let h = &hits(&v, &r.top)[0];
    assert_eq!((h.size, h.mtime), (Some(9000), Some(400)));
    let r = search(&v, &parse("모름"), &Options::default()).unwrap();
    let h = &hits(&v, &r.top)[0];
    assert_eq!((h.size, h.mtime), (None, None));
    assert_eq!(Sort::parse("size"), Some(Sort::Size));
    assert_eq!(Sort::parse("date"), Some(Sort::Date));
    assert_eq!(Sort::parse("x"), None);
}

#[test]
fn literal_match_first_but_broad_set_unchanged() {
    let n = build();
    let v = n.view();
    let got = names(&v, "예산_집행", Sort::Relevance);
    // 넓은 일치(구분 기호 무시)는 그대로 5건, 그중 글자 그대로 "예산_집행"이 든 이름이 맨 앞.
    assert_eq!(got.len(), 5);
    assert_eq!(got[0], "예산_집행.hwp");
    assert!(parse("예산_집행").literal == ["예산_집행"]);
    // 구분 기호가 없는 낱말은 글자 그대로 비교가 필요 없다(관련도 순서 그대로).
    assert!(parse("예산집행").literal.is_empty());
    assert!(parse("Report").literal.is_empty());
    // 대소문자는 무시.
    let mut b = Builder::new();
    let vol = b.add_volume("C:", 5);
    b.push(vol, 20, 5, "a_b_FINAL.txt", 0);
    b.push(vol, 21, 5, "ab final.txt", 0);
    b.push(vol, 22, 5, "AB_Final.txt", 0);
    let n2 = b.finish();
    let got = names(&n2.view(), "b_final", Sort::Relevance);
    assert_eq!(got.len(), 3);
    assert_eq!(got[..2].iter().filter(|s| s.to_lowercase().contains("b_final")).count(), 2);
    assert_eq!(got[2], "ab final.txt");
}

#[test]
fn segment_round_trip_and_usn_time() {
    let mut n = build();
    let dir = std::env::temp_dir().join(format!("fn-meta-{}-{}", std::process::id(), n.len()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("names.seg");
    seg::write(&n, &p).unwrap();
    let s = NamesSeg::open(&p, true).unwrap();
    assert_eq!(names(&s.view(), "예산집행", Sort::Size), names(&n.view(), "예산집행", Sort::Size));
    assert_eq!(names(&s.view(), "예산집행", Sort::Date), names(&n.view(), "예산집행", Sort::Date));
    drop(s);
    // USN: 이름 바꾸기는 크기를 이어받고 시각은 기록 시각으로.
    assert!(n.apply(&Change { vol: 0, rec: 11, parent: 10, name: "예산_집행_최종.hwp".into(), attrs: 0, kind: ChangeKind::Upsert, time: 999 }));
    let v = n.view();
    let r = search(&v, &parse("최종"), &Options::default()).unwrap();
    let h = &hits(&v, &r.top)[0];
    assert_eq!((h.size, h.mtime), (Some(500), Some(999)));
    // 시각 0(모름)은 옛 시각 유지.
    assert!(!n.apply(&Change { vol: 0, rec: 11, parent: 10, name: "예산_집행_최종.hwp".into(), attrs: 0, kind: ChangeKind::Upsert, time: 0 }));
    // 다시 만들기(compact)도 값을 지닌다.
    let c = n.compact();
    let v = c.view();
    let r = search(&v, &parse("최종"), &Options::default()).unwrap();
    let h = &hits(&v, &r.top)[0];
    assert_eq!((h.size, h.mtime), (Some(500), Some(999)));
    let _ = std::fs::remove_dir_all(dir);
}
