//! main의 연관 검색과 통합 경로가 같은 원천·묶음 근거를 사용하는지 검증한다.
#![cfg(windows)]
use kfind_core::{Sentence, Writer, WriterOptions};
use kfind_names::{Builder, Names, Options, Searcher};
use ktm::backend::{
    self, Answer, Backend, Row,
    unified::{self, Source},
};
use ktm::tm_state::{self, ResultKind, Tab};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

// 같은 시각값이 나오면 병렬 시험의 임시 폴더 이름이 겹친다.
static FIXTURE_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    names: Names,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "unified1m-{}-{}-{}",
            std::process::id(),
            FIXTURE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut writer = Writer::open(&root, WriterOptions::default()).unwrap();
        let mut builder = Builder::new();
        let vol = builder.add_volume(&root.to_string_lossy(), 5);
        let records = [
            ("2026 공사 계약.txt", vec!["2026", "공사 계약"]),
            ("분리문단.txt", vec!["2026", "공사", "계약"]),
            ("분리문장.txt", vec!["2026", "공사 안내. 계약 검토."]),
            ("2026.txt", vec!["2026"]),
            ("2026 공사. 계약.txt", vec!["일반 기록"]),
            ("R&D a;b.txt", vec!["R&D a;b"]),
            ("공사 계약.txt", vec!["2026", "공사 계약"]),
            ("2026 교차금지.txt", vec!["공사 계약"]),
        ];
        let mut table = String::new();
        let mut paragraphs = Vec::new();
        for (i, (name, texts)) in records.iter().enumerate() {
            let id = i as u64 + 1;
            let path = root.join(name).to_string_lossy().into_owned();
            let sentences: Vec<_> = texts
                .iter()
                .enumerate()
                .flat_map(|(para, text)| {
                    // 분리문장은 같은 문단의 서로 다른 두 색인 문장이다.
                    text.split(". ").map(move |text| Sentence {
                        para: para as u32,
                        level: 8,
                        text,
                    })
                })
                .collect();
            writer.add_document(id, 0, &sentences).unwrap();
            table.push_str(&format!(
                "{}\n",
                json!({"op":"put","key":name,"id":id,"meta":{"path":path}})
            ));
            for (para, text) in texts.iter().enumerate() {
                paragraphs.push((path.clone(), para as u32, *text));
            }
            builder.push_meta(vol, id + 10, 5, name, 0, 256, 134399520000000000);
        }
        writer.commit().unwrap();
        std::fs::write(root.join("doctable.jsonl"), table).unwrap();
        let rows: Vec<_> = paragraphs
            .iter()
            .map(|(p, n, t)| (p.as_str(), *n, *t))
            .collect();
        ktm::paragraphs::create_fixture(&root.join("content.sqlite"), &rows).unwrap();
        Self {
            root,
            names: builder.finish(),
        }
    }
    fn backend(&self, related: bool) -> Backend {
        Backend::open(&self.root)
            .unwrap()
            .with_paragraphs(Some(self.root.join("content.sqlite")))
            .with_related_queries(related)
    }
    fn names(&self, query: &str, top: usize, related: bool) -> Value {
        let view = self.names.view();
        let opts = Options {
            limit: top,
            ..Default::default()
        };
        let result = if related {
            kfind_names::query::search_related(&view, query, &opts).unwrap()
        } else {
            Searcher::new()
                .search_groups(
                    &view,
                    self.names.generation(),
                    &kfind_names::query::parse_groups(query),
                    &opts,
                )
                .unwrap()
        };
        let items: Vec<_> = kfind_names::query::hits(&view, &result.top)
            .into_iter()
            .map(|h| json!({"name":h.name,"dir":h.dir,"is_dir":false,"size":256,"mtime":h.mtime}))
            .collect();
        json!({"ok":true,"result":{"items":items,"related_groups":related}})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

/// 실제 Windows 파이프로 명령·원문 질의·상한을 검사한다. 설치 서비스는 사용하지 않는다.
fn names_call(fixture: &Fixture, query: &str, top: usize, related: bool) -> Answer {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, INVALID_HANDLE_VALUE},
        Storage::FileSystem::PIPE_ACCESS_DUPLEX,
        System::Pipes::{ConnectNamedPipe, CreateNamedPipeW},
    };
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let pipe = format!(
        r"\\.\pipe\unified1m-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let wide: Vec<_> = pipe.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            0,
            1,
            65536,
            65536,
            0,
            std::ptr::null(),
        )
    };
    assert_ne!(handle, INVALID_HANDLE_VALUE);
    let response = serde_json::to_vec(&fixture.names(query, top, related)).unwrap();
    let expected_query = query.to_owned();
    let handle = handle as usize;
    let server = std::thread::spawn(move || unsafe {
        let handle = handle as windows_sys::Win32::Foundation::HANDLE;
        if ConnectNamedPipe(handle, std::ptr::null_mut()) == 0
            && GetLastError() != ERROR_PIPE_CONNECTED
        {
            CloseHandle(handle);
            panic!("합성 파이프 연결 실패");
        }
        let mut file = std::fs::File::from_raw_handle(handle);
        let mut prefix = [0; 4];
        file.read_exact(&mut prefix).unwrap();
        let mut bytes = vec![0; u32::from_le_bytes(prefix) as usize];
        file.read_exact(&mut bytes).unwrap();
        let request: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            request["command"],
            if related {
                "names_related"
            } else {
                "names_query"
            }
        );
        assert_eq!(request["text"], expected_query);
        assert_eq!(request["limit"], top);
        file.write_all(&(response.len() as u32).to_le_bytes())
            .unwrap();
        file.write_all(&response).unwrap();
    });
    let answer = if related {
        backend::pipe_names_related(&pipe, query, top).unwrap()
    } else {
        Answer {
            rows: backend::pipe_names_limit(&pipe, query, "relevance", top).unwrap(),
            ..Default::default()
        }
    };
    server.join().unwrap();
    answer
}

fn evidence(rows: &[Row]) -> Vec<Value> {
    rows.iter().map(|r| json!({"path":unified::path_key(&r.path),"para":r.para,"level":r.level,
        "groups":r.group_matches.iter().map(|g| json!([g.group_index,g.query,g.paragraphs,g.filename])).collect::<Vec<_>>() })).collect()
}

fn matrix(unified_enabled: bool, related: bool) {
    let fixture = Fixture::new();
    let settings: ktm::settings::Settings =
        serde_json::from_value(json!({"unified_search":unified_enabled,"related_queries":related}))
            .unwrap();
    let saved: ktm::settings::Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(settings, saved);
    let jobs = unified::search_jobs(settings.unified_search, 80);
    assert_eq!(
        jobs,
        if unified_enabled {
            vec![(Tab::All, 80)]
        } else {
            vec![(Tab::Names, 20), (Tab::Hierarchy, 20), (Tab::Body, 40)]
        }
    );
    for pipe_mode in [false, true] {
        assert_eq!(
            unified::source(ResultKind::Names, related, pipe_mode),
            if related {
                Source::NamesRelated
            } else {
                Source::Names
            }
        );
        for kind in [ResultKind::Hierarchy, ResultKind::Body] {
            assert_eq!(
                unified::source(kind, related, pipe_mode),
                if pipe_mode && !related {
                    Source::ContentPipe
                } else {
                    Source::ContentLocal
                }
            );
        }
    }
    let mut local = fixture.backend(settings.related_queries);
    for query in [
        "2026; 공사 계약",
        ";2026;; 공사 계약;",
        "R&D; \"a;b\"",
        "2026; 없는검색어",
    ] {
        // 통합을 끈 main 경로의 각 원천 응답을 직접 얻는다.
        let names = names_call(&fixture, query, 20, related);
        let hierarchy = local
            .search_with_scope_ext_limit(
                query,
                true,
                None,
                None,
                None,
                tm_state::MatchMode::AllTerms,
                20,
                None,
            )
            .unwrap();
        let body = local
            .search_with_scope_ext_limit(
                query,
                false,
                None,
                None,
                None,
                tm_state::MatchMode::AllTerms,
                40,
                None,
            )
            .unwrap();
        if related && query.contains("공사") {
            assert_eq!(
                names
                    .rows
                    .iter()
                    .map(|r| r.name.as_str())
                    .collect::<Vec<_>>(),
                ["2026 공사 계약.txt", "2026 공사. 계약.txt"]
            );
            assert_eq!(
                body.rows.iter().map(|r| r.doc_id).collect::<Vec<_>>(),
                [1, 7]
            );
            assert!(
                names
                    .rows
                    .iter()
                    .all(|r| r.group_matches.len() == 2
                        && r.group_matches.iter().all(|g| g.filename))
            );
            assert!(body.rows.iter().all(|r| {
                r.group_matches.len() == 2
                    && r.group_matches
                        .iter()
                        .all(|g| !g.filename && !g.paragraphs.is_empty())
            }));
        }
        let batches = vec![
            (ResultKind::Names, names.rows),
            (ResultKind::Hierarchy, hierarchy.rows),
            (ResultKind::Body, body.rows),
        ];
        if !unified_enabled {
            let combined = tm_state::combine_results(&batches, |r| unified::path_key(&r.path));
            let paths: std::collections::HashSet<_> = combined
                .iter()
                .map(|r| unified::path_key(&r.row.path))
                .collect();
            assert_eq!(paths.len(), combined.len());
            if related && query.contains("공사") {
                assert_eq!(combined.len(), 3);
                let shared = combined
                    .iter()
                    .find(|r| r.row.name == "2026 공사 계약.txt")
                    .unwrap();
                assert_eq!(
                    shared.kinds,
                    [ResultKind::Names, ResultKind::Hierarchy, ResultKind::Body]
                );
            }
            continue;
        }
        let mut snapshots = Vec::new();
        unified::stream(
            80,
            &AtomicBool::new(false),
            |kind, top| match unified::source(kind, related, false) {
                Source::NamesRelated => Ok(names_call(&fixture, query, top, true)),
                Source::Names => {
                    let (q, _) = unified::names_query(query, None, None, None);
                    Ok(names_call(&fixture, &q, top, false))
                }
                Source::ContentLocal => local.search_with_scope_ext_limit(
                    query,
                    kind == ResultKind::Hierarchy,
                    None,
                    None,
                    None,
                    tm_state::MatchMode::AllTerms,
                    top,
                    None,
                ),
                Source::ContentPipe => unreachable!(),
            },
            |s| snapshots.push(s),
        )
        .unwrap();
        assert_eq!(snapshots.len(), 3);
        assert!(snapshots.iter().all(|s| s.errors.is_empty()));
        assert!(snapshots[2].final_part);
        assert_eq!(
            evidence(&snapshots[2].answer.rows),
            evidence(&unified::merge(&batches, 80)),
            "{query}"
        );
        if related && query.contains("공사") {
            assert_eq!(
                snapshots[2]
                    .answer
                    .rows
                    .iter()
                    .map(|r| r.name.as_str())
                    .collect::<Vec<_>>(),
                ["2026 공사 계약.txt", "2026 공사. 계약.txt", "공사 계약.txt"]
            );
            assert_eq!(snapshots[2].answer.groups, ["2026", "공사 계약"]);
        }
    }
    drop(local);
}

#[test]
fn both_off_keep_main_jobs_and_sources() {
    matrix(false, false);
}
#[test]
fn related_only_keeps_main_literal_groups_and_evidence() {
    matrix(false, true);
}
#[test]
fn unified_only_keeps_legacy_sources_and_streams() {
    matrix(true, false);
}
#[test]
fn both_on_use_names_related_and_main_paragraph_semantics() {
    matrix(true, true);
}
