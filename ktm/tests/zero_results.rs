//! QMSBUG1: '있는 파일이 0건'이 될 수 있는 경로를 가짜 파이프 서비스와 창의 상태 전환 순서로
//! 재현한다. 설치된 색인 서비스·사용자 색인은 건드리지 않고 임시 이름의 파이프만 쓴다.
#![cfg(windows)]
use ktm::backend::{self, Answer, unified};
use ktm::notice::{self, Facts, NameCoverage, Notice};
use ktm::tm_state::{self, Event, Phase, ResultKind, State};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// 요청 하나에 정해 둔 응답 하나를 돌려주는 가짜 색인 서비스.
fn serve_once(response: Value) -> (String, std::thread::JoinHandle<Value>) {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::{
        Foundation::{ERROR_PIPE_CONNECTED, GetLastError, INVALID_HANDLE_VALUE},
        Storage::FileSystem::PIPE_ACCESS_DUPLEX,
        System::Pipes::{ConnectNamedPipe, CreateNamedPipeW},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let name = format!(
        r"\\.\pipe\qmsbug1-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        CreateNamedPipeW(wide.as_ptr(), PIPE_ACCESS_DUPLEX, 0, 1, 65536, 65536, 0, std::ptr::null())
    };
    assert_ne!(handle, INVALID_HANDLE_VALUE);
    let handle = handle as usize;
    let server = std::thread::spawn(move || {
        let handle = handle as windows_sys::Win32::Foundation::HANDLE;
        unsafe {
            assert!(
                ConnectNamedPipe(handle, std::ptr::null_mut()) != 0
                    || GetLastError() == ERROR_PIPE_CONNECTED
            );
        }
        let mut pipe = unsafe { std::fs::File::from_raw_handle(handle as _) };
        let mut length = [0; 4];
        pipe.read_exact(&mut length).unwrap();
        let mut request = vec![0; u32::from_le_bytes(length) as usize];
        pipe.read_exact(&mut request).unwrap();
        let bytes = serde_json::to_vec(&response).unwrap();
        pipe.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
        pipe.write_all(&bytes).unwrap();
        pipe.flush().unwrap();
        serde_json::from_slice(&request).unwrap()
    });
    (name, server)
}

fn names_error(response: Value) -> String {
    let (pipe, server) = serve_once(response);
    let error = backend::pipe_names(&pipe, "합의각서", "relevance").unwrap_err();
    assert_eq!(server.join().unwrap()["command"], "names_query");
    error
}

fn coverage(result: Value) -> NameCoverage {
    let (pipe, server) = serve_once(json!({"ok":true,"result":result}));
    let status = backend::pipe_names_status(&pipe);
    assert_eq!(server.join().unwrap(), json!({"command":"names_status"}));
    notice::name_coverage(status.as_ref().map_err(String::as_str))
}

fn status(state: &str, mode: &str, entries: u64, note: Option<&str>) -> Value {
    json!({"state":state,"mode":mode,"entries":entries + 1,"last_error":null,
        "volumes":[{"name":"C:","source":"walk","entries":entries,"note":note}]})
}

fn searching() -> State {
    let shown = tm_state::step(&State::default(), Event::Show).0;
    tm_state::step(&shown, Event::Input("합의각서".into())).0
}

fn facts<'a>(state: &'a State, coverage: Option<&'a NameCoverage>) -> Facts<'a> {
    Facts {
        phase: state.phase,
        pending: state.parts_pending(),
        error: state.error.as_deref(),
        found: state.result_count,
        visible: state.result_count,
        hidden_by_settings: false,
        coverage,
        scoped: false,
    }
}

fn shown(state: &State, coverage: Option<&NameCoverage>) -> Notice {
    notice::notice(facts(state, coverage)).expect("0건에는 안내가 있어야 한다")
}

/// 창의 분리 경로(파일명 → 위계 → 본문 작업)와 같은 순서로 상태를 전환한다.
fn legacy(names: Result<usize, String>, content: Result<usize, String>) -> State {
    let mut state = searching();
    for (kind, result) in [
        (ResultKind::Names, names),
        (ResultKind::Hierarchy, content.clone()),
        (ResultKind::Body, content),
    ] {
        let generation = state.generation;
        let event = match result {
            Ok(count) => Event::SearchPartFinished { generation, kind, count, final_part: false },
            Err(message) => Event::SearchFailed { generation, message },
        };
        state = tm_state::step(&state, event).0;
    }
    state
}

#[test]
fn missing_service_with_empty_content_is_a_failure_not_zero_results() {
    let pipe = format!(r"\\.\pipe\qmsbug1-absent-{}", std::process::id());
    let error = backend::pipe_names(&pipe, "합의각서", "relevance").unwrap_err();
    assert!(error.contains("연결할 수 없습니다"), "{error}");
    let state = legacy(Err(error), Ok(0));
    assert_eq!(state.phase, Phase::Error);
    let note = shown(&state, None);
    assert_eq!(note.title, "검색을 완료하지 못했습니다");
    assert!(note.detail.contains("연결할 수 없습니다"), "{}", note.detail);
    assert_eq!(notice::summary(facts(&state, None), Some(0.0)), "검색 못함");
}

#[test]
fn unavailable_name_index_states_have_their_own_words() {
    for (state_name, expected) in [
        ("empty", "아직 만들지 않았습니다"),
        ("building", "준비하는 중"),
        ("failed", "만들기에 실패"),
    ] {
        let error = names_error(
            json!({"ok":false,"error":format!("names index unavailable ({state_name})")}),
        );
        assert!(error.contains(expected), "{error}");
        let note = shown(&legacy(Err(error), Ok(0)), None);
        assert_eq!(
            note.title == "파일명 색인을 준비하는 중입니다",
            state_name == "building",
            "{note:?}"
        );
        assert_ne!(note.title, "검색 결과가 없습니다");
    }
}

#[test]
fn zero_names_are_qualified_by_what_the_index_covers() {
    let zero = legacy(Ok(0), Ok(0));
    assert_eq!(zero.phase, Phase::Results);

    let empty = coverage(status("ready", "service", 0, None));
    assert!(matches!(empty, NameCoverage::NotSearched(_)), "{empty:?}");
    assert_eq!(shown(&zero, Some(&empty)).title, "파일명을 검색하지 못했습니다");
    assert_eq!(notice::summary(facts(&zero, Some(&empty)), Some(0.0)), "검색 못함");

    let user = coverage(status("ready", "user", 12, None));
    let note = shown(&zero, Some(&user));
    assert_eq!(note.title, "검색 결과가 없습니다");
    assert!(note.detail.contains("등록한 폴더 1곳(항목 12개)"), "{}", note.detail);

    let broken = coverage(status("ready", "service", 12, Some("mft: denied")));
    assert!(matches!(&broken, NameCoverage::Limited(m) if m.contains("C:")), "{broken:?}");

    // 권한 없는 폴더 몇 개는 정상이다.
    let full = coverage(status("ready", "service", 12, Some("3 folders unreadable")));
    assert!(matches!(full, NameCoverage::Full(_)), "{full:?}");
    assert_eq!(shown(&zero, Some(&full)).title, "검색 결과가 없습니다");
    assert_eq!(notice::summary(facts(&zero, Some(&full)), Some(0.25)), "0개 · 0.25초");

    let unknown = notice::name_coverage(Err("서비스 응답 시간이 초과되었습니다"));
    assert!(matches!(unknown, NameCoverage::Limited(_)));
}

#[test]
fn opt_in_unified_path_keeps_the_failure_next_to_empty_sources() {
    let mut state = searching();
    let mut notes = Vec::new();
    unified::stream(
        80,
        &AtomicBool::new(false),
        |kind, _| match kind {
            ResultKind::Names => Err("서비스 없음".into()),
            _ => Ok(Answer::default()),
        },
        |snapshot| {
            let event = Event::SearchPartFinished {
                generation: state.generation,
                kind: snapshot.completed,
                count: snapshot.answer.rows.len(),
                final_part: snapshot.final_part,
            };
            state = tm_state::step(&state, event).0;
            // 창은 상태 전환 뒤 구간 오류를 다시 적는다.
            state.error = (!snapshot.errors.is_empty()).then(|| snapshot.errors.join(" · "));
            notes.push(shown(&state, None));
        },
    )
    .unwrap();
    assert_eq!(notes.len(), 3);
    for note in notes {
        assert_eq!(note.title, "검색을 완료하지 못했습니다");
        assert!(note.detail.contains("파일명: 서비스 없음"), "{}", note.detail);
    }
}

#[test]
fn pending_hidden_and_scoped_zeroes_are_told_apart() {
    let state = searching();
    assert_eq!(shown(&state, None).title, "검색 중…");
    let first = tm_state::step(
        &state,
        Event::SearchPartFinished {
            generation: state.generation,
            kind: ResultKind::Names,
            count: 0,
            final_part: false,
        },
    )
    .0;
    assert_eq!(shown(&first, None).title, "검색 중…", "본문이 남았으면 '결과 없음'이 아니다");

    let found = legacy(Ok(3), Ok(0));
    let mut f = facts(&found, None);
    f.found = 3;
    f.visible = 0;
    assert_eq!(notice::notice(f).unwrap().title, "조건 때문에 결과가 숨겨져 있습니다");
    f.visible = 3;
    assert_eq!(notice::notice(f), None);

    let zero = legacy(Ok(0), Ok(0));
    let mut f = facts(&zero, None);
    f.hidden_by_settings = true;
    assert_eq!(notice::notice(f).unwrap().title, "설정 때문에 결과가 숨겨져 있습니다");
    f.hidden_by_settings = false;
    f.scoped = true;
    assert!(notice::notice(f).unwrap().detail.contains("조건이 걸려"));
}

/// 대신할 검색이 없는 입력·같은 칩 선택은 세대를 올리지 않는다. 창은 세대가 오를 때만 취소한다.
#[test]
fn repeated_input_or_chip_does_not_supersede_the_running_search() {
    let state = searching();
    for event in [
        Event::Input("합의각서".into()),
        Event::SetDateFilter(tm_state::DateFilter::All),
        Event::SetExtFilter(None),
        Event::SetFolderFilter(None),
    ] {
        let (next, effects) = tm_state::step(&state, event);
        assert_eq!(next.generation, state.generation);
        assert!(effects.is_empty());
    }
    let (next, _) = tm_state::step(&state, Event::Input("합의각서 안".into()));
    assert_ne!(next.generation, state.generation);
}
