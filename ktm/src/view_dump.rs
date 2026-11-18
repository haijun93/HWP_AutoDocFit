//! Test-only textual model of the quick-search rows shown by the native window.
//!
//! This deliberately has no drawing code: its labels, excerpts, badges and UTF-8
//! highlight ranges are composed from the same existing calculations as painting.
use crate::backend::{compute_location_badge, extension_label, Row};
use crate::{highlight, tm_state::{ResultKind, State}};

#[derive(Clone, Debug, Default)]
pub struct ViewRow {
    pub kind: String,
    pub name: String,
    pub path: String,
    pub excerpt: String,
    pub highlights: Vec<(usize, usize)>,
    pub badges: Vec<String>,
}

/// Format one result as the strings and highlight spans the native row painter consumes.
pub fn result_row(row: &Row, terms: &[String], kinds: &[ResultKind], all_tab: bool) -> ViewRow {
    let kind = extension_label(&row.name).to_string();
    let name = row.name.clone();
    let path = row.parent.clone();
    let excerpt = if row.snippet.is_empty() { row.location.clone() } else { row.snippet.clone() };
    let excerpt_terms = if row.snippet.is_empty() { &[][..] } else { terms };
    let highlights = highlight::ranges_terms(&excerpt, excerpt_terms);
    let mut badges = vec![kind.clone()];
    if all_tab && !kinds.is_empty() {
        badges.push(kinds.iter().map(|k| match k {
            ResultKind::Names => "이름",
            ResultKind::Hierarchy => "위계",
            ResultKind::Body => "본문",
        }).collect::<Vec<_>>().join("·"));
    }
    if !row.snippet.is_empty() {
        if let Some(badge) = compute_location_badge(&row.location) {
            badges.push(badge);
        }
    }
    ViewRow { kind, name, path, excerpt, highlights, badges }
}

/// The visible text of the empty-query topic buttons, using the painter's exact label rule.
pub fn topic_buttons(topics: &[String]) -> Vec<String> {
    topics.iter().take(10).enumerate().map(|(i, topic)| format!("{}. {}", i + 1, topic)).collect()
}

/// The visible recent-search labels, using State's actual favorite/recent de-duplication.
pub fn recent_searches(state: &State) -> Vec<String> {
    state.history_entries().into_iter().map(|entry| {
        let prefix = if entry.is_favorite { "★ " } else { "🕒 " };
        if entry.is_favorite && !entry.name.is_empty() && entry.name != entry.query {
            format!("{prefix}{}: {}", entry.name, entry.query)
        } else {
            format!("{prefix}{}", entry.query)
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::FavoriteQuery;

    fn row(name: &str, parent: &str, snippet: &str, location: &str) -> Row {
        Row { name: name.into(), parent: parent.into(), snippet: snippet.into(), location: location.into(), ..Row::default() }
    }

    fn assert_no_replacement_question_marks(dump: &str) {
        assert!(!dump.contains("??"), "한글 덤프에 대체 문자 ??가 있음: {dump}");
    }

    #[test]
    fn empty_query_topic_button_dump() {
        let dump = topic_buttons(&["예산".into(), "공사".into(), "계획".into()]).join("\n");
        assert_eq!(dump, "1. 예산\n2. 공사\n3. 계획");
        assert_no_replacement_question_marks(&dump);
    }

    #[test]
    fn budget_three_row_view_dump_is_frozen() {
        let terms = vec!["예산".to_string()];
        let rows = [
            row("2026_예산안.hwp", "C:\\문서\\기획", "2026년 예산 편성 계획", "3쪽"),
            row("예산집행.hwpx", "C:\\문서\\재정", "예산 집행 세부 기준", "시트1 12행"),
            row("예산보고서.pdf", "C:\\문서\\보고", "상반기 예산 보고", "문단 4"),
        ];
        let dump = rows.iter().map(|r| {
            let view = result_row(r, &terms, &[], false);
            format!("{}\t{}\t{}\t{}\t{:?}\t{:?}", view.kind, view.name, view.path, view.excerpt, view.highlights, view.badges)
        }).collect::<Vec<_>>().join("\n");
        assert_eq!(dump, concat!(
            "HWP\t2026_예산안.hwp\tC:\\문서\\기획\t2026년 예산 편성 계획\t[(8, 14)]\t[\"HWP\", \"3쪽\"]\n",
            "HWPX\t예산집행.hwpx\tC:\\문서\\재정\t예산 집행 세부 기준\t[(0, 6)]\t[\"HWPX\", \"시트1 12행\"]\n",
            "PDF\t예산보고서.pdf\tC:\\문서\\보고\t상반기 예산 보고\t[(10, 16)]\t[\"PDF\"]"
        ));
        assert_no_replacement_question_marks(&dump);
    }

    #[test]
    fn all_tab_duplicate_document_has_two_reason_badges() {
        let kinds = [ResultKind::Names, ResultKind::Body];
        let view = result_row(&row("예산안.hwp", "C:\\문서", "예산 편성", "본문"), &["예산".into()], &kinds, true);
        let dump = format!("{}\t{}\t{:?}", view.name, view.excerpt, view.badges);
        assert_eq!(dump, "예산안.hwp\t예산 편성\t[\"HWP\", \"이름·본문\"]");
        assert_eq!(view.badges.len(), 2);
        assert_no_replacement_question_marks(&dump);
    }

    #[test]
    fn wildcard_query_result_view_dump_is_frozen() {
        let query = "예*집행";
        let parsed = kfind_core::parse_query_syntax(query);
        assert_eq!(parsed.wildcards, [query]);
        let view = result_row(&row("예산집행계획.docx", "D:\\회계", "예산집행 세부안", "문단 2"), &[], &[], false);
        let dump = format!("{}\t{}\t{}\t{:?}", view.kind, view.name, view.excerpt, view.highlights);
        assert_eq!(dump, "DOCX\t예산집행계획.docx\t예산집행 세부안\t[]");
        assert_no_replacement_question_marks(&dump);
    }

    #[test]
    fn related_query_groups_are_ready_for_chips_in_input_order() {
        let groups = kfind_core::parse_groups("2026; 공사 계약");
        let labels: Vec<String> = groups.into_iter().map(str::to_owned).collect();
        assert_eq!(labels, ["2026", "공사 계약"]);
    }

    #[test]
    fn recent_search_list_dump_is_frozen() {
        let mut state = State::default();
        state.favorite_queries = vec![FavoriteQuery::new("예산", "예산 업무")];
        state.recent_queries = vec!["예산".into(), "공사".into(), "계획".into()];
        let dump = recent_searches(&state).join("\n");
        assert_eq!(dump, "★ 예산 업무: 예산\n🕒 공사\n🕒 계획");
        assert_no_replacement_question_marks(&dump);
    }
}
