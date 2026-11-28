//! 앱 안 통합 검색 전용 JSONL 호스트(`ktm --search-host`). 창·설정·색인 쓰기·상주 설치를 하지 않는다.
//! 앱이 표준 입력으로 명령을 한 줄씩 보내고, 표준 입력이 닫히면 끝난다.
use crate::backend::{self, Answer, Backend, Result, Row, unified::{self, Cursor, Session, SessionState}};
use crate::notice::{self, Facts, NameCoverage};
use crate::tm_state::{MatchMode, Phase, ResultKind};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::{self, BufRead, Write}, path::PathBuf, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc}};

#[derive(Clone, Debug, Deserialize)]
pub struct Start {
    pub generation: u64,
    pub query: String,
    pub roots: Vec<String>,
    #[serde(default)] pub extensions: Vec<String>,
    /// 조사 폴더 아래에서 건너뛸 폴더 이름(대소문자 무시).
    #[serde(default)] pub exclude_dirs: Vec<String>,
    /// YYYYMMDD 시작·끝(둘 다 포함).
    pub dates: Option<(i64, i64)>,
    #[serde(default)] pub related_queries: bool,
    #[serde(default = "default_mode")] pub match_mode: String,
    #[serde(default = "default_sort")] pub sort: String,
    #[serde(default = "default_limit")] pub limit: usize,
    #[serde(default = "default_pipe")] pub pipe: String,
    pub index_root: Option<PathBuf>,
    /// 앱 본문 문단 표(발췌·연관 검색용, 읽기 전용).
    pub paragraphs: Option<PathBuf>,
}
fn default_mode() -> String { "all".into() }
fn default_sort() -> String { "relevance".into() }
fn default_limit() -> usize { 80 }
fn default_pipe() -> String { backend::PIPE_NAME.into() }

impl Start {
    pub fn validate(&self) -> Result<()> {
        if self.generation == 0 || self.query.trim().is_empty() || self.query.len() > 8192
            || !(1..=80).contains(&self.limit) || self.roots.len() > 128
            || self.roots.iter().any(|s| s.is_empty() || s.contains(['"', '\n', '\r']))
            || self.extensions.iter().any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric()))
            || !matches!(self.match_mode.as_str(), "all" | "any" | "phrase")
            || !matches!(self.sort.as_str(), "relevance" | "name" | "recent") {
            return Err("통합 검색 요청을 확인해 주세요.".into());
        }
        Ok(())
    }
}

/// 앱의 파일 열기가 거부하는 항목은 단 배정 전에 뺀다(개수와 페이지가 화면과 같아지게).
fn allowed(path: &str, roots: &[String], extensions: &[String], exclude_dirs: &[String]) -> bool {
    let key = unified::path_key(path);
    roots.iter().any(|root| {
        let prefix = format!("{}\\", unified::path_key(root).trim_end_matches('\\'));
        let Some(rest) = key.strip_prefix(&prefix) else { return false };
        let mut parts: Vec<&str> = rest.split('\\').collect();
        let name = parts.pop().unwrap_or("");
        let bare = name.trim_start_matches(|c: char| {
            c.is_whitespace() || "'\"‘’“”`\u{200b}\u{feff}".contains(c)
        });
        !name.is_empty()
            && (extensions.is_empty()
                || name.rsplit_once('.').is_some_and(|(_, e)| extensions.iter().any(|x| x.to_lowercase() == e)))
            && !bare.starts_with("#ieeee")
            && !name.starts_with("~$")
            && !parts.iter().any(|part| {
                part.starts_with('.') || exclude_dirs.iter().any(|d| d.to_lowercase() == *part)
            })
    })
}

/// 수집한 세션과 화면 안내에 필요한 사실.
pub struct Collected {
    pub session: Session,
    /// 페이지 발췌에 쓰는 본문 색인(본문 조회까지 갔을 때만).
    pub backend: Option<Backend>,
    pub coverage: Option<NameCoverage>,
}

/// 0건·검색 못 함·색인 준비 중 안내는 QMSBUG1(`notice`)의 문구를 그대로 쓴다.
pub fn describe(
    found: usize,
    pending: bool,
    errors: &[String],
    coverage: Option<&NameCoverage>,
    scoped: bool,
) -> (Value, String) {
    let mut error = (!errors.is_empty()).then(|| errors.join(" / "));
    // 색인이 비어 있어 못 찾은 것은 '파일명을 검색하지 못했습니다'로 알린다.
    // 위계·본문도 실패했으면 그 사유가 가려지지 않게 오류를 남긴다.
    if found == 0 && errors.iter().all(|e| e.starts_with("파일명"))
        && matches!(coverage, Some(NameCoverage::NotSearched(m)) if !m.contains("준비하는 중")) {
        error = None;
    }
    let facts = Facts {
        phase: if pending { Phase::Searching } else if error.is_some() { Phase::Error } else { Phase::Results },
        pending,
        error: error.as_deref(),
        found,
        visible: found,
        hidden_by_settings: false,
        coverage,
        scoped,
    };
    let value = notice::notice(facts)
        .map(|n| json!({"title":n.title, "detail":n.detail}))
        .unwrap_or(Value::Null);
    (value, notice::summary(facts, None))
}

fn scoped(request: &Start) -> bool {
    request.dates.is_some()
}

/// 원천 조회가 끝날 때마다 부분 결과를 발행하고, 전수 세션을 돌려준다.
pub fn collect(request: &Start, cancel: &Arc<AtomicBool>, mut publish: impl FnMut(Value)) -> Result<Collected> {
    request.validate()?;
    let mode = match request.match_mode.as_str() {
        "any" => MatchMode::AnyTerm, "phrase" => MatchMode::Exact, _ => MatchMode::AllTerms,
    };
    let mut backend: Option<Backend> = None;
    let coverage = std::cell::RefCell::new(None::<NameCoverage>);
    let ext = (!request.extensions.is_empty()).then(|| request.extensions.join(","));
    let session = unified::collect_with(request.limit, cancel, true, |kind, top| {
        let mut answer = Answer::default();
        // 빈 조사 폴더를 전체 드라이브 검색으로 확대하지 않는다.
        if request.roots.is_empty() {
            return Ok(answer);
        }
        if kind == ResultKind::Names {
            // 연관 검색은 질의 전체가 200자 이하여야 해서 묶음마다 붙는 폴더·확장자 조건을 빼고
            // 한 번만 묻는다. 범위는 아래에서 거른다.
            let scopes: Vec<Option<&str>> = if request.related_queries {
                vec![None]
            } else {
                request.roots.iter().map(|root| Some(root.as_str())).collect()
            };
            let ext = ext.as_deref().filter(|_| !request.related_queries);
            for folder in scopes {
                if cancel.load(Ordering::Relaxed) { return Err("검색 취소".into()); }
                let (text, dates) = unified::names_query(&request.query, request.dates, folder, ext);
                let mut part = if top == 0 {
                    backend::pipe_names_snapshot(&request.pipe, &text, &request.sort, request.related_queries)?
                } else if request.related_queries {
                    backend::pipe_names_related(&request.pipe, &text, top)?
                } else {
                    Answer { rows: backend::pipe_names_limit(&request.pipe, &text, &request.sort, top)?,
                        terms: unified::query_terms(&request.query),
                        groups: kfind_core::split_semicolon_groups(&request.query), ..Answer::default() }
                };
                unified::filter_name_dates(&mut part.rows, dates);
                answer.rows.extend(part.rows);
                answer.terms.extend(part.terms);
                answer.groups = part.groups;
            }
        } else {
            if backend.is_none() {
                let root = request.index_root.clone().map(Ok).unwrap_or_else(crate::paths::default_index)?;
                let paragraphs = request.paragraphs.clone().or_else(crate::paragraphs::default_path);
                backend = Some(Backend::open_readonly(&root)?
                    .with_paragraphs(paragraphs)
                    .with_related_queries(request.related_queries));
            }
            // 본문 색인의 폴더 조건은 하위 폴더를 포함하지 않고, 확장자 조건은 색인 메타가 있어야 맞는다.
            // 둘 다 넘기지 않고 아래에서 경로로 거른다.
            answer = backend.as_mut().unwrap().search_snapshot(&request.query, kind == ResultKind::Hierarchy,
                request.dates, None, None, mode, Some(cancel.clone()), true)?;
        }
        // 중첩 조사 폴더도 하나의 원천 문서로 취급한다.
        let mut seen = std::collections::HashSet::new();
        answer.rows.retain(|row| {
            allowed(&row.path, &request.roots, &request.extensions, &request.exclude_dirs) && seen.insert(unified::path_key(&row.path))
        });
        if kind == ResultKind::Names && top != 0 { answer.rows.truncate(top); }
        // 파일명 0건은 색인 범위를 확인한다. 비어 있으면 '0건'이 아니라 '검색 못 함'이다.
        if kind == ResultKind::Names && top == 0 && answer.rows.is_empty() {
            let status = backend::pipe_names_status(&request.pipe);
            let value = notice::name_coverage(status.as_ref().map_err(String::as_str));
            let failed = matches!(value, NameCoverage::NotSearched(_)).then(|| value.message().to_owned());
            *coverage.borrow_mut() = Some(value);
            if let Some(message) = failed { return Err(message); }
        }
        Ok(answer)
    }, |snapshot| {
        let (note, summary) = describe(snapshot.answer.rows.len(), !snapshot.final_part,
            &snapshot.errors, coverage.borrow().as_ref(), scoped(request));
        publish(json!({"event":"part", "generation":request.generation,
            "completed":unified::label(snapshot.completed), "final_part":snapshot.final_part,
            "answer":answer_json(&snapshot.answer), "errors":snapshot.errors,
            "notice":note, "summary":summary}));
    })?;
    Ok(Collected { session, backend, coverage: coverage.into_inner() })
}

pub fn answer_json(answer: &Answer) -> Value {
    json!({"rows":answer.rows.iter().map(row_json).collect::<Vec<_>>(),
        "terms":answer.terms, "groups":answer.groups})
}

fn row_json(r: &Row) -> Value {
    // kfind 문서 번호는 앱 타임라인 번호와 다르므로 내보내지 않는다. 앱 동작은 path 를 쓴다.
    json!({"path":r.path, "name":r.name, "parent":r.parent,
        "snippet":r.snippet, "location":r.location, "level":r.level, "count":r.count,
        "tier":r.match_kinds.first().map(|k| unified::label(*k))})
}

/// 커서의 내부 좌표를 외부로 내지 않고 세션마다 발행한 번호만 받는다.
pub struct Pages {
    pub generation: u64,
    collected: Collected,
    cursors: Vec<Cursor>,
    limit: usize,
    scoped: bool,
}

impl Pages {
    pub fn new(request: &Start, collected: Collected) -> Self {
        Self { generation: request.generation, collected, cursors: vec![Cursor::default()],
            limit: request.limit, scoped: scoped(request) }
    }

    pub fn page(&mut self, cursor: usize) -> Result<Value> {
        let at = *self.cursors.get(cursor).ok_or("잘못된 검색 커서입니다.")?;
        let mut page = self.collected.session.page(at, self.limit)?;
        // 전수 조회는 발췌를 미리 만들지 않으므로 보이는 페이지만 채운다.
        if let Some(backend) = self.collected.backend.as_mut() {
            backend.fill_snippets_with_cloud_policy(&mut page.answer.rows, &page.answer.terms, true);
        }
        let next = page.next.map(|next| {
            if let Some(i) = self.cursors.iter().position(|c| *c == next) { i }
            else { self.cursors.push(next); self.cursors.len() - 1 }
        });
        let session = &self.collected.session;
        let (note, summary) = describe(page.totals.iter().sum(), false, &session.errors,
            self.collected.coverage.as_ref(), self.scoped);
        Ok(json!({"event":"page", "generation":self.generation,
            "cursor":cursor, "next_cursor":next, "totals":page.totals,
            "incomplete":session.state == SessionState::Incomplete, "errors":session.errors,
            "answer":answer_json(&page.answer), "notice":note, "summary":summary}))
    }
}

struct Gate { generation: u64, cancel: Arc<AtomicBool> }

fn emit(gate: &Mutex<Gate>, generation: u64, value: Value) {
    let gate = gate.lock().unwrap();
    if gate.generation != generation || gate.cancel.load(Ordering::Relaxed) { return; }
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

pub fn run() -> Result<()> {
    let gate = Arc::new(Mutex::new(Gate { generation: 0, cancel: Arc::new(AtomicBool::new(false)) }));
    let (tx, rx) = mpsc::channel::<(Value, Arc<AtomicBool>)>();
    let worker_gate = gate.clone();
    let worker = std::thread::spawn(move || {
        let mut pages: Option<Pages> = None;
        for (value, cancel) in rx {
            let generation = value["generation"].as_u64().unwrap_or(0);
            if value["command"] == "close" { pages = None; continue; }
            if cancel.load(Ordering::Relaxed) { continue; }
            let result = match value["command"].as_str() {
                Some("start") => {
                    pages = None;
                    serde_json::from_value::<Start>(value).map_err(|e| e.to_string()).and_then(|request| {
                        let collected = collect(&request, &cancel, |part| emit(&worker_gate, generation, part))?;
                        pages.insert(Pages::new(&request, collected)).page(0)
                    })
                }
                Some("page") => {
                    let cursor = value["cursor"].as_u64().and_then(|n| usize::try_from(n).ok());
                    match (pages.as_mut(), cursor) {
                        (Some(p), Some(c)) if p.generation == generation => p.page(c),
                        _ => Err("만료된 검색 세션입니다. 다시 검색해 주세요.".into()),
                    }
                }
                _ => Err("지원하지 않는 통합 검색 명령입니다.".into()),
            };
            emit(&worker_gate, generation, result.unwrap_or_else(|error| {
                let (note, summary) = describe(0, false, std::slice::from_ref(&error), None, false);
                json!({"event":"error", "generation":generation, "message":error,
                    "notice":note, "summary":summary})
            }));
        }
    });
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.len() > 65536 { continue; }
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        let generation = value["generation"].as_u64().unwrap_or(0);
        let command = value["command"].as_str().unwrap_or("");
        let mut current = gate.lock().unwrap();
        if command == "start" {
            // 세대는 늘기만 한다. 늦게 온 start 가 새 검색을 바꾸지 않는다.
            if generation <= current.generation { continue; }
            current.cancel.store(true, Ordering::Relaxed);
            current.generation = generation;
            current.cancel = Arc::new(AtomicBool::new(false));
        } else if generation != current.generation {
            continue;
        }
        if matches!(command, "cancel" | "close") {
            current.cancel.store(true, Ordering::Relaxed);
            // 세션 해제도 같은 작업 스레드에서 한다.
            let _ = tx.send((json!({"command":"close", "generation":generation}), current.cancel.clone()));
        } else {
            let _ = tx.send((value, current.cancel.clone()));
        }
    }
    gate.lock().unwrap().cancel.store(true, Ordering::Relaxed);
    drop(tx);
    let _ = worker.join();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_documents_the_app_can_open_inside_a_root_are_kept() {
        let roots = vec![r"Z:\합성\조사".to_owned()];
        let skip = vec!["AppData".to_owned()];
        let ext = vec!["hwpx".to_owned(), "txt".to_owned()];
        assert!(allowed("Z:/합성/조사/하위/문서.HWPX", &roots, &ext, &skip));
        assert!(!allowed(r"Z:\합성\조사\문서.exe", &roots, &ext, &skip));
        assert!(allowed(r"Z:\합성\조사\문서.exe", &roots, &[], &skip));
        for path in [
            r"Z:\합성\조사2\문서.hwpx",
            r"Z:\합성\문서.hwpx",
            r"Z:\합성\조사\appdata\문서.hwpx",
            r"Z:\합성\조사\.git\문서.txt",
            r"Z:\합성\조사\~$문서.hwpx",
            r"Z:\합성\조사\ '#IEeEE 문서.hwpx",
            r"Z:\합성\조사\..\밖.hwpx",
            "",
        ] {
            assert!(!allowed(path, &roots, &ext, &skip), "{path}");
        }
        assert!(!allowed(r"Z:\합성\조사\문서.hwpx", &[], &ext, &skip));
    }

    #[test]
    fn zero_unsearched_preparing_and_partial_use_qmsbug1_wording() {
        let title = |v: &Value| v["title"].as_str().unwrap_or("").to_owned();
        let (note, summary) = describe(0, false, &[], None, false);
        assert_eq!((title(&note).as_str(), summary.as_str()), ("검색 결과가 없습니다", "0개"));
        let (note, summary) = describe(0, true, &[], None, false);
        assert_eq!((title(&note).as_str(), summary.as_str()), ("검색 중…", "검색 중…"));
        let errors = vec!["파일명 전수 조회: 서비스에 연결할 수 없습니다".to_owned()];
        let (note, summary) = describe(0, false, &errors, None, false);
        assert_eq!((title(&note).as_str(), summary.as_str()), ("검색을 완료하지 못했습니다", "검색 못함"));
        let errors = vec!["파일명 전수 조회: 파일명 색인을 준비하는 중입니다".to_owned()];
        let (note, summary) = describe(0, false, &errors, None, false);
        assert_eq!((title(&note).as_str(), summary.as_str()), ("파일명 색인을 준비하는 중입니다", "검색 못함"));
        let empty = NameCoverage::NotSearched("파일명 색인이 비어 있습니다.".into());
        let errors = vec!["파일명 전수 조회: 파일명 색인이 비어 있습니다.".to_owned()];
        let (note, summary) = describe(0, false, &errors, Some(&empty), false);
        assert_eq!((title(&note).as_str(), summary.as_str()), ("파일명을 검색하지 못했습니다", "검색 못함"));
        // 파일명 색인이 비었고 하위 단도 실패하면 그 사유를 가리지 않는다.
        let errors = vec!["파일명 전수 조회: 비어 있음".to_owned(), "위계 전수 조회: 본문 색인이 없습니다".to_owned()];
        let (note, _) = describe(0, false, &errors, Some(&empty), false);
        assert!(note["detail"].as_str().unwrap().contains("본문 색인이 없습니다"), "{note}");
        // 결과가 있으면 안내 대신 목록을 보인다(오류는 errors 로 따로 보인다).
        assert!(describe(3, false, &errors, None, false).0.is_null());
    }
}
