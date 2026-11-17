//! 퀵몽땅검색 창 순수 레이아웃 계산 및 테스트.
//! 모든 OS에서 단위 시험(cargo test)으로 검증한다.

use windows_sys::Win32::Foundation::RECT;

// ---- 기본 치수 (96 DPI 기준 논리 픽셀) ----
pub const HEAD_H: i32 = 32;
pub const CAPTION_W: i32 = 46;
pub const SEARCH_BOX_H: i32 = 32;
pub const TAB_H: i32 = 28;
pub const FILTER_H: i32 = 24;
pub const ROW_H: i32 = 70;
pub const ROW_FOLDER_W: i32 = 28;
pub const STATUS_H: i32 = 24;
pub const PREVIEW_H: i32 = 64;
pub const OPTION_H: i32 = 58;
pub const SUMMARY_H: i32 = 32;
pub const SIDEBAR_W: i32 = 220;
pub const SIDEBAR_ROW_H: i32 = 26;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalKind {
    Statistics,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalHit {
    Outside,
    Close,
    Tab(usize),
    Content,
}

#[derive(Clone, Copy)]
pub struct ModalRects {
    pub panel: RECT,
    pub close: RECT,
    pub tabs: [RECT; 4],
    pub content: RECT,
}

pub fn modal_rects(width: i32, height: i32, tabs: usize) -> ModalRects {
    let margin_x = (width / 12).clamp(18, 64);
    let margin_y = (height / 14).clamp(16, 48);
    let panel = RECT {
        left: margin_x,
        top: margin_y,
        right: (width - margin_x).max(margin_x + 1),
        bottom: (height - margin_y).max(margin_y + 1),
    };
    let header_h = 48;
    let tab_h = 38;
    let close = RECT {
        left: panel.right - 44,
        top: panel.top + 5,
        right: panel.right - 8,
        bottom: panel.top + 41,
    };
    let usable = (panel.right - panel.left - 24).max(0);
    let count = tabs.clamp(1, 4);
    let tab_w = usable / count as i32;
    let mut rects = [RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    }; 4];
    for (i, rect) in rects.iter_mut().enumerate() {
        let left = panel.left + 12 + tab_w * i as i32;
        *rect = RECT {
            left,
            top: panel.top + header_h,
            right: if i + 1 == count {
                panel.right - 12
            } else {
                left + tab_w
            },
            bottom: panel.top + header_h + tab_h,
        };
    }
    let content_top = panel.top + header_h + tab_h;
    ModalRects {
        panel,
        close,
        tabs: rects,
        content: RECT {
            left: panel.left + 16,
            top: content_top + 12,
            right: panel.right - 16,
            bottom: panel.bottom - 16,
        },
    }
}

pub fn hit_modal(x: i32, y: i32, width: i32, height: i32, tabs: usize) -> ModalHit {
    let r = modal_rects(width, height, tabs);
    if x < r.panel.left || x >= r.panel.right || y < r.panel.top || y >= r.panel.bottom {
        return ModalHit::Outside;
    }
    if x >= r.close.left && x < r.close.right && y >= r.close.top && y < r.close.bottom {
        return ModalHit::Close;
    }
    for (i, tab) in r.tabs.iter().take(tabs.clamp(1, 4)).enumerate() {
        if x >= tab.left && x < tab.right && y >= tab.top && y < tab.bottom {
            return ModalHit::Tab(i);
        }
    }
    ModalHit::Content
}

pub fn header_chip_rects(width: i32, button_width: i32, scale: i32) -> [(i32, i32); 2] {
    let left = (width - 2 * button_width - 166 * scale).max(150 * scale);
    let right = (width - 2 * button_width - 86 * scale).max(230 * scale);
    [(left, right), (right + 4 * scale, right + 82 * scale)]
}

pub fn hit_header_chip(
    x: i32,
    y: i32,
    width: i32,
    button_width: i32,
    head: i32,
    scale: i32,
) -> Option<usize> {
    if y < 0 || y >= head {
        return None;
    }
    header_chip_rects(width, button_width, scale)
        .iter()
        .position(|(l, r)| x >= *l && x < *r)
}

/// 사이드바 행의 의미. 인덱스는 화면에 그린 동적 목록의 인덱스다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarHit {
    Toggle,
    IndexedFolder(usize),
    Recommended(usize),
    Preset(usize),
    History(usize),
    ClearHistory,
}

/// 사이드바 클릭을 순수 판정한다. y는 클라이언트 좌표, 목록 길이는 그려진 항목 수다.
pub fn hit_sidebar(
    x: i32,
    y: i32,
    width: i32,
    open: bool,
    indexed: usize,
    presets: usize,
    history: usize,
    drives: usize,
) -> Option<SidebarHit> {
    if !open || x < 0 || x >= width || y < 40 {
        return None;
    }
    let row = ((y - 40) / SIDEBAR_ROW_H) as usize;
    let folders = 2..2 + indexed;
    let recommendations_header = 2 + indexed;
    let recommendations = recommendations_header + 1..recommendations_header + 1 + drives;
    let preset_header = recommendations_header + 1 + drives;
    let preset_rows = preset_header + 1..preset_header + 1 + presets;
    let history_header = preset_header + 1 + presets;
    let history_rows = history_header + 1..history_header + 1 + history;
    let clear = history_header + 1 + history;
    if row == 0 {
        Some(SidebarHit::Toggle)
    } else if folders.contains(&row) {
        Some(SidebarHit::IndexedFolder(row - 2))
    } else if recommendations.contains(&row) {
        Some(SidebarHit::Recommended(row - (recommendations_header + 1)))
    } else if preset_rows.contains(&row) {
        Some(SidebarHit::Preset(row - (preset_header + 1)))
    } else if history_rows.contains(&row) {
        Some(SidebarHit::History(row - (history_header + 1)))
    } else if row == clear {
        Some(SidebarHit::ClearHistory)
    } else {
        None
    }
}

pub fn hit_sidebar_add(x: i32, y: i32, width: i32, open: bool) -> bool {
    open && y >= 40 + SIDEBAR_ROW_H && y < 40 + 2 * SIDEBAR_ROW_H && x >= width - 34 && x < width
}

pub fn hit_match_chip(x: i32, left: i32, chip_width: i32, count: usize) -> Option<usize> {
    if x < left || chip_width <= 0 {
        return None;
    }
    let idx = ((x - left) / chip_width) as usize;
    (idx < count).then_some(idx)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionControl {
    SearchMode,
    Sort,
    FileTypes,
    Period,
    ExcludeFilename,
    WithinQuery,
    Preset,
    MatchMode,
    CustomDays,
}

/// 옵션 바 각 영역을 가용 폭에 맞춰 동일 폭으로 배치한다.
pub fn option_spans(left: i32, right: i32) -> [(i32, i32); 7] {
    let avail = (right - left).max(0);
    std::array::from_fn(|i| {
        (
            left + avail * i as i32 / 7,
            left + avail * (i + 1) as i32 / 7,
        )
    })
}

pub fn hit_option(
    x: i32,
    y: i32,
    top: i32,
    height: i32,
    left: i32,
    right: i32,
) -> Option<OptionControl> {
    if y < top || y >= top + height {
        return None;
    }
    if y >= top + height / 2 {
        let chip_start = left + 2;
        return if x >= chip_start && x < chip_start + 3 * 86 {
            Some(OptionControl::MatchMode)
        } else if x >= chip_start + 3 * 86 && x < right {
            Some(OptionControl::CustomDays)
        } else {
            None
        };
    }
    let spans = option_spans(left, right);
    let idx = spans.iter().position(|(a, b)| x >= *a && x < *b)?;
    Some(
        [
            OptionControl::SearchMode,
            OptionControl::Sort,
            OptionControl::FileTypes,
            OptionControl::Period,
            OptionControl::ExcludeFilename,
            OptionControl::WithinQuery,
            OptionControl::Preset,
        ][idx],
    )
}

pub fn hit_summary_group(
    x: i32,
    y: i32,
    top: i32,
    height: i32,
    left: i32,
    right: i32,
) -> Option<bool> {
    if y < top || y >= top + height || x < left || x >= right {
        return None;
    }
    let mid = left + (right - left) / 2;
    if x < mid { Some(true) } else { Some(false) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    Relevance,
    Newest,
    Oldest,
    Name,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Keyword,
    Filename,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptionBar {
    pub search_mode: SearchMode,
    pub sort: SortMode,
    pub file_types: Vec<String>,
    pub days: Option<u32>,
    pub exclude_filename: bool,
    pub within_query: String,
    pub preset_name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchGroups {
    pub filename: usize,
    pub body: usize,
}

pub fn filter_result_indices(
    names: &[String],
    snippets: &[String],
    query: &str,
    exclude_filename: bool,
) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    (0..names.len().min(snippets.len()))
        .filter(|&i| {
            let name_match = !exclude_filename && names[i].to_lowercase().contains(&q);
            let body_match = snippets[i].to_lowercase().contains(&q);
            q.is_empty() || name_match || body_match
        })
        .collect()
}

/// 결과 내 검색과 확장자 선택을 현재 표시 결과에 적용한다.
pub fn filter_rows_within_query(
    rows: &[crate::backend::Row],
    query: &str,
    exclude_filename: bool,
    types: &[String],
) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    rows.iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let ext = std::path::Path::new(&row.name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let type_ok = types.is_empty() || types.iter().any(|t| t.eq_ignore_ascii_case(&ext));
            let text_ok = q.is_empty()
                || (!exclude_filename && row.name.to_lowercase().contains(&q))
                || row.snippet.to_lowercase().contains(&q);
            let body_only_ok = !exclude_filename || !row.snippet.is_empty();
            (type_ok && text_ok && body_only_ok).then_some(i)
        })
        .collect()
}

pub fn display_result_indices(
    rows: &[crate::backend::Row],
    query: &str,
    exclude_filename: bool,
    types: &[String],
    collapse_filename: bool,
    collapse_body: bool,
) -> Vec<usize> {
    filter_rows_within_query(rows, query, exclude_filename, types)
        .into_iter()
        .filter(|i| {
            rows.get(*i).is_some_and(|row| {
                !((row.snippet.is_empty() && collapse_filename)
                    || (!row.snippet.is_empty() && collapse_body))
            })
        })
        .collect()
}

/// Apply settings that affect which backend results are visible.
pub fn filter_row_indices(
    rows: &[crate::backend::Row],
    settings: &crate::settings::Settings,
    selected_folder: Option<&str>,
) -> Vec<usize> {
    filter_row_indices_with_roots(rows, settings, selected_folder, &[])
}

pub fn filter_row_indices_with_roots(
    rows: &[crate::backend::Row],
    settings: &crate::settings::Settings,
    selected_folder: Option<&str>,
    indexed_roots: &[String],
) -> Vec<usize> {
    let root = selected_folder.map(normalize_path);
    let roots: Vec<String> = indexed_roots.iter().map(|p| normalize_path(p)).collect();
    let excluded: Vec<String> = settings
        .excluded_directories
        .iter()
        .map(|p| normalize_path(p))
        .collect();
    let indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let path = normalize_path(&row.path);
            let parent = normalize_path(&row.parent);
            if !settings.include_subfolders {
                if let Some(root) = root.as_deref() {
                    if parent != root {
                        return None;
                    }
                } else {
                    let known_roots = if roots.is_empty() {
                        let mut found: Vec<String> = rows
                            .iter()
                            .map(|row| normalize_path(&row.parent))
                            .filter(|p| !p.is_empty())
                            .collect();
                        found.sort();
                        found.dedup();
                        let all = found.clone();
                        found.retain(|candidate| {
                            !all.iter().any(|other| {
                                candidate != other && candidate.starts_with(&format!("{other}\\"))
                            })
                        });
                        found
                    } else {
                        roots.clone()
                    };
                    if !known_roots.is_empty() && !known_roots.iter().any(|known| parent == *known)
                    {
                        return None;
                    }
                }
            }
            if excluded
                .iter()
                .any(|dir| path == *dir || path.starts_with(&format!("{dir}\\")))
            {
                return None;
            }
            Some(i)
        })
        .collect();
    let grouped = if settings.version_grouping {
        let versions = crate::backend::group_document_versions(
            &indices.iter().map(|i| rows[*i].clone()).collect::<Vec<_>>(),
        );
        versions
            .iter()
            .filter_map(|row| indices.iter().find(|i| rows[**i].path == row.path).copied())
            .collect()
    } else {
        indices
    };
    grouped
        .into_iter()
        .take(settings.bounded_max_results())
        .collect()
}

pub fn remove_excluded_directory(excluded: &mut Vec<String>, path: &str) -> bool {
    let before = excluded.len();
    excluded.retain(|entry| !entry.eq_ignore_ascii_case(path));
    excluded.len() != before
}

pub fn scaled_window_size(width: i32, height: i32, old_scale: u32, new_scale: u32) -> (i32, i32) {
    let old_scale = old_scale.max(1) as i64;
    let new_scale = new_scale.max(1) as i64;
    (
        ((width as i64 * new_scale + old_scale / 2) / old_scale).clamp(1, i32::MAX as i64) as i32,
        ((height as i64 * new_scale + old_scale / 2) / old_scale).clamp(1, i32::MAX as i64) as i32,
    )
}

pub fn settings_visible_indices(
    rows: &[crate::backend::Row],
    settings: &crate::settings::Settings,
    selected_folder: Option<&str>,
    query: &str,
    exclude_filename: bool,
    types: &[String],
    collapsed_filename: bool,
    collapsed_body: bool,
) -> Vec<usize> {
    let allowed = filter_row_indices(rows, settings, selected_folder);
    display_result_indices(
        rows,
        query,
        exclude_filename,
        types,
        collapsed_filename,
        collapsed_body,
    )
    .into_iter()
    .filter(|i| allowed.contains(i))
    .collect()
}

fn normalize_path(path: &str) -> String {
    path.trim()
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase()
}

pub fn density_row_height(compact: bool) -> i32 {
    if compact { 56 } else { ROW_H }
}

pub fn click_opens(mode: u8, double_click: bool, folder_button: bool) -> bool {
    folder_button
        || if mode == 1 {
            !double_click
        } else {
            double_click
        }
}

pub fn row_meta(show_location: bool, show_date: bool, parent: &str, date: &str) -> String {
    match (
        show_location && !parent.is_empty(),
        show_date && !date.is_empty(),
    ) {
        (true, true) => format!("{parent}  |  {date}"),
        (true, false) => parent.to_string(),
        (false, true) => date.to_string(),
        (false, false) => String::new(),
    }
}

pub fn copy_result_text(names: &[String], paths: &[String]) -> String {
    names
        .iter()
        .zip(paths)
        .map(|(n, p)| format!("{n}\t{p}"))
        .collect::<Vec<_>>()
        .join("\r\n")
}

pub fn match_groups(names: &[String], snippets: &[String]) -> MatchGroups {
    let n = names.len().min(snippets.len());
    MatchGroups {
        filename: (0..n).filter(|&i| !names[i].is_empty()).count(),
        body: (0..n).filter(|&i| !snippets[i].is_empty()).count(),
    }
}

pub fn sidebar_width(open: bool, width: i32) -> i32 {
    if open {
        SIDEBAR_W.min(width.max(0) / 2)
    } else {
        0
    }
}

pub fn calc_results_top(head: i32, search: i32, tabs: i32, options: i32, summary: i32) -> i32 {
    head + search + tabs + options + summary
}

/// 상단 캡션 단추.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeadButton {
    Minimize,
    Close,
}

pub const HEAD_BUTTONS: [HeadButton; 2] = [HeadButton::Minimize, HeadButton::Close];

/// 상단 캡션 단추의 가로 영역 [left, right) 계산.
pub fn head_button_span(b: HeadButton, width: i32, button_w: i32) -> (i32, i32) {
    let i = HEAD_BUTTONS.iter().position(|x| *x == b).unwrap_or(0) as i32;
    let from_right = HEAD_BUTTONS.len() as i32 - i;
    let left = width - from_right * button_w;
    (left, left + button_w)
}

/// 좌표 (x, y)가 어느 캡션 단추 위인지 판정.
pub fn head_button_hit(x: i32, y: i32, width: i32, button_w: i32, head: i32) -> Option<HeadButton> {
    if y < 0 || y >= head || x < 0 || x >= width {
        return None;
    }
    HEAD_BUTTONS.iter().copied().find(|b| {
        let (l, r) = head_button_span(*b, width, button_w);
        x >= l && x < r
    })
}

/// 단추 글리프 (Segoe MDL2 Assets).
pub fn head_button_glyph(b: HeadButton) -> char {
    match b {
        HeadButton::Minimize => '\u{E921}', // ChromeMinimize
        HeadButton::Close => '\u{E8BB}',    // ChromeClose
    }
}

/// 파일명 확장자에 따른 표시 꼬리표.
pub fn extension_tag(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "hwp" | "hwpx" => "HWP",
        "pdf" => "PDF",
        "doc" | "docx" => "DOC",
        "txt" => "TXT",
        "xls" | "xlsx" | "csv" => "XLS",
        "ppt" | "pptx" => "PPT",
        _ => "문서",
    }
}

/// 결과 목록 시작 Y 좌표 계산.
pub fn calc_list_top(
    head_h: i32,
    search_h: i32,
    tab_h: i32,
    filter_h: i32,
    has_autocomplete: bool,
    ac_h: i32,
) -> i32 {
    let base = head_h + search_h + tab_h + filter_h;
    if has_autocomplete { base + ac_h } else { base }
}

/// Y position after the fixed header, option/filter/summary bands and optional autocomplete.
pub fn calc_content_top(
    head: i32,
    search: i32,
    tabs: i32,
    options: i32,
    filters: i32,
    summary: i32,
    has_ac: bool,
    ac: i32,
) -> i32 {
    head + search + tabs + options + filters + summary + if has_ac { ac } else { 0 }
}

#[cfg(test)]
mod content_top_tests {
    use super::*;
    #[test]
    fn content_bands_are_sequential() {
        assert_eq!(calc_content_top(32, 44, 28, 30, 24, 32, false, 22), 190);
        assert_eq!(calc_content_top(32, 44, 28, 30, 24, 32, true, 22), 212);
    }
}

#[cfg(test)]
mod interaction_hit_tests {
    use super::*;
    #[test]
    fn sidebar_toggle_hit() {
        assert_eq!(
            hit_sidebar(12, 45, 200, true, 2, 1, 3, 0),
            Some(SidebarHit::Toggle)
        );
    }
    #[test]
    fn sidebar_add_folder_hit_is_header_plus_only() {
        assert!(hit_sidebar_add(190, 40 + 26 + 4, 200, true));
        assert!(!hit_sidebar_add(20, 40 + 26 + 4, 200, true));
        assert!(!hit_sidebar_add(190, 40 + 26 + 4, 200, false));
    }
    #[test]
    fn match_chip_hit_returns_index_and_rejects_gaps() {
        assert_eq!(hit_match_chip(100, 100, 86, 3), Some(0));
        assert_eq!(hit_match_chip(185, 100, 86, 3), Some(0));
        assert_eq!(hit_match_chip(186, 100, 86, 3), Some(1));
        assert_eq!(hit_match_chip(272, 100, 86, 3), Some(2));
        assert_eq!(hit_match_chip(358, 100, 86, 3), None);
        assert_eq!(hit_match_chip(99, 100, 86, 3), None);
    }
    #[test]
    fn sidebar_indexed_folder_hit() {
        assert_eq!(
            hit_sidebar(12, 40 + 2 * 26 + 2, 200, true, 3, 0, 0, 0),
            Some(SidebarHit::IndexedFolder(0))
        );
    }
    #[test]
    fn sidebar_recommended_path_hit() {
        assert_eq!(
            hit_sidebar(12, 40 + 6 * 26 + 1, 200, true, 3, 0, 0, 3),
            Some(SidebarHit::Recommended(0))
        );
    }
    #[test]
    fn sidebar_preset_hit() {
        assert_eq!(
            hit_sidebar(12, 40 + 8 * 26 + 1, 200, true, 1, 2, 0, 3),
            Some(SidebarHit::Preset(0))
        );
    }
    #[test]
    fn sidebar_history_hit() {
        assert_eq!(
            hit_sidebar(12, 40 + 9 * 26 + 1, 200, true, 0, 1, 2, 3),
            Some(SidebarHit::History(0))
        );
    }
    #[test]
    fn sidebar_clear_hit() {
        assert_eq!(
            hit_sidebar(12, 40 + 5 * 26 + 1, 200, true, 0, 0, 0, 0),
            Some(SidebarHit::ClearHistory)
        );
    }
    #[test]
    fn sidebar_closed_or_outside_is_miss() {
        assert_eq!(hit_sidebar(12, 45, 200, false, 0, 0, 0, 0), None);
        assert_eq!(hit_sidebar(201, 45, 200, true, 0, 0, 0, 0), None);
    }
    #[test]
    fn option_regions_cover_all_controls() {
        let spans = option_spans(10, 710);
        assert_eq!(spans[0], (10, 110));
        assert_eq!(spans[6], (610, 710));
    }
    #[test]
    fn option_hit_selects_period_region() {
        assert_eq!(
            hit_option(360, 35, 30, 30, 10, 710),
            Some(OptionControl::Period)
        );
    }
    #[test]
    fn option_hit_rejects_outside() {
        assert_eq!(hit_option(710, 35, 30, 30, 10, 710), None);
        assert_eq!(hit_option(50, 60, 30, 30, 10, 710), None);
    }
    #[test]
    fn summary_group_hit_splits_filename_and_body() {
        assert_eq!(hit_summary_group(20, 45, 40, 24, 10, 110), Some(true));
        assert_eq!(hit_summary_group(90, 45, 40, 24, 10, 110), Some(false));
    }
}

/// 탭 3개([본문], [위계], [파일명])의 RECT 계산.
pub fn calc_tab_rects(pad: i32, tab_y: i32, tab_h: i32, width: i32) -> [RECT; 4] {
    let avail = (width - 2 * pad).max(0);
    let tab_w = avail / 4;
    let l0 = pad;
    let r0 = l0 + tab_w;
    let l1 = r0;
    let r1 = l1 + tab_w;
    let l2 = r1;
    let r2 = l2 + tab_w;
    let l3 = r2;
    let r3 = width - pad;
    [
        RECT {
            left: l0,
            top: tab_y,
            right: r0,
            bottom: tab_y + tab_h,
        },
        RECT {
            left: l1,
            top: tab_y,
            right: r1,
            bottom: tab_y + tab_h,
        },
        RECT {
            left: l2,
            top: tab_y,
            right: r2,
            bottom: tab_y + tab_h,
        },
        RECT { left: l3, top: tab_y, right: r3, bottom: tab_y + tab_h },
    ]
}

/// 탭 히트 판정 (0: 전체, 1: 본문, 2: 위계, 3: 파일명).
pub fn hit_tab(x: i32, y: i32, pad: i32, tab_y: i32, tab_h: i32, width: i32) -> Option<usize> {
    if y < tab_y || y >= tab_y + tab_h || x < pad || x >= width - pad {
        return None;
    }
    let rects = calc_tab_rects(pad, tab_y, tab_h, width);
    rects.iter().position(|r| x >= r.left && x < r.right)
}

/// 결과 행의 RECT 계산.
pub fn calc_row_rect(top_y: i32, index: usize, scroll_top: usize, row_h: i32, width: i32) -> RECT {
    let y = top_y + (index.saturating_sub(scroll_top)) as i32 * row_h;
    RECT {
        left: 0,
        top: y,
        right: width,
        bottom: y + row_h,
    }
}

/// 행 오른쪽 [폴더 열기] 단추 영역 계산.
pub fn calc_folder_button_rect(
    row_top: i32,
    row_right: i32,
    margin_x: i32,
    margin_y: i32,
    btn_w: i32,
    btn_h: i32,
) -> RECT {
    RECT {
        left: row_right - btn_w - margin_x,
        top: row_top + margin_y,
        right: row_right - margin_x,
        bottom: row_top + margin_y + btn_h,
    }
}

/// 폴더 열기 단추 히트 판정.
pub fn hit_folder_button(x: i32, y: i32, btn_rect: RECT) -> bool {
    x >= btn_rect.left && x < btn_rect.right && y >= btn_rect.top && y < btn_rect.bottom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_row(path: &str, name: &str, parent: &str) -> crate::backend::Row {
        crate::backend::Row {
            path: path.into(),
            name: name.into(),
            parent: parent.into(),
            ..Default::default()
        }
    }
    #[test]
    fn density_heights_differ() {
        assert_eq!(density_row_height(true), 56);
        assert_eq!(density_row_height(false), 70);
    }
    #[test]
    fn click_open_single_and_double_modes() {
        assert!(click_opens(1, false, false));
        assert!(!click_opens(1, true, false));
        assert!(!click_opens(2, false, false));
        assert!(click_opens(2, true, false));
    }
    #[test]
    fn folder_button_opens_independent_of_policy() {
        assert!(click_opens(2, false, true));
    }
    #[test]
    fn meta_location_and_date_enabled() {
        assert_eq!(
            row_meta(true, true, "C:\\docs", "2026. 01. 01."),
            "C:\\docs  |  2026. 01. 01."
        );
    }
    #[test]
    fn meta_location_toggle_hides_parent() {
        assert_eq!(row_meta(false, true, "C:\\docs", "date"), "date");
    }
    #[test]
    fn meta_date_toggle_hides_date() {
        assert_eq!(row_meta(true, false, "C:\\docs", "date"), "C:\\docs");
    }
    #[test]
    fn meta_both_toggles_hide_all() {
        assert_eq!(row_meta(false, false, "C:\\docs", "date"), "");
    }
    #[test]
    fn include_subfolders_off_keeps_only_selected_direct_children() {
        let rows = vec![
            settings_row("C:\\root\\a.txt", "a.txt", "C:\\root"),
            settings_row("C:\\root\\sub\\b.txt", "b.txt", "C:\\root\\sub"),
        ];
        let s = crate::settings::Settings {
            include_subfolders: false,
            ..Default::default()
        };
        assert_eq!(filter_row_indices(&rows, &s, Some("c:/root")), vec![0]);
    }
    #[test]
    fn include_subfolders_off_without_selection_keeps_indexed_roots_only() {
        let rows = vec![
            settings_row("C:\\root\\a.txt", "a.txt", "C:\\root"),
            settings_row("C:\\root\\sub\\b.txt", "b.txt", "C:\\root\\sub"),
            settings_row("D:\\other\\c.txt", "c.txt", "D:\\other"),
        ];
        let s = crate::settings::Settings {
            include_subfolders: false,
            ..Default::default()
        };
        let roots = vec!["C:\\root".into(), "D:\\other".into()];
        assert_eq!(
            filter_row_indices_with_roots(&rows, &s, None, &roots),
            vec![0, 2]
        );
    }

    #[test]
    fn removing_excluded_directory_is_case_insensitive_and_reports_change() {
        let mut excluded = vec!["C:\\Temp".to_string(), "D:\\Cache".to_string()];
        assert!(remove_excluded_directory(&mut excluded, "c:\\temp"));
        assert_eq!(excluded, vec!["D:\\Cache"]);
        assert!(!remove_excluded_directory(&mut excluded, "C:\\missing"));
    }

    #[test]
    fn ui_scale_recalculates_window_size_proportionally() {
        assert_eq!(scaled_window_size(800, 600, 100, 125), (1000, 750));
        assert_eq!(scaled_window_size(1000, 750, 125, 90), (720, 540));
    }
    #[test]
    fn excluded_directories_filter_descendants_case_insensitively() {
        let rows = vec![
            settings_row("C:\\root\\a.txt", "a.txt", "C:\\root"),
            settings_row("C:\\root\\secret\\b.txt", "b.txt", "C:\\root\\secret"),
        ];
        let s = crate::settings::Settings {
            excluded_directories: vec!["c:/ROOT/secret".into()],
            ..Default::default()
        };
        assert_eq!(filter_row_indices(&rows, &s, None), vec![0]);
    }
    #[test]
    fn result_limit_is_applied() {
        let rows = (0..6)
            .map(|i| settings_row(&format!("C:\\{i}.txt"), "x.txt", "C:\\"))
            .collect::<Vec<_>>();
        let s = crate::settings::Settings {
            max_results: 50,
            ..Default::default()
        };
        assert_eq!(filter_row_indices(&rows, &s, None).len(), 6);
    }
    #[test]
    fn version_grouping_keeps_one_row_per_version_family() {
        let rows = vec![
            settings_row("C:\\A_v1.txt", "A_v1.txt", "C:\\"),
            settings_row("C:\\A_v2.txt", "A_v2.txt", "C:\\"),
        ];
        let s = crate::settings::Settings {
            version_grouping: true,
            ..Default::default()
        };
        assert_eq!(filter_row_indices(&rows, &s, None).len(), 1);
    }
    #[test]
    fn result_limit_clamps_large_config_to_200() {
        let s = crate::settings::Settings {
            max_results: 999,
            ..Default::default()
        };
        let rows = (0..240)
            .map(|i| settings_row(&format!("C:\\{i}.txt"), "x.txt", "C:\\"))
            .collect::<Vec<_>>();
        assert_eq!(filter_row_indices(&rows, &s, None).len(), 200);
    }
    #[test]
    fn settings_visible_indices_apply_types_and_settings_limit() {
        let rows = vec![
            settings_row("C:\\a.pdf", "a.pdf", "C:\\"),
            settings_row("C:\\b.txt", "b.txt", "C:\\"),
        ];
        let s = crate::settings::Settings {
            max_results: 50,
            ..Default::default()
        };
        assert_eq!(
            settings_visible_indices(&rows, &s, None, "", false, &["pdf".into()], false, false),
            vec![0]
        );
    }
    #[test]
    fn excluded_directory_does_not_match_neighbor_prefix() {
        let rows = vec![settings_row(
            "C:\\secretary\\a.txt",
            "a.txt",
            "C:\\secretary",
        )];
        let s = crate::settings::Settings {
            excluded_directories: vec!["C:\\secret".into()],
            ..Default::default()
        };
        assert_eq!(filter_row_indices(&rows, &s, None), vec![0]);
    }
    #[test]
    fn direct_child_filter_without_root_keeps_existing_behavior() {
        let rows = vec![settings_row(
            "C:\\root\\sub\\b.txt",
            "b.txt",
            "C:\\root\\sub",
        )];
        let s = crate::settings::Settings {
            include_subfolders: false,
            ..Default::default()
        };
        assert_eq!(filter_row_indices(&rows, &s, None), vec![0]);
    }
    #[test]
    fn row_height_baseline_is_70_points() {
        assert_eq!(density_row_height(false), ROW_H);
    }
    #[test]
    fn click_policy_rejects_non_single_click_values_as_double_click() {
        assert!(!click_opens(9, false, false));
        assert!(click_opens(9, true, false));
    }

    #[test]
    fn modal_panel_stays_inside_normal_window() {
        let r = modal_rects(800, 600, 2);
        assert!(
            r.panel.left >= 0 && r.panel.right <= 800 && r.panel.top >= 0 && r.panel.bottom <= 600
        );
    }
    #[test]
    fn modal_panel_clamps_small_window_margins() {
        let r = modal_rects(180, 120, 2);
        assert!(r.panel.left < r.panel.right && r.panel.top < r.panel.bottom);
    }
    #[test]
    fn modal_tabs_fill_two_tab_width() {
        let r = modal_rects(800, 600, 2);
        assert!(r.tabs[0].left < r.tabs[1].left);
        assert_eq!(r.tabs[0].right, r.tabs[1].left);
    }
    #[test]
    fn modal_tabs_fill_four_tab_width() {
        let r = modal_rects(900, 700, 4);
        assert!(r.tabs[0].right <= r.tabs[1].left + 1);
        assert!(r.tabs[3].right == r.panel.right - 12);
    }
    #[test]
    fn modal_close_hit_detected() {
        let r = modal_rects(800, 600, 2);
        assert_eq!(
            hit_modal(r.close.left, r.close.top, 800, 600, 2),
            ModalHit::Close
        );
    }
    #[test]
    fn modal_outside_hit_detected() {
        assert_eq!(hit_modal(0, 0, 800, 600, 2), ModalHit::Outside);
    }
    #[test]
    fn modal_content_hit_detected() {
        let r = modal_rects(800, 600, 2);
        assert_eq!(
            hit_modal(r.content.left, r.content.top, 800, 600, 2),
            ModalHit::Content
        );
    }
    #[test]
    fn modal_first_tab_hit_detected() {
        let r = modal_rects(800, 600, 2);
        assert_eq!(
            hit_modal(r.tabs[0].left, r.tabs[0].top, 800, 600, 2),
            ModalHit::Tab(0)
        );
    }
    #[test]
    fn modal_second_tab_hit_detected() {
        let r = modal_rects(800, 600, 2);
        assert_eq!(
            hit_modal(r.tabs[1].left, r.tabs[1].top, 800, 600, 2),
            ModalHit::Tab(1)
        );
    }
    #[test]
    fn modal_fourth_tab_hit_detected() {
        let r = modal_rects(900, 700, 4);
        assert_eq!(
            hit_modal(r.tabs[3].left, r.tabs[3].top, 900, 700, 4),
            ModalHit::Tab(3)
        );
    }
    #[test]
    fn header_stats_chip_hit_detected() {
        let (l, r) = header_chip_rects(700, 46, 1)[0];
        assert_eq!(hit_header_chip(l, 10, 700, 46, 32, 1), Some(0));
        assert!(r > l);
    }
    #[test]
    fn header_settings_chip_hit_detected() {
        let (l, _) = header_chip_rects(700, 46, 1)[1];
        assert_eq!(hit_header_chip(l, 10, 700, 46, 32, 1), Some(1));
    }
    #[test]
    fn header_chip_misses_caption_buttons() {
        assert_eq!(hit_header_chip(690, 10, 700, 46, 32, 1), None);
    }
    #[test]
    fn header_chip_misses_below_header() {
        assert_eq!(hit_header_chip(600, 33, 700, 46, 32, 1), None);
    }

    #[test]
    fn options_default_to_keyword_relevance() {
        let o = OptionBar::default();
        assert_eq!(o.search_mode, SearchMode::Keyword);
        assert_eq!(o.sort, SortMode::Relevance);
    }
    #[test]
    fn sort_modes_are_distinct() {
        assert_ne!(SortMode::Newest, SortMode::Oldest);
        assert_ne!(SortMode::Name, SortMode::Relevance);
    }
    #[test]
    fn sidebar_width_clamps_to_half_window() {
        assert_eq!(sidebar_width(true, 300), 150);
    }
    #[test]
    fn sidebar_width_is_zero_when_closed() {
        assert_eq!(sidebar_width(false, 800), 0);
    }
    #[test]
    fn sidebar_width_handles_negative_window() {
        assert_eq!(sidebar_width(true, -1), 0);
    }
    #[test]
    fn result_top_sums_header_regions() {
        assert_eq!(calc_results_top(32, 48, 28, 30, 32), 170);
    }
    #[test]
    fn empty_within_query_keeps_every_result() {
        let a = vec!["a".into(), "b".into()];
        assert_eq!(filter_result_indices(&a, &a, "", false), vec![0, 1]);
    }
    #[test]
    fn within_filter_matches_filename_case_insensitive() {
        let n = vec!["Report.PDF".into()];
        let s = vec![String::new()];
        assert_eq!(filter_result_indices(&n, &s, "report", false), vec![0]);
    }
    #[test]
    fn within_filter_matches_body() {
        let n = vec!["x".into()];
        let s = vec!["Needle here".into()];
        assert_eq!(filter_result_indices(&n, &s, "needle", false), vec![0]);
    }
    #[test]
    fn exclude_filename_removes_filename_only_match() {
        let n = vec!["needle".into()];
        let s = vec![String::new()];
        assert!(filter_result_indices(&n, &s, "needle", true).is_empty());
    }
    #[test]
    fn exclude_filename_keeps_body_match() {
        let n = vec!["needle".into()];
        let s = vec!["needle in body".into()];
        assert_eq!(filter_result_indices(&n, &s, "needle", true), vec![0]);
    }
    #[test]
    fn within_filter_trims_query() {
        let n = vec!["abc".into()];
        let s = vec![String::new()];
        assert_eq!(filter_result_indices(&n, &s, " abc ", false), vec![0]);
    }
    #[test]
    fn within_filter_shorter_arrays_are_safe() {
        let n = vec!["a".into(), "a".into()];
        let s = vec!["a".into()];
        assert_eq!(filter_result_indices(&n, &s, "a", false), vec![0]);
    }
    #[test]
    fn match_groups_count_nonempty_separately() {
        let n = vec!["a".into(), "b".into()];
        let s = vec!["body".into(), String::new()];
        assert_eq!(
            match_groups(&n, &s),
            MatchGroups {
                filename: 2,
                body: 1
            }
        );
    }
    #[test]
    fn match_groups_empty_input() {
        assert_eq!(match_groups(&[], &[]), MatchGroups::default());
    }
    #[test]
    fn match_groups_bounds_to_shorter_input() {
        let n = vec!["a".into(), "b".into()];
        let s = vec!["x".into()];
        assert_eq!(match_groups(&n, &s).filename, 1);
    }
    #[test]
    fn clipboard_payload_contains_name_and_full_path() {
        let n = vec!["문서.hwp".into()];
        let p = vec!["C:\\자료\\문서.hwp".into()];
        assert_eq!(copy_result_text(&n, &p), "문서.hwp\tC:\\자료\\문서.hwp");
    }
    #[test]
    fn clipboard_payload_uses_crlf_for_multiple_rows() {
        let n = vec!["a".into(), "b".into()];
        let p = vec!["A".into(), "B".into()];
        assert_eq!(copy_result_text(&n, &p), "a\tA\r\nb\tB");
    }
    #[test]
    fn clipboard_payload_ignores_unpaired_values() {
        assert_eq!(copy_result_text(&["a".into()], &[]), "");
    }
    #[test]
    fn option_hit_selects_every_option_lane() {
        for i in 0..7 {
            assert_eq!(hit_option(5 + i * 100, 10, 0, 20, 0, 700).is_some(), true);
        }
    }
    #[test]
    fn sidebar_hit_rejects_header_rows() {
        assert_eq!(hit_sidebar(10, 40 + 26, 220, true, 2, 1, 1, 2), None);
    }
    fn row(name: &str, snippet: &str) -> crate::backend::Row {
        crate::backend::Row {
            match_kinds: Vec::new(),
            doc_id: 0,
            path: format!("C:\\{name}"),
            name: name.into(),
            parent: "C:\\".into(),
            snippet: snippet.into(),
            location: String::new(),
            level: 0,
            count: 0,
            para: None,
            snippet_tried: false,
            preview: String::new(),
            date: 0,
            rank: 0,
        }
    }
    #[test]
    fn result_filter_matches_body_case_insensitively() {
        let rows = vec![row("a.txt", "Needle")];
        assert_eq!(
            filter_rows_within_query(&rows, "needle", false, &[]),
            vec![0]
        );
    }
    #[test]
    fn result_filter_can_exclude_filename() {
        let rows = vec![row("needle.txt", "")];
        assert!(filter_rows_within_query(&rows, "needle", true, &[]).is_empty());
    }
    #[test]
    fn exclude_filename_hides_filename_only_results_without_within_query() {
        let rows = vec![row("match.txt", ""), row("other.txt", "body")];
        assert_eq!(filter_rows_within_query(&rows, "", true, &[]), vec![1]);
    }
    #[test]
    fn result_filter_matches_selected_file_types() {
        let rows = vec![row("a.hwp", "x"), row("b.pdf", "x")];
        assert_eq!(
            filter_rows_within_query(&rows, "", false, &["pdf".into()]),
            vec![1]
        );
    }
    #[test]
    fn result_filter_empty_query_keeps_all_types() {
        let rows = vec![row("a.hwp", ""), row("b.pdf", "")];
        assert_eq!(filter_rows_within_query(&rows, "", false, &[]), vec![0, 1]);
    }
    #[test]
    fn displayed_result_indices_respect_collapsed_groups() {
        let rows = vec![row("name-only.txt", ""), row("body.pdf", "match")];
        assert_eq!(
            display_result_indices(&rows, "", false, &[], true, false),
            vec![1]
        );
        assert_eq!(
            display_result_indices(&rows, "", false, &[], false, true),
            vec![0]
        );
    }

    #[test]
    fn test_head_button_hit_layout() {
        let width = 700;
        let button_w = 46;
        let head = 32;

        // Minimize: [608, 654), Close: [654, 700)
        assert_eq!(
            head_button_hit(654, 10, width, button_w, head),
            Some(HeadButton::Close)
        );
        assert_eq!(
            head_button_hit(699, 10, width, button_w, head),
            Some(HeadButton::Close)
        );
        assert_eq!(
            head_button_hit(608, 10, width, button_w, head),
            Some(HeadButton::Minimize)
        );
        assert_eq!(
            head_button_hit(653, 10, width, button_w, head),
            Some(HeadButton::Minimize)
        );

        // 단추 밖
        assert_eq!(head_button_hit(607, 10, width, button_w, head), None);
        assert_eq!(head_button_hit(100, 10, width, button_w, head), None);
        assert_eq!(head_button_hit(660, 32, width, button_w, head), None);
        assert_eq!(head_button_hit(660, -1, width, button_w, head), None);
    }

    #[test]
    fn test_head_button_glyphs() {
        assert_eq!(head_button_glyph(HeadButton::Minimize), '\u{E921}');
        assert_eq!(head_button_glyph(HeadButton::Close), '\u{E8BB}');
    }

    #[test]
    fn test_extension_tag_detection() {
        assert_eq!(extension_tag("보고서.hwp"), "HWP");
        assert_eq!(extension_tag("공문.hwpx"), "HWP");
        assert_eq!(extension_tag("결과.pdf"), "PDF");
        assert_eq!(extension_tag("계약서.docx"), "DOC");
        assert_eq!(extension_tag("참고.txt"), "TXT");
        assert_eq!(extension_tag("통계.xlsx"), "XLS");
        assert_eq!(extension_tag("데이터.csv"), "XLS");
        assert_eq!(extension_tag("발표.pptx"), "PPT");
        assert_eq!(extension_tag("기타.zip"), "문서");
        assert_eq!(extension_tag("확장자없음"), "문서");
    }

    #[test]
    fn test_calc_list_top_with_and_without_autocomplete() {
        let top_without_ac = calc_list_top(32, 44, 28, 32, false, 28);
        assert_eq!(top_without_ac, 136);

        let top_with_ac = calc_list_top(32, 44, 28, 32, true, 28);
        assert_eq!(top_with_ac, 164);
    }

    #[test]
    fn test_tab_rects_and_hits() {
        let pad = 12;
        let tab_y = 70;
        let tab_h = 28;
        let width = 700;

        let rects = calc_tab_rects(pad, tab_y, tab_h, width);
        assert_eq!(rects[0].left, 12);
        assert_eq!(rects[3].right, 688);
        assert!(rects[0].right == rects[1].left);
        assert!(rects[1].right == rects[2].left);

        // 첫 번째 탭 클릭
        assert_eq!(hit_tab(15, 80, pad, tab_y, tab_h, width), Some(0));
        // 두 번째 탭 클릭
        assert_eq!(hit_tab(300, 80, pad, tab_y, tab_h, width), Some(1));
        // 세 번째와 네 번째 탭 클릭
        assert_eq!(hit_tab(500, 80, pad, tab_y, tab_h, width), Some(2));
        assert_eq!(hit_tab(680, 80, pad, tab_y, tab_h, width), Some(3));
        // 탭 밖 클릭
        assert_eq!(hit_tab(5, 80, pad, tab_y, tab_h, width), None);
        assert_eq!(hit_tab(695, 80, pad, tab_y, tab_h, width), None);
        assert_eq!(hit_tab(300, 60, pad, tab_y, tab_h, width), None);
        assert_eq!(hit_tab(300, 100, pad, tab_y, tab_h, width), None);
    }

    #[test]
    fn test_row_rect_and_folder_button() {
        let top_y = 140;
        let row_h = 70;
        let width = 700;

        let r0 = calc_row_rect(top_y, 0, 0, row_h, width);
        assert_eq!(r0.top, 140);
        assert_eq!(r0.bottom, 210);
        assert_eq!(r0.left, 0);
        assert_eq!(r0.right, 700);

        let r1 = calc_row_rect(top_y, 1, 0, row_h, width);
        assert_eq!(r1.top, 210);
        assert_eq!(r1.bottom, 280);

        // 스크롤 시
        let r1_scrolled = calc_row_rect(top_y, 1, 1, row_h, width);
        assert_eq!(r1_scrolled.top, 140);

        // 폴더 열기 단추
        let btn_rc = calc_folder_button_rect(r0.top, r0.right, 6, 4, 28, 22);
        assert_eq!(btn_rc.left, 700 - 28 - 6);
        assert_eq!(btn_rc.right, 700 - 6);
        assert_eq!(btn_rc.top, 144);
        assert_eq!(btn_rc.bottom, 166);

        assert!(hit_folder_button(670, 150, btn_rc));
        assert!(!hit_folder_button(660, 150, btn_rc));
        assert!(!hit_folder_button(670, 140, btn_rc));
    }

    #[test]
    fn test_frame_hit_integration() {
        use crate::frame::Hit;
        // 크기 조절 가장자리(6px)와 상단 캡션(32px) 판정
        let (w, h, grip, head) = (700, 510, 6, 32);
        let hit = |x: i32, y: i32, on_btn: bool| {
            crate::frame::frame_hit(x, y, w, h, grip, head, on_btn, false, false)
        };
        // 상단 캡션 단추 위면 HTCLIENT
        assert_eq!(hit(680, 10, true), Hit::Client);
        // 상단 제목 영역은 HTCAPTION
        assert_eq!(hit(100, 10, false), Hit::Caption);
        // 좌측/우측/하단 모서리 크기 조절
        assert_eq!(hit(2, 200, false), Hit::Left);
        assert_eq!(hit(698, 200, false), Hit::Right);
        assert_eq!(hit(350, 508, false), Hit::Bottom);
        assert_eq!(hit(698, 508, false), Hit::BottomRight);
    }

    #[test]
    fn test_preview_layout_and_visible_count() {
        let bottom = 600;
        let top_y = 140;
        let status_h = 24;
        let rh = 70;

        // 선택 없을 때: preview_h = 0
        let visible_without_sel = ((bottom - top_y - status_h).max(0) / rh).max(0) as usize;
        assert_eq!(visible_without_sel, (600 - 140 - 24) / 70); // 436 / 70 = 6

        // 선택 있을 때: preview_h = PREVIEW_H (64)
        let preview_h = PREVIEW_H;
        let visible_with_sel =
            ((bottom - top_y - status_h - preview_h).max(0) / rh).max(0) as usize;
        assert_eq!(visible_with_sel, (600 - 140 - 24 - 64) / 70); // 372 / 70 = 5

        // 미리보기 영역 상단 및 하단 좌표
        let prev_y = bottom - status_h - preview_h;
        assert_eq!(prev_y, 600 - 24 - 64); // 512
    }
}
