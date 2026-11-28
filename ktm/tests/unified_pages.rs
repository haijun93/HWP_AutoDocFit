//! 전수 최고 단 배정 및 이어보기. 설치본·사용자 문서 없이 실제 합성 색인을 사용한다.
use ktm::backend::{
    self, Answer, Backend, Row,
    unified::{self, Cursor, Session},
};
use ktm::tm_state::{MatchMode, ResultKind};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "../examples/unified_support/mod.rs"]
mod support;

fn answer(ids: impl IntoIterator<Item = u64>) -> Answer {
    Answer {
        rows: ids
            .into_iter()
            .map(|id| Row {
                doc_id: id,
                path: format!("Z:/synthetic/{id}.txt"),
                ..Row::default()
            })
            .collect(),
        ..Answer::default()
    }
}

#[test]
fn global_tiers_and_every_page_match_independent_exhaustive_oracle() {
    let inputs = [
        (ResultKind::Body, (1..=4301).rev().collect::<Vec<_>>()),
        (
            ResultKind::Names,
            (1..=1403).filter(|id| id % 2 == 1).collect(),
        ),
        (
            ResultKind::Hierarchy,
            (1..=3107).filter(|id| id % 3 != 0).rev().collect(),
        ),
    ];
    let mut expected = HashMap::new();
    for id in 1..=4301 {
        expected.insert(
            id,
            if id <= 1403 && id % 2 == 1 {
                ResultKind::Names
            } else if id <= 3107 && id % 3 != 0 {
                ResultKind::Hierarchy
            } else {
                ResultKind::Body
            },
        );
    }
    let session = Session::from_complete(
        inputs
            .iter()
            .map(|(kind, ids)| {
                let mut batch = answer(ids.clone());
                if *kind == ResultKind::Body {
                    batch.folder_counts = vec![("synthetic".into(), 4301)];
                    batch.autocompletes = vec!["보편검색".into()];
                    batch.groups = vec!["묶음".into()];
                    batch.terms = vec!["강조".into()];
                }
                (*kind, batch)
            })
            .collect(),
    );
    for limit in [1, 7, 20, 39, 80, 200] {
        let mut cursor = Cursor::default();
        let mut actual = HashMap::new();
        let mut ordered: [Vec<u64>; 3] = Default::default();
        loop {
            let page = session.page(cursor, limit).unwrap();
            assert_eq!(page.answer.folder_counts, [("synthetic".into(), 4301)]);
            assert_eq!(page.answer.autocompletes, ["보편검색"]);
            assert_eq!(page.answer.groups, ["묶음"]);
            assert_eq!(page.answer.terms, ["강조"]);
            assert!(!page.answer.rows.is_empty());
            assert!(page.answer.rows.len() <= limit.min(80));
            for row in page.answer.rows {
                assert_eq!(row.match_kinds, [expected[&row.doc_id]]);
                assert!(actual.insert(row.doc_id, row.match_kinds[0]).is_none());
                let i = match row.match_kinds[0] {
                    ResultKind::Names => 0,
                    ResultKind::Hierarchy => 1,
                    _ => 2,
                };
                ordered[i].push(row.doc_id);
            }
            let Some(next) = page.next else { break };
            assert_ne!(cursor, next);
            cursor = next;
        }
        assert_eq!(actual, expected);
        for (i, kind) in [ResultKind::Names, ResultKind::Hierarchy, ResultKind::Body]
            .into_iter()
            .enumerate()
        {
            let original = &inputs.iter().find(|(k, _)| *k == kind).unwrap().1;
            assert_eq!(
                ordered[i],
                original
                    .iter()
                    .copied()
                    .filter(|id| expected[id] == kind)
                    .collect::<Vec<_>>()
            );
        }
    }
    assert!(session.page(Cursor::default(), 0).is_err());
}

#[test]
fn complete_names_protocol_rejects_silent_truncation_and_missing_capability() {
    for value in [
        serde_json::json!({"ok":true,"result":{"items":[]}}),
        serde_json::json!({"ok":true,"result":{"complete":true,"total":201,"items":[]}}),
    ] {
        assert!(
            backend::decode_names_snapshot(&serde_json::to_vec(&value).unwrap(), "예산", false)
                .is_err()
        );
    }
    assert!(
        backend::decode_names_snapshot(
            br#"{"ok":true,"result":{"complete":true,"total":0,"items":[]}}"#,
            "예산",
            false
        )
        .is_ok()
    );
}

#[test]
fn stream_publishes_small_names_first_and_stops_on_unproved_higher_tier_or_cancel() {
    let cancel = AtomicBool::new(false);
    let mut calls = Vec::new();
    let mut snapshots = Vec::new();
    let session = unified::collect(
        80,
        &cancel,
        |kind, top| {
            calls.push((kind, top));
            if kind == ResultKind::Hierarchy {
                return Err("synthetic failure".into());
            }
            let mut batch = answer(1..=if top == 0 { 1201 } else { top as u64 });
            batch.folder_counts = vec![("synthetic".into(), 1201)];
            batch.autocompletes = vec!["보편검색".into()];
            batch.groups = vec!["묶음".into()];
            batch.terms = vec!["강조".into()];
            Ok(batch)
        },
        |s| snapshots.push(s),
    )
    .unwrap();
    assert_eq!(
        calls,
        [
            (ResultKind::Names, 20),
            (ResultKind::Names, 0),
            (ResultKind::Hierarchy, 0)
        ]
    );
    assert_eq!(snapshots[0].answer.rows.len(), 20);
    assert!(snapshots.last().unwrap().final_part);
    for snapshot in &snapshots {
        assert_eq!(snapshot.answer.folder_counts, [("synthetic".into(), 1201)]);
        assert_eq!(snapshot.answer.autocompletes, ["보편검색"]);
        assert_eq!(snapshot.answer.groups, ["묶음"]);
        assert_eq!(snapshot.answer.terms, ["강조"]);
    }
    assert_eq!(session.totals(), [1201, 0, 0]);
    assert_eq!(session.errors.len(), 1);
    assert_eq!(session.state, unified::SessionState::Incomplete);
    let mut published = 0;
    assert!(
        unified::collect(
            80,
            &cancel,
            |_, _| {
                cancel.store(true, Ordering::Relaxed);
                Ok(Answer::default())
            },
            |_| published += 1
        )
        .is_err()
    );
    assert_eq!(published, 0);
    cancel.store(false, Ordering::Relaxed);
    let mut calls = 0;
    assert!(
        unified::collect(
            80,
            &cancel,
            |_, _| {
                calls += 1;
                Ok(answer(1..=20))
            },
            |_| cancel.store(true, Ordering::Relaxed)
        )
        .is_err()
    );
    assert_eq!(calls, 1);
    let mut calls = Vec::new();
    let session = unified::collect(
        80,
        &AtomicBool::new(false),
        |kind, top| {
            calls.push((kind, top));
            if top == 0 {
                Err("missing snapshot capability".into())
            } else {
                Ok(answer(1..=20))
            }
        },
        |_| {},
    )
    .unwrap();
    assert_eq!(session.totals(), [20, 0, 0]);
    assert_eq!(calls, [(ResultKind::Names, 20), (ResultKind::Names, 0)]);
}

#[test]
fn full_snapshot_path_preserves_the_four_setting_combinations_and_group_evidence() {
    let root = std::env::temp_dir().join(format!("unified2-settings-{}", std::process::id()));
    let corpus = support::Corpus::create(&root).unwrap();
    assert!(!ktm::settings::Settings::default().unified_search);
    for related in [false, true] {
        let mut backend = corpus.backend().unwrap().with_related_queries(related);
        for query in ["2026; 공사 계약", ";2026;; 공사 계약;", "2026; 없는검색어"] {
            let mut fetch = |kind, top| {
                if kind == ResultKind::Names {
                    if !related {
                        return corpus.names(query, if top == 0 { usize::MAX } else { top });
                    }
                    let view = corpus.names.view();
                    let found = kfind_names::query::search_related(
                        &view,
                        query,
                        &kfind_names::Options {
                            limit: if top == 0 { usize::MAX } else { top },
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    let items = kfind_names::query::hits(&view, &found.top)
                        .into_iter()
                        .map(|h| serde_json::json!({"name":h.name,"dir":h.dir}))
                        .collect::<Vec<_>>();
                    backend::decode_names_snapshot(&serde_json::to_vec(&serde_json::json!({"ok":true,"result":{"complete":true,"related_groups":true,"total":items.len(),"items":items}})).unwrap(),query,true)
                } else if top == 0 {
                    backend.search_snapshot(
                        query,
                        kind == ResultKind::Hierarchy,
                        None,
                        None,
                        None,
                        MatchMode::AllTerms,
                        None,
                        true,
                    )
                } else {
                    backend.search_with_scope_ext_limit(
                        query,
                        kind == ResultKind::Hierarchy,
                        None,
                        None,
                        None,
                        MatchMode::AllTerms,
                        top,
                        None,
                    )
                }
            };
            // 통합 꺼짐은 기존 원천·합성을 그대로 사용한다.
            let mut batches = Vec::new();
            for (kind, _) in unified::TIERS {
                batches.push((kind, fetch(kind, 200).unwrap().rows));
            }
            let expected = unified::merge(&batches, 80);
            let session =
                unified::collect(80, &AtomicBool::new(false), &mut fetch, |_| {}).unwrap();
            assert!(session.errors.is_empty(), "{:?}", session.errors);
            let page = session.page(Cursor::default(), 80).unwrap();
            let signature = |rows: &[Row]| {
                rows.iter()
                    .map(|r| {
                        (
                            unified::path_key(&r.path),
                            r.match_kinds.clone(),
                            r.group_matches.len(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                signature(&page.answer.rows),
                signature(&expected),
                "related={related}, query={query}"
            );
            assert!(page.next.is_none());
            if related && !query.contains("없는검색어") {
                assert!(page.answer.rows.iter().all(|r| r.group_matches.len() == 2));
            }
        }
    }
    drop(corpus);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn full_identity_preserves_aliases_past_old_candidates_and_keeps_same_name_different_folders() {
    let mut names = answer(1..=1201);
    names.rows.push(Row {
        path: "Z:/other/1.txt".into(),
        ..Row::default()
    });
    let mut body = answer([1201, 1202]);
    body.rows[0].path.clear();
    let session =
        Session::from_complete(vec![(ResultKind::Body, body), (ResultKind::Names, names)]);
    assert_eq!(session.totals(), [1202, 0, 1]);
}

#[test]
fn real_index_over_service_and_candidate_limits_has_exact_ownership_and_page_union() {
    use kfind_core::{Sentence, Writer, WriterOptions};
    use kfind_names::{Builder, Options, Searcher};
    let root = std::env::temp_dir().join(format!(
        "unified2-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let mut writer = Writer::open(&root, WriterOptions::default()).unwrap();
    let mut builder = Builder::new();
    let vol = builder.add_volume("Z:\\synthetic", 5);
    let mut table = String::new();
    for id in 1..=1801u64 {
        let name = if id <= 1201 {
            format!("보편검색-{id:04}.txt")
        } else {
            format!("기록-{id:04}.txt")
        };
        builder.push_meta(vol, id + 10, 5, &name, 0, 32, 0);
        writer
            .add_document(
                id,
                id as i64,
                &[Sentence {
                    para: 0,
                    level: if id % 2 == 0 { 0 } else { 8 },
                    text: "보편검색",
                }],
            )
            .unwrap();
        table.push_str(&format!("{}\n",serde_json::json!({"op":"put","key":id.to_string(),"id":id,"meta":{"path":format!("Z:\\synthetic\\{name}")}})));
    }
    writer.commit().unwrap();
    drop(writer);
    std::fs::write(root.join("doctable.jsonl"), table).unwrap();
    let names = builder.finish();
    let mut backend = Backend::open(&root).unwrap();
    let mut snapshots = Vec::new();
    let session = unified::collect(
        80,
        &AtomicBool::new(false),
        |kind, top| {
            if kind == ResultKind::Names {
                let view = names.view();
                let found = Searcher::new()
                    .search_groups(
                        &view,
                        names.generation(),
                        &kfind_names::query::parse_groups("보편검색 file:"),
                        &Options {
                            limit: if top == 0 { usize::MAX } else { top },
                            ..Options::default()
                        },
                    )
                    .unwrap();
                let rows = kfind_names::query::hits(&view, &found.top)
                    .into_iter()
                    .map(|h| Row {
                        path: format!("{}\\{}", h.dir, h.name),
                        name: h.name,
                        ..Row::default()
                    })
                    .collect();
                Ok(Answer {
                    rows,
                    ..Answer::default()
                })
            } else {
                backend.search_snapshot(
                    "보편검색",
                    kind == ResultKind::Hierarchy,
                    None,
                    None,
                    None,
                    MatchMode::AllTerms,
                    None,
                    true,
                )
            }
        },
        |s| snapshots.push(s),
    )
    .unwrap();
    assert!(session.errors.is_empty(), "{:?}", session.errors);
    assert_eq!(session.totals(), [1201, 600, 0]);
    assert_eq!(snapshots[0].answer.rows.len(), 20);
    let mut seen = HashSet::new();
    let mut cursor = Cursor::default();
    loop {
        let page = session.page(cursor, 80).unwrap();
        for row in page.answer.rows {
            let id = row
                .name
                .trim_end_matches(".txt")
                .rsplit('-')
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap();
            assert!(seen.insert(id));
            assert_eq!(
                row.match_kinds,
                [if id <= 1201 {
                    ResultKind::Names
                } else {
                    ResultKind::Hierarchy
                }]
            );
        }
        let Some(next) = page.next else { break };
        cursor = next;
    }
    assert_eq!(seen, (1..=1801).collect());
    drop(backend);
    drop(names);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn app_policy_keeps_hierarchy_and_body_when_names_fail_but_still_stops_on_hierarchy() {
    let run = |failing: ResultKind| {
        let mut calls = Vec::new();
        let mut snapshots = Vec::new();
        let session = unified::collect_with(
            80,
            &AtomicBool::new(false),
            true,
            |kind, top| {
                calls.push((kind, top));
                if kind == failing {
                    return Err("synthetic failure".into());
                }
                Ok(answer(if kind == ResultKind::Body { 1..=5 } else { 1..=3 }))
            },
            |s| snapshots.push(s),
        )
        .unwrap();
        assert_eq!(session.state, unified::SessionState::Incomplete);
        assert_eq!(session.errors.len(), 1);
        assert_eq!(snapshots.iter().filter(|s| s.final_part).count(), 1);
        assert!(snapshots.last().unwrap().final_part);
        (session.totals(), calls.len(), session.errors[0].clone())
    };
    let (totals, calls, error) = run(ResultKind::Names);
    assert_eq!((totals, calls), ([0, 3, 2], 4));
    assert!(error.starts_with("파일명"), "{error}");
    let (totals, calls, _) = run(ResultKind::Hierarchy);
    assert_eq!((totals, calls), ([3, 0, 0], 3));
}
