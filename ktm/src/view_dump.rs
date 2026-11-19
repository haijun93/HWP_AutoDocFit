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

/// Textual view of the related-query chips under the search edit.
pub fn related_query_chips(query: &str, width: i32, scale: i32) -> Vec<String> {
    let groups: Vec<String> = kfind_core::parse_groups(query)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let rects = crate::layout::related_chip_rects(&groups, 0, 0, width, scale);
    groups
        .into_iter()
        .zip(rects)
        .map(|(label, rect)| {
            format!(
                "{}:{}:{}:{}",
                label,
                rect.left,
                rect.top,
                rect.right - rect.left
            )
        })
        .collect()
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

/// Dump the native quick-search bands with the same row captions and geometry as the painter.
pub fn quick_search_bands(
    width: i32,
    height: i32,
    scale: i32,
    has_related: bool,
    show_options: bool,
    has_autocomplete: bool,
    related: &[String],
    summary: &str,
    status: &str,
) -> Vec<String> {
    let bands = crate::layout::quick_search_layout(scale, width, height, has_related, show_options, has_autocomplete);
    let mut dump = Vec::new();
    push_band_line(&mut dump, "related", bands.related, &related.join(" · "));
    push_band_line(&mut dump, "options", bands.options, "통합 검색 | 정렬:관련도 | 형식:전체 | 기간:전체 | 제외:□ | 결과 내:Ctrl+F | [옵션]");
    push_band_line(&mut dump, "filters", bands.filters, "기간: 전체 · 오늘 · 7일간 · 30일간 · 올해 | 형식: 전체 · HWP · PDF · DOC · XLS");
    push_band_line(&mut dump, "summary", bands.summary, summary);
    push_band_line(&mut dump, "autocomplete", bands.autocomplete, "자동완성: 예산 · 공사 · 계약");
    dump.push(format!("list {}..{height} \"결과 목록\"", bands.list_top));
    push_band_line(&mut dump, "status", windows_sys::Win32::Foundation::RECT { left: 0, top: height - 24 * scale / 100, right: width, bottom: height }, status);
    dump
}

fn push_band_line(dump: &mut Vec<String>, name: &str, rect: windows_sys::Win32::Foundation::RECT, text: &str) {
    if rect.bottom > rect.top {
        dump.push(format!("{name} {}..{} \"{text}\"", rect.top, rect.bottom));
    }
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
        let dump = related_query_chips("2026; 공사 계약", 320, 100).join("\n");
        assert_eq!(dump, "2026:0:0:56\n공사 계약:60:0:64");
    }

    #[test]
    fn related_query_chip_view_dump_wraps_with_layout_and_suppresses_single_query() {
        let dump = related_query_chips("2026; 공사 계약; 안전관리계획", 130, 100).join("\n");
        assert_eq!(dump, "2026:0:0:56\n공사 계약:60:0:64\n안전관리계획:0:28:72");
        assert!(related_query_chips("단일 검색어", 320, 100).is_empty());
        assert_no_replacement_question_marks(&dump);
    }

    #[test]
    fn quick_search_empty_results_related_and_error_band_dumps_are_frozen() {
        // Hand calculation from quick_search_layout: at 100%, search ends at 76 and (탭 줄이 없어져)
        // the option/filter/summary rows end at 134/158/190; with chips at 125%, 35px is added
        // before them, and each later row height is rounded by integer scale(value * 125 / 100).
        let empty = quick_search_bands(640, 900, 100, false, true, false, &[],
            "☷ 0개 · 0.00초", "↑↓ 선택 · Enter 열기 · Ctrl+D 즐겨찾기");
        assert_eq!(empty, [
            "options 76..134 \"통합 검색 | 정렬:관련도 | 형식:전체 | 기간:전체 | 제외:□ | 결과 내:Ctrl+F | [옵션]\"",
            "filters 134..158 \"기간: 전체 · 오늘 · 7일간 · 30일간 · 올해 | 형식: 전체 · HWP · PDF · DOC · XLS\"",
            "summary 158..190 \"☷ 0개 · 0.00초\"",
            "list 190..900 \"결과 목록\"",
            "status 876..900 \"↑↓ 선택 · Enter 열기 · Ctrl+D 즐겨찾기\"",
        ]);

        let results = quick_search_bands(640, 900, 125, false, true, false, &[],
            "☷ 2개 · 0.12초", "2 documents");
        assert_eq!(results[0], "options 95..167 \"통합 검색 | 정렬:관련도 | 형식:전체 | 기간:전체 | 제외:□ | 결과 내:Ctrl+F | [옵션]\"");
        assert_eq!(results[1].split_whitespace().take(2).collect::<Vec<_>>(), ["filters", "167..197"]);
        assert_eq!(results[2].split_whitespace().take(2).collect::<Vec<_>>(), ["summary", "197..237"]);
        assert_eq!(results[3], "list 237..900 \"결과 목록\"");

        let related = quick_search_bands(640, 900, 125, true, true, false,
            &["예산".into(), "공사".into()], "☷ 2개 · 0.12초", "2 documents");
        assert_eq!(related[0], "related 95..130 \"예산 · 공사\"");
        assert_eq!(related[1], "options 130..202 \"통합 검색 | 정렬:관련도 | 형식:전체 | 기간:전체 | 제외:□ | 결과 내:Ctrl+F | [옵션]\"");
        assert_eq!(related[4], "list 272..900 \"결과 목록\"");

        let error = quick_search_bands(640, 900, 150, false, true, false, &[],
            "☷ 0개 · 0.00초", "검색 서비스 오류");
        assert_eq!(error[0], "options 114..201 \"통합 검색 | 정렬:관련도 | 형식:전체 | 기간:전체 | 제외:□ | 결과 내:Ctrl+F | [옵션]\"");
        assert_eq!(error[3], "list 285..900 \"결과 목록\"");
        assert_eq!(error[4], "status 864..900 \"검색 서비스 오류\"");
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
