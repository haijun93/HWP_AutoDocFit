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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub phase: Phase,
    pub query: String,
    pub tab: Tab,
    pub generation: u64,
    pub result_count: usize,
    pub selected: Option<usize>,
    pub error: Option<String>,
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
    SearchFinished { generation: u64, count: usize },
    SearchFailed { generation: u64, message: String },
    MoveSelection(isize),
    Select(usize),
    Open { folder: bool },
    OpenFailed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Show,
    Hide,
    Search {
        generation: u64,
        query: String,
        tab: Tab,
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
    // Increment even for an empty/short query: an older completion is now invalid.
    state.generation = state.generation.wrapping_add(1);
    let query = state.query.trim();
    if state.tab != Tab::Names && query.chars().count() >= if explicit { 1 } else { 2 } {
        state.phase = Phase::Searching;
        effects.push(Effect::Search {
            generation: state.generation,
            query: query.to_owned(),
            tab: state.tab,
        });
    } else {
        state.phase = if query.is_empty() {
            Phase::Visible
        } else {
            Phase::Typing
        };
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
        Event::Submit => {
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
            if state.phase == Phase::Results && state.result_count > 0 {
                let index = state.selected.unwrap_or(0);
                state.selected = Some(
                    index
                        .saturating_add_signed(delta)
                        .min(state.result_count - 1),
                );
            }
        }
        Event::Select(index) => {
            if state.phase == Phase::Results && index < state.result_count {
                state.selected = Some(index);
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
    fn names_slot_is_disabled() {
        let state = step(&visible(), Event::Input("예산".into())).0;
        let (state, effects) = step(&state, Event::SwitchTab(Tab::Names));
        assert!(effects.is_empty());
        assert!(step(&state, Event::Submit).1.is_empty());
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
}
