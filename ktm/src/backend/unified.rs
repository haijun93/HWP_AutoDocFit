//! Opt-in, bounded, streaming composition of the existing three search sources.
//! No handles, index copies, background threads or persistent caches live here.
use super::{Answer, MAX_SEARCH_RESULTS, Result, Row};
use crate::tm_state::ResultKind;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

pub const TIERS: [(ResultKind, usize); 3] = [
    (ResultKind::Names, 20),
    (ResultKind::Hierarchy, 20),
    (ResultKind::Body, 40),
];
pub const OUTPUT_LIMIT: usize = 80;

/// 연관 검색은 파일명 전용 명령과 원문 문단 backend를 함께 사용한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Names,
    NamesRelated,
    ContentPipe,
    ContentLocal,
}

pub fn source(kind: ResultKind, related_queries: bool, pipe_mode: bool) -> Source {
    match kind {
        ResultKind::Names if related_queries => Source::NamesRelated,
        ResultKind::Names => Source::Names,
        _ if pipe_mode && !related_queries => Source::ContentPipe,
        _ => Source::ContentLocal,
    }
}

pub fn search_jobs(enabled: bool, limit: usize) -> Vec<(crate::tm_state::Tab, usize)> {
    use crate::tm_state::Tab;
    if enabled {
        vec![(Tab::All, limit)]
    } else {
        vec![(Tab::Names, 20), (Tab::Hierarchy, 20), (Tab::Body, 40)]
    }
}

fn tier(row: &Row) -> usize {
    match row.match_kinds.first() {
        Some(ResultKind::Names) => 0,
        Some(ResultKind::Hierarchy) => 1,
        _ => 2,
    }
}

/// Presentation sorting may reorder rows only inside their winning tier.
pub fn sort_within_tiers(rows: &mut [Row], sort: super::BodySort) {
    super::sort_rows(rows, sort);
    rows.sort_by_key(tier);
}

pub fn match_groups(rows: &[&Row]) -> crate::layout::MatchGroups {
    crate::layout::MatchGroups {
        filename: rows.iter().filter(|r| tier(r) == 0).count(),
        body: rows.iter().filter(|r| tier(r) != 0).count(),
    }
}

// The name parser accepts ext:hwp;pdf while the content syntax parser accepts
// comma lists. Protect that existing filename syntax before using the latter.
fn normalize_ext_lists(query: &str) -> String {
    let mut output = String::with_capacity(query.len());
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in query.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        if !quoted && c.is_whitespace() {
            start = i + c.len_utf8();
        }
        let ext = query.get(start..i).is_some_and(|token| {
            token
                .get(..4)
                .is_some_and(|p| p.eq_ignore_ascii_case("ext:"))
        });
        output.push(if c == ';' && !quoted && ext { ',' } else { c });
    }
    output
}

pub fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for group in kfind_core::split_semicolon_groups(query) {
        let syntax = kfind_core::parse_query_syntax(&group);
        terms.extend(syntax.clean_text.split_whitespace().map(str::to_owned));
        terms.extend(syntax.phrases);
    }
    terms.sort();
    terms.dedup();
    terms
}

/// Serialize common scope options for the legacy pipe query protocol.
pub fn scoped_query(
    query: &str,
    dates: Option<(i64, i64)>,
    folder: Option<&str>,
    ext: Option<&str>,
    mode: crate::tm_state::MatchMode,
) -> String {
    let mut parts: Vec<String> = kfind_core::split_semicolon_groups(query)
        .into_iter()
        .map(|group| super::query_for_match_mode(&group, mode))
        .collect();
    let mut suffix = Vec::new();
    if let Some((start, end)) = dates {
        suffix.push(format!(
            "after:{} before:{}",
            crate::tm_state::format_ymd(start),
            crate::tm_state::format_ymd(end)
        ));
    }
    if let Some(folder) = folder {
        suffix.push(format!("path:\"{folder}\""));
    }
    if let Some(ext) = ext {
        suffix.push(format!("ext:{ext}"));
    }
    if !suffix.is_empty() {
        for group in &mut parts {
            *group = format!("{} {group}", suffix.join(" "));
        }
    }
    parts.join("; ")
}

/// Names use FILETIME metadata and a separate parser; do not send after:/before:
/// as filename words. Keep quoted groups and exclusions when rebuilding the request.
pub fn names_query(
    query: &str,
    dates: Option<(i64, i64)>,
    folder: Option<&str>,
    ext: Option<&str>,
) -> (String, Option<(i64, i64)>) {
    let mut groups = Vec::new();
    let mut start = dates.map(|d| d.0);
    let mut end = dates.map(|d| d.1);
    for group in kfind_core::split_semicolon_groups(query) {
        let syntax = kfind_core::parse_query_syntax(&normalize_ext_lists(&group));
        // apply_syntax lets an explicit query date override a UI preset too.
        if syntax.start_date.is_some() {
            start = syntax.start_date;
        }
        if syntax.end_date.is_some() {
            end = syntax.end_date;
        }
        let mut parts = vec![syntax.clean_text];
        parts.extend(syntax.phrases.into_iter().map(|p| format!("\"{p}\"")));
        parts.extend(syntax.excludes.into_iter().map(|p| format!("-\"{p}\"")));
        if let Some(path) = syntax.folder {
            parts.push(format!("path:\"{path}\""));
        }
        if let Some(path) = folder {
            parts.push(format!("path:\"{path}\""));
        }
        let mut types = syntax.file_types.unwrap_or_default();
        if let Some(ext) = ext {
            types.extend(kfind_core::parse_ext_term(ext).unwrap_or_default());
        }
        if !types.is_empty() {
            parts.push(format!("ext:{}", types.join(",")));
        }
        parts.push("file:".into());
        groups.push(parts.join(" "));
    }
    let dates = if start.is_some() || end.is_some() {
        Some((start.unwrap_or(0), end.unwrap_or(99991231)))
    } else {
        None
    };
    (groups.join("; "), dates)
}

pub fn filter_name_dates(rows: &mut Vec<Row>, dates: Option<(i64, i64)>) {
    let Some((start, end)) = dates else { return };
    rows.retain(|row| {
        super::format_filetime(row.date)
            .and_then(|s| s.get(..10).map(|d| d.replace('-', "")))
            .and_then(|d| d.parse::<i64>().ok())
            .is_some_and(|d| start <= d && d <= end)
    });
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub answer: Answer,
    pub completed: ResultKind,
    pub final_part: bool,
    /// A failed source is completed too; later successes must not erase its error.
    pub errors: Vec<String>,
}

/// Windows lexical identity, independent of host OS; never opens the user's file.
pub fn path_key(path: &str) -> String {
    let mut p = path.replace('/', "\\").to_lowercase();
    if let Some(rest) = p.strip_prefix(r"\\?\unc\") {
        p = format!(r"\\{rest}");
    } else if let Some(rest) = p.strip_prefix(r"\\?\") {
        p = rest.to_owned();
    }
    let prefix = if p.starts_with(r"\\") {
        r"\\"
    } else if p.starts_with('\\') {
        r"\"
    } else {
        ""
    };
    let mut components = Vec::new();
    for part in p.split('\\').filter(|p| !p.is_empty() && *p != ".") {
        if part == ".."
            && components
                .last()
                .is_some_and(|p: &&str| *p != ".." && !p.ends_with(':'))
        {
            components.pop();
        } else {
            components.push(part);
        }
    }
    format!("{prefix}{}", components.join("\\"))
}

/// Highest returned source wins even when that source's display quota is full.
/// Input order is irrelevant; rank within each source remains stable.
pub fn merge(batches: &[(ResultKind, Vec<Row>)], limit: usize) -> Vec<Row> {
    let mut aliases = HashMap::new();
    for (kind, _) in TIERS {
        for (_, batch) in batches.iter().filter(|(k, _)| *k == kind) {
            for row in batch.iter().take(MAX_SEARCH_RESULTS) {
                if row.doc_id != 0 && !row.path.is_empty() {
                    aliases
                        .entry(row.doc_id)
                        .or_insert_with(|| path_key(&row.path));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for (kind, cap) in TIERS {
        let mut accepted = 0;
        for (_, batch) in batches.iter().filter(|(k, _)| *k == kind) {
            for (ordinal, row) in batch.iter().take(MAX_SEARCH_RESULTS).enumerate() {
                let key = if row.doc_id != 0 {
                    aliases
                        .get(&row.doc_id)
                        .cloned()
                        .unwrap_or_else(|| format!("id:{}", row.doc_id))
                } else if !row.path.is_empty() {
                    path_key(&row.path)
                } else {
                    format!("anonymous:{kind:?}:{ordinal}")
                };
                if !seen.insert(key) || accepted >= cap {
                    continue;
                }
                accepted += 1;
                let mut row = row.clone();
                row.match_kinds = vec![kind];
                row.rank = output.len();
                output.push(row);
            }
        }
    }
    output.truncate(limit.min(OUTPUT_LIMIT));
    output
}

/// One worker request, three incremental snapshots. Sources are called only after
/// the preceding snapshot has been published. Cancellation never publishes stale work.
pub fn stream(
    limit: usize,
    cancel: &AtomicBool,
    mut fetch: impl FnMut(ResultKind, usize) -> Result<Answer>,
    mut publish: impl FnMut(Snapshot),
) -> Result<()> {
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err("검색 취소".to_owned())
        } else {
            Ok(())
        }
    };
    let mut batches = Vec::with_capacity(3);
    let mut errors = Vec::new();
    let mut terms = Vec::new();
    let mut groups = Vec::new();
    let mut folder_counts = Vec::new();
    let mut autocompletes = Vec::new();
    for (kind, _) in TIERS {
        check()?;
        let result = fetch(kind, MAX_SEARCH_RESULTS);
        check()?;
        match result {
            Ok(mut answer) => {
                answer.rows.truncate(MAX_SEARCH_RESULTS);
                batches.push((kind, answer.rows));
                for term in answer.terms {
                    if !terms.contains(&term) {
                        terms.push(term);
                    }
                }
                if !answer.groups.is_empty() {
                    groups = answer
                        .groups
                        .into_iter()
                        .map(|g| g.trim().to_owned())
                        .collect();
                }
                if !answer.folder_counts.is_empty() {
                    folder_counts = answer.folder_counts;
                }
                if !answer.autocompletes.is_empty() {
                    autocompletes = answer.autocompletes;
                }
            }
            Err(error) => errors.push(format!("{}: {error}", label(kind))),
        }
        let answer = Answer {
            rows: merge(&batches, limit),
            terms: terms.clone(),
            groups: groups.clone(),
            folder_counts: folder_counts.clone(),
            autocompletes: autocompletes.clone(),
        };
        check()?;
        publish(Snapshot {
            answer,
            completed: kind,
            final_part: kind == ResultKind::Body,
            errors: errors.clone(),
        });
    }
    Ok(())
}

pub fn label(kind: ResultKind) -> &'static str {
    match kind {
        ResultKind::Names => "파일명",
        ResultKind::Hierarchy => "위계",
        ResultKind::Body => "본문",
    }
}

/// 전수 응답으로 확정한 세션. 기존 bounded merge/stream과 계약을 구분한다.
/// 생성 후 색인을 다시 읽지 않으므로 이어보기 도중 순서나 단이 바뀌지 않는다.
#[derive(Debug, Default)]
pub struct Session {
    tiers: [Vec<Row>; 3],
    pub terms: Vec<String>,
    pub groups: Vec<String>,
    pub folder_counts: Vec<(String, u32)>,
    pub autocompletes: Vec<String>,
    pub errors: Vec<String>,
    pub state: SessionState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionState {
    #[default]
    Ready,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor([usize; 3]);

#[derive(Debug)]
pub struct Page {
    pub answer: Answer,
    pub next: Option<Cursor>,
    pub totals: [usize; 3],
}

impl Session {
    /// 호출자는 후보가 잘리지 않은 완전한 원천 응답만 전달해야 한다.
    pub fn from_complete(batches: Vec<(ResultKind, Answer)>) -> Self {
        let mut aliases = HashMap::new();
        for (kind, _) in TIERS {
            for (_, answer) in batches.iter().filter(|(k, _)| *k == kind) {
                for row in &answer.rows {
                    if row.doc_id != 0 && !row.path.is_empty() {
                        aliases
                            .entry(row.doc_id)
                            .or_insert_with(|| path_key(&row.path));
                    }
                }
            }
        }
        let mut session = Self::default();
        let mut seen = HashSet::new();
        // 전수 Row는 복사하지 않고 승자만 세션으로 이동한다.
        let mut batches = batches;
        batches.sort_by_key(|(kind, _)| match kind {
            ResultKind::Names => 0,
            ResultKind::Hierarchy => 1,
            _ => 2,
        });
        for (kind, mut answer) in batches {
            let i = match kind {
                ResultKind::Names => 0,
                ResultKind::Hierarchy => 1,
                _ => 2,
            };
            for term in answer.terms {
                if !session.terms.contains(&term) {
                    session.terms.push(term);
                }
            }
            if !answer.groups.is_empty() {
                session.groups = answer.groups;
            }
            if !answer.folder_counts.is_empty() {
                session.folder_counts = answer.folder_counts;
            }
            if !answer.autocompletes.is_empty() {
                session.autocompletes = answer.autocompletes;
            }
            for (ordinal, mut row) in answer.rows.drain(..).enumerate() {
                let key = identity(&row, &aliases, kind, ordinal);
                if seen.insert(key) {
                    row.match_kinds = vec![kind];
                    row.rank = session.tiers[i].len();
                    session.tiers[i].push(row);
                }
            }
        }
        session
    }

    pub fn totals(&self) -> [usize; 3] {
        self.tiers.each_ref().map(Vec::len)
    }

    /// 커서는 세션 내부 위치이며 다른 세션과 섞지 않는다. limit=0은 오류.
    pub fn page(&self, cursor: Cursor, limit: usize) -> Result<Page> {
        if limit == 0
            || cursor
                .0
                .iter()
                .zip(&self.tiers)
                .any(|(n, rows)| *n > rows.len())
        {
            return Err("잘못된 통합 검색 페이지 요청".into());
        }
        let mut next = cursor;
        let mut rows = Vec::new();
        let limit = limit.min(OUTPUT_LIMIT);
        for (i, (_, cap)) in TIERS.into_iter().enumerate() {
            let count = cap
                .min(limit - rows.len())
                .min(self.tiers[i].len() - cursor.0[i]);
            rows.extend_from_slice(&self.tiers[i][cursor.0[i]..cursor.0[i] + count]);
            next.0[i] += count;
        }
        let has_more = next
            .0
            .iter()
            .zip(&self.tiers)
            .any(|(n, rows)| *n < rows.len());
        Ok(Page {
            answer: Answer {
                rows,
                terms: self.terms.clone(),
                groups: self.groups.clone(),
                folder_counts: self.folder_counts.clone(),
                autocompletes: self.autocompletes.clone(),
            },
            next: has_more.then_some(next),
            totals: self.totals(),
        })
    }
}

/// 첫 파일명은 20개만 요청해 먼저 발행한다. 이후 top=0은 명시적 전수 계약이다.
/// 상위 원천 실패 때 하위 단을 확정하지 않고 오류와 이미 검증된 단만 남긴다.
pub fn collect(
    limit: usize,
    cancel: &AtomicBool,
    fetch: impl FnMut(ResultKind, usize) -> Result<Answer>,
    publish: impl FnMut(Snapshot),
) -> Result<Session> {
    collect_with(limit, cancel, false, fetch, publish)
}

/// `keep_lower_without_names`: 앱 통합 보기(INTEG119)는 파일명 원천이 실패해도 위계·본문을 보인다.
/// 이때도 세션은 `Incomplete`이고 실패 사유가 `errors`에 남는다.
pub fn collect_with(
    limit: usize,
    cancel: &AtomicBool,
    keep_lower_without_names: bool,
    mut fetch: impl FnMut(ResultKind, usize) -> Result<Answer>,
    mut publish: impl FnMut(Snapshot),
) -> Result<Session> {
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err("검색 취소".to_owned())
        } else {
            Ok(())
        }
    };
    check()?;
    let first = fetch(ResultKind::Names, limit.min(20).max(1));
    check()?;
    let mut provisional = None;
    if let Ok(first) = first {
        provisional = Some(first.clone());
        let session = Session::from_complete(vec![(ResultKind::Names, first)]);
        publish(Snapshot {
            answer: session.page(Cursor::default(), limit)?.answer,
            completed: ResultKind::Names,
            final_part: false,
            errors: Vec::new(),
        });
    }
    let mut batches = Vec::new();
    let mut errors = Vec::new();
    for (kind, _) in TIERS {
        check()?;
        let mut stop = false;
        match fetch(kind, 0) {
            Ok(answer) => batches.push((kind, answer)),
            Err(error) => {
                stop = !(keep_lower_without_names && kind == ResultKind::Names);
                if kind == ResultKind::Names {
                    if let Some(first) = provisional.take() {
                        batches.push((kind, first));
                    }
                }
                errors.push(format!("{} 전수 조회: {error}", label(kind)));
            }
        }
        check()?;
        // 파일명 첫 화면은 이미 발행했다. 다음 위계 스냅샷에 전수 이름을 반영해
        // 같은 첫 페이지를 한 번 더 그리는 비용을 피한다.
        if kind == ResultKind::Names && provisional.is_some() && errors.is_empty() {
            continue;
        }
        // 첫 화면만 복사한다. 전수 응답 복제는 하지 않는다.
        let mut answer = preview(&batches, limit);
        for (_, part) in &batches {
            for term in &part.terms {
                if !answer.terms.contains(term) {
                    answer.terms.push(term.clone());
                }
            }
            if !part.groups.is_empty() {
                answer.groups = part.groups.clone();
            }
            if !part.folder_counts.is_empty() {
                answer.folder_counts = part.folder_counts.clone();
            }
            if !part.autocompletes.is_empty() {
                answer.autocompletes = part.autocompletes.clone();
            }
        }
        publish(Snapshot {
            answer,
            completed: kind,
            final_part: kind == ResultKind::Body || stop,
            errors: errors.clone(),
        });
        if stop {
            break;
        }
    }
    check()?;
    let mut session = Session::from_complete(batches);
    if !errors.is_empty() {
        session.state = SessionState::Incomplete;
    }
    session.errors = errors;
    Ok(session)
}

/// 전역 중복 검사는 전수에 적용하되 발행하는 Row 복사는 첫 페이지로 제한한다.
fn preview(batches: &[(ResultKind, Answer)], limit: usize) -> Answer {
    let mut aliases = HashMap::new();
    for (kind, _) in TIERS {
        for (_, answer) in batches.iter().filter(|(k, _)| *k == kind) {
            for row in &answer.rows {
                if row.doc_id != 0 && !row.path.is_empty() {
                    aliases
                        .entry(row.doc_id)
                        .or_insert_with(|| path_key(&row.path));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for (kind, cap) in TIERS {
        let mut count = 0;
        for (_, answer) in batches.iter().filter(|(k, _)| *k == kind) {
            for (ordinal, row) in answer.rows.iter().enumerate() {
                let key = identity(row, &aliases, kind, ordinal);
                if !seen.insert(key) || count >= cap || rows.len() >= limit.min(OUTPUT_LIMIT) {
                    continue;
                }
                let mut row = row.clone();
                row.match_kinds = vec![kind];
                row.rank = count;
                rows.push(row);
                count += 1;
            }
        }
    }
    Answer {
        rows,
        ..Answer::default()
    }
}

fn identity(row: &Row, aliases: &HashMap<u64, String>, kind: ResultKind, ordinal: usize) -> String {
    if row.doc_id != 0 {
        aliases
            .get(&row.doc_id)
            .cloned()
            .unwrap_or_else(|| format!("id:{}", row.doc_id))
    } else if !row.path.is_empty() {
        path_key(&row.path)
    } else {
        format!("anonymous:{kind:?}:{ordinal}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, id: u64) -> Row {
        Row {
            path: path.into(),
            doc_id: id,
            ..Row::default()
        }
    }

    #[test]
    fn identity_handles_windows_paths_id_aliases_and_anonymous_rows() {
        assert_eq!(
            path_key(r"\\?\C:\자료\.\임시\..\예산.hwp\"),
            path_key("c:/자료/예산.hwp")
        );
        assert_eq!(
            path_key(r"\\?\UNC\server\share\a.hwp"),
            path_key("//SERVER/share/a.hwp")
        );
        let rows = merge(
            &[
                (ResultKind::Body, vec![row("", 41), row("", 0), row("", 0)]),
                (ResultKind::Hierarchy, vec![row("c:/자료/예산.hwp", 41)]),
                (
                    ResultKind::Names,
                    vec![row(r"C:\자료\예산.hwp", 0), row(r"C:\다른폴더\예산.hwp", 0)],
                ),
            ],
            80,
        );
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].match_kinds, [ResultKind::Names]);
        assert_eq!(rows[1].match_kinds, [ResultKind::Names]);
    }

    #[test]
    fn quotas_apply_after_dedup_and_hidden_higher_candidates_do_not_reappear() {
        let names = (1..=25).map(|id| row(&format!("/{id}"), id)).collect();
        let hierarchy = (1..=50).map(|id| row(&format!("/{id}"), id)).collect();
        let body = (1..=100).map(|id| row(&format!("/{id}"), id)).collect();
        let rows = merge(
            &[
                (ResultKind::Body, body),
                (ResultKind::Names, names),
                (ResultKind::Hierarchy, hierarchy),
            ],
            80,
        );
        assert_eq!(rows.len(), 80);
        assert_eq!(rows[20].doc_id, 26);
        assert_eq!(rows[40].doc_id, 51);
        assert_eq!(rows[79].doc_id, 90);
        assert!(rows.iter().all(|r| r.match_kinds.len() == 1));
    }

    #[test]
    fn stream_publishes_names_before_requesting_content_and_keeps_errors() {
        use std::cell::Cell;
        let published = Cell::new(0);
        let mut snapshots = Vec::new();
        stream(
            80,
            &AtomicBool::new(false),
            |kind, top| {
                assert_eq!(top, MAX_SEARCH_RESULTS);
                match kind {
                    ResultKind::Names => Ok(Answer {
                        rows: vec![row("/name", 0)],
                        ..Answer::default()
                    }),
                    ResultKind::Hierarchy => {
                        assert_eq!(published.get(), 1);
                        Err("offline".into())
                    }
                    ResultKind::Body => {
                        assert_eq!(published.get(), 2);
                        Ok(Answer::default())
                    }
                }
            },
            |s| {
                published.set(published.get() + 1);
                snapshots.push(s);
            },
        )
        .unwrap();
        assert_eq!(snapshots.len(), 3);
        assert!(!snapshots[0].final_part);
        assert!(snapshots[2].final_part);
        assert_eq!(snapshots[2].errors, ["위계: offline"]);
        assert_eq!(snapshots[2].answer.rows.len(), 1);
    }

    #[test]
    fn cancellation_before_fetch_after_fetch_and_between_parts_never_publishes() {
        let stopped = AtomicBool::new(true);
        assert!(
            stream(
                80,
                &stopped,
                |_, _| panic!("must not fetch"),
                |_| panic!("must not publish")
            )
            .is_err()
        );
        stopped.store(false, Ordering::Relaxed);
        assert!(
            stream(
                80,
                &stopped,
                |_, _| {
                    stopped.store(true, Ordering::Relaxed);
                    Ok(Answer::default())
                },
                |_| panic!("must not publish")
            )
            .is_err()
        );
        stopped.store(false, Ordering::Relaxed);
        let mut calls = 0;
        assert!(
            stream(
                80,
                &stopped,
                |_, _| {
                    calls += 1;
                    Ok(Answer::default())
                },
                |_| stopped.store(true, Ordering::Relaxed)
            )
            .is_err()
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn old_settings_stay_disabled_and_flag_roundtrips() {
        let mut settings: crate::settings::Settings = serde_json::from_str("{}").unwrap();
        assert!(!settings.unified_search);
        settings.unified_search = true;
        let decoded: crate::settings::Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(decoded.unified_search);
    }

    #[test]
    fn all_five_sorts_preserve_tiers_and_summary_uses_the_winning_reason() {
        let batches = TIERS.map(|(kind, _)| {
            (
                kind,
                vec![
                    Row {
                        name: "나".into(),
                        date: 2,
                        count: 4,
                        ..row(&format!("/{kind:?}/2"), 0)
                    },
                    Row {
                        name: "가".into(),
                        date: 1,
                        count: 8,
                        ..row(&format!("/{kind:?}/1"), 0)
                    },
                ],
            )
        });
        let original = merge(&batches, 80);
        for mode in [
            super::super::BodySort::Relevance,
            super::super::BodySort::Confidence,
            super::super::BodySort::Date,
            super::super::BodySort::Oldest,
            super::super::BodySort::Name,
        ] {
            let mut rows = original.clone();
            sort_within_tiers(&mut rows, mode);
            assert!(rows.windows(2).all(|w| tier(&w[0]) <= tier(&w[1])));
            if mode == super::super::BodySort::Name {
                assert_eq!(
                    rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
                    ["가", "나", "가", "나", "가", "나"]
                );
            }
            let groups = match_groups(&rows.iter().collect::<Vec<_>>());
            assert_eq!((groups.filename, groups.body), (2, 4));
        }
    }

    #[test]
    fn scope_dates_and_related_quoted_groups_do_not_turn_into_name_terms() {
        let (query, dates) = names_query(
            r#"2026; "공사 계약" -초안 ext:hwp;pdf after:2026-01-01 before:2026-12-31"#,
            None,
            Some(r"C:\합성 자료"),
            Some("txt"),
        );
        assert!(!query.contains("after:"));
        assert!(!query.contains("before:"));
        assert!(query.contains("ext:hwp,pdf,txt"));
        assert!(query.contains(r#"-"초안""#));
        assert_eq!(dates, Some((20260101, 20261231)));
        assert_eq!(kfind_core::split_semicolon_groups(&query).len(), 2);
        assert_eq!(query_terms("2026; 공사 계약"), ["2026", "계약", "공사"]);
        let scoped = scoped_query(
            "예산; 집행",
            Some((20260101, 20261231)),
            None,
            Some("pdf"),
            crate::tm_state::MatchMode::Exact,
        );
        assert!(scoped.contains("\"예산\""));
        assert_eq!(kfind_core::split_semicolon_groups(&scoped).len(), 2);
        let mut rows = vec![row("/unknown", 1)];
        filter_name_dates(&mut rows, dates);
        assert!(rows.is_empty());
    }

    #[test]
    fn failed_sources_still_complete_and_a_total_limit_is_enforced() {
        let mut snapshots = Vec::new();
        stream(
            7,
            &AtomicBool::new(false),
            |_, _| Err("offline".into()),
            |s| snapshots.push(s),
        )
        .unwrap();
        assert_eq!(snapshots.len(), 3);
        assert!(snapshots[2].final_part);
        assert_eq!(snapshots[2].errors.len(), 3);
        assert!(snapshots[2].answer.rows.is_empty());
        let rows = (1..=1000).map(|id| row(&format!("/{id}"), id)).collect();
        assert_eq!(merge(&[(ResultKind::Body, rows)], 7).len(), 7);
    }
}
