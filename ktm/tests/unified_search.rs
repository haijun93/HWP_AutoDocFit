#[path = "../examples/unified_support/mod.rs"]
mod support;
use ktm::backend::unified;
use ktm::tm_state::ResultKind;
use std::sync::atomic::AtomicBool;

#[test]
fn seeded_thousand_documents_have_exact_tiers_order_identity_and_evidence() {
    let root = std::env::temp_dir().join(format!(
        "unified1-corpus-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let corpus = support::Corpus::create(&root).unwrap();
    assert_eq!(corpus.paths.len(), 1000);
    assert_eq!(
        std::fs::read_dir(root.join("documents")).unwrap().count(),
        1000
    );
    let mut backend = corpus.backend().unwrap();
    for query in [
        "예산집행",
        "2026; 공사 계약",
        "2026; 공사 계약; 없는검색어",
        "없는검색어",
    ] {
        let mut snapshots = Vec::new();
        unified::stream(
            80,
            &AtomicBool::new(false),
            |kind, top| corpus.fetch(&mut backend, query, kind, top),
            |snapshot| snapshots.push(snapshot),
        )
        .unwrap();
        assert!(snapshots.iter().all(|s| s.errors.is_empty()));
        let rows = &snapshots.last().unwrap().answer.rows;
        let ids: Vec<_> = rows.iter().map(|r| corpus.id_for(r)).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "{query}: duplicate documents");
        assert!(rows.iter().all(|r| r.match_kinds.len() == 1));
        match query {
            "예산집행" => {
                // Independent, hand-computable oracle: names are lexical 230..259,
                // 480..509; hierarchy ranks all level-0 docs by decreasing date;
                // relevance puts adjacent level-8 docs ahead of separated headings.
                // Hidden higher-tier candidates must not leak back into lower tiers.
                let expected: Vec<u64> = (230..=249)
                    .chain((210..=229).rev())
                    .chain((440..=479).rev())
                    .collect();
                assert_eq!(ids, expected);
                assert_eq!(
                    rows.iter()
                        .filter(|r| r.match_kinds == [ResultKind::Names])
                        .count(),
                    20
                );
                assert_eq!(
                    rows.iter()
                        .filter(|r| r.match_kinds == [ResultKind::Hierarchy])
                        .count(),
                    20
                );
                assert_eq!(
                    rows.iter()
                        .filter(|r| r.match_kinds == [ResultKind::Body])
                        .count(),
                    40
                );
                assert!(
                    rows[20..]
                        .iter()
                        .all(|r| !r.location.is_empty() && r.para.is_some())
                );
                // Source prefetch can be suppressed by higher tiers; a location
                // remains evidence until existing lazy snippet loading runs.
                assert!(rows[20..].iter().all(|r| {
                    !ktm::view_dump::result_row(r, &[], &r.match_kinds, true)
                        .excerpt
                        .is_empty()
                }));
                assert!(
                    snapshots[0]
                        .answer
                        .rows
                        .iter()
                        .all(|r| r.match_kinds == [ResultKind::Names])
                );
            }
            "2026; 공사 계약" => {
                // Both groups occur in separate paragraphs; 905/906 have only one
                // group and must never enter the document intersection.
                assert_eq!(ids, [901, 904, 903, 902]);
                assert_eq!(rows[0].match_kinds, [ResultKind::Names]);
                assert!(
                    rows[1..]
                        .iter()
                        .all(|r| r.match_kinds == [ResultKind::Hierarchy])
                );
                assert_eq!(snapshots[2].answer.groups, ["2026", "공사 계약"]);
            }
            _ => assert!(rows.is_empty(), "{query}"),
        }
    }
    drop(backend);
    drop(corpus);
    std::fs::remove_dir_all(root).unwrap();
}
