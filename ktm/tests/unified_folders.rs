//! MERGE120: 통합 검색의 파일명 질의를 실제 파일명 엔진(kfind-names)에 합성 항목으로 확인한다.
use kfind_names::Builder;
use kfind_names::query::{Options, Searcher, hits, parse_groups, search_related};
use ktm::backend::unified::{folder_names_query, names_query};

#[test]
fn folder_and_file_queries_split_the_matches_and_related_input_needs_no_operators() {
    let mut builder = Builder::new();
    let vol = builder.add_volume("Z:\\synthetic", 5);
    builder.push_meta(vol, 10, 5, "2026 공사 계약 v1.2", 0x10, 0, 0);
    builder.push_meta(vol, 11, 5, "2026 공사 계약.hwpx", 0, 32, 0);
    let names = builder.finish();
    let view = names.view();
    let run = |text: &str| -> Vec<(String, bool)> {
        let found = Searcher::new()
            .search_groups(&view, names.generation(), &parse_groups(text),
                &Options { limit: 10, ..Options::default() })
            .unwrap();
        hits(&view, &found.top).into_iter().map(|h| (h.name, h.is_dir)).collect()
    };

    let folders = folder_names_query("공사 계약", None, None).unwrap();
    assert_eq!(run(&folders), [("2026 공사 계약 v1.2".to_owned(), true)]);
    let (files, _) = names_query("공사 계약", None, None, None);
    assert_eq!(run(&files), [("2026 공사 계약.hwpx".to_owned(), false)]);

    // 연관 검색의 서비스 문법은 `file:`을 낱말로 읽는다. 그래서 호스트는 입력을 그대로 보낸다.
    let related = |text: &str| {
        search_related(&view, text, &Options { limit: 10, ..Options::default() }).unwrap().total
    };
    assert_eq!(related("2026; 공사 계약"), 2);
    let (with_operators, _) = names_query("2026; 공사 계약", None, None, None);
    assert_eq!(related(&with_operators), 0);
}
