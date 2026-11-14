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
            if state.date_filter != df {
                state.date_filter = df;
                let explicit = state.query.trim().chars().count() == 1 && state.phase != Phase::Typing;
                search(&mut state, &mut effects, explicit);
            }
        }
        Event::SetCustomDate(start, end) => {
            state.date_filter = DateFilter::Custom;
            state.custom_date_range = Some((start, end));
            let explicit = state.query.trim().chars().count() == 1 && state.phase != Phase::Typing;
            search(&mut state, &mut effects, explicit);
        }
        Event::SetFolderFilter(f) => {
            if state.selected_folder != f {
                state.selected_folder = f;
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
}
