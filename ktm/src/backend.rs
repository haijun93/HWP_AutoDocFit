//! Read-only mapped content queries and a bounded service-pipe alternative.
//! The published index holds comparison text and positions only (no original text).
//! Snippets come from the app's SQLite paragraph table (Q-A6), read-only, and only for
//! rows about to be shown; without that table `snippet` stays empty.
use crate::paragraphs::Paragraphs;
use kfind_core::{DocHit, Index, Order, SearchOptions};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

pub mod unified;

pub type Result<T> = std::result::Result<T, String>;
pub const PIPE_NAME: &str = r"\\.\pipe\HwpAutoDocFit.Index";
/// 검색·이름 질의 기본 제한 시간.
pub const PIPE_SEARCH_TIMEOUT: Duration = Duration::from_secs(5);
/// 상태 확인 등 가벼운 호출 제한 시간.
pub const PIPE_LIGHT_TIMEOUT: Duration = Duration::from_secs(2);
pub const PIPE_TIMEOUT_ERROR: &str = "서비스 응답 시간이 초과되었습니다";
const MAX_TIMED_OUT_PIPE_CALLS: usize = 4;
static ACTIVE_PIPE_CALLS: AtomicUsize = AtomicUsize::new(0);
const LIMIT: usize = 50;
pub const MAX_SEARCH_RESULTS: usize = 200;
pub fn clamp_search_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_SEARCH_RESULTS)
}
/// 검색 직후 원문을 함께 가져올 앞 결과 수(기본 창 크기에서 보이는 줄 + 여유). 나머지는 스크롤할 때.
pub const SNIPPET_PREFETCH: usize = 8;
/// 발췌: 첫 일치 앞 글자 수, 최대 글자 수.
const EXCERPT_BEFORE: usize = 16;
const EXCERPT_MAX: usize = 120;

pub const fn is_cloud_only_attributes(attrs: u32) -> bool {
    attrs & (0x1000 | 0x400000 | 0x40000) != 0
}

fn path_is_cloud_only(path: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        return std::fs::metadata(path)
            .map(|metadata| is_cloud_only_attributes(metadata.file_attributes()))
            .unwrap_or(false);
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// 검색 한 번의 결과: 줄과 강조 낱말(검색어 낱말 + 엔진이 나눈 조각), 폴더별 개수 및 자동완성.
#[derive(Clone, Debug, Default)]
pub struct Answer {
    pub rows: Vec<Row>,
    pub terms: Vec<String>,
    /// 세미콜론으로 나눈 연관 검색어 묶음(순서 보존).
    pub groups: Vec<String>,
    pub folder_counts: Vec<(String, u32)>,
    pub autocompletes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Row {
    /// Complete group evidence, independent of the currently displayed excerpt.
    pub group_matches: Vec<GroupMatch>,
    pub match_kinds: Vec<crate::tm_state::ResultKind>,
    pub doc_id: u64,
    pub path: String,
    pub name: String,
    pub parent: String,
    pub snippet: String,
    pub location: String,
    pub level: u8,
    pub count: u32,
    /// 첫 일치 문장의 문단 번호(원문 찾기용).
    pub para: Option<u32>,
    /// 원문 찾기를 이미 해 봤는지(없어도 다시 찾지 않음).
    pub snippet_tried: bool,
    /// 일치 문단 전체 원문(결과 선택 시 3줄 미리보기용).
    pub preview: String,
    /// 수정일시(수정일순 정렬용).
    pub date: i64,
    /// 원래 검색 순위(관련도순 복원용).
    pub rank: usize,
}

#[derive(Clone, Debug, Default)]
pub struct GroupMatch {
    pub group_index: usize,
    pub query: String,
    pub paragraphs: Vec<(u32, u8)>,
    pub filename: bool,
}

#[derive(Clone, Debug, Default)]
pub struct DocumentStats {
    pub total: usize,
    pub indexed: usize,
    pub total_bytes: Option<u64>,
    pub by_type: Vec<(String, usize)>,
    pub by_year: Vec<(i64, usize)>,
    pub by_folder: Vec<(String, usize)>,
    pub recent: Vec<Row>,
    pub largest: Vec<Row>,
}

/// Computes statistics from available row metadata. Byte totals remain unknown because
/// the search index deliberately does not store file sizes.
pub fn document_stats(rows: &[Row]) -> DocumentStats {
    use std::collections::BTreeMap;
    let mut types = BTreeMap::<String, usize>::new();
    let mut years = BTreeMap::<i64, usize>::new();
    let mut folders = BTreeMap::<String, usize>::new();
    for row in rows {
        *types
            .entry(extension_label(&row.name).to_ascii_lowercase())
            .or_default() += 1;
        if row.date > 0 {
            *years.entry(row.date / 10000).or_default() += 1;
        }
        if !row.parent.is_empty() {
            *folders.entry(row.parent.clone()).or_default() += 1;
        }
    }
    let mut recent: Vec<Row> = rows.iter().filter(|r| r.date > 0).cloned().collect();
    recent.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.name.cmp(&b.name)));
    recent.truncate(5);
    DocumentStats {
        total: rows.len(),
        indexed: rows.iter().filter(|r| r.doc_id != 0).count(),
        total_bytes: None,
        by_type: types.into_iter().collect(),
        by_year: years.into_iter().rev().collect(),
        by_folder: {
            let mut v: Vec<_> = folders.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            v.truncate(10);
            v
        },
        recent,
        largest: Vec::new(),
    }
}

/// Keep one representative for versioned names; prefer an explicit final/latest name,
/// otherwise retain the highest vN token and use stable lexical ties.
pub fn group_document_versions(rows: &[Row]) -> Vec<Row> {
    use std::collections::BTreeMap;
    let mut groups = BTreeMap::<String, Vec<Row>>::new();
    for row in rows {
        let stem = Path::new(&row.name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&row.name);
        let key = version_group_key(stem);
        groups.entry(key).or_default().push(row.clone());
    }
    groups
        .into_values()
        .map(|mut group| {
            group.sort_by(|a, b| {
                version_rank(&a.name)
                    .cmp(&version_rank(&b.name))
                    .reverse()
                    .then_with(|| b.date.cmp(&a.date))
                    .then_with(|| a.name.cmp(&b.name))
            });
            group.remove(0)
        })
        .collect()
}

fn version_group_key(stem: &str) -> String {
    let lower = stem
        .to_lowercase()
        .replace("final", "")
        .replace("latest", "")
        .replace("최종", "");
    let mut key = String::new();
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'v' && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            continue;
        }
        key.push(chars[i]);
        i += 1;
    }
    key.trim_matches([' ', '-', '_', '(', ')', '[', ']'])
        .to_string()
}

fn version_rank(name: &str) -> (u8, u64) {
    let lower = name.to_lowercase();
    if lower.contains("final") || lower.contains("최종") || lower.contains("latest") {
        return (2, 0);
    }
    let chars: Vec<char> = lower.chars().collect();
    for i in 0..chars.len().saturating_sub(1) {
        if chars[i] == 'v' && chars[i + 1].is_ascii_digit() {
            let digits: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            return (1, digits.parse().unwrap_or(0));
        }
    }
    (0, 0)
}

/// QMS-RESULTS: 위치 메타로부터 표시할 배지 텍스트를 계산한다.
/// 쪽·시트·행 등 kfind 위치 메타가 있는 것만 반환하고, 일반 문단이나 위계는 생략(None).
pub fn compute_location_badge(location: &str) -> Option<String> {
    let trimmed = location
        .trim()
        .trim_matches(|c| c == '⟦' || c == '⟧' || c == '[' || c == ']' || c == '(' || c == ')')
        .trim();
    if trimmed.is_empty() {
        return None;
    }
    // 쪽 메타: "3쪽", "12쪽", "1쪽", "3p" 등
    let has_page = trimmed.contains('쪽') || trimmed.ends_with('p') || trimmed.ends_with('P');
    // 시트 메타: "시트1 12행", "시트 4행", "절대경로시트 1행" 등
    let has_sheet = trimmed.contains("시트");
    // 행 메타: "1행", "12행", "매출 12행" 등
    let has_row = trimmed.contains('행');

    if has_page || has_sheet || has_row {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// QMS-RESULTS: 본문 탭 결과 정렬 옵션 (관련도·수정일·이름).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BodySort {
    #[default]
    Relevance,
    Confidence,
    Date,
    Oldest,
    Name,
}

impl BodySort {
    pub fn next(&self) -> Self {
        match self {
            BodySort::Relevance => BodySort::Date,
            BodySort::Confidence => BodySort::Date,
            BodySort::Date => BodySort::Name,
            BodySort::Oldest => BodySort::Name,
            BodySort::Name => BodySort::Relevance,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            BodySort::Relevance => "관련도순",
            BodySort::Confidence => "신뢰도순",
            BodySort::Date => "최신순",
            BodySort::Oldest => "오래된순",
            BodySort::Name => "이름순",
        }
    }
}

/// QMS-RESULTS: 본문 탭 결과를 지정한 정렬 방식에 따라 정렬한다.
pub fn sort_rows(rows: &mut [Row], sort: BodySort) {
    match sort {
        BodySort::Relevance => {
            rows.sort_by_key(|r| r.rank);
        }
        BodySort::Confidence => {
            rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.rank.cmp(&b.rank)));
        }
        BodySort::Date => {
            rows.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.rank.cmp(&b.rank)));
        }
        BodySort::Oldest => {
            rows.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.rank.cmp(&b.rank)));
        }
        BodySort::Name => {
            rows.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.rank.cmp(&b.rank)));
        }
    }
}

/// Readable local date label; index dates contain day precision, so time is midnight.
pub fn format_korean_datetime(ymd: i64) -> Option<String> {
    let y = ymd / 10000;
    let m = (ymd / 100) % 100;
    let d = ymd % 100;
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if y < 1 || d < 1 || d > month_days {
        return None;
    }
    Some(format!("{y:04}. {m:02}. {d:02}. 오전 12:00"))
}

pub fn extension_label(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "hwp" => "HWP",
        "hwpx" => "HWPX",
        "docx" => "DOCX",
        "pdf" => "PDF",
        "xls" | "xlsx" => "XLS",
        "ppt" | "pptx" => "PPT",
        "md" => "MD",
        "txt" => "TXT",
        _ => "FILE",
    }
}

pub fn shift_ymd_months(ymd: i64, delta_months: i64) -> i64 {
    let y = ymd / 10000;
    let m = (ymd / 100) % 100;
    let d = ymd % 100;
    let absolute = y * 12 + (m - 1) + delta_months;
    let target_y = absolute.div_euclid(12);
    let target_m = absolute.rem_euclid(12) + 1;
    let last = match target_m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if target_y % 4 == 0 && (target_y % 100 != 0 || target_y % 400 == 0) => 29,
        _ => 28,
    };
    target_y * 10000 + target_m * 100 + d.min(last)
}

/// Appends a Korean date/time using the index's day precision.
pub fn format_korean_datetime_at_hour(ymd: i64, hour: u8, minute: u8) -> Option<String> {
    let base = format_korean_datetime(ymd)?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let suffix = if hour < 12 { "오전" } else { "오후" };
    let hour12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    Some(format!(
        "{} {suffix} {hour12:02}:{minute:02}",
        base.split(" ").take(3).collect::<Vec<_>>().join(" ")
    ))
}

/// Transforms the plain terms of a query while retaining its path/date/ext/exclusion syntax.
pub fn query_for_match_mode(query: &str, mode: crate::tm_state::MatchMode) -> String {
    if mode == crate::tm_state::MatchMode::AllTerms {
        return query.to_owned();
    }
    let syntax = kfind_core::parse_query_syntax(query);
    if syntax.clean_text.is_empty() {
        return query.to_owned();
    }
    let mut parts = Vec::new();
    let quoted = format!("\"{}\"", syntax.clean_text);
    if mode == crate::tm_state::MatchMode::Exact {
        parts.push(quoted);
    } else {
        parts.push(syntax.clean_text.clone());
    }
    if let Some(folder) = syntax.folder {
        parts.push(format!("path:\"{folder}\""));
    }
    if let Some(types) = syntax.file_types {
        parts.push(format!("ext:{}", types.join(",")));
    }
    if let Some(start) = syntax.start_date {
        parts.push(format!("after:{}", crate::tm_state::format_ymd(start)));
    }
    if let Some(end) = syntax.end_date {
        parts.push(format!("before:{}", crate::tm_state::format_ymd(end)));
    }
    parts.extend(syntax.excludes.into_iter().map(|w| format!("-{w}")));
    for phrase in syntax.phrases {
        parts.push(format!("\"{phrase}\""));
    }
    parts.join(" ")
}

pub struct Backend {
    related_queries: bool,
    root: PathBuf,
    index: Index,
    table_stamp: Option<(u64, Option<std::time::SystemTime>)>,
    paths: HashMap<u64, String>,
    para_path: Option<PathBuf>,
    paragraphs: Option<Paragraphs>,
    para_tried: bool,
}

impl Backend {
    fn search_related(&mut self, query: &str, date_range: Option<(i64,i64)>, folder: Option<&str>,
                      ext: Option<&str>, top: usize,
                      cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
                      _skip_cloud_network: bool) -> Result<Answer> {
        use kfind_core::related_query::{parse_related_query, related_matches};
        let groups = parse_related_query(query)?;
        if groups.is_empty() { return Ok(Answer::default()); }
        self.index.reload().map_err(|e|e.to_string())?;
        self.open_paragraphs();
        let paragraphs = self.paragraphs.as_mut().ok_or("묶음 검색에 필요한 문단 색인이 없습니다.")?;
        self.paths = read_paths(&self.root.join("doctable.jsonl"), None)?;
        let mut maps = Vec::<HashMap<u64,Row>>::new();
        for (group_index, group) in groups.iter().enumerate() {
            let anchor=group.terms.iter().max_by_key(|t|t.chars().count()).expect("nonempty group");
            let opts = SearchOptions {
                top: 0, refs_per_doc: usize::MAX, split: kfind_core::SplitMode::Off,
                phrases: vec![anchor.clone()], cancel: cancel.clone(),
                folder: folder.map(str::to_owned),
                file_types: ext.and_then(kfind_core::parse_ext_term),
                start_date: date_range.map(|d|d.0), end_date: date_range.map(|d|d.1),
                ..SearchOptions::default()
            };
            // Bitmap candidates are a superset: the stored raw paragraph verifies
            // punctuation and quoted whitespace using the common literal contract.
            let result = self.index.search_by("", &opts, Order::Hierarchy).map_err(|e|e.to_string())?;
            let mut found = HashMap::new();
            for hit in result.hits {
                if cancel.as_ref().is_some_and(|c|c.load(Ordering::Relaxed)) { return Err("검색 취소".into()); }
                let Some(path)=self.paths.get(&hit.doc_id) else { continue; };
                let mut matched = std::collections::BTreeMap::new();
                let mut primary = None;
                for sentence in &hit.sentences {
                    if matched.contains_key(&sentence.para) { continue; }
                    if let Some((body, location))=paragraphs.body_and_location(path,sentence.para)? {
                        if related_matches(&body,group) {
                            matched.insert(sentence.para,sentence.level);
                            if primary.as_ref().is_none_or(|p: &(u8,u32,String,Option<String>)| (sentence.level,sentence.para)<(p.0,p.1)) {
                                primary=Some((sentence.level,sentence.para,body,location));
                            }
                        }
                    }
                }
                if let Some((level,para,body,location))=primary {
                    let mut r=row(&hit,Some(path));
                    r.level=level; r.para=Some(para); r.count=matched.len() as u32;
                    r.snippet=crate::highlight::excerpt(&body,&group.terms,EXCERPT_BEFORE,EXCERPT_MAX);
                    r.preview=body; r.snippet_tried=true;
                    if let Some(location)=location { r.location=location; }
                    r.group_matches.push(GroupMatch { group_index,query:group.query.clone(),paragraphs:matched.into_iter().collect(),filename:false });
                    found.insert(hit.doc_id,r);
                }
            }
            maps.push(found);
        }
        let mut rows: Vec<Row>=maps.remove(0).into_values().filter(|r|maps.iter().all(|m|m.contains_key(&r.doc_id))).collect();
        for r in &mut rows {
            for map in &maps {
                let other=&map[&r.doc_id];
                r.count=r.count.saturating_add(other.count);
                r.group_matches.extend(other.group_matches.clone());
                if (other.level,other.para)<(r.level,r.para) {
                    r.level=other.level; r.para=other.para; r.snippet=other.snippet.clone();
                    r.preview=other.preview.clone(); r.location=other.location.clone();
                }
            }
        }
        rows.sort_by(|a,b|a.level.cmp(&b.level).then_with(||b.count.cmp(&a.count)).then_with(||a.path.to_lowercase().cmp(&b.path.to_lowercase())));
        rows.truncate(clamp_search_limit(top));
        for (i,r) in rows.iter_mut().enumerate() { r.rank=i; }
        Ok(Answer { rows, terms:groups.iter().flat_map(|g|g.terms.clone()).collect(),
            groups:groups.iter().map(|g|g.query.clone()).collect(), ..Answer::default() })
    }

    pub fn open(root: &Path) -> Result<Self> {
        // Index::open treats a missing manifest as empty; the UI needs an explicit
        // unavailable state and must never create the user's index directory.
        if !root.join("MANIFEST.kfm").is_file() {
            return Err("본문 색인이 없습니다. 색인 서비스에서 먼저 색인해 주세요.".into());
        }
        let index = Index::open(root).map_err(|e| match e {
            kfind_core::Error::FormatVersion { .. } | kfind_core::Error::RuleVersion { .. } => {
                // 업데이트 직후 옛 형식 색인: 서비스에 질의 한 번을 보내 즉시 새로 만들게 한다(응답은 쓰지 않음).
                let _ = pipe_call_with_timeout(
                    PIPE_NAME,
                    &json!({"command":"query","id":1,"text":"색인","top":1}),
                    PIPE_LIGHT_TIMEOUT,
                );
                "색인을 새 형식으로 다시 만드는 중입니다. 잠시 뒤 다시 검색해 주세요.".to_string()
            }
            other => other.to_string(),
        })?;
        Ok(Self {
            related_queries: false,
            root: root.to_owned(),
            index,
            table_stamp: None,
            paths: HashMap::new(),
            para_path: None,
            paragraphs: None,
            para_tried: false,
        })
    }

    /// 원문 문단 표 위치(없으면 원문 없이 위치만 보여 줌).
    pub fn with_paragraphs(mut self, path: Option<PathBuf>) -> Self {
        self.para_path = path;
        self
    }

    pub fn with_related_queries(mut self, enabled: bool) -> Self {
        self.related_queries = enabled;
        self
    }

    /// 시작할 때 한 번: 문서 표 읽기, 문단 표 열기, 질의 한 번으로 매핑 쪽을 데운다(창 표시 경로에서 무거운 일 0).
    pub fn warm(&mut self) -> Result<()> {
        self.search_cancel("문서", false, None)?;
        self.search_cancel("문서", true, None)?;
        self.open_paragraphs();
        if let Some(p) = self.paragraphs.as_mut() {
            p.body("", 0)?;
        }
        Ok(())
    }

    fn open_paragraphs(&mut self) {
        if !self.para_tried {
            self.para_tried = true;
            self.paragraphs = self
                .para_path
                .as_deref()
                .and_then(|p| Paragraphs::open(p).ok());
        }
    }

    pub fn has_paragraphs(&mut self) -> bool {
        self.open_paragraphs();
        self.paragraphs.is_some()
    }

    /// 아직 찾아보지 않은 줄의 원문 발췌를 채운다(문단 표가 없으면 아무것도 안 함).
    pub fn fill_snippets(&mut self, rows: &mut [Row], terms: &[String]) {
        self.fill_snippets_with_cloud_policy(rows, terms, false);
    }

    pub fn fill_snippets_with_cloud_policy(
        &mut self,
        rows: &mut [Row],
        terms: &[String],
        skip_cloud_network: bool,
    ) {
        self.open_paragraphs();
        let Some(p) = self.paragraphs.as_mut() else {
            return;
        };
        for row in rows.iter_mut().filter(|r| !r.snippet_tried) {
            row.snippet_tried = true;
            let Some(para) = row.para else { continue };
            if row.path.is_empty() {
                continue;
            }
            if skip_cloud_network && path_is_cloud_only(&row.path) {
                continue;
            }
            if let Ok(Some((body, loc_label))) = p.body_and_location(&row.path, para) {
                row.snippet = crate::highlight::excerpt(&body, terms, EXCERPT_BEFORE, EXCERPT_MAX);
                row.preview = body;
                if let Some(lbl) = loc_label {
                    if !lbl.is_empty() {
                        row.location = lbl;
                    }
                }
            }
        }
    }

    pub fn search(&mut self, query: &str, hierarchy: bool) -> Result<Vec<Row>> {
        self.search_cancel(query, hierarchy, None).map(|a| a.rows)
    }

    /// SEMI3: 2글자부터 실시간 접두사 자동 완성 (빈도순).
    pub fn autocomplete(&self, prefix: &str, limit: usize) -> Vec<kfind_core::TopicItem> {
        let prefix = prefix.trim();
        if prefix.chars().count() < 2 {
            return Vec::new();
        }
        self.index.autocomplete(prefix, limit)
    }

    /// SEMI3: 빈 검색어 첫 화면 주제 1~10위.
    pub fn top_topics(&self, limit: usize) -> Vec<kfind_core::TopicItem> {
        self.index.top_topics(limit)
    }

    /// SEMI2: 현재 검색어에 대한 폴더별 결과 수 집계.
    pub fn count_by_folder(&self, query: &str) -> Vec<kfind_core::FolderCount> {
        let syntax = kfind_core::parse_query_syntax(query);
        let mut opts = SearchOptions::default();
        let _ = self.index.apply_syntax(&syntax, &mut opts);
        let q = if syntax.clean_text.is_empty() && !syntax.phrases.is_empty() {
            ""
        } else if syntax.clean_text.is_empty() {
            query
        } else {
            &syntax.clean_text
        };
        self.index.count_by_folder(q, &opts).unwrap_or_default()
    }

    pub fn search_cancel(
        &mut self,
        query: &str,
        hierarchy: bool,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Answer> {
        self.search_with_scope(query, hierarchy, None, None, cancel)
    }

    /// SEMI2 scope API(기간 거르기·폴더 거르기) 및 QMS-QUERY 문법을 반영한 검색.
    pub fn search_with_scope(
        &mut self,
        query: &str,
        hierarchy: bool,
        date_range: Option<(i64, i64)>,
        folder: Option<&str>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Answer> {
        self.search_with_scope_ext(
            query,
            hierarchy,
            date_range,
            folder,
            None,
            crate::tm_state::MatchMode::AllTerms,
            cancel,
        )
    }

    /// QMS-SCOPE: 기간·폴더·확장자 범위와 문법을 함께 반영한 검색.
    pub fn search_with_scope_ext(
        &mut self,
        query: &str,
        hierarchy: bool,
        date_range: Option<(i64, i64)>,
        folder: Option<&str>,
        ext: Option<&str>,
        match_mode: crate::tm_state::MatchMode,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Answer> {
        self.search_with_scope_ext_limit(
            query, hierarchy, date_range, folder, ext, match_mode, LIMIT, cancel,
        )
    }

    /// QMS-EXPORT: 지정 상한(최대 1,000건 등)을 반영한 범위 검색.
    pub fn search_with_scope_ext_limit(
        &mut self,
        query: &str,
        hierarchy: bool,
        date_range: Option<(i64, i64)>,
        folder: Option<&str>,
        ext: Option<&str>,
        match_mode: crate::tm_state::MatchMode,
        top: usize,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Answer> {
        self.search_with_scope_ext_limit_cloud(
            query, hierarchy, date_range, folder, ext, match_mode, top, cancel, false,
        )
    }

    pub fn search_with_scope_ext_limit_cloud(
        &mut self,
        query: &str,
        hierarchy: bool,
        date_range: Option<(i64, i64)>,
        folder: Option<&str>,
        ext: Option<&str>,
        match_mode: crate::tm_state::MatchMode,
        top: usize,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        skip_cloud_network: bool,
    ) -> Result<Answer> {
        if self.related_queries {
            return self.search_related(query, date_range, folder, ext, top, cancel, skip_cloud_network);
        }
        self.index.reload().map_err(|e| e.to_string())?;
        let query = query.trim();
        if query.is_empty() {
            return Ok(Answer::default());
        }
        let (start_date, end_date) = match date_range {
            Some((s, e)) => (Some(s), Some(e)),
            None => (None, None),
        };
        let query_text = query_for_match_mode(query, match_mode);
        let syntax = kfind_core::parse_query_syntax(&query_text);
        let mut opts = SearchOptions {
            top: clamp_search_limit(top),
            refs_per_doc: 1,
            cancel,
            start_date,
            end_date,
            folder: folder.map(str::to_owned),
            ..SearchOptions::default()
        };
        self.index
            .apply_syntax(&syntax, &mut opts)
            .map_err(|e| e.to_string())?;
        if let Some(ext_val) = ext {
            if let Some(parsed_exts) = kfind_core::parse_ext_term(ext_val) {
                let mut list = opts.file_types.take().unwrap_or_default();
                for e in parsed_exts {
                    if !list.contains(&e) {
                        list.push(e);
                    }
                }
                opts.file_types = Some(list);
            }
        }

        let search_text = if syntax.clean_text.is_empty() && !syntax.phrases.is_empty() {
            ""
        } else if syntax.clean_text.is_empty() {
            query_text.as_str()
        } else {
            &syntax.clean_text
        };

        // 1글자 훑기는 이미 위계 먼저 정렬한다(모든 일치가 붙어 있는 일치).
        let order = if hierarchy {
            Order::Hierarchy
        } else {
            Order::Relevance
        };
        let result = if match_mode == crate::tm_state::MatchMode::AnyTerm {
            let mut merged = kfind_core::SearchResult::default();
            for term in syntax.clean_text.split_whitespace() {
                let one = if kfind_core::fold(term).chars().count() == 1 {
                    self.index.search_character(term, &opts)
                } else {
                    self.index.search_groups_by(term, &opts, order)
                }
                .map_err(|e| e.to_string())?;
                merged.words.extend(one.words);
                merged.hits.extend(one.hits);
            }
            let mut by_doc = HashMap::<u64, kfind_core::DocHit>::new();
            for hit in merged.hits {
                by_doc
                    .entry(hit.doc_id)
                    .and_modify(|old| {
                        old.count = old.count.saturating_add(hit.count);
                        old.level = old.level.min(hit.level);
                        old.adjacent |= hit.adjacent;
                    })
                    .or_insert(hit);
            }
            merged.hits = by_doc.into_values().collect();
            merged.hits.sort_by(|a, b| {
                b.adjacent
                    .cmp(&a.adjacent)
                    .then_with(|| a.level.cmp(&b.level))
                    .then_with(|| b.count.cmp(&a.count))
                    .then_with(|| b.date.cmp(&a.date))
            });
            merged.hits.truncate(top);
            merged.total_docs = merged.hits.len();
            merged
        } else if kfind_core::fold(search_text).chars().count() == 1 {
            self.index
                .search_character(search_text, &opts)
                .map_err(|e| e.to_string())?
        } else if !syntax.wildcards.is_empty() {
            self.index.search_syntax(&query_text, &opts).map_err(|e| e.to_string())?
        } else {
            self.index
                .search_groups_by(search_text, &opts, order)
                .map_err(|e| e.to_string())?
        };
        let mut terms: Vec<String> = search_text.split_whitespace().map(str::to_owned).collect();
        for p in &syntax.phrases {
            terms.push(p.clone());
        }
        for w in &result.words {
            terms.push(w.text.clone());
            terms.extend(w.pieces.iter().flatten().cloned());
        }
        if terms.is_empty() || kfind_core::fold(search_text).chars().count() == 1 {
            terms.push(kfind_core::fold(search_text));
        }
        terms.sort();
        terms.dedup();
        let path = self.root.join("doctable.jsonl");
        let stamp = match std::fs::metadata(&path) {
            Ok(m) => Some((m.len(), m.modified().ok())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        if stamp != self.table_stamp {
            let paths = read_paths(&path, None)?;
            self.paths = paths;
            self.table_stamp = stamp;
        }
        let mut rows: Vec<Row> = result
            .hits
            .iter()
            .map(|h| row(h, self.paths.get(&h.doc_id).map(String::as_str)))
            .collect();
        for (i, r) in rows.iter_mut().enumerate() {
            r.rank = i;
        }
        let n = rows.len().min(SNIPPET_PREFETCH);
        self.fill_snippets_with_cloud_policy(&mut rows[..n], &terms, skip_cloud_network);

        // SEMI3: 2글자부터 자동완성 후보
        let autocompletes = if query.chars().count() >= 2 {
            self.index
                .autocomplete(query, 8)
                .into_iter()
                .map(|item| item.term)
                .collect()
        } else {
            Vec::new()
        };

        // SEMI2: 현재 결과 기준 폴더별 개수 집계
        let mut folder_map: HashMap<String, u32> = HashMap::new();
        for r in &rows {
            if !r.parent.is_empty() {
                let folder_display =
                    if let Some(pos) = r.parent.rfind('\\').or_else(|| r.parent.rfind('/')) {
                        r.parent[pos + 1..].to_owned()
                    } else {
                        r.parent.clone()
                    };
                if !folder_display.is_empty() {
                    *folder_map.entry(folder_display).or_insert(0) += 1;
                }
            }
        }
        let mut folder_counts: Vec<(String, u32)> = folder_map.into_iter().collect();
        folder_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        folder_counts.truncate(6);

        Ok(Answer {
            rows,
            terms,
            groups: kfind_core::split_semicolon_groups(query),
            folder_counts,
            autocompletes,
        })
    }
}

pub fn display_query_groups(query: &str, enabled: bool) -> Vec<String> {
    if enabled {
        kfind_core::related_query::parse_related_query(query).unwrap_or_default().into_iter().map(|g|g.query).collect()
    } else { kfind_core::split_semicolon_groups(query) }
}

fn row(hit: &DocHit, path: Option<&str>) -> Row {
    let path = path.unwrap_or_default().to_owned();
    let (parent, name) = if let Some(pos) = path.rfind('\\').or_else(|| path.rfind('/')) {
        (path[..pos].to_owned(), path[pos + 1..].to_owned())
    } else {
        let p = Path::new(&path);
        let name = p
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("문서 #{} (경로 정보 없음)", hit.doc_id));
        let parent = p
            .parent()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        (parent, name)
    };
    let location = hit
        .sentences
        .first()
        .map(|s| format!("문단 {} · 위계 {}", s.para + 1, s.level))
        .unwrap_or_else(|| format!("위계 {}", hit.level));
    Row {
        group_matches: Vec::new(),
        match_kinds: Vec::new(),
        doc_id: hit.doc_id,
        path,
        name,
        parent,
        snippet: String::new(),
        preview: String::new(),
        location,
        level: hit.level,
        count: hit.count,
        date: hit.date,
        para: hit.sentences.first().map(|s| s.para),
        snippet_tried: false,
        rank: 0,
    }
}

/// Replay metadata without a writer/compactor (optionally restricted to IDs).
/// A final incomplete record is ignored exactly as the service does; bytes stay intact.
fn read_paths(path: &Path, ids: Option<&HashSet<u64>>) -> Result<HashMap<u64, String>> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut paths = HashMap::new();
    let mut keys = HashMap::<String, u64>::new();
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        // A document record may include many spans; bound it to the service's
        // per-document input ceiling instead of trusting arbitrary allocations.
        let n = reader
            .by_ref()
            .take(64 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if n > 64 * 1024 * 1024 {
            return Err("문서 표 기록 크기가 상한을 넘었습니다.".into());
        }
        // Ignore spans and the rest of DocMeta during deserialization so large
        // documents do not allocate a second copy of every sentence position.
        #[derive(serde::Deserialize)]
        struct Record {
            op: String,
            #[serde(default)]
            key: String,
            #[serde(default)]
            id: u64,
            #[serde(default)]
            meta: Meta,
        }
        #[derive(Default, serde::Deserialize)]
        struct Meta {
            #[serde(default)]
            path: String,
        }
        let value: Record = match serde_json::from_slice(&line) {
            Ok(v) => v,
            Err(_) if reader.fill_buf().map_err(|e| e.to_string())?.is_empty() => break,
            Err(e) => return Err(format!("문서 표가 손상되었습니다: {e}")),
        };
        let key = value.key.as_str();
        match value.op.as_str() {
            "put" => {
                if let Some(old) = keys.remove(key) {
                    paths.remove(&old);
                }
                let id = value.id;
                if ids.is_none_or(|ids| ids.contains(&id)) {
                    paths.insert(id, value.meta.path);
                    keys.insert(key.to_owned(), id);
                }
            }
            "del" => {
                if let Some(old) = keys.remove(key) {
                    paths.remove(&old);
                }
            }
            "next" => {}
            _ => return Err("문서 표에 알 수 없는 기록이 있습니다.".into()),
        }
    }
    Ok(paths)
}

/// Blocking alternative; call only on the search worker. The service has a
/// ten-second operation timeout. Responses are length-prefixed and capped here.
pub fn pipe_search(name: &str, query: &str, hierarchy: bool) -> Result<Vec<Row>> {
    pipe_search_limit(name, query, hierarchy, LIMIT)
}

/// QMS-EXPORT: 지정 상한을 반영한 파이프 검색.
pub fn pipe_search_limit(name: &str, query: &str, hierarchy: bool, top: usize) -> Result<Vec<Row>> {
    if kfind_core::fold(query).chars().count() < 2 {
        return Err(
            "서비스 파이프는 2글자부터 검색합니다. 1글자는 직접 색인 읽기가 필요합니다.".into(),
        );
    }
    if hierarchy {
        return Err("서비스 파이프는 전체 위계 순위 질의를 아직 지원하지 않습니다.".into());
    }
    let bytes = pipe_call_with_timeout(
        name,
        &json!({"command":"query", "id":1, "text":query,"top":top,"refs":1}),
        PIPE_SEARCH_TIMEOUT,
    )?;
    decode_rows(&bytes)
}

/// 길이 접두 JSON 요청 하나 → 응답 바이트(상한 8MB).
pub fn pipe_call(name: &str, request: &Value) -> Result<Vec<u8>> {
    pipe_call_with_timeout(name, request, PIPE_SEARCH_TIMEOUT)
}

/// 제한 시간 동안 작업 스레드의 블로킹 파이프 입출력을 기다린다.
pub fn pipe_call_with_timeout(name: &str, request: &Value, timeout: Duration) -> Result<Vec<u8>> {
    let request = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    let mut active = ACTIVE_PIPE_CALLS.load(Ordering::Acquire);
    loop {
        if active >= MAX_TIMED_OUT_PIPE_CALLS {
            return Err("서비스 요청이 너무 많습니다. 잠시 후 다시 시도해 주세요.".into());
        }
        match ACTIVE_PIPE_CALLS.compare_exchange_weak(
            active,
            active + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break,
            Err(current) => active = current,
        }
    }
    let name = name.to_owned();
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("ktm-pipe-call".into())
        .spawn(move || {
            struct ActiveCall;
            impl Drop for ActiveCall {
                fn drop(&mut self) {
                    ACTIVE_PIPE_CALLS.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _active = ActiveCall;
            let result = pipe_call_blocking(&name, &request);
            let _ = tx.send(result);
        })
        .map_err(|e| {
            ACTIVE_PIPE_CALLS.fetch_sub(1, Ordering::AcqRel);
            e.to_string()
        })?;
    rx.recv_timeout(timeout).map_err(|e| match e {
        mpsc::RecvTimeoutError::Timeout => PIPE_TIMEOUT_ERROR.to_owned(),
        mpsc::RecvTimeoutError::Disconnected => "서비스 작업 스레드가 종료되었습니다.".into(),
    })?
}

/// 파이프 입출력 오류를 사용자 문구로 바꾼다. 서비스가 꺼져 파이프가 없으면 Windows가
/// "지정된 파일을 찾을 수 없습니다. (os error 2)"를 돌려주는데, 어떤 파일인지 알 수 없어 혼란스럽다.
pub fn friendly_pipe_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            "검색 서비스(색인 서비스)에 연결할 수 없습니다. 서비스가 실행 중인지 확인해 주세요.".into()
        }
        std::io::ErrorKind::PermissionDenied => {
            "검색 서비스에 접근할 권한이 없습니다. 같은 사용자 계정으로 실행 중인지 확인해 주세요.".into()
        }
        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::UnexpectedEof => {
            "검색 서비스와의 연결이 끊어졌습니다. 잠시 뒤 다시 검색해 주세요.".into()
        }
        _ => e.to_string(),
    }
}

fn pipe_call_blocking(name: &str, request: &[u8]) -> Result<Vec<u8>> {
    let mut pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name)
        .map_err(|e| friendly_pipe_error(&e))?;
    pipe.write_all(&(request.len() as u32).to_le_bytes())
        .and_then(|_| pipe.write_all(request))
        .map_err(|e| e.to_string())?;
    let mut prefix = [0; 4];
    pipe.read_exact(&mut prefix).map_err(|e| friendly_pipe_error(&e))?;
    let n = u32::from_le_bytes(prefix) as usize;
    if n > 8 * 1024 * 1024 {
        return Err("검색 응답 크기가 상한을 넘었습니다.".into());
    }
    let mut bytes = vec![0; n];
    pipe.read_exact(&mut bytes).map_err(|e| friendly_pipe_error(&e))?;
    Ok(bytes)
}

/// 파일명 정렬(Q-F2): 파이프 `names_query`의 `sort` 값과 화면 이름.
pub const NAME_SORTS: [(&str, &str); 4] = [
    ("relevance", "관련도"),
    ("name", "이름순"),
    ("size", "크기순"),
    ("date", "날짜순"),
];

/// 파일명 탭: 서비스가 메모리에 쥔 드라이브 전체 이름 색인에 `names_query`(서비스 SID 범위, 다른 사용자 프로필 제외).
pub fn pipe_names(name: &str, query: &str, sort: &str) -> Result<Vec<Row>> {
    pipe_names_limit(name, query, sort, LIMIT)
}

/// QMS-EXPORT: 지정 상한을 반영한 파일명 색인 질의.
pub fn pipe_names_limit(name: &str, query: &str, sort: &str, limit: usize) -> Result<Vec<Row>> {
    let bytes = pipe_call_with_timeout(
        name,
        &json!({"command":"names_query","id":1,"text":query,"limit":limit,"sort":sort}),
        PIPE_SEARCH_TIMEOUT,
    )?;
    decode_names(&bytes)
}

pub fn pipe_names_related(name: &str, query: &str, limit: usize) -> Result<Answer> {
    let groups=kfind_core::related_query::parse_related_query(query)?;
    let bytes=pipe_call_with_timeout(name,&json!({"command":"names_related","id":1,"text":query,"limit":limit}),PIPE_SEARCH_TIMEOUT)?;
    let value: Value=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
    if value["result"]["related_groups"]!=true { return Err("파일명 서비스에 연관 검색 기능이 필요합니다.".into()); }
    let mut rows=decode_names(&bytes)?;
    for r in &mut rows {
        r.group_matches=groups.iter().enumerate().map(|(i,g)|GroupMatch {
            group_index:i,query:g.query.clone(),paragraphs:Vec::new(),filename:true,
        }).collect();
    }
    Ok(Answer { rows,terms:groups.iter().flat_map(|g|g.terms.clone()).collect(),
        groups:groups.into_iter().map(|g|g.query).collect(),..Answer::default() })
}

fn decode_names(bytes: &[u8]) -> Result<Vec<Row>> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value["ok"] != true {
        let e = value["error"].as_str().unwrap_or("색인 서비스 오류");
        return Err(if e.contains("unavailable") {
            format!("파일명 색인을 준비하는 중입니다({e}).")
        } else {
            e.to_owned()
        });
    }
    let items = value["result"]["items"]
        .as_array()
        .ok_or("파일명 응답에 결과가 없습니다.")?;
    Ok(items
        .iter()
        .map(|i| {
            let name = i["name"].as_str().unwrap_or_default().to_owned();
            let dir = i["dir"].as_str().unwrap_or_default().to_owned();
            let path = if dir.is_empty() {
                name.clone()
            } else if dir.ends_with('\\') {
                format!("{dir}{name}")
            } else {
                format!("{dir}\\{name}")
            };
            let is_dir = i["is_dir"].as_bool().unwrap_or(false);
            let mut info = Vec::new();
            if is_dir {
                info.push("폴더".to_owned());
            } else if let Some(s) = i["size"].as_u64() {
                info.push(format_size(s));
            }
            if let Some(t) = i["mtime"].as_i64().and_then(format_filetime) {
                info.push(t);
            }
            let mtime_i64 = i["mtime"].as_i64().unwrap_or(0);
            Row {
                path,
                name,
                parent: dir,
                location: info.join(" · "),
                level: 255,
                snippet_tried: true,
                date: mtime_i64,
                ..Row::default()
            }
        })
        .collect())
}

/// Reads name-index state from the service pipe without changing service state.
pub fn pipe_names_status(name: &str) -> Result<Value> {
    let bytes = pipe_call_with_timeout(
        name,
        &json!({"command":"names_status"}),
        PIPE_LIGHT_TIMEOUT,
    )?;
    decode_names_status(&bytes)
}

fn decode_names_status(bytes: &[u8]) -> Result<Value> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value["ok"] != true {
        return Err(value["error"].as_str().unwrap_or("색인 상태 조회 오류").to_owned());
    }
    value.get("result").cloned().ok_or_else(|| "색인 상태 응답이 없습니다.".into())
}

/// 크기 글: 1023 B · 1.5 KB · 12.3 MB · 1.2 GB.
pub fn format_size(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut k = 0;
    while v >= 1024.0 && k < U.len() - 1 {
        v /= 1024.0;
        k += 1;
    }
    if k == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[k])
    }
}

/// FILETIME(100ns, 1601-01-01 UTC) → 지역 시각 "2026-11-14 09:05". 범위 밖이면 None.
#[cfg(windows)]
pub fn format_filetime(t: i64) -> Option<String> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    if t <= 0 {
        return None;
    }
    unsafe {
        let ft = FILETIME {
            dwLowDateTime: t as u32,
            dwHighDateTime: (t >> 32) as u32,
        };
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        if FileTimeToSystemTime(&ft, &mut utc) == 0
            || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0
        {
            return None;
        }
        Some(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute
        ))
    }
}

#[cfg(not(windows))]
pub fn format_filetime(t: i64) -> Option<String> {
    if t <= 0 {
        return None;
    }
    const WINDOWS_TICK: i64 = 10_000_000;
    const SEC_TO_UNIX_EPOCH: i64 = 11_644_473_600;
    let sec = (t / WINDOWS_TICK) - SEC_TO_UNIX_EPOCH;
    let (y, m, d) = kfind_core::unix_to_ymd(sec + 9 * 3600);
    let day_sec = (sec + 9 * 3600).rem_euclid(86400);
    let hour = day_sec / 3600;
    let minute = (day_sec % 3600) / 60;
    Some(format!("{y:04}-{m:02}-{d:02} {hour:02}:{minute:02}"))
}

/// 현재 한국 표준시(KST: UTC+9) 기준 (year, month, day) 반환.
pub fn current_kst_ymd() -> (u32, u32, u32) {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let kst_secs = secs + 9 * 3600;
    kfind_core::unix_to_ymd(kst_secs)
}

/// Shift a YYYYMMDD date by whole Gregorian days (negative values go backward).
pub fn shift_ymd(ymd: i64, delta_days: i64) -> i64 {
    let y = ymd / 10000;
    let m = (ymd % 10000) / 100;
    let d = ymd % 100;
    let days = days_from_civil(y, m, d) + delta_days;
    let (ny, nm, nd) = civil_from_days(days);
    (ny * 10000 + nm * 100 + nd) as i64
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    (y, m, d)
}

/// 현재 한국 표준시(KST) 기준 이번 주 월요일~일요일 날짜 (YYYYMMDD, YYYYMMDD) 반환.
pub fn current_kst_week_range() -> (u32, u32) {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let kst_secs = secs + 9 * 3600;
    let days = kst_secs.div_euclid(86400);
    // 1970-01-01 was Thursday. (days + 3) % 7 -> 0 = Mon, ..., 6 = Sun
    let dow = (days + 3).rem_euclid(7);
    let mon_days = days - dow;
    let sun_days = mon_days + 6;
    let (sy, sm, sd) = kfind_core::unix_to_ymd(mon_days * 86400);
    let (ey, em, ed) = kfind_core::unix_to_ymd(sun_days * 86400);
    (sy * 10000 + sm * 100 + sd, ey * 10000 + em * 100 + ed)
}

/// 기간 거르기 필터와 직접 입력 범위를 (start_date, end_date)로 변환.
pub fn date_range_for_filter(
    filter: crate::tm_state::DateFilter,
    custom: Option<(i64, i64)>,
) -> Option<(i64, i64)> {
    match filter {
        crate::tm_state::DateFilter::All => None,
        crate::tm_state::DateFilter::Custom => custom,
        crate::tm_state::DateFilter::Today => {
            let (y, m, d) = current_kst_ymd();
            let ymd = (y * 10000 + m * 100 + d) as i64;
            Some((ymd, ymd))
        }
        crate::tm_state::DateFilter::Last7Days | crate::tm_state::DateFilter::Last30Days => {
            let days = if filter == crate::tm_state::DateFilter::Last7Days { 7 } else { 30 };
            let (y, m, d) = current_kst_ymd();
            let end = (y * 10000 + m * 100 + d) as i64;
            Some((shift_ymd(end, -days), end))
        }
        crate::tm_state::DateFilter::ThisYear => {
            let (y, _, _) = current_kst_ymd();
            let start = (y * 10000 + 101) as i64;
            let end = (y * 10000 + 1231) as i64;
            Some((start, end))
        }
    }
}

fn decode_rows(bytes: &[u8]) -> Result<Vec<Row>> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value["ok"] != true {
        return Err(value["error"].as_str().unwrap_or("색인 서비스 오류").into());
    }
    let hits = value["result"]["hits"]
        .as_array()
        .ok_or("검색 응답에 결과가 없습니다.")?;
    let mut rows: Vec<Row> = hits
        .iter()
        .map(|h| {
            let hit = DocHit {
                doc_id: h["doc_id"].as_u64().ok_or("문서 번호가 없습니다.")?,
                adjacent: h["adjacent"].as_bool().unwrap_or(false),
                level: h["level"].as_u64().unwrap_or(255) as u8,
                count: h["count"].as_u64().unwrap_or(0) as u32,
                date: h["date"].as_i64().unwrap_or(0),
                sentences: h["sentences"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(1)
                    .map(|s| kfind_core::SentenceRef {
                        ord: s["ord"].as_u64().unwrap_or(0) as u32,
                        para: s["para"].as_u64().unwrap_or(0) as u32,
                        level: s["level"].as_u64().unwrap_or(255) as u8,
                        adjacent: s["adjacent"].as_bool().unwrap_or(false),
                    })
                    .collect(),
            };
            Ok(row(&hit, h["path"].as_str()))
        })
        .collect::<Result<Vec<Row>>>()?;
    for (i, r) in rows.iter_mut().enumerate() {
        r.rank = i;
    }
    Ok(rows)
}

#[cfg(test)]
mod pipe_error_tests {
    use super::*;

    #[test]
    fn missing_pipe_is_reported_as_unreachable_service_not_os_error_2() {
        let error = std::io::Error::from_raw_os_error(2);
        let message = friendly_pipe_error(&error);
        assert!(message.contains("검색 서비스"), "{message}");
        assert!(!message.contains("os error"), "{message}");
        // 존재하지 않는 파이프를 실제로 열어도 같은 문구가 나온다.
        let message = pipe_call_blocking(r"\.\pipe\ktm-test-no-such-pipe-xyz", b"{}").unwrap_err();
        assert!(!message.contains("os error 2"), "{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semi1_groups_use_raw_paragraphs_and_complete_evidence() {
        use kfind_core::{Writer,WriterOptions,Sentence};
        let t=Temp::new();
        let mut writer=Writer::open(&t.0,WriterOptions::default()).unwrap();
        writer.add_document(1,0,&[Sentence{para:0,level:0,text:"2026"},Sentence{para:1,level:8,text:"공사 계약"}]).unwrap();
        writer.add_document(2,0,&[Sentence{para:0,level:8,text:"2026"},Sentence{para:1,level:8,text:"공사"},Sentence{para:2,level:8,text:"계약"}]).unwrap();
        writer.add_document(3,0,&[Sentence{para:0,level:8,text:"2026"},Sentence{para:1,level:8,text:"공사 안내"},Sentence{para:1,level:8,text:"계약 검토"}]).unwrap();
        writer.commit().unwrap();
        let mut table=String::new();
        for id in 1..=3 { table.push_str(&format!("{{\"op\":\"put\",\"key\":\"k{id}\",\"id\":{id},\"meta\":{{\"path\":\"synthetic/{id}.txt\"}}}}\n")); }
        std::fs::write(t.0.join("doctable.jsonl"),table).unwrap();
        let db=t.0.join("content.sqlite");
        crate::paragraphs::create_fixture(&db,&[
            ("synthetic/1.txt",0,"2026"),("synthetic/1.txt",1,"공사 계약"),
            ("synthetic/2.txt",0,"2026"),("synthetic/2.txt",1,"공사"),("synthetic/2.txt",2,"계약"),
            ("synthetic/3.txt",0,"2026"),("synthetic/3.txt",1,"공사 안내. 계약 검토.")]).unwrap();
        let mut backend=Backend::open(&t.0).unwrap().with_paragraphs(Some(db)).with_related_queries(true);
        let answer=backend.search_cancel(" ;2026;; 공사 계약;",false,None).unwrap();
        // 문서 3은 한 문단의 서로 다른 두 문장에 묶음 낱말이 나뉘므로 제외한다.
        assert_eq!(answer.rows.iter().map(|r|r.doc_id).collect::<Vec<_>>(),[1]);
        assert_eq!(answer.groups,["2026","공사 계약"]);
        assert_eq!(answer.rows[0].group_matches.len(),2);
        assert_eq!(answer.rows[0].group_matches[1].paragraphs,[(1,8)]);
        let groups=vec![vec!["2026".into()],vec!["공사".into(),"계약".into()]];
        let view=crate::view_dump::result_row_with_groups(&answer.rows[0],&answer.terms,&groups,&[],false);
        assert!(view.badges.contains(&"2묶음 일치".into()));
        assert_eq!(display_query_groups("; 2026;; \"공사; 계약\";",true),["2026","\"공사; 계약\""]);
    }

    #[cfg(windows)]
    fn fake_pipe(
        reply: Option<Vec<u8>>,
    ) -> (String, mpsc::Sender<()>, std::thread::JoinHandle<()>) {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::FromRawHandle;
        use windows_sys::Win32::Foundation::{
            CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
        use windows_sys::Win32::System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
        };
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            r"\\.\pipe\ktm-timeout-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let wide: Vec<u16> = std::ffi::OsStr::new(&name)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                65536,
                65536,
                0,
                std::ptr::null(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        let (close_tx, close_rx) = mpsc::channel();
        let handle = handle as usize;
        let thread = std::thread::spawn(move || unsafe {
            let handle = handle as windows_sys::Win32::Foundation::HANDLE;
            if ConnectNamedPipe(handle, std::ptr::null_mut()) == 0
                && GetLastError() != ERROR_PIPE_CONNECTED
            {
                CloseHandle(handle);
                return;
            }
            let mut pipe = std::fs::File::from_raw_handle(handle as _);
            let mut prefix = [0; 4];
            if pipe.read_exact(&mut prefix).is_ok() {
                let mut request = vec![0; u32::from_le_bytes(prefix) as usize];
                if pipe.read_exact(&mut request).is_ok() {
                    if let Some(response) = reply {
                        let _ = pipe.write_all(&(response.len() as u32).to_le_bytes());
                        let _ = pipe.write_all(&response);
                    } else {
                        let _ = close_rx.recv_timeout(Duration::from_secs(2));
                    }
                }
            }
        });
        (name, close_tx, thread)
    }

    #[cfg(windows)]
    #[test]
    fn pipe_timeout_normal_response_error_and_overflow_guard() {
        let (name, close, server) = fake_pipe(None);
        let started = std::time::Instant::now();
        let error = pipe_call_with_timeout(
            &name,
            &json!({"command":"query"}),
            Duration::from_millis(40),
        )
        .unwrap_err();
        assert_eq!(error, PIPE_TIMEOUT_ERROR);
        assert!(started.elapsed() < Duration::from_secs(1));
        let _ = close.send(());
        server.join().unwrap();

        let response = br#"{"ok":true}"#.to_vec();
        let (name, _, server) = fake_pipe(Some(response.clone()));
        assert_eq!(
            pipe_call_with_timeout(&name, &json!({"command":"status"}), Duration::from_secs(1))
                .unwrap(),
            response
        );
        server.join().unwrap();

        let mut stalled = Vec::new();
        for _ in 0..MAX_TIMED_OUT_PIPE_CALLS {
            let (name, close, server) = fake_pipe(None);
            stalled.push((name, close, server));
        }
        let workers: Vec<_> = stalled
            .iter()
            .map(|(name, _, _)| {
                let name = name.clone();
                std::thread::spawn(move || {
                    pipe_call_with_timeout(
                        &name,
                        &json!({"command":"query"}),
                        Duration::from_millis(1500),
                    )
                })
            })
            .collect();
        // 막힌 호출이 모두 진행 중(카운터가 상한)이 될 때까지 기다린 뒤 초과 호출을 보낸다(시간 의존 제거).
        let wait_start = std::time::Instant::now();
        while ACTIVE_PIPE_CALLS.load(Ordering::Acquire) < MAX_TIMED_OUT_PIPE_CALLS
            && wait_start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(1));
        }
        let overflow =
            pipe_call_with_timeout("unused", &json!({}), Duration::from_millis(1)).unwrap_err();
        assert!(overflow.contains("요청이 너무 많습니다"));
        for worker in workers {
            assert_eq!(worker.join().unwrap().unwrap_err(), PIPE_TIMEOUT_ERROR);
        }
        for (_, close, server) in stalled {
            let _ = close.send(());
            server.join().unwrap();
        }
    }

    #[test]
    fn stats_count_type_year_folder_and_unknown_size() {
        let rows = vec![
            Row {
                name: "a.hwp".into(),
                path: "C:\\x\\a.hwp".into(),
                parent: "C:\\x".into(),
                date: 20250101,
                doc_id: 1,
                ..Row::default()
            },
            Row {
                name: "b.pdf".into(),
                path: "C:\\y\\b.pdf".into(),
                parent: "C:\\y".into(),
                date: 20240101,
                doc_id: 2,
                ..Row::default()
            },
        ];
        let s = document_stats(&rows);
        assert_eq!(s.total, 2);
        assert_eq!(s.indexed, 2);
        assert_eq!(s.total_bytes, None);
        assert_eq!(s.by_type.len(), 2);
        assert_eq!(s.by_year.len(), 2);
        assert_eq!(s.by_folder.len(), 2);
    }
    #[test]
    fn stats_recent_top_five_is_descending() {
        let rows: Vec<Row> = (1..=7)
            .map(|d| Row {
                name: format!("{d}.hwp"),
                date: 20250100 + d,
                doc_id: d as u64,
                ..Row::default()
            })
            .collect();
        let s = document_stats(&rows);
        assert_eq!(s.recent.len(), 5);
        assert!(s.recent.windows(2).all(|w| w[0].date >= w[1].date));
    }
    #[test]
    fn stats_ignores_missing_dates_for_recent_and_year() {
        let s = document_stats(&[Row {
            name: "x.hwp".into(),
            ..Row::default()
        }]);
        assert!(s.recent.is_empty());
        assert!(s.by_year.is_empty());
    }
    #[test]
    fn stats_folder_counts_sorted_and_bounded() {
        let rows: Vec<Row> = (0..15)
            .map(|i| Row {
                name: format!("{i}.hwp"),
                parent: format!("C:\\{i}"),
                ..Row::default()
            })
            .collect();
        let s = document_stats(&rows);
        assert_eq!(s.by_folder.len(), 10);
    }
    #[test]
    fn version_group_keeps_highest_v_number() {
        let rows = vec![
            Row {
                name: "Plan_v1.docx".into(),
                ..Row::default()
            },
            Row {
                name: "Plan_v2.docx".into(),
                ..Row::default()
            },
        ];
        let g = group_document_versions(&rows);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name, "Plan_v2.docx");
    }
    #[test]
    fn version_group_prefers_final() {
        let rows = vec![
            Row {
                name: "Plan_v9.docx".into(),
                ..Row::default()
            },
            Row {
                name: "Plan_Final.docx".into(),
                ..Row::default()
            },
        ];
        assert_eq!(group_document_versions(&rows)[0].name, "Plan_Final.docx");
    }
    #[test]
    fn version_group_keeps_unversioned_name() {
        let rows = vec![Row {
            name: "Memo.docx".into(),
            ..Row::default()
        }];
        assert_eq!(group_document_versions(&rows).len(), 1);
    }
    #[test]
    fn version_group_is_case_insensitive() {
        let rows = vec![
            Row {
                name: "Report_V1.pdf".into(),
                ..Row::default()
            },
            Row {
                name: "report_v2.pdf".into(),
                ..Row::default()
            },
        ];
        assert_eq!(group_document_versions(&rows).len(), 1);
    }
    #[test]
    fn version_group_handles_empty_input() {
        assert!(group_document_versions(&[]).is_empty());
    }
    #[test]
    fn cloud_only_attribute_bits_are_detected_individually() {
        for bit in [0x1000, 0x400000, 0x40000] {
            assert!(is_cloud_only_attributes(bit));
        }
        assert!(!is_cloud_only_attributes(0));
        assert!(!is_cloud_only_attributes(0x20));
    }
    use kfind_core::{Sentence, Writer, WriterOptions};
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ktm-backend-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                seq,
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn mapped_queries_character_hierarchy_deleted_and_read_only() {
        let t = Temp::new();
        let mut writer = Writer::open(&t.0, WriterOptions::default()).unwrap();
        writer
            .add_document(
                1,
                10,
                &[Sentence {
                    para: 0,
                    level: 3,
                    text: "예산집행 계획",
                }],
            )
            .unwrap();
        writer
            .add_document(
                2,
                20,
                &[Sentence {
                    para: 1,
                    level: 0,
                    text: "예산집행 추진",
                }],
            )
            .unwrap();
        writer
            .add_document(
                3,
                30,
                &[Sentence {
                    para: 2,
                    level: 0,
                    text: "삭제 예산",
                }],
            )
            .unwrap();
        writer.commit().unwrap();
        writer.delete_document(3).unwrap();
        writer.commit().unwrap();
        let records = concat!(
            "{\"op\":\"put\",\"key\":\"a\",\"id\":1,\"meta\":{\"path\":\"folder/a.hwp\"}}\n",
            "{\"op\":\"put\",\"key\":\"b\",\"id\":2,\"meta\":{\"path\":\"folder/b.hwp\"}}\n",
            "{bad"
        );
        std::fs::write(t.0.join("doctable.jsonl"), records).unwrap();
        let before = std::fs::read(t.0.join("MANIFEST.kfm")).unwrap();
        let mut backend = Backend::open(&t.0).unwrap();
        let hits = backend.search("예산", true).unwrap();
        assert_eq!(hits.iter().map(|r| r.doc_id).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(hits[0].name, "b.hwp");
        assert!(hits[0].snippet.is_empty());
        assert_eq!(backend.search("예", false).unwrap().len(), 2);
        assert_eq!(std::fs::read(t.0.join("MANIFEST.kfm")).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(t.0.join("doctable.jsonl")).unwrap(),
            records
        );
    }
    #[test]
    fn snippets_come_from_paragraph_table_only_for_prefetched_rows() {
        let t = Temp::new();
        let mut writer = Writer::open(&t.0, WriterOptions::default()).unwrap();
        let mut table = String::new();
        for id in 1..=12u64 {
            writer
                .add_document(
                    id,
                    id as i64,
                    &[Sentence {
                        para: 2,
                        level: (id % 3) as u8,
                        text: "예산 집행 계획",
                    }],
                )
                .unwrap();
            table.push_str(&format!(
                "{{\"op\":\"put\",\"key\":\"k{id}\",\"id\":{id},\"meta\":{{\"path\":\"D:\\\\d\\\\{id}.hwp\"}}}}\n"
            ));
        }
        writer.commit().unwrap();
        std::fs::write(t.0.join("doctable.jsonl"), table).unwrap();
        let db = t.0.join("timeline-content.sqlite");
        let bodies: Vec<(String, String)> = (1..=12)
            .map(|id| (format!("D:\\d\\{id}.hwp"), format!("머리말 {id} 이 문서는 아주 긴 머리글을 가진 시험 문단이다\n올해 예산 집행 계획을 세운다 {id}")))
            .collect();
        let rows: Vec<(&str, u32, &str)> = bodies
            .iter()
            .map(|(p, b)| (p.as_str(), 2, b.as_str()))
            .collect();
        crate::paragraphs::create_fixture(&db, &rows).unwrap();
        let mut backend = Backend::open(&t.0)
            .unwrap()
            .with_paragraphs(Some(db.clone()));
        backend.warm().unwrap();
        let mut answer = backend.search_cancel("예산집행", true, None).unwrap();
        assert_eq!(answer.rows.len(), 12);
        assert!(
            answer.terms.iter().any(|w| w == "예산") && answer.terms.iter().any(|w| w == "집행")
        );
        // 위계 순(0 → 1 → 2).
        assert!(answer.rows.windows(2).all(|w| w[0].level <= w[1].level));
        for r in &answer.rows[..SNIPPET_PREFETCH] {
            assert!(r.snippet.contains("예산 집행 계획"), "{:?}", r.snippet);
            assert!(r.snippet.starts_with('…'));
        }
        assert!(
            answer.rows[SNIPPET_PREFETCH..]
                .iter()
                .all(|r| r.snippet.is_empty() && !r.snippet_tried)
        );
        let terms = answer.terms.clone();
        backend.fill_snippets(&mut answer.rows[SNIPPET_PREFETCH..], &terms);
        assert!(answer.rows.iter().all(|r| !r.snippet.is_empty()));
        // 문단 표가 없으면 원문 없이 위치만.
        let mut plain = Backend::open(&t.0)
            .unwrap()
            .with_paragraphs(Some(t.0.join("none.sqlite")));
        let a = plain.search_cancel("예산", false, None).unwrap();
        assert!(
            a.rows
                .iter()
                .all(|r| r.snippet.is_empty() && r.location.starts_with("문단 3"))
        );
        assert!(!t.0.join("none.sqlite").exists());
    }
    #[test]
    fn names_rows_size_date_and_errors() {
        let rows = decode_names(
            br#"{"ok":true,"result":{"items":[{"name":"a.hwp","dir":"C:\\x","is_dir":false,"size":1536,"mtime":133758000000000000},{"name":"x","dir":"C:","is_dir":true,"size":null,"mtime":null},{"name":"C:","dir":"","is_dir":true}]}}"#,
        )
        .unwrap();
        assert_eq!(rows[0].path, "C:\\x\\a.hwp");
        assert!(
            rows[0].location.starts_with("1.5 KB · 2024-"),
            "{}",
            rows[0].location
        );
        assert_eq!(rows[1].path, "C:\\x");
        assert_eq!(rows[1].location, "폴더");
        assert_eq!(rows[2].path, "C:");
        assert!(rows.iter().all(|r| r.snippet.is_empty() && r.snippet_tried));
        let e = decode_names(br#"{"ok":false,"error":"names index unavailable (building)"}"#)
            .unwrap_err();
        assert!(e.contains("준비하는 중"));
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_filetime(0), None);
        assert!(pipe_names(r"\\.\pipe\ktm-none-test-pipe", "예산", "size").is_err());
    }

    #[test]
    fn names_status_decodes_read_only_status_envelope() {
        let status = decode_names_status(
            br#"{"ok":true,"result":{"state":"empty","mode":"user","entries":0,"volumes":[],"last_error":null}}"#,
        )
        .unwrap();
        assert_eq!(status["state"], "empty");
        assert_eq!(status["entries"], 0);
        assert_eq!(
            decode_names_status(br#"{"ok":false,"error":"service offline"}"#)
                .unwrap_err(),
            "service offline"
        );
    }
    #[test]
    fn replay_replacement_deletion_and_missing_index() {
        let t = Temp::new();
        assert!(Backend::open(&t.0).is_err());
        let p = t.0.join("doctable.jsonl");
        std::fs::write(
            &p,
            concat!(
                "{\"op\":\"put\",\"key\":\"a\",\"id\":1,\"meta\":{\"path\":\"old\"}}\n",
                "{\"op\":\"put\",\"key\":\"a\",\"id\":2,\"meta\":{\"path\":\"new\"}}\n",
                "{\"op\":\"del\",\"key\":\"a\"}\n"
            ),
        )
        .unwrap();
        assert!(
            read_paths(&p, Some(&[1, 2].into_iter().collect()))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn character_scan_honors_cancellation_and_scope() {
        use std::sync::{Arc, atomic::AtomicBool};
        let t = Temp::new();
        let mut writer = Writer::open(&t.0, WriterOptions::default()).unwrap();
        writer
            .add_document(
                1,
                1,
                &[Sentence {
                    para: 0,
                    level: 2,
                    text: "예산집행",
                }],
            )
            .unwrap();
        writer.commit().unwrap();
        let index = Index::open(&t.0).unwrap();
        let opts = SearchOptions {
            cancel: Some(Arc::new(AtomicBool::new(true))),
            ..SearchOptions::default()
        };
        assert!(matches!(
            index.search_character("예", &opts),
            Err(kfind_core::Error::Cancelled)
        ));
        let opts = SearchOptions {
            allow: Some(Arc::new(kfind_core::RoaringTreemap::new())),
            ..SearchOptions::default()
        };
        assert!(index.search_character("예", &opts).unwrap().hits.is_empty());
        assert!(
            index
                .search_character("예산", &SearchOptions::default())
                .is_err()
        );
    }
    #[test]
    fn kfind_response_rows_flow_into_all_tab_combination() {
        // kfind pipe.rs query emits result.hits; names_query emits result.items.
        // These are the service's JSON envelopes and field names, decoded by the
        // same functions used by the named-pipe client.
        let names = decode_names(r#"{"ok":true,"result":{"items":[{"name":"예산안.hwp","dir":"C:\\문서","is_dir":false,"size":2048,"mtime":0}]}}"#.as_bytes()).unwrap();
        let hierarchy = decode_rows(r#"{"ok":true,"result":{"hits":[{"doc_id":41,"path":"C:\\문서\\예산안.hwp","level":1,"count":1,"date":0,"sentences":[]}]}}"#.as_bytes()).unwrap();
        let body = decode_rows(r#"{"ok":true,"result":{"hits":[{"doc_id":41,"path":"c:/문서/예산안.hwp","level":8,"count":2,"date":0,"sentences":[{"ord":1,"para":3,"level":8,"adjacent":false}]}]}}"#.as_bytes()).unwrap();
        let batches = [
            (crate::tm_state::ResultKind::Names, names),
            (crate::tm_state::ResultKind::Hierarchy, hierarchy),
            (crate::tm_state::ResultKind::Body, body),
        ];
        let combined = crate::tm_state::combine_results(&batches, |r| {
            r.path.replace('/', "\\").to_lowercase()
        });
        assert_eq!(combined.len(), 1);
        assert_eq!(combined[0].row.name, "예산안.hwp");
        assert_eq!(
            combined[0].kinds,
            [
                crate::tm_state::ResultKind::Names,
                crate::tm_state::ResultKind::Hierarchy,
                crate::tm_state::ResultKind::Body,
            ]
        );
    }

    #[test]
    fn kfind_response_empty_errors_groups_and_tier_caps_flow() {
        use crate::tm_state::{ResultKind, combine_results};
        assert!(
            decode_rows(br#"{"ok":true,"result":{"hits":[]}}"#)
                .unwrap()
                .is_empty()
        );
        assert!(
            decode_names(br#"{"ok":true,"result":{"items":[]}}"#)
                .unwrap()
                .is_empty()
        );
        assert!(
            decode_rows(br#"{"ok":false,"error":"service offline"}"#)
                .unwrap_err()
                .contains("offline")
        );
        assert_eq!(kfind_core::parse_groups("예산; 결산"), ["예산", "결산"]);

        let names: Vec<Row> = (0..25)
            .map(|i| Row {
                path: format!("n{i}"),
                ..Row::default()
            })
            .collect();
        let hierarchy: Vec<Row> = (0..25)
            .map(|i| Row {
                path: format!("h{i}"),
                ..Row::default()
            })
            .collect();
        let body: Vec<Row> = (0..50)
            .map(|i| Row {
                path: format!("b{i}"),
                ..Row::default()
            })
            .collect();
        let combined = combine_results(
            &[
                (ResultKind::Names, names),
                (ResultKind::Hierarchy, hierarchy),
                (ResultKind::Body, body),
            ],
            |r| r.path.clone(),
        );
        assert_eq!(combined.len(), 80);
        assert_eq!(combined.iter().filter(|r| r.priority == 0).count(), 20);
        assert_eq!(combined.iter().filter(|r| r.priority == 1).count(), 20);
        assert_eq!(combined.iter().filter(|r| r.priority == 2).count(), 40);
    }

    #[test]
    fn service_error_and_original_text_absence() {
        assert!(
            decode_rows(br#"{"ok":false,"error":"offline"}"#)
                .unwrap_err()
                .contains("offline")
        );
        let rows = decode_rows(
            br#"{"ok":true,"result":{"hits":[{"doc_id":4,"path":"x.hwp","level":1,"count":2}]}}"#,
        )
        .unwrap();
        assert_eq!(rows[0].name, "x.hwp");
        assert!(rows[0].snippet.is_empty());
        assert_eq!(rows[0].location, "위계 1");
        assert_eq!(rows[0].para, None);
    }

    #[test]
    fn test_date_range_for_filter_calculations() {
        use crate::tm_state::DateFilter;
        assert_eq!(date_range_for_filter(DateFilter::All, None), None);
        assert_eq!(
            date_range_for_filter(DateFilter::Custom, Some((20260101, 20260228))),
            Some((20260101, 20260228))
        );

        let today = date_range_for_filter(DateFilter::Today, None).unwrap();
        assert_eq!(today.0, today.1);
        assert!(today.0 >= 20260101);

        let week = date_range_for_filter(DateFilter::Last7Days, None).unwrap();
        assert_eq!(week.1, today.1);
        assert_eq!(week.0, shift_ymd(today.1, -7));

        let month = date_range_for_filter(DateFilter::Last30Days, None).unwrap();
        assert_eq!(month.1, today.1);
        assert_eq!(month.0, shift_ymd(today.1, -30));

        let year = date_range_for_filter(DateFilter::ThisYear, None).unwrap();
        assert_eq!(year.0 % 10000, 101);
        assert_eq!(year.1 % 10000, 1231);
    }

    #[test]
    fn test_backend_autocomplete_top_topics_and_scope_search() {
        use kfind_core::DocumentMeta;
        let t = Temp::new();
        let mut writer = Writer::open(&t.0, WriterOptions::default()).unwrap();
        writer
            .add_document_with_meta(
                1,
                20260115,
                &DocumentMeta {
                    path: Some("C:\\공문서\\총무과\\청년지원조례.hwp".to_string()),
                    file_type: Some("hwp".to_string()),
                    ..Default::default()
                },
                &[Sentence {
                    para: 0,
                    level: 0,
                    text: "청년지원 조례안 제정",
                }],
            )
            .unwrap();
        writer
            .add_document_with_meta(
                2,
                20260220,
                &DocumentMeta {
                    path: Some("C:\\공문서\\복지과\\청년주거지원.hwp".to_string()),
                    file_type: Some("hwp".to_string()),
                    ..Default::default()
                },
                &[Sentence {
                    para: 0,
                    level: 0,
                    text: "청년주거 지원 종합대책",
                }],
            )
            .unwrap();
        writer
            .add_document_with_meta(
                3,
                20260310,
                &DocumentMeta {
                    path: Some("C:\\공문서\\총무과\\예산편성.hwp".to_string()),
                    file_type: Some("hwp".to_string()),
                    ..Default::default()
                },
                &[Sentence {
                    para: 0,
                    level: 0,
                    text: "2026년도 본예산 편성 계획",
                }],
            )
            .unwrap();
        writer
            .add_document_with_meta(
                4,
                20260401,
                &DocumentMeta {
                    path: Some("C:\\공문서\\총무과\\청년지원계획.pdf".to_string()),
                    file_type: Some("pdf".to_string()),
                    ..Default::default()
                },
                &[Sentence {
                    para: 0,
                    level: 0,
                    text: "청년지원 추진 세부 계획",
                }],
            )
            .unwrap();
        writer.commit().unwrap();

        let doctable = concat!(
            "{\"op\":\"put\",\"key\":\"1\",\"id\":1,\"meta\":{\"path\":\"C:\\\\공문서\\\\총무과\\\\청년지원조례.hwp\"}}\n",
            "{\"op\":\"put\",\"key\":\"2\",\"id\":2,\"meta\":{\"path\":\"C:\\\\공문서\\\\복지과\\\\청년주거지원.hwp\"}}\n",
            "{\"op\":\"put\",\"key\":\"3\",\"id\":3,\"meta\":{\"path\":\"C:\\\\공문서\\\\총무과\\\\예산편성.hwp\"}}\n",
            "{\"op\":\"put\",\"key\":\"4\",\"id\":4,\"meta\":{\"path\":\"C:\\\\공문서\\\\총무과\\\\청년지원계획.pdf\"}}\n"
        );
        std::fs::write(t.0.join("doctable.jsonl"), doctable).unwrap();

        let mut backend = Backend::open(&t.0).unwrap();

        // 1. SEMI3 자동 완성 (2글자부터, 빈도순)
        let ac_short = backend.autocomplete("청", 5);
        assert!(ac_short.is_empty(), "2글자 미만은 빈 결과");
        let ac = backend.autocomplete("청년", 5);
        assert!(!ac.is_empty());
        let ac_terms: Vec<&str> = ac.iter().map(|item| item.term.as_str()).collect();
        assert!(ac_terms.contains(&"청년지원") || ac_terms.contains(&"청년주거"));

        // 2. SEMI3 첫 화면 주제 1~10위
        let top = backend.top_topics(10);
        assert!(!top.is_empty());
        assert!(top.len() <= 10);

        // 3. SEMI2 기간 거르기
        let ans_jan = backend
            .search_with_scope("청년", false, Some((20260101, 20260131)), None, None)
            .unwrap();
        assert_eq!(ans_jan.rows.len(), 1);
        assert_eq!(ans_jan.rows[0].doc_id, 1);

        // 4. SEMI2 폴더 거르기
        let ans_bokji = backend
            .search_with_scope("청년", false, None, Some("복지과"), None)
            .unwrap();
        assert_eq!(ans_bokji.rows.len(), 1);
        assert_eq!(ans_bokji.rows[0].doc_id, 2);

        // 5. 폴더별 개수 집계 반환
        let ans_all = backend
            .search_with_scope("청년", false, None, None, None)
            .unwrap();
        assert_eq!(ans_all.rows.len(), 3);
        assert!(!ans_all.folder_counts.is_empty());

        // 6. QMS-QUERY 질의 문법 통합 검증
        // 6-1. 정확 구문 ("청년지원 조례안")
        let ans_phrase = backend
            .search_with_scope("\"청년지원 조례안\"", false, None, None, None)
            .unwrap();
        assert_eq!(ans_phrase.rows.len(), 1);
        assert_eq!(ans_phrase.rows[0].doc_id, 1);

        // 6-2. 제외어 (-조례안) -> 1번 제외, 2번과 4번 일치 (최신순: 4번 -> 2번)
        let ans_ex = backend
            .search_with_scope("청년 -조례안", false, None, None, None)
            .unwrap();
        assert_eq!(ans_ex.rows.len(), 2);
        let ex_ids: Vec<u64> = ans_ex.rows.iter().map(|r| r.doc_id).collect();
        assert!(ex_ids.contains(&2) && ex_ids.contains(&4) && !ex_ids.contains(&1));

        // 6-3. 폴더 path:복지과
        let ans_path = backend
            .search_with_scope("청년 path:복지과", false, None, None, None)
            .unwrap();
        assert_eq!(ans_path.rows.len(), 1);
        assert_eq!(ans_path.rows[0].doc_id, 2);

        // 6-4. 날짜 after:2026-02-01 (20260220인 2번, 20260401인 4번)
        let ans_after = backend
            .search_with_scope("청년 after:2026-02-01", false, None, None, None)
            .unwrap();
        assert_eq!(ans_after.rows.len(), 2);
        let after_ids: Vec<u64> = ans_after.rows.iter().map(|r| r.doc_id).collect();
        assert!(after_ids.contains(&2) && after_ids.contains(&4));

        // 6-5. 문법 오류 복구: 검색이 막히지 않고 정상 반환
        let ans_err =
            backend.search_with_scope("청년 ext: - after:invalid", false, None, None, None);
        assert!(ans_err.is_ok());

        // 7. QMS-SCOPE 확장자 범위 및 연동 검증
        // 7-1. 인자로 ext: Some("pdf") 전달
        let ans_pdf = backend
            .search_with_scope_ext(
                "청년",
                false,
                None,
                None,
                Some("pdf"),
                crate::tm_state::MatchMode::AllTerms,
                None,
            )
            .unwrap();
        assert_eq!(ans_pdf.rows.len(), 1);
        assert_eq!(ans_pdf.rows[0].doc_id, 4);

        // 7-2. 질의 토큰 ext:hwp
        let ans_ext_token = backend
            .search_with_scope("청년 ext:hwp", false, None, None, None)
            .unwrap();
        assert_eq!(ans_ext_token.rows.len(), 2); // doc 1, doc 2
    }

    #[test]
    fn test_compute_location_badge() {
        // 쪽 메타
        assert_eq!(compute_location_badge("3쪽"), Some("3쪽".into()));
        assert_eq!(compute_location_badge("12쪽"), Some("12쪽".into()));
        assert_eq!(compute_location_badge("1쪽"), Some("1쪽".into()));
        assert_eq!(compute_location_badge("3p"), Some("3p".into()));
        assert_eq!(compute_location_badge("⟦3쪽⟧"), Some("3쪽".into()));
        assert_eq!(compute_location_badge("[12쪽]"), Some("12쪽".into()));

        // 시트 및 행 메타
        assert_eq!(
            compute_location_badge("시트1 12행"),
            Some("시트1 12행".into())
        );
        assert_eq!(
            compute_location_badge("매출 12행"),
            Some("매출 12행".into())
        );
        assert_eq!(compute_location_badge("1행"), Some("1행".into()));
        assert_eq!(compute_location_badge("시트 4행"), Some("시트 4행".into()));
        assert_eq!(
            compute_location_badge("절대경로시트 1행"),
            Some("절대경로시트 1행".into())
        );

        // 일반 문단 번호·위계 등은 배지 생략(None)
        assert_eq!(compute_location_badge("문단 1 · 위계 8"), None);
        assert_eq!(compute_location_badge("문단 3"), None);
        assert_eq!(compute_location_badge("위계 2"), None);
        assert_eq!(compute_location_badge("section0 문단 1"), None);
        assert_eq!(compute_location_badge("본문"), None);
        assert_eq!(compute_location_badge(""), None);
        assert_eq!(compute_location_badge("   "), None);
    }

    #[test]
    fn test_body_sort_cycle_and_sort_rows() {
        assert_eq!(BodySort::Relevance.next(), BodySort::Date);
        assert_eq!(BodySort::Date.next(), BodySort::Name);
        assert_eq!(BodySort::Name.next(), BodySort::Relevance);

        assert_eq!(BodySort::Relevance.label(), "관련도순");
        assert_eq!(BodySort::Date.label(), BodySort::Date.label());
        assert_eq!(BodySort::Name.label(), "이름순");

        let mut rows = vec![
            Row {
                name: "다.hwp".into(),
                date: 100,
                rank: 0,
                ..Row::default()
            },
            Row {
                name: "가.hwp".into(),
                date: 300,
                rank: 1,
                ..Row::default()
            },
            Row {
                name: "나.hwp".into(),
                date: 200,
                rank: 2,
                ..Row::default()
            },
        ];

        // 수정일순 정렬: 최신 날짜(300 -> 200 -> 100)
        sort_rows(&mut rows, BodySort::Date);
        assert_eq!(rows[0].name, "가.hwp");
        assert_eq!(rows[1].name, "나.hwp");
        assert_eq!(rows[2].name, "다.hwp");

        // 이름순 정렬: 오름차순(가 -> 나 -> 다)
        sort_rows(&mut rows, BodySort::Name);
        assert_eq!(rows[0].name, "가.hwp");
        assert_eq!(rows[1].name, "나.hwp");
        assert_eq!(rows[2].name, "다.hwp");

        // 관련도순 정렬: 원래 순위(rank 0 -> rank 1 -> rank 2, 다 -> 가 -> 나)
        sort_rows(&mut rows, BodySort::Relevance);
        assert_eq!(rows[0].name, "다.hwp");
        assert_eq!(rows[1].name, "가.hwp");
        assert_eq!(rows[2].name, "나.hwp");
    }

    #[test]
    fn body_sort_confidence_and_oldest_are_deterministic() {
        let mut rows = vec![
            Row {
                name: "a".into(),
                date: 20260103,
                count: 2,
                rank: 0,
                ..Row::default()
            },
            Row {
                name: "b".into(),
                date: 20260101,
                count: 8,
                rank: 1,
                ..Row::default()
            },
            Row {
                name: "c".into(),
                date: 20260102,
                count: 8,
                rank: 2,
                ..Row::default()
            },
        ];
        sort_rows(&mut rows, BodySort::Confidence);
        assert_eq!(
            rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        sort_rows(&mut rows, BodySort::Oldest);
        assert_eq!(
            rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
    }

    #[test]
    fn korean_datetime_format_checks_dates_and_meridiem() {
        assert_eq!(
            format_korean_datetime(20260203).as_deref(),
            Some("2026. 02. 03. 오전 12:00")
        );
        assert_eq!(
            format_korean_datetime_at_hour(20260203, 0, 5).as_deref(),
            Some("2026. 02. 03. 오전 12:05")
        );
        assert_eq!(
            format_korean_datetime_at_hour(20260203, 13, 7).as_deref(),
            Some("2026. 02. 03. 오후 01:07")
        );
        assert_eq!(format_korean_datetime(20260229), None);
        assert_eq!(
            format_korean_datetime(20240229).as_deref(),
            Some("2024. 02. 29. 오전 12:00")
        );
        assert_eq!(format_korean_datetime(20260431), None);
        assert_eq!(format_korean_datetime_at_hour(20260203, 24, 0), None);
    }

    #[test]
    fn calendar_month_shift_clamps_end_of_month() {
        assert_eq!(shift_ymd_months(20260831, -6), 20260228);
        assert_eq!(shift_ymd_months(20240831, -6), 20240229);
        assert_eq!(shift_ymd_months(20260131, 1), 20260228);
    }

    #[test]
    fn match_mode_query_preserves_scope_tokens() {
        let q = "budget launch path:\"docs\\2026\" ext:hwp after:2026-01-01 -draft";
        assert_eq!(
            query_for_match_mode(q, crate::tm_state::MatchMode::AllTerms),
            q
        );
        let any = query_for_match_mode(q, crate::tm_state::MatchMode::AnyTerm);
        assert!(
            any.contains("budget launch")
                && any.contains("path:")
                && any.contains("ext:hwp")
                && any.contains("after:")
                && any.contains("-draft")
        );
        let exact = query_for_match_mode(q, crate::tm_state::MatchMode::Exact);
        assert!(exact.starts_with("\"budget launch\""));
    }

    #[test]
    fn extension_badges_cover_requested_types() {
        for (path, label) in [
            ("a.hwp", "HWP"),
            ("a.hwpx", "HWPX"),
            ("a.docx", "DOCX"),
            ("a.pdf", "PDF"),
            ("a.xls", "XLS"),
            ("a.ppt", "PPT"),
            ("a.md", "MD"),
            ("a.txt", "TXT"),
        ] {
            assert_eq!(extension_label(path), label);
        }
    }

    #[test]
    fn direct_n_day_window_covers_today_and_previous_days() {
        let end = 20260310;
        for days in [1i64, 7, 30, 90, 180, 365] {
            let start = shift_ymd(end, -(days - 1));
            assert!(start <= end);
            assert!(shift_ymd(start, days - 1) == end);
        }
    }
}
