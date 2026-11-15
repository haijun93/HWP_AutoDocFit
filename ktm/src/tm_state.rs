//! Pure popup/search transitions. Native handles and search results live in the adapter.
//! Hidden input/completions cannot show the window; only `Show` does so.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Hidden,
    Visible,
    Typing,
    Searching,
    Results,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Body,
    Hierarchy,
    Names,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DateFilter {
    #[default]
    All,
    Today,
    ThisWeek,
    ThisMonth,
    ThisYear,
    Custom,
}

impl DateFilter {
    pub fn label(&self) -> &'static str {
        match self {
            DateFilter::All => "기간 전체",
            DateFilter::Today => "오늘",
            DateFilter::ThisWeek => "이번 주",
            DateFilter::ThisMonth => "이번 달",
            DateFilter::ThisYear => "올해",
            DateFilter::Custom => "직접 입력",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub phase: Phase,
    pub query: String,
    pub tab: Tab,
    pub generation: u64,
    pub result_count: usize,
    pub selected: Option<usize>,
    pub error: Option<String>,
    // SEMI3 & SEMI2 additions:
    pub autocompletes: Vec<String>,
    pub selected_autocomplete: Option<usize>,
    pub top_topics: Vec<String>,
    pub selected_top_topic: Option<usize>,
    pub date_filter: DateFilter,
    pub custom_date_range: Option<(i64, i64)>,
    pub selected_folder: Option<String>,
    pub folder_counts: Vec<(String, u32)>,
    pub selected_ext: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            phase: Phase::Hidden,
            query: String::new(),
            tab: Tab::Body,
            generation: 0,
            result_count: 0,
            selected: None,
            error: None,
            autocompletes: Vec::new(),
            selected_autocomplete: None,
            top_topics: Vec::new(),
            selected_top_topic: None,
            date_filter: DateFilter::All,
            custom_date_range: None,
            selected_folder: None,
            folder_counts: Vec::new(),
            selected_ext: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Show,
    Hide,
    Input(String),
    Submit,
    SwitchTab(Tab),
    /// 같은 검색어·칸으로 다시 찾기(파일명 정렬 바꿈 등). 앞 결과는 무효.
    Refresh,
    SearchFinished { generation: u64, count: usize },
    SearchFailed { generation: u64, message: String },
    MoveSelection(isize),
    Select(usize),
    Open { folder: bool },
    OpenFailed(String),
    // SEMI3 & SEMI2 events:
    SetDateFilter(DateFilter),
    SetCustomDate(i64, i64),
    SetFolderFilter(Option<String>),
    SetExtFilter(Option<String>),
    SetAutocompletes(Vec<String>),
    SetTopTopics(Vec<String>),
    SetFolderCounts(Vec<(String, u32)>),
    SelectAutocomplete(Option<usize>),
    ApplyAutocomplete(usize),
    SelectTopTopic(Option<usize>),
    ApplyTopTopic(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Show,
    Hide,
    Search {
        generation: u64,
        query: String,
        tab: Tab,
        date_filter: DateFilter,
        custom_date_range: Option<(i64, i64)>,
        folder: Option<String>,
        ext: Option<String>,
    },
    Open {
        index: usize,
        folder: bool,
    },
}

fn clear_results(state: &mut State) {
    state.result_count = 0;
    state.selected = None;
    state.error = None;
}

fn search(state: &mut State, effects: &mut Vec<Effect>, explicit: bool) {
    clear_results(state);
    state.selected_autocomplete = None;
    state.selected_top_topic = None;
    // Increment even for an empty/short query: an older completion is now invalid.
    state.generation = state.generation.wrapping_add(1);
    let query = state.query.trim();
    if query.chars().count() >= if explicit { 1 } else { 2 } {
        state.phase = Phase::Searching;
        effects.push(Effect::Search {
            generation: state.generation,
            query: query.to_owned(),
            tab: state.tab,
            date_filter: state.date_filter,
            custom_date_range: state.custom_date_range,
            folder: state.selected_folder.clone(),
            ext: state.selected_ext.clone(),
        });
    } else {
        if query.is_empty() {
            state.autocompletes.clear();
            state.phase = Phase::Visible;
        } else {
            state.phase = Phase::Typing;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchScope {
    pub folder: Option<String>,
    pub ext: Option<String>,
    pub date_filter: DateFilter,
    pub custom_date_range: Option<(i64, i64)>,
}

impl State {
    pub fn scope(&self) -> SearchScope {
        SearchScope {
            folder: self.selected_folder.clone(),
            ext: self.selected_ext.clone(),
            date_filter: self.date_filter,
            custom_date_range: self.custom_date_range,
        }
    }
}

/// 질의 토큰 종류 (QMS-SCOPE: 질의 문법 ↔ 범위 UI 양방향 반영)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryToken {
    Path(String),
    Ext(String),
    After(String),
    Before(String),
    Text(String),
}

fn strip_case_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix)
    {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

pub fn parse_query_tokens(raw: &str) -> Vec<QueryToken> {
    let mut tokens = Vec::new();
    let chars: Vec<(usize, char)> = raw.char_indices().collect();
    let len = raw.len();
    let mut i = 0;

    while i < chars.len() {
        let (byte_idx, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        let slice = &raw[byte_idx..];

        // 1. path:"..." 또는 path:...
        if let Some(rest) = strip_case_prefix(slice, "path:") {
            let path_start = byte_idx + 5;
            if let Some(first_char) = rest.chars().next() {
                if first_char == '"' || first_char == '“' {
                    let content_start = path_start + first_char.len_utf8();
                    let mut j = i;
                    while j < chars.len() && chars[j].0 < content_start {
                        j += 1;
                    }
                    let mut close_idx = None;
                    let mut escaped = false;
                    while j < chars.len() {
                        let (_, c2) = chars[j];
                        if escaped {
                            escaped = false;
                        } else if c2 == '\\' {
                            escaped = true;
                        } else if c2 == '"' || c2 == '”' {
                            close_idx = Some(j);
                            break;
                        }
                        j += 1;
                    }
                    if let Some(c_idx) = close_idx {
                        let content_end = chars[c_idx].0;
                        let content = &raw[content_start..content_end];
                        tokens.push(QueryToken::Path(content.replace("\\\"", "\"")));
                        i = c_idx + 1;
                        continue;
                    } else {
                        let content = &raw[content_start..];
                        tokens.push(QueryToken::Path(content.replace("\\\"", "\"")));
                        break;
                    }
                }
            }
            // 일반 path:경로 (다음 공백까지)
            let mut j = i;
            let mut end_pos = len;
            while j < chars.len() {
                let (pos, c2) = chars[j];
                if c2.is_whitespace() {
                    end_pos = pos;
                    break;
                }
                j += 1;
            }
            let val = &raw[path_start..end_pos];
            if !val.is_empty() {
                tokens.push(QueryToken::Path(val.to_string()));
            }
            i = j;
            continue;
        }

        // 2. ext:...
        if let Some(_) = strip_case_prefix(slice, "ext:") {
            let ext_start = byte_idx + 4;
            let mut j = i;
            let mut end_pos = len;
            while j < chars.len() {
                let (pos, c2) = chars[j];
                if c2.is_whitespace() {
                    end_pos = pos;
                    break;
                }
                j += 1;
            }
            let val = &raw[ext_start..end_pos];
            if !val.is_empty() {
                tokens.push(QueryToken::Ext(val.to_string()));
            }
            i = j;
            continue;
        }

        // 3. after:...
        if let Some(_) = strip_case_prefix(slice, "after:") {
            let after_start = byte_idx + 6;
            let mut j = i;
            let mut end_pos = len;
            while j < chars.len() {
                let (pos, c2) = chars[j];
                if c2.is_whitespace() {
                    end_pos = pos;
                    break;
                }
                j += 1;
            }
            let val = &raw[after_start..end_pos];
            if !val.is_empty() {
                tokens.push(QueryToken::After(val.to_string()));
            }
            i = j;
            continue;
        }

        // 4. before:...
        if let Some(_) = strip_case_prefix(slice, "before:") {
            let before_start = byte_idx + 7;
            let mut j = i;
            let mut end_pos = len;
            while j < chars.len() {
                let (pos, c2) = chars[j];
                if c2.is_whitespace() {
                    end_pos = pos;
                    break;
                }
                j += 1;
            }
            let val = &raw[before_start..end_pos];
            if !val.is_empty() {
                tokens.push(QueryToken::Before(val.to_string()));
            }
            i = j;
            continue;
        }

        // 5. 따옴표 구문 "..."
        if c == '"' || c == '“' {
            let mut j = i + 1;
            let mut close_idx = None;
            let mut escaped = false;
            while j < chars.len() {
                let (_, c2) = chars[j];
                if escaped {
                    escaped = false;
                } else if c2 == '\\' {
                    escaped = true;
                } else if c2 == '"' || c2 == '”' {
                    close_idx = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(c_idx) = close_idx {
                let end_pos = if c_idx + 1 < chars.len() {
                    chars[c_idx + 1].0
                } else {
                    len
                };
                tokens.push(QueryToken::Text(raw[byte_idx..end_pos].to_string()));
                i = c_idx + 1;
                continue;
            } else {
                tokens.push(QueryToken::Text(raw[byte_idx..].to_string()));
                break;
            }
        }

        // 6. 일반 낱말
        let mut j = i;
        let mut end_pos = len;
        while j < chars.len() {
            let (pos, c2) = chars[j];
            if c2.is_whitespace() {
                end_pos = pos;
                break;
            }
            j += 1;
        }
        tokens.push(QueryToken::Text(raw[byte_idx..end_pos].to_string()));
        i = j;
    }

    tokens
}

pub fn tokens_to_query(tokens: &[QueryToken]) -> String {
    let mut parts = Vec::with_capacity(tokens.len());
    for t in tokens {
        match t {
            QueryToken::Path(p) => {
                if p.contains(' ') || p.contains('\t') {
                    parts.push(format!("path:\"{}\"", p));
                } else {
                    parts.push(format!("path:{}", p));
                }
            }
            QueryToken::Ext(e) => parts.push(format!("ext:{}", e)),
            QueryToken::After(a) => parts.push(format!("after:{}", a)),
            QueryToken::Before(b) => parts.push(format!("before:{}", b)),
            QueryToken::Text(t) => parts.push(t.clone()),
        }
    }
    parts.join(" ")
}

pub fn set_folder_token(query: &str, folder: Option<&str>) -> String {
    let mut tokens = parse_query_tokens(query);
    if let Some(f) = folder {
        let trimmed = f.trim();
        if trimmed.is_empty() {
            tokens.retain(|t| !matches!(t, QueryToken::Path(_)));
        } else {
            let mut replaced = false;
            for t in &mut tokens {
                if let QueryToken::Path(p) = t {
                    *p = trimmed.to_string();
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                tokens.push(QueryToken::Path(trimmed.to_string()));
            }
        }
    } else {
        tokens.retain(|t| !matches!(t, QueryToken::Path(_)));
    }
    tokens_to_query(&tokens)
}

pub fn set_ext_token(query: &str, ext: Option<&str>) -> String {
    let mut tokens = parse_query_tokens(query);
    if let Some(e) = ext {
        let trimmed = e.trim();
        if trimmed.is_empty() {
            tokens.retain(|t| !matches!(t, QueryToken::Ext(_)));
        } else {
            let mut replaced = false;
            for t in &mut tokens {
                if let QueryToken::Ext(val) = t {
                    *val = trimmed.to_string();
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                tokens.push(QueryToken::Ext(trimmed.to_string()));
            }
        }
    } else {
        tokens.retain(|t| !matches!(t, QueryToken::Ext(_)));
    }
    tokens_to_query(&tokens)
}

pub fn set_date_tokens(
    query: &str,
    after: Option<&str>,
    before: Option<&str>,
) -> String {
    let mut tokens = parse_query_tokens(query);
    tokens.retain(|t| !matches!(t, QueryToken::After(_) | QueryToken::Before(_)));
    if let Some(a) = after {
        let tr = a.trim();
        if !tr.is_empty() {
            tokens.push(QueryToken::After(tr.to_string()));
        }
    }
    if let Some(b) = before {
        let tr = b.trim();
        if !tr.is_empty() {
            tokens.push(QueryToken::Before(tr.to_string()));
        }
    }
    tokens_to_query(&tokens)
}

pub fn format_ymd(ymd: i64) -> String {
    let y = ymd / 10000;
    let m = (ymd % 10000) / 100;
    let d = ymd % 100;
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn date_filter_to_tokens(
    filter: DateFilter,
    custom: Option<(i64, i64)>,
) -> (Option<String>, Option<String>) {
    match filter {
        DateFilter::All => (None, None),
        DateFilter::Today => {
            let (y, m, d) = crate::backend::current_kst_ymd();
            let s = format!("{y:04}-{m:02}-{d:02}");
            (Some(s.clone()), Some(s))
        }
        DateFilter::ThisWeek => {
            let (s_ymd, e_ymd) = crate::backend::current_kst_week_range();
            (Some(format_ymd(s_ymd as i64)), Some(format_ymd(e_ymd as i64)))
        }
        DateFilter::ThisMonth => {
            let (y, m, _) = crate::backend::current_kst_ymd();
            let s = format!("{y:04}-{m:02}-01");
            let e = format!("{y:04}-{m:02}-31");
            (Some(s), Some(e))
        }
        DateFilter::ThisYear => {
            let (y, _, _) = crate::backend::current_kst_ymd();
            let s = format!("{y:04}-01-01");
            let e = format!("{y:04}-12-31");
            (Some(s), Some(e))
        }
        DateFilter::Custom => {
            if let Some((s_val, e_val)) = custom {
                (Some(format_ymd(s_val)), Some(format_ymd(e_val)))
            } else {
                (None, None)
            }
        }
    }
}

pub fn detect_date_filter_from_range(
    start: Option<i64>,
    end: Option<i64>,
) -> (DateFilter, Option<(i64, i64)>) {
    if start.is_none() && end.is_none() {
        return (DateFilter::All, None);
    }
    let s = start.unwrap_or(0);
    let e = end.unwrap_or(99991231);

    let (ty, tm, td) = crate::backend::current_kst_ymd();
    let today_val = (ty * 10000 + tm * 100 + td) as i64;
    if s == today_val && e == today_val {
        return (DateFilter::Today, None);
    }

    let (ws, we) = crate::backend::current_kst_week_range();
    if s == ws as i64 && e == we as i64 {
        return (DateFilter::ThisWeek, None);
    }

    let m_start = (ty * 10000 + tm * 100 + 1) as i64;
    let m_end = (ty * 10000 + tm * 100 + 31) as i64;
    if s == m_start && e == m_end {
        return (DateFilter::ThisMonth, None);
    }

    let y_start = (ty * 10000 + 101) as i64;
    let y_end = (ty * 10000 + 1231) as i64;
    if s == y_start && e == y_end {
        return (DateFilter::ThisYear, None);
    }

    (DateFilter::Custom, Some((s, e)))
}

/// Returns the next state and work for the native adapter. No I/O or Win32 calls.
pub fn step(current: &State, event: Event) -> (State, Vec<Effect>) {
    let mut state = current.clone();
    let mut effects = Vec::new();
    match event {
        Event::Show => {
            if state.phase == Phase::Hidden {
                effects.push(Effect::Show);
                search(&mut state, &mut effects, false);
            }
        }
        Event::Hide => {
            if state.phase != Phase::Hidden {
                state.generation = state.generation.wrapping_add(1);
                state.phase = Phase::Hidden;
                clear_results(&mut state);
                effects.push(Effect::Hide);
            }
        }
        _ if state.phase == Phase::Hidden => {}
        Event::Input(query) => {
            if state.query != query {
                state.query = query;
                let syntax = kfind_core::parse_query_syntax(&state.query);
                state.selected_folder = syntax.folder.clone();
                state.selected_ext = syntax.file_types.as_ref().map(|v| v.join(","));
                let (df, custom) = detect_date_filter_from_range(syntax.start_date, syntax.end_date);
                state.date_filter = df;
                state.custom_date_range = custom;
                search(&mut state, &mut effects, false);
            }
        }
        Event::SwitchTab(tab) => {
            if state.tab != tab {
                state.tab = tab;
                search(&mut state, &mut effects, false);
            }
        }
        Event::Refresh => {
            let explicit = state.query.trim().chars().count() == 1 && state.phase != Phase::Typing;
            search(&mut state, &mut effects, explicit);
        }
        Event::Submit => {
            if let Some(idx) = state.selected_autocomplete {
                if idx < state.autocompletes.len() {
                    state.query = state.autocompletes[idx].clone();
                    state.selected_autocomplete = None;
                    search(&mut state, &mut effects, true);
                    return (state, effects);
                }
            }
            if let Some(idx) = state.selected_top_topic {
                if idx < state.top_topics.len() {
                    state.query = state.top_topics[idx].clone();
                    state.selected_top_topic = None;
                    search(&mut state, &mut effects, true);
                    return (state, effects);
                }
            }
            if state.phase == Phase::Results && state.selected.is_some() {
                effects.push(Effect::Open {
                    index: state.selected.unwrap(),
                    folder: false,
                });
            } else {
                search(&mut state, &mut effects, true);
            }
        }
        Event::SearchFinished { generation, count } => {
            if state.phase == Phase::Searching && generation == state.generation {
                state.phase = Phase::Results;
                state.result_count = count;
                state.selected = if count > 0 { Some(0) } else { None };
            }
        }
        Event::SearchFailed {
            generation,
            message,
        } => {
            if state.phase == Phase::Searching && generation == state.generation {
                state.phase = Phase::Error;
                state.error = Some(message);
            }
        }
        Event::MoveSelection(delta) => {
            if state.phase == Phase::Visible && !state.top_topics.is_empty() {
                let n = state.top_topics.len();
                let current = state.selected_top_topic.unwrap_or(0);
                if delta > 0 {
                    state.selected_top_topic = Some((current + delta as usize).min(n - 1));
                } else if current as isize + delta >= 0 {
                    state.selected_top_topic = Some((current as isize + delta) as usize);
                } else {
                    state.selected_top_topic = None;
                }
            } else if !state.autocompletes.is_empty() && (state.phase == Phase::Typing || state.phase == Phase::Searching || state.selected.is_none()) {
                let n = state.autocompletes.len();
                if let Some(cur) = state.selected_autocomplete {
                    if delta < 0 && cur == 0 {
                        state.selected_autocomplete = None;
                    } else if delta < 0 {
                        state.selected_autocomplete = Some(cur.saturating_sub(1));
                    } else if cur + 1 < n {
                        state.selected_autocomplete = Some(cur + 1);
                    } else {
                        state.selected_autocomplete = None;
                        if state.phase == Phase::Results && state.result_count > 0 {
                            state.selected = Some(0);
                        }
                    }
                } else if delta > 0 {
                    state.selected_autocomplete = Some(0);
                }
            } else if state.phase == Phase::Results && state.result_count > 0 {
                let index = state.selected.unwrap_or(0);
                if delta < 0 && index == 0 && !state.autocompletes.is_empty() {
                    state.selected = None;
                    state.selected_autocomplete = Some(state.autocompletes.len() - 1);
                } else {
                    state.selected = Some(
                        index
                            .saturating_add_signed(delta)
                            .min(state.result_count - 1),
                    );
                }
            }
        }
        Event::Select(index) => {
            if state.phase == Phase::Results && index < state.result_count {
                state.selected = Some(index);
                state.selected_autocomplete = None;
            }
        }
        Event::Open { folder } => {
            if state.phase == Phase::Results {
                if let Some(index) = state.selected {
                    effects.push(Effect::Open { index, folder });
                }
            }
        }
        Event::OpenFailed(message) => {
            state.error = Some(message);
        }
        Event::SetDateFilter(df) => {
            if state.date_filter != df || state.custom_date_range.is_some() {
                state.date_filter = df;
                state.custom_date_range = None;
                let (after, before) = date_filter_to_tokens(df, None);
                state.query = set_date_tokens(&state.query, after.as_deref(), before.as_deref());
                let explicit = state.query.trim().chars().count() == 1 && state.phase != Phase::Typing;
                search(&mut state, &mut effects, explicit);
            }
        }
        Event::SetCustomDate(start, end) => {
            state.date_filter = DateFilter::Custom;
            state.custom_date_range = Some((start, end));
            let s = format_ymd(start);
            let e = format_ymd(end);
            state.query = set_date_tokens(&state.query, Some(&s), Some(&e));
            let explicit = state.query.trim().chars().count() == 1 && state.phase != Phase::Typing;
            search(&mut state, &mut effects, explicit);
        }
        Event::SetFolderFilter(f) => {
            if state.selected_folder != f {
                state.selected_folder = f.clone();
                state.query = set_folder_token(&state.query, f.as_deref());
                let explicit = state.query.trim().chars().count() == 1 && state.phase != Phase::Typing;
                search(&mut state, &mut effects, explicit);
            }
        }
        Event::SetExtFilter(e) => {
            if state.selected_ext != e {
                state.selected_ext = e.clone();
                state.query = set_ext_token(&state.query, e.as_deref());
                let explicit = state.query.trim().chars().count() == 1 && state.phase != Phase::Typing;
                search(&mut state, &mut effects, explicit);
            }
        }
        Event::SetAutocompletes(list) => {
            state.autocompletes = list;
            state.selected_autocomplete = None;
        }
        Event::SetTopTopics(list) => {
            state.top_topics = list;
            state.selected_top_topic = None;
        }
        Event::SetFolderCounts(counts) => {
            state.folder_counts = counts;
        }
        Event::SelectAutocomplete(idx) => {
            state.selected_autocomplete = idx;
        }
        Event::ApplyAutocomplete(idx) => {
            if idx < state.autocompletes.len() {
                state.query = state.autocompletes[idx].clone();
                state.selected_autocomplete = None;
                search(&mut state, &mut effects, true);
            }
        }
        Event::SelectTopTopic(idx) => {
            state.selected_top_topic = idx;
        }
        Event::ApplyTopTopic(idx) => {
            if idx < state.top_topics.len() {
                state.query = state.top_topics[idx].clone();
                state.selected_top_topic = None;
                search(&mut state, &mut effects, true);
            }
        }
    }
    (state, effects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible() -> State {
        step(&State::default(), Event::Show).0
    }
    fn results(count: usize) -> State {
        let state = step(&visible(), Event::Input("예산".into())).0;
        step(
            &state,
            Event::SearchFinished {
                generation: state.generation,
                count,
            },
        )
        .0
    }

    #[test]
    fn korean_two_characters_search_one_character_needs_submit() {
        let (state, effects) = step(&visible(), Event::Input("한".into()));
        assert_eq!(state.phase, Phase::Typing);
        assert!(effects.is_empty());
        let (state, effects) = step(&state, Event::Submit);
        assert_eq!(state.phase, Phase::Searching);
        assert!(matches!(&effects[..], [Effect::Search { query, .. }] if query == "한"));
        let (state, effects) = step(&state, Event::Input("한글".into()));
        assert_eq!(state.phase, Phase::Searching);
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn whitespace_never_searches() {
        let state = step(&visible(), Event::Input("  ".into())).0;
        let (state, effects) = step(&state, Event::Submit);
        assert_eq!(state.phase, Phase::Visible);
        assert!(effects.is_empty());
    }

    #[test]
    fn tab_preserves_query_and_invalidates_old_search() {
        let old = step(&visible(), Event::Input("예산".into())).0;
        let (state, effects) = step(&old, Event::SwitchTab(Tab::Hierarchy));
        assert_eq!(state.query, "예산");
        assert!(matches!(
            &effects[..],
            [Effect::Search {
                tab: Tab::Hierarchy,
                ..
            }]
        ));
        assert_eq!(
            step(
                &state,
                Event::SearchFinished {
                    generation: old.generation,
                    count: 9
                }
            )
            .0,
            state
        );
    }

    #[test]
    fn shortening_cancels_pending_completion() {
        let old = step(&visible(), Event::Input("예산".into())).0;
        let state = step(&old, Event::Input("예".into())).0;
        assert_eq!(
            step(
                &state,
                Event::SearchFinished {
                    generation: old.generation,
                    count: 9
                }
            )
            .0,
            state
        );
    }

    #[test]
    fn names_tab_searches_with_shared_query_and_refresh_reruns() {
        let state = step(&visible(), Event::Input("예산".into())).0;
        let (state, effects) = step(&state, Event::SwitchTab(Tab::Names));
        assert!(matches!(&effects[..], [Effect::Search { tab: Tab::Names, query, .. }] if query == "예산"));
        let g = state.generation;
        let (state, effects) = step(&state, Event::Refresh);
        assert!(state.generation > g);
        assert!(matches!(&effects[..], [Effect::Search { tab: Tab::Names, .. }]));
        // 앞 세대의 결과는 버린다.
        assert_eq!(step(&state, Event::SearchFinished { generation: g, count: 3 }).0, state);
        // 1글자는 Enter로만 찾고, 그 뒤 다시 찾기는 1글자 그대로.
        let one = step(&state, Event::Input("예".into())).0;
        assert_eq!(one.phase, Phase::Typing);
        assert!(step(&one, Event::Refresh).1.is_empty());
        let (one, fx) = step(&one, Event::Submit);
        assert_eq!(fx.len(), 1);
        assert_eq!(step(&one, Event::Refresh).1.len(), 1);
        // 숨긴 상태에서는 아무것도 안 함.
        let hidden = step(&one, Event::Hide).0;
        assert!(step(&hidden, Event::Refresh).1.is_empty());
    }

    #[test]
    fn selection_clamps_and_open_carries_folder_intent() {
        let state = results(3);
        let state = step(&state, Event::MoveSelection(99)).0;
        assert_eq!(state.selected, Some(2));
        assert_eq!(
            step(&state, Event::Open { folder: true }).1,
            vec![Effect::Open {
                index: 2,
                folder: true
            }]
        );
        assert_eq!(
            step(&state, Event::Submit).1,
            vec![Effect::Open {
                index: 2,
                folder: false
            }]
        );
        assert_eq!(step(&state, Event::MoveSelection(-99)).0.selected, Some(0));
        assert_eq!(step(&state, Event::Select(99)).0, state);
    }

    #[test]
    fn zero_results_cannot_open() {
        let state = results(0);
        assert_eq!(state.selected, None);
        assert!(step(&state, Event::Open { folder: false }).1.is_empty());
    }

    #[test]
    fn hide_invalidates_results_and_hidden_events_cannot_show() {
        let old = step(&visible(), Event::Input("예산".into())).0;
        let (state, effects) = step(&old, Event::Hide);
        assert_eq!(effects, vec![Effect::Hide]);
        assert_eq!(state.phase, Phase::Hidden);
        for event in [
            Event::Input("새검색".into()),
            Event::Submit,
            Event::SearchFinished {
                generation: old.generation,
                count: 9,
            },
        ] {
            assert_eq!(step(&state, event), (state.clone(), vec![]));
        }
        let (shown, effects) = step(&state, Event::Show);
        assert_eq!(shown.phase, Phase::Searching);
        assert!(matches!(effects.first(), Some(Effect::Show)));
    }

    #[test]
    fn error_can_retry_and_stale_error_is_ignored() {
        let state = step(&visible(), Event::Input("예산".into())).0;
        assert_eq!(
            step(
                &state,
                Event::SearchFailed {
                    generation: 0,
                    message: "old".into()
                }
            )
            .0,
            state
        );
        let state = step(
            &state,
            Event::SearchFailed {
                generation: state.generation,
                message: "색인 없음".into(),
            },
        )
        .0;
        assert_eq!(state.phase, Phase::Error);
        assert_eq!(state.error.as_deref(), Some("색인 없음"));
        let (state, effects) = step(&state, Event::Submit);
        assert_eq!(state.phase, Phase::Searching);
        assert_eq!(state.error, None);
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn unchanged_input_and_tab_do_not_duplicate_work() {
        let state = results(3);
        assert_eq!(
            step(&state, Event::Input(state.query.clone())),
            (state.clone(), vec![])
        );
        assert_eq!(step(&state, Event::SwitchTab(state.tab)), (state, vec![]));
    }

    #[test]
    fn autocomplete_up_down_selection_and_submit() {
        let state = step(&visible(), Event::Input("청년".into())).0;
        let state = step(
            &state,
            Event::SetAutocompletes(vec!["청년주거".into(), "청년창업".into(), "청년지원".into()]),
        )
        .0;
        assert_eq!(state.autocompletes.len(), 3);
        assert_eq!(state.selected_autocomplete, None);

        // 아래 방향키로 첫 번째 자동 완성 선택
        let state = step(&state, Event::MoveSelection(1)).0;
        assert_eq!(state.selected_autocomplete, Some(0));

        // 다시 아래로 이동
        let state = step(&state, Event::MoveSelection(1)).0;
        assert_eq!(state.selected_autocomplete, Some(1));

        // 위로 이동
        let state = step(&state, Event::MoveSelection(-1)).0;
        assert_eq!(state.selected_autocomplete, Some(0));

        // Enter 누르면 선택된 자동 완성 검색어로 검색 실행
        let (state, effects) = step(&state, Event::Submit);
        assert_eq!(state.query, "청년주거");
        assert_eq!(state.selected_autocomplete, None);
        assert!(matches!(
            &effects[..],
            [Effect::Search { query, .. }] if query == "청년주거"
        ));
    }

    #[test]
    fn empty_query_top_topics_selection_and_submit() {
        let state = visible();
        let state = step(
            &state,
            Event::SetTopTopics(vec!["예산".into(), "공사".into(), "계획".into()]),
        )
        .0;
        assert_eq!(state.phase, Phase::Visible);
        assert_eq!(state.top_topics.len(), 3);

        // 단추 직접 적용
        let (state, effects) = step(&state, Event::ApplyTopTopic(1));
        assert_eq!(state.query, "공사");
        assert!(matches!(
            &effects[..],
            [Effect::Search { query, .. }] if query == "공사"
        ));
    }

    #[test]
    fn date_and_folder_filter_triggers_search_with_scope() {
        let state = step(&visible(), Event::Input("사업".into())).0;
        assert_eq!(state.date_filter, DateFilter::All);
        assert_eq!(state.selected_folder, None);

        // 기간 필터: 오늘
        let (state, effects) = step(&state, Event::SetDateFilter(DateFilter::Today));
        assert_eq!(state.date_filter, DateFilter::Today);
        assert!(matches!(
            &effects[..],
            [Effect::Search { date_filter: DateFilter::Today, folder: None, .. }]
        ));

        // 폴더 필터: "총무과"
        let (state, effects) = step(&state, Event::SetFolderFilter(Some("총무과".into())));
        assert_eq!(state.selected_folder.as_deref(), Some("총무과"));
        assert!(matches!(
            &effects[..],
            [Effect::Search { date_filter: DateFilter::Today, folder: Some(f), .. }] if f == "총무과"
        ));
    }

    #[test]
    fn test_scope_buttons_to_query_tokens_and_back() {
        let state = step(&visible(), Event::Input("예산".into())).0;
        assert_eq!(state.query, "예산");

        // 1. 확장자 단추: HWP 선택
        let (state, effects) = step(&state, Event::SetExtFilter(Some("hwp".into())));
        assert_eq!(state.selected_ext.as_deref(), Some("hwp"));
        assert_eq!(state.query, "예산 ext:hwp");
        assert!(matches!(&effects[..], [Effect::Search { ext: Some(e), query, .. }] if e == "hwp" && query == "예산 ext:hwp"));

        // 2. 폴더 단추: "총무과" 선택 -> 기존 ext 보존
        let (state, effects) = step(&state, Event::SetFolderFilter(Some("총무과".into())));
        assert_eq!(state.selected_folder.as_deref(), Some("총무과"));
        assert_eq!(state.query, "예산 ext:hwp path:총무과");
        assert!(matches!(&effects[..], [Effect::Search { folder: Some(f), ext: Some(e), .. }] if f == "총무과" && e == "hwp"));

        // 3. 폴더 변경: 공백이 있는 폴더 -> 따옴표 감싸기
        let (state, _) = step(&state, Event::SetFolderFilter(Some("C:\\공문서 2026".into())));
        assert_eq!(state.selected_folder.as_deref(), Some("C:\\공문서 2026"));
        assert_eq!(state.query, "예산 ext:hwp path:\"C:\\공문서 2026\"");

        // 4. 확장자 변경: pdf로 교체
        let (state, _) = step(&state, Event::SetExtFilter(Some("pdf".into())));
        assert_eq!(state.selected_ext.as_deref(), Some("pdf"));
        assert_eq!(state.query, "예산 ext:pdf path:\"C:\\공문서 2026\"");

        // 5. 기간 필터: 직접 날짜 지정 (20260101 ~ 20261231)
        let (state, _) = step(&state, Event::SetCustomDate(20260101, 20261231));
        assert_eq!(state.date_filter, DateFilter::Custom);
        assert_eq!(state.custom_date_range, Some((20260101, 20261231)));
        assert_eq!(state.query, "예산 ext:pdf path:\"C:\\공문서 2026\" after:2026-01-01 before:2026-12-31");

        // 6. 확장자 해제 (None)
        let (state, _) = step(&state, Event::SetExtFilter(None));
        assert_eq!(state.selected_ext, None);
        assert_eq!(state.query, "예산 path:\"C:\\공문서 2026\" after:2026-01-01 before:2026-12-31");

        // 7. 폴더 해제 (None)
        let (state, _) = step(&state, Event::SetFolderFilter(None));
        assert_eq!(state.selected_folder, None);
        assert_eq!(state.query, "예산 after:2026-01-01 before:2026-12-31");

        // 8. 기간 해제 (DateFilter::All)
        let (state, _) = step(&state, Event::SetDateFilter(DateFilter::All));
        assert_eq!(state.date_filter, DateFilter::All);
        assert_eq!(state.custom_date_range, None);
        assert_eq!(state.query, "예산");
    }

    #[test]
    fn test_query_tokens_input_synchronizes_scope_state() {
        // 검색창에 직접 path, ext, after, before 입력
        let (state, _) = step(
            &visible(),
            Event::Input("공사 계약 ext:pdf,hwp path:\"기안\\2026\" after:2026-05-01 before:2026-05-31".into()),
        );

        assert_eq!(state.selected_ext.as_deref(), Some("pdf,hwp"));
        assert_eq!(state.selected_folder.as_deref(), Some("기안\\2026"));
        assert_eq!(state.date_filter, DateFilter::Custom);
        assert_eq!(state.custom_date_range, Some((20260501, 20260531)));

        // 검색창에서 범위 토큰을 지우면 상태도 리셋
        let (state, _) = step(&state, Event::Input("공사 계약".into()));
        assert_eq!(state.selected_ext, None);
        assert_eq!(state.selected_folder, None);
        assert_eq!(state.date_filter, DateFilter::All);
        assert_eq!(state.custom_date_range, None);
    }

    #[test]
    fn test_token_parser_and_quote_protection() {
        // 따옴표 안의 문법 키워드는 보호되어야 함
        let tokens = parse_query_tokens("\"path:보호\" \"ext:pdf\" ext:hwp path:문서");
        assert_eq!(
            tokens,
            vec![
                QueryToken::Text("\"path:보호\"".into()),
                QueryToken::Text("\"ext:pdf\"".into()),
                QueryToken::Ext("hwp".into()),
                QueryToken::Path("문서".into()),
            ]
        );

        let query = tokens_to_query(&tokens);
        assert_eq!(query, "\"path:보호\" \"ext:pdf\" ext:hwp path:문서");

        // 토큰 치환 헬퍼
        let replaced_ext = set_ext_token(&query, Some("docx"));
        assert_eq!(replaced_ext, "\"path:보호\" \"ext:pdf\" ext:docx path:문서");

        let removed_path = set_folder_token(&replaced_ext, None);
        assert_eq!(removed_path, "\"path:보호\" \"ext:pdf\" ext:docx");
    }

    #[test]
    fn test_scope_roundtrip_fidelity() {
        let original_query = "보고서";
        let (state, _) = step(&visible(), Event::Input(original_query.into()));

        // 단추 조작으로 범위 설정
        let (state, _) = step(&state, Event::SetExtFilter(Some("pdf".into())));
        let (state, _) = step(&state, Event::SetFolderFilter(Some("감사과".into())));
        let (state, _) = step(&state, Event::SetCustomDate(20260301, 20260315));

        let generated_query = state.query.clone();
        let scope_before = state.scope();

        // 생성된 문자열로 새로운 창에서 Event::Input을 발생시킴
        let (reconstructed, _) = step(&visible(), Event::Input(generated_query));
        let scope_after = reconstructed.scope();

        assert_eq!(scope_before, scope_after);
        assert_eq!(scope_after.ext.as_deref(), Some("pdf"));
        assert_eq!(scope_after.folder.as_deref(), Some("감사과"));
        assert_eq!(scope_after.date_filter, DateFilter::Custom);
        assert_eq!(scope_after.custom_date_range, Some((20260301, 20260315)));
    }
}
