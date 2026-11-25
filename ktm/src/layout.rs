//! 퀵몽땅검색 창 순수 레이아웃 계산 및 테스트.
//! 모든 OS에서 단위 시험(cargo test)으로 검증한다.

use windows_sys::Win32::Foundation::RECT;

// ---- 기본 치수 (96 DPI 기준 논리 픽셀) ----
pub const HEAD_H: i32 = 32;
pub const CAPTION_W: i32 = 46;
pub const SEARCH_BOX_H: i32 = 32;
pub const TAB_H: i32 = 28;
pub const FILTER_H: i32 = 24;
/// Two-line result card: filename/meta on line one, matching excerpt on line two.
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

/// 오른쪽 위 [통계]·[설정] 칩의 가로 범위. 캡션 단추 폭(button_width)에 비례해 배율을 따른다
/// (예전엔 정수 배율 1을 곱해 125%·150%에서 그려진 칩과 눌리는 칸이 어긋났다).
pub fn header_chip_rects(width: i32, button_width: i32) -> [(i32, i32); 2] {
    let s = |v: i32| v * button_width.max(1) / CAPTION_W;
    let left = (width - 2 * button_width - s(166)).max(s(150));
    let right = (width - 2 * button_width - s(86)).max(s(230));
    [(left, right), (right + s(4), right + s(82))]
}

/// 창 위쪽 띠에서 클라이언트(클릭 가능) 영역이어야 하는 단추인지: 캡션 단추 또는 [통계]·[설정] 칩.
/// WM_NCHITTEST가 이 영역을 HTCAPTION(창 끌기)으로 돌려주면 클릭이 클라이언트에 오지 않아
/// 칩이 작동하지 않으므로, 판정을 순수 함수 하나로 모은다.
pub fn header_is_client_button(x: i32, y: i32, width: i32, button_width: i32, head: i32) -> bool {
    head_button_hit(x, y, width, button_width, head).is_some()
        || hit_header_chip(x, y, width, button_width, head).is_some()
}

pub fn hit_header_chip(
    x: i32,
    y: i32,
    width: i32,
    button_width: i32,
    head: i32,
) -> Option<usize> {
    if y < 0 || y >= head {
        return None;
    }
    header_chip_rects(width, button_width)
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

/// 옵션 바에서 편집 가능한 결과 내 영역에 나머지 항목의 두 배 폭을 배정한다.
pub fn option_spans(left: i32, right: i32) -> [(i32, i32); 7] {
    let avail = (right - left).max(0);
    // 균등 분할은 좁은 창에서 값이 긴 '결과 내' 영역을 말줄임표 하나로 잘랐다.
    const WEIGHTS: [i32; 7] = [1, 1, 1, 1, 1, 2, 1];
    let total: i32 = WEIGHTS.iter().sum();
    let mut used = 0;
    std::array::from_fn(|i| {
        let start = left + avail * used / total;
        used += WEIGHTS[i];
        (start, left + avail * used / total)
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
                || row.snippet.to_lowercase().contains(&q)
                || (exclude_filename && !row.snippet_tried);
            // Excluding filename matches should suppress filename-only rows, not
            // rows whose body snippet has not been loaded yet. The latter are
            // fetched asynchronously when rows become visible.
            let body_only_ok = !exclude_filename
                || !row.snippet.is_empty()
                || ( !q.is_empty() && !row.snippet_tried );
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
    sidebar_width_px(open, width, SIDEBAR_W)
}

/// DPI 배율을 적용한 사이드바 화소 폭(`full`=`scale(SIDEBAR_W)`)을 쓰는 같은 계산. 창 폭의 절반을 넘지 않는다.
pub fn sidebar_width_px(open: bool, width: i32, full: i32) -> i32 {
    if open {
        full.min(width.max(0) / 2)
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
#[derive(Clone, Copy)]
pub struct QuickSearchLayout {
    pub related: RECT,
    pub tabs: RECT,
    pub options: RECT,
    pub filters: RECT,
    pub summary: RECT,
    pub autocomplete: RECT,
    pub list_top: i32,
}

pub fn hit_quick_band(x: i32, y: i32, band: RECT) -> bool {
    x >= band.left && x < band.right && y >= band.top && y < band.bottom
}

fn quick_layout_at(option_top: i32, width: i32, height: i32, scale: i32, has_related: bool, show_options: bool, has_autocomplete: bool, option_height: i32, filter_height: i32, summary_height: i32, autocomplete_height: i32) -> QuickSearchLayout {
    let s = |v: i32| v * scale.max(1) / 100;
    let clip = |top: i32, bottom: i32| RECT {
        left: 0,
        top: top.clamp(0, height.max(0)),
        right: width.max(0),
        bottom: bottom.clamp(top.clamp(0, height.max(0)), height.max(0)),
    };
    let related_h = if has_related { s(28) } else { 0 };
    let search_bottom = s(HEAD_H + SEARCH_BOX_H + 12);
    let related_top = search_bottom;
    // 탭 줄은 없어졌다(통합 검색 하나). 높이 0의 빈 띠로만 남겨 옛 필드 참조를 깨지 않는다.
    let tab_top = related_top + related_h;
    // show_options는 '옵션 팝업(두 번째 줄)이 열렸는가'다. 닫히면 첫 줄 높이만 차지한다.
    let option_h = if show_options { option_height } else { option_height / 2 };
    let filter_top = option_top + option_h;
    let summary_top = filter_top + filter_height;
    let autocomplete_top = summary_top + summary_height + if has_autocomplete { s(4) } else { 0 };
    let autocomplete_h = if has_autocomplete { autocomplete_height } else { 0 };
    let list_top = autocomplete_top + autocomplete_h;
    QuickSearchLayout {
        related: clip(related_top, related_top + related_h),
        tabs: clip(tab_top, tab_top),
        options: clip(option_top, option_top + option_h),
        filters: clip(filter_top, filter_top + filter_height),
        summary: clip(summary_top, summary_top + summary_height),
        autocomplete: clip(autocomplete_top, autocomplete_top + autocomplete_h),
        list_top: list_top.clamp(0, height.max(0)),
    }
}

/// Shared quick-search geometry for painting, hit testing and result-list placement.
pub fn quick_search_layout(scale: i32, width: i32, height: i32, has_related: bool, show_options: bool, has_autocomplete: bool) -> QuickSearchLayout {
    let s = |v: i32| v * scale.max(1) / 100;
    let related_h = if has_related { s(28) } else { 0 };
    let option_top = s(HEAD_H + SEARCH_BOX_H + 12) + related_h;
    quick_layout_at(option_top, width, height, scale, has_related, show_options, has_autocomplete, s(OPTION_H), s(FILTER_H), s(SUMMARY_H), s(22))
}

/// Compatibility wrappers delegate their band positions to the shared layout calculation.
pub fn filter_band_top(option_top: i32, option_h: i32) -> i32 {
    quick_layout_at(option_top, i32::MAX, i32::MAX, 100, false, true, false, option_h, 0, 0, 0).filters.top
}

pub fn summary_band_top(option_top: i32, option_h: i32, filter_h: i32) -> i32 {
    quick_layout_at(option_top, i32::MAX, i32::MAX, 100, false, true, false, option_h, filter_h, 0, 0).summary.top
}

pub fn calc_content_top(head: i32, search: i32, tabs: i32, options: i32, filters: i32, summary: i32, has_ac: bool, ac: i32) -> i32 {
    let top = head + search + tabs;
    let b = quick_layout_at(top, i32::MAX, i32::MAX, 100, false, options > 0, false, options, filters, summary, 0);
    b.summary.bottom + if has_ac { ac } else { 0 }
}

#[cfg(test)]
mod content_top_tests {
    use super::*;

    #[test]
    fn option_filter_summary_bands_do_not_overlap_and_end_at_content_top() {
        let (oy, oh, fh, sh) = (100, OPTION_H, FILTER_H, SUMMARY_H);
        assert_eq!(filter_band_top(oy, oh), oy + oh);
        assert_eq!(summary_band_top(oy, oh, fh), oy + oh + fh);
        assert!(summary_band_top(oy, oh, fh) >= filter_band_top(oy, oh) + fh);
        assert_eq!(summary_band_top(oy, oh, fh) + sh, oy + calc_content_top(0, 0, 0, oh, fh, sh, false, 0));
    }
    #[test]
    fn quick_search_bands_are_sequential_at_supported_scales_and_clip_short_windows() {
        for scale in [100, 125, 150, 200] {
            for height in [150, 190, 260, 900] {
                for has_related in [false, true] {
                    for show_options in [false, true] {
                        for has_autocomplete in [false, true] {
                            let b = quick_search_layout(scale, 640, height, has_related, show_options, has_autocomplete);
                            let rects = [b.related, b.tabs, b.options, b.filters, b.summary, b.autocomplete];
                            for rect in rects {
                                assert!(0 <= rect.top && rect.top <= rect.bottom && rect.bottom <= height,
                                    "scale={scale} height={height}: {}..{}", rect.top, rect.bottom);
                            }
                            for pair in rects.windows(2) {
                                assert!(pair[0].bottom <= pair[1].top,
                                    "scale={scale} height={height}: {} > {}", pair[0].bottom, pair[1].top);
                            }
                            assert!(b.list_top <= height);
                            let visible = rects.iter().filter(|r| r.bottom > r.top).collect::<Vec<_>>();
                            for (i, row) in visible.iter().enumerate() {
                                let y = row.top + (row.bottom - row.top - 1) / 2;
                                assert!(hit_quick_band(10, y, **row));
                                for (j, other) in visible.iter().enumerate() {
                                    if i != j { assert!(!hit_quick_band(10, y, **other)); }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn filter_and_summary_hit_bands_are_disjoint_and_scale_correctly() {
        for scale in [100, 125, 150, 200] {
            let s = |v: i32| v * scale / 100;
            let b = quick_search_layout(scale, s(900), s(900), false, true, false);
            let filter_click_y = b.filters.top + (b.filters.bottom - b.filters.top) / 2;
            let summary_click_y = b.summary.top + (b.summary.bottom - b.summary.top) / 2;
            assert!(hit_quick_band(10, filter_click_y, b.filters));
            assert!(!hit_quick_band(10, filter_click_y, b.summary));
            assert!(hit_quick_band(10, summary_click_y, b.summary));
        }
    }
    #[test]
    fn content_bands_are_sequential() {
        assert_eq!(calc_content_top(32, 44, 28, 30, 24, 32, false, 22), 190);
        assert_eq!(calc_content_top(32, 44, 28, 30, 24, 32, true, 22), 212);
    }
}

#[cfg(test)]
mod quick_search_card_tests {
    use super::*;

    #[test]
    fn two_line_result_card_fits_painter_lines_at_common_scales() {
        assert_eq!(two_line_row_height(100), 46);
        assert_eq!(two_line_row_height(125), 57);
        assert_eq!(two_line_row_height(0), 1);
    }

    #[test]
    fn two_line_card_density_heights_scale_without_changing_legacy_row_contract() {
        for (scale, comfortable, compact) in [
            (90, 41, 34),
            (100, 46, 38),
            (110, 50, 41),
            (125, 57, 47),
        ] {
            assert_eq!(result_card_height(scale, false), comfortable);
            assert_eq!(result_card_height(scale, true), compact);
        }
        assert_eq!(density_row_height(false), 70);
        assert_eq!(density_row_height(true), 56);
    }

    #[test]
    fn first_line_split_keeps_name_and_path_apart() {
        // 공간이 넉넉하면 파일명 바로 뒤(8px 띄움)에서 경로가 시작한다.
        assert_eq!(result_first_line_split(68, 400, 120, 70, true), (188, Some(196)));
        // 파일명이 길면 경로 최소 폭을 남기고 파일명을 줄인다.
        assert_eq!(result_first_line_split(68, 400, 900, 70, true), (322, Some(330)));
        // 경로가 필요 없거나 칸이 좁으면 파일명이 끝까지 쓴다.
        assert_eq!(result_first_line_split(68, 400, 120, 70, false), (400, None));
        assert_eq!(result_first_line_split(68, 190, 120, 70, true), (190, None));
    }

    #[test]
    fn related_chip_layout_wraps_and_is_absent_for_single_term() {
        assert!(related_chip_rects(&["2026".into()], 10, 20, 300, 100).is_empty());
        let labels = vec!["2026".into(), "공사 계약".into(), "안전관리계획".into()];
        let rects = related_chip_rects(&labels, 10, 20, 130, 100);
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].top, 20);
        assert_eq!(rects[1].top, 48);
        assert_eq!(rects[2].top, 76);
        assert_eq!(related_chip_band_height(1, 100), 0);
        assert_eq!(related_chip_band_height(2, 125), 35);
    }

    #[test]
    fn two_line_card_rects_share_height_for_placement_scroll_and_hit_test() {
        let row_h = result_card_height(125, false);
        let first = calc_row_rect(200, 0, 0, row_h, 640);
        let second = calc_row_rect(200, 1, 0, row_h, 640);
        assert_eq!(first.bottom - first.top, 57);
        assert_eq!(second.top, first.bottom);
        assert_eq!(calc_row_rect(200, 1, 1, row_h, 640).top, 200);
        assert_eq!(crate::frame::row_index(256, 200, row_h, 0, 5), Some(0));
        assert_eq!(crate::frame::row_index(257, 200, row_h, 0, 5), Some(1));
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
        assert_eq!(spans[0].0, 10);
        assert_eq!(spans[6].1, 710);
        assert!(spans.windows(2).all(|pair| pair[0].1 == pair[1].0));
    }

    #[test]
    fn within_query_option_gets_double_width_without_gaps() {
        for (left, right) in [(12, 220), (272, 496), (12, 900)] {
            let spans = option_spans(left, right);
            assert_eq!(spans[0].0, left);
            assert_eq!(spans[6].1, right);
            assert!(spans.windows(2).all(|pair| pair[0].1 == pair[1].0));
            assert!(spans[5].1 - spans[5].0 >= (right - left) / 4);
        }
    }
    #[test]
    fn option_hit_selects_period_region() {
        let spans = option_spans(10, 710);
        let period_x = spans[3].0 + (spans[3].1 - spans[3].0) / 2;
        assert_eq!(
            hit_option(period_x, 35, 30, 30, 10, 710),
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

/// Height needed by the two-line result card painter, including a small vertical inset.
pub fn two_line_row_height(scale: i32) -> i32 {
    let s = scale.max(1);
    (46 * s).div_euclid(100).max(1)
}

/// Two-line card height for the selected density, in scaled pixels.
pub fn result_card_height(scale: i32, compact: bool) -> i32 {
    let logical = if compact { 38 } else { 46 };
    (logical * scale.max(1)).div_euclid(100).max(1)
}

/// 결과 카드 첫 줄의 가로 배분: 파일명 오른쪽 끝과(공간이 되면) 흐린 폴더 경로의 시작 x.
/// 파일명이 한 줄을 다 쓰더라도 경로가 `min_path` 폭은 갖도록 파일명을 줄인다.
/// 경로를 그릴 이유가 없거나 공간이 모자라면 경로는 None(파일명이 끝까지 쓴다).
pub fn result_first_line_split(
    name_left: i32,
    limit: i32,
    name_width: i32,
    min_path: i32,
    want_path: bool,
) -> (i32, Option<i32>) {
    let avail = (limit - name_left).max(0);
    if !want_path || avail < min_path * 2 {
        return (limit, None);
    }
    let name_right = (name_left + name_width).min(limit - min_path - 8).max(name_left);
    (name_right, Some(name_right + 8))
}

/// Search-input group-chip band height. No band is reserved for a single query.
pub fn related_chip_band_height(group_count: usize, scale: i32) -> i32 {
    if group_count > 1 {
        (28 * scale.max(1)).div_euclid(100).max(1)
    } else {
        0
    }
}

/// Pure placement for related-query chips below the search edit.
pub fn related_chip_rects(
    groups: &[String],
    left: i32,
    top: i32,
    right: i32,
    scale: i32,
) -> Vec<RECT> {
    if groups.len() < 2 { return Vec::new(); }
    let s = scale.max(1);
    let height = related_chip_band_height(groups.len(), s);
    let mut x = left;
    let mut y = top;
    let mut rects = Vec::with_capacity(groups.len());
    for group in groups {
        let width = ((group.chars().count() as i32 * 8 + 24) * s / 100)
            .clamp(54 * s / 100, 220 * s / 100);
        if x > left && x + width > right {
            x = left;
            y += height;
        }
        rects.push(RECT {
            left: x,
            top: y,
            right: (x + width).min(right),
            bottom: y + height - 2 * s / 100,
        });
        x += width + related_chip_gap(s);
    }
    rects
}

/// 연관 검색어 칩 사이 가로 간격(논리 6px에 DPI 배율 `scale`(100=96dpi)을 곱한다).
pub fn related_chip_gap(scale: i32) -> i32 {
    (6 * scale.max(1) / 100).max(2)
}

/// 후보 라벨을 긴 것부터 보고 `avail` 화소 안에 들어가는 첫 후보를 돌려준다.
/// 모두 넘치면 마지막(가장 짧은) 후보를 쓴다. `measure`는 라벨의 화소 폭.
pub fn pick_fitting_label(candidates: &[String], avail: i32, measure: impl Fn(&str) -> i32) -> String {
    for c in candidates {
        if measure(c) <= avail {
            return c.clone();
        }
    }
    candidates.last().cloned().unwrap_or_default()
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
        let (l, r) = header_chip_rects(700, 46)[0];
        assert_eq!(hit_header_chip(l, 10, 700, 46, 32), Some(0));
        assert!(r > l);
    }
    #[test]
    fn header_settings_chip_hit_detected() {
        let (l, _) = header_chip_rects(700, 46)[1];
        assert_eq!(hit_header_chip(l, 10, 700, 46, 32), Some(1));
    }
    #[test]
    fn header_chip_misses_caption_buttons() {
        assert_eq!(hit_header_chip(690, 10, 700, 46, 32), None);
    }
    #[test]
    fn header_chips_are_client_area_so_clicks_reach_the_window_at_every_scale() {
        // 통계·설정 칩이 HTCAPTION(창 끌기)이 되면 클릭이 클라이언트에 오지 않아 작동하지 않았다.
        for scale in [100, 125, 150, 200] {
            let bw = CAPTION_W * scale / 100;
            let head = HEAD_H * scale / 100;
            let width = 700 * scale / 100;
            for (i, (l, r)) in header_chip_rects(width, bw).into_iter().enumerate() {
                let mid = (l + r) / 2;
                assert!(header_is_client_button(mid, head / 2, width, bw, head), "chip {i} scale {scale}");
                assert_eq!(hit_header_chip(mid, head / 2, width, bw, head), Some(i));
                // 칩은 캡션 단추(최소화·닫기)와 겹치지 않는다.
                assert!(r <= width - 2 * bw, "chip {i} overlaps caption buttons at {scale}");
            }
            // 칩 두 개도 서로 겹치지 않는다.
            let chips = header_chip_rects(width, bw);
            assert!(chips[0].1 <= chips[1].0);
            // 제목 줄 빈 곳은 여전히 끌기(캡션) 영역이다.
            assert!(!header_is_client_button(10, head / 2, width, bw, head));
        }
    }

    #[test]
    fn option_band_first_row_and_popup_row_do_not_share_height() {
        // 옵션 줄 글자는 위쪽 절반, 팝업(모두 포함 등)은 아래쪽 절반에만 그려 겹치지 않는다.
        for scale in [100, 125, 150, 200] {
            let closed = quick_search_layout(scale, 700, 900, false, false, false);
            let open = quick_search_layout(scale, 700, 900, false, true, false);
            let half = OPTION_H * scale / 100 / 2;
            assert_eq!(closed.options.bottom - closed.options.top, half);
            assert_eq!(open.options.bottom - open.options.top, OPTION_H * scale / 100);
            assert_eq!(open.filters.top, open.options.bottom);
            // 탭 줄은 없다.
            assert_eq!(open.tabs.bottom - open.tabs.top, 0);
            // 첫 줄(top..top+half)에서는 옵션 단추를, 둘째 줄에서는 팝업 칩을 판정한다.
            let top = open.options.top;
            assert_eq!(hit_option(10, top + half / 2, top, open.options.bottom - top, 0, 700), Some(OptionControl::SearchMode));
            assert_eq!(hit_option(12, top + half + 1, top, open.options.bottom - top, 0, 700), Some(OptionControl::MatchMode));
        }
    }

    #[test]
    fn header_chip_misses_below_header() {
        assert_eq!(hit_header_chip(600, 33, 700, 46, 32), None);
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
    fn sidebar_open_keeps_content_width_when_window_grows_by_sidebar() {
        // 사이드바를 열면 창을 사이드바 폭만큼 넓혀(window.rs) 내용 영역이 기본 폭 700 아래로 줄지 않는다.
        let base = 700;
        let grown = base + SIDEBAR_W;
        assert_eq!(sidebar_width(true, grown), SIDEBAR_W);
        assert_eq!(grown - sidebar_width(true, grown), base);
        // 닫으면 다시 기본 폭을 전부 쓴다.
        assert_eq!(base - sidebar_width(false, base), base);
    }
    #[test]
    fn sidebar_width_scales_with_dpi_and_keeps_content_width() {
        // 144dpi(배율 150%): 사이드바 330px, 창을 330px 넓히면 내용 폭 700*1.5가 그대로다.
        let full = SIDEBAR_W * 150 / 100;
        assert_eq!(full, 330);
        let base = 1050;
        let grown = base + full;
        assert_eq!(sidebar_width_px(true, grown, full), full);
        assert_eq!(grown - sidebar_width_px(true, grown, full), base);
        // 96dpi 기본값은 예전 함수와 같다.
        assert_eq!(sidebar_width_px(true, 920, SIDEBAR_W), sidebar_width(true, 920));
        // 좁은 창에서는 절반을 넘지 않는다.
        assert_eq!(sidebar_width_px(true, 400, full), 200);
        assert_eq!(sidebar_width_px(false, 920, full), 0);
    }
    #[test]
    fn related_chip_gap_scales_with_dpi() {
        assert_eq!(related_chip_gap(100), 6);
        assert_eq!(related_chip_gap(150), 9);
        assert!(related_chip_gap(0) >= 2);
        let groups = vec!["예산".to_string(), "계획".to_string()];
        let rects = related_chip_rects(&groups, 0, 0, 2000, 150);
        assert_eq!(rects[1].left - rects[0].right, 9);
    }
    #[test]
    fn pick_fitting_label_falls_back_to_short_form() {
        let c = vec!["파일명 매치 7 ▾".to_string(), "이름 7 ▾".to_string()];
        let m = |s: &str| s.chars().count() as i32 * 10;
        assert_eq!(pick_fitting_label(&c, 200, m), "파일명 매치 7 ▾");
        assert_eq!(pick_fitting_label(&c, 90, m), "이름 7 ▾");
        // 모두 넘치면 가장 짧은 것.
        assert_eq!(pick_fitting_label(&c, 10, m), "이름 7 ▾");
        assert_eq!(pick_fitting_label(&[], 10, m), "");
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
            group_matches: Vec::new(),
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
            snippet_tried: true,
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
    fn exclude_filename_keeps_rows_pending_body_snippet_load() {
        let mut pending = row("filename-match.txt", "");
        pending.snippet_tried = false;
        let loaded_without_match = row("other.txt", "unrelated body");
        let loaded_with_match = row("third.txt", "needle in body");
        assert_eq!(
            filter_rows_within_query(
                &[pending, loaded_without_match, loaded_with_match],
                "needle",
                true,
                &[],
            ),
            vec![0, 2]
        );
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
