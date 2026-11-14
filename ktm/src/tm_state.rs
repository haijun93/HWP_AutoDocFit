//! Pure popup/search transitions. Native handles and search results live in the adapter.
//! Hidden input/completions cannot show the window; only `Show` does so.
use crate::backend::Row;
use std::collections::{HashMap, HashSet};

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
    All,
    Names,
    Hierarchy,
    Body,
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
            tab: Tab::All,
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
    /// 같은 검색어·칸으로 다시 찾기(파일명 정렬 바꿈 등). 앞 결과는 무효.
    Refresh,
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
    if query.chars().count() >= if explicit { 1 } else { 2 } {
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
        Event::Refresh => {
            let explicit = state.query.trim().chars().count() == 1 && state.phase != Phase::Typing;
            search(&mut state, &mut effects, explicit);
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

/// 전체 경로 정규화 (중복 제거 열쇠: 소문자화, 슬래시 통일, 공백 제거)
pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.replace('/', "\\").to_lowercase()
}

fn row_key(row: &Row) -> String {
    let p = normalize_path(&row.path);
    if !p.is_empty() {
        p
    } else if row.doc_id != 0 {
        format!("doc_id:{}", row.doc_id)
    } else {
        row.name.trim().to_lowercase()
    }
}

/// 1순위 파일명(names), 2순위 위계(hier), 3순위 본문(body) 결과를 통합 순서로 병합한다.
/// - 같은 문서가 여러 단에 걸리면 가장 높은 단에 한 번만 둔다.
/// - "제목에도 있음·본문 n문장" 같은 표시를 붙인다 (중복 제거 열쇠: 정규화한 전체 경로).
/// - 반환 목록은 항상 1단(파일명) -> 2단(위계) -> 3단(본문) 순서이다.
pub fn merge_tiers(names: Vec<Row>, hier: Vec<Row>, body: Vec<Row>) -> Vec<Row> {
    let mut hier_map: HashMap<String, Row> = HashMap::with_capacity(hier.len());
    for r in &hier {
        let k = row_key(r);
        hier_map.entry(k).or_insert_with(|| r.clone());
    }

    let mut body_map: HashMap<String, Row> = HashMap::with_capacity(body.len());
    for r in &body {
        let k = row_key(r);
        body_map.entry(k).or_insert_with(|| r.clone());
    }

    let mut seen: HashSet<String> = HashSet::with_capacity(names.len() + hier.len() + body.len());
    let mut out: Vec<Row> = Vec::with_capacity(names.len() + hier.len() + body.len());

    // 1단: 파일명 일치
    for mut r in names {
        let k = row_key(&r);
        r.tier = 1;
        let h_opt = hier_map.get(&k);
        let b_opt = body_map.get(&k);
        let has_hier = h_opt.is_some();
        let has_body = b_opt.is_some();
        let body_count = b_opt.map(|b| b.count).unwrap_or(0);

        let note = if has_hier && has_body {
            Some(format!("제목에도 있음 · 본문 {}문장", body_count))
        } else if has_hier {
            Some("제목에도 있음".to_string())
        } else if has_body {
            Some(format!("본문 {}문장", body_count))
        } else {
            None
        };

        if let Some(n) = note {
            if r.location.is_empty() {
                r.location = n;
            } else {
                r.location = format!("{} · {}", r.location, n);
            }
        }

        if r.doc_id == 0 {
            if let Some(h) = h_opt {
                r.doc_id = h.doc_id;
                r.para = h.para;
                if !h.snippet.is_empty() {
                    r.snippet = h.snippet.clone();
                }
            } else if let Some(b) = b_opt {
                r.doc_id = b.doc_id;
                r.para = b.para;
                if !b.snippet.is_empty() {
                    r.snippet = b.snippet.clone();
                }
            }
        }

        seen.insert(k);
        out.push(r);
    }

    // 2단: 위계 일치 (본문보다 높은 위계에서 일치)
    for mut r in hier {
        let k = row_key(&r);
        if seen.contains(&k) {
            continue;
        }
        r.tier = 2;
        let b_opt = body_map.get(&k);
        if let Some(b) = b_opt {
            let note = format!("본문 {}문장", b.count);
            if r.location.is_empty() {
                r.location = note;
            } else {
                r.location = format!("{} · {}", r.location, note);
            }
        }
        seen.insert(k);
        out.push(r);
    }

    // 3단: 본문 일치 (본문에서만 일치)
    for mut r in body {
        let k = row_key(&r);
        if seen.contains(&k) {
            continue;
        }
        r.tier = 3;
        seen.insert(k);
        out.push(r);
    }

    out
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
    fn merge_tiers_order_and_deduplication() {
        let n1 = Row {
            path: "C:\\Docs\\Report.hwp".into(),
            name: "Report.hwp".into(),
            location: "1.2 MB · 2026-11-14".into(),
            ..Row::default()
        };
        let n2 = Row {
            path: "C:\\Docs\\OnlyName.hwp".into(),
            name: "OnlyName.hwp".into(),
            location: "500 KB".into(),
            ..Row::default()
        };

        // Report.hwp는 hier에도 있고, body에도 있음 (대소문자/슬래시 다름)
        let h1 = Row {
            path: "c:/docs/report.hwp".into(),
            name: "Report.hwp".into(),
            location: "문단 1 · 위계 1".into(),
            level: 1,
            count: 2,
            ..Row::default()
        };
        let h2 = Row {
            path: "C:\\Docs\\OnlyHier.hwp".into(),
            name: "OnlyHier.hwp".into(),
            location: "문단 3 · 위계 2".into(),
            level: 2,
            count: 1,
            ..Row::default()
        };

        let b1 = Row {
            path: "C:\\docs\\REPORT.HWP".into(),
            name: "Report.hwp".into(),
            location: "문단 10 · 위계 8".into(),
            level: 8,
            count: 5,
            ..Row::default()
        };
        let b2 = Row {
            path: "c:/docs/onlyhier.hwp".into(),
            name: "OnlyHier.hwp".into(),
            location: "문단 20 · 위계 8".into(),
            level: 8,
            count: 3,
            ..Row::default()
        };
        let b3 = Row {
            path: "C:\\Docs\\OnlyBody.hwp".into(),
            name: "OnlyBody.hwp".into(),
            location: "문단 5 · 위계 8".into(),
            level: 8,
            count: 4,
            ..Row::default()
        };

        let merged = merge_tiers(vec![n1, n2], vec![h1, h2], vec![b1, b2, b3]);

        // 결과 문서 수: Report, OnlyName, OnlyHier, OnlyBody 총 4건
        assert_eq!(merged.len(), 4);

        // 1순위: 파일명(Report, OnlyName) -> 2순위: 위계(OnlyHier) -> 3순위: 본문(OnlyBody)
        assert_eq!(merged[0].name, "Report.hwp");
        assert_eq!(merged[0].tier, 1);
        // Report는 hier와 body에 모두 있으므로: "제목에도 있음 · 본문 5문장"
        assert!(
            merged[0].location.contains("제목에도 있음 · 본문 5문장"),
            "location: {}",
            merged[0].location
        );

        assert_eq!(merged[1].name, "OnlyName.hwp");
        assert_eq!(merged[1].tier, 1);
        assert_eq!(merged[1].location, "500 KB");

        assert_eq!(merged[2].name, "OnlyHier.hwp");
        assert_eq!(merged[2].tier, 2);
        // OnlyHier는 body에도 있으므로 "본문 3문장"이 붙어야 함
        assert!(
            merged[2].location.contains("본문 3문장"),
            "location: {}",
            merged[2].location
        );

        assert_eq!(merged[3].name, "OnlyBody.hwp");
        assert_eq!(merged[3].tier, 3);
    }

    #[test]
    fn state_machine_drops_late_results() {
        let (state, fx) = step(&visible(), Event::Input("공사".into()));
        assert_eq!(state.phase, Phase::Searching);
        let g1 = state.generation;

        // 사용자가 다음 글자 입력 ("공사 계약")
        let (state2, fx2) = step(&state, Event::Input("공사 계약".into()));
        assert_eq!(state2.phase, Phase::Searching);
        let g2 = state2.generation;
        assert!(g2 > g1);

        // 이전 세대(g1)의 검색 완료 결과가 늦게 도착함 -> 버려져야 함!
        let (state_stale, _) = step(&state2, Event::SearchFinished { generation: g1, count: 10 });
        assert_eq!(state_stale.phase, Phase::Searching);
        assert_eq!(state_stale.result_count, 0);

        // 현재 세대(g2)의 검색 완료 결과 도착 -> 반영됨!
        let (state_curr, _) = step(&state2, Event::SearchFinished { generation: g2, count: 5 });
        assert_eq!(state_curr.phase, Phase::Results);
        assert_eq!(state_curr.result_count, 5);
        assert_eq!(state_curr.selected, Some(0));
    }

    #[test]
    fn search_groups_query_with_semicolon_and_tabs() {
        let state = visible();
        assert_eq!(state.tab, Tab::All);

        // 연관 검색어 ';' 입력
        let (state, fx) = step(&state, Event::Input("2026; 공사 계약".into()));
        assert_eq!(state.phase, Phase::Searching);
        assert_eq!(state.query, "2026; 공사 계약");
        assert!(matches!(
            &fx[..],
            [Effect::Search { query, tab, .. }] if query == "2026; 공사 계약" && *tab == Tab::All
        ));

        // 탭 전환 (모두 -> 파일명 -> 위계 -> 본문) 검색어는 공유 유지
        let (state, fx) = step(&state, Event::SwitchTab(Tab::Names));
        assert_eq!(state.tab, Tab::Names);
        assert_eq!(state.query, "2026; 공사 계약");

        let (state, fx) = step(&state, Event::SwitchTab(Tab::Hierarchy));
        assert_eq!(state.tab, Tab::Hierarchy);
        assert_eq!(state.query, "2026; 공사 계약");

        let (state, fx) = step(&state, Event::SwitchTab(Tab::Body));
        assert_eq!(state.tab, Tab::Body);
        assert_eq!(state.query, "2026; 공사 계약");
    }
}
