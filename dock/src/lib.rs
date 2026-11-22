//! Pure layout and menu model for the native bottom dock.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl Rect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
    Auto,
}
/// 포그라운드 창이 독 화면 영역과 일부라도 겹치는지 판정한다.
pub fn foreground_covers_dock(foreground: Rect, dock: Rect) -> bool {
    foreground.left < dock.right && foreground.right > dock.left
        && foreground.top < dock.bottom && foreground.bottom > dock.top
}

/// 선택된 모니터 가장자리의 재등장 허용 영역 안에 포인터가 있는지 판정한다.
pub fn pointer_on_reveal_edge(point: (i32, i32), monitor: Rect, edge: Edge, tolerance: i32) -> bool {
    let t = tolerance.max(0);
    match edge {
        Edge::Bottom => point.0 >= monitor.left && point.0 < monitor.right && point.1 >= monitor.bottom - t && point.1 <= monitor.bottom,
        Edge::Top => point.0 >= monitor.left && point.0 < monitor.right && point.1 >= monitor.top && point.1 < monitor.top + t,
        Edge::Left => point.0 >= monitor.left && point.0 < monitor.left + t && point.1 >= monitor.top && point.1 < monitor.bottom,
        Edge::Right => point.0 >= monitor.right - t && point.0 <= monitor.right && point.1 >= monitor.top && point.1 < monitor.bottom,
        Edge::Auto => false,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockRect {
    pub rect: Rect,
    pub edge: Edge,
}

/// Work area is supplied by the monitor/work-area query; automatic taskbar hiding is therefore handled naturally.
pub fn dock_rect(
    work: Rect,
    monitor: Rect,
    edge: Edge,
    dpi: u32,
    logical_width: i32,
    logical_height: i32,
    gap_dip: i32,
) -> DockRect {
    let scale = |v: i32| ((v as i64 * dpi.max(1) as i64 + 48) / 96) as i32;
    let edge = if edge == Edge::Auto {
        [
            (Edge::Bottom, monitor.bottom - work.bottom),
            (Edge::Top, work.top - monitor.top),
            (Edge::Left, work.left - monitor.left),
            (Edge::Right, monitor.right - work.right),
        ]
        .into_iter()
        .max_by_key(|x| x.1)
        .filter(|x| x.1 > 0)
        .map(|x| x.0)
        .unwrap_or(Edge::Bottom)
    } else {
        edge
    };
    let (w, h, g) = (scale(logical_width), scale(logical_height), scale(gap_dip));
    let rect = match edge {
        Edge::Bottom => {
            let x = work.left + (work.width() - w) / 2;
            Rect {
                left: x,
                top: work.bottom - h - g,
                right: x + w,
                bottom: work.bottom - g,
            }
        }
        Edge::Top => {
            let x = work.left + (work.width() - w) / 2;
            Rect {
                left: x,
                top: work.top + g,
                right: x + w,
                bottom: work.top + g + h,
            }
        }
        Edge::Left => {
            let y = work.top + (work.height() - w) / 2;
            Rect {
                left: work.left + g,
                top: y,
                right: work.left + g + h,
                bottom: y + w,
            }
        }
        Edge::Right => {
            let y = work.top + (work.height() - w) / 2;
            Rect {
                left: work.right - h - g,
                top: y,
                right: work.right - g,
                bottom: y + w,
            }
        }
        Edge::Auto => unreachable!(),
    };
    DockRect { rect, edge }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconPlacement {
    pub center_x: f32,
    pub center_y: f32,
    pub scale: f32,
    pub hit: Rect,
}
pub fn icon_scales(pointer_x: f32, centers: &[f32], radius: f32, max_scale: f32) -> Vec<f32> {
    centers
        .iter()
        .map(|x| {
            1.0 + (max_scale.max(1.0) - 1.0)
                * (1.0 - (pointer_x - *x).abs() / radius.max(1.0)).clamp(0.0, 1.0)
        })
        .collect()
}
pub fn icon_layout(
    bounds: Rect,
    count: usize,
    icon_dip: i32,
    gap_dip: i32,
    dpi: u32,
    pointer_x: Option<f32>,
    radius_dip: f32,
    max_scale: f32,
) -> Vec<IconPlacement> {
    if count == 0 {
        return vec![];
    }
    let s = dpi.max(1) as f32 / 96.0;
    let icon = icon_dip as f32 * s;
    let gap = gap_dip as f32 * s;
    let total = count as f32 * icon + count.saturating_sub(1) as f32 * gap;
    let start = bounds.left as f32 + (bounds.width() as f32 - total) / 2.0;
    let centers = (0..count)
        .map(|i| start + i as f32 * (icon + gap) + icon / 2.0)
        .collect::<Vec<_>>();
    let scales = pointer_x
        .map(|x| icon_scales(x, &centers, radius_dip * s, max_scale))
        .unwrap_or_else(|| vec![1.; count]);
    centers
        .into_iter()
        .zip(scales)
        .map(|(cx, z)| {
            let d = icon * z;
            let cy = bounds.top as f32 + bounds.height() as f32 / 2.;
            IconPlacement {
                center_x: cx,
                center_y: cy,
                scale: z,
                hit: Rect {
                    left: (cx - d / 2.) as i32,
                    top: (cy - d / 2.) as i32,
                    right: (cx + d / 2.) as i32,
                    bottom: (cy + d / 2.) as i32,
                },
            }
        })
        .collect()
}
pub fn hit_test(p: (i32, i32), icons: &[IconPlacement]) -> Option<usize> {
    icons.iter().position(|i| {
        p.0 >= i.hit.left && p.0 < i.hit.right && p.1 >= i.hit.top && p.1 < i.hit.bottom
    })
}
pub const TAB_NAMES: [&str; 16] = [
    "문서편집",
    "표템플릿",
    "표그림판",
    "다이어그램",
    "마인드맵",
    "붙임사진 편집",
    "파일명 일괄변경",
    "몽땅검색",
    "문서뷰",
    "사진뷰",
    "파일변환",
    "PDF 편집",
    "자료함",
    "업무자료함/노트북",
    "아웃라이너",
    "검색 벤치마크",
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub tab: u8,
    pub name: &'static str,
    pub group: u8,
}

/// Kept in the same order as `WorkspaceTabBar.kSidebarGroups`.
pub const SIDEBAR_GROUPS: [(&str, &[u8]); 4] = [
    ("문서 작업", &[0, 1, 5]),
    ("그리기", &[2, 3, 4]),
    ("찾기·자료", &[7, 8, 9]),
    ("변환·도구", &[6, 10, 11, 14, 15]),
];

pub fn menu_items(hidden_tabs: &[u8], developer_mode: bool) -> Vec<MenuItem> {
    let mut result = Vec::new();
    for (group, (_, tabs)) in SIDEBAR_GROUPS.iter().enumerate() {
        for &tab in *tabs {
            let developer_only = matches!(tab, 12 | 13 | 15);
            if (developer_only && !developer_mode) || (tab != 0 && hidden_tabs.contains(&tab)) {
                continue;
            }
            result.push(MenuItem { tab, name: TAB_NAMES[tab as usize], group: group as u8 });
        }
        // Developer-only entries live in the existing '찾기·자료' grouping.
        if group == 2 && developer_mode {
            for tab in [12, 13] {
                result.push(MenuItem { tab, name: TAB_NAMES[tab as usize], group: group as u8 });
            }
        }
    }
    result
}

/// Read `hidden_tabs` from the app's UTF-8 settings JSON. Invalid or missing
/// values fall back to the built-in sidebar default (no optional entries hidden).
pub fn parse_hidden_tabs(settings_json: &str) -> std::collections::BTreeSet<u8> {
    let default = std::collections::BTreeSet::new();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(settings_json) else {
        return default;
    };
    let Some(values) = value.get("hidden_tabs").and_then(|v| v.as_array()) else {
        return default;
    };
    values.iter().filter_map(|v| v.as_u64())
        .filter(|n| (1..=15).contains(n) && *n != 12 && *n != 13)
        .map(|n| n as u8).collect()
}
pub fn parse_auto_hide(settings_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(settings_json).ok()
        .and_then(|v| v.get("dock_auto_hide").and_then(|x| x.as_bool())).unwrap_or(false)
}

/// Stable Win32 auto-reset event name used to request a Flutter tab.
pub fn tab_event_name(tab: u8) -> Option<String> {
    (tab < 16).then(|| format!("docfit-open-tab-{tab}"))
}
/// Event paired with `tab_event_name`; signaling it asks the Flutter host to collapse that tab.
pub fn tab_close_event_name(tab: u8) -> Option<String> {
    (tab < 16).then(|| format!("docfit-close-tab-{tab}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route { Search, VirtualFolder, Clipboard, MainTab(u8), Invalid }
pub fn route(tab: u8) -> Route {
    match tab { 7 => Route::Search, 12 => Route::VirtualFolder, 13 => Route::Clipboard,
        0..=15 => Route::MainTab(tab), _ => Route::Invalid }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowState { Closed, OpenUnfocused, OpenFocused }
pub fn click_state(state: WindowState) -> WindowState {
    match state { WindowState::Closed => WindowState::OpenFocused,
        WindowState::OpenUnfocused => WindowState::OpenFocused,
        WindowState::OpenFocused => WindowState::OpenUnfocused }
}
pub fn click_action(state: WindowState) -> ClickAction {
    match state {
        WindowState::OpenFocused => ClickAction::Close,
        WindowState::Closed | WindowState::OpenUnfocused => ClickAction::Open,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickAction { Open, Close }
pub fn running_dot(state: WindowState) -> bool { state != WindowState::Closed }

/// Small GDI-friendly vector vocabulary. Coordinates are normalized to a 24×24 view box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconShape {
    Line { x1: f32, y1: f32, x2: f32, y2: f32 },
    Rect { left: f32, top: f32, right: f32, bottom: f32, filled: bool },
    Circle { cx: f32, cy: f32, radius: f32, filled: bool },
    Polygon { points: &'static [(f32, f32)], filled: bool },
}

const DOC: &[IconShape] = &[IconShape::Rect { left: 5., top: 3., right: 19., bottom: 21., filled: false }, IconShape::Line { x1: 8., y1: 8., x2: 16., y2: 8. }, IconShape::Line { x1: 8., y1: 12., x2: 16., y2: 12. }, IconShape::Line { x1: 8., y1: 16., x2: 14., y2: 16. }];
const TABLE: &[IconShape] = &[IconShape::Rect { left: 3., top: 5., right: 21., bottom: 19., filled: false }, IconShape::Line { x1: 3., y1: 10., x2: 21., y2: 10. }, IconShape::Line { x1: 3., y1: 14., x2: 21., y2: 14. }, IconShape::Line { x1: 9., y1: 5., x2: 9., y2: 19. }, IconShape::Line { x1: 15., y1: 5., x2: 15., y2: 19. }];
const GRID: &[IconShape] = &[IconShape::Rect { left: 4., top: 4., right: 20., bottom: 20., filled: false }, IconShape::Line { x1: 4., y1: 12., x2: 20., y2: 12. }, IconShape::Line { x1: 12., y1: 4., x2: 12., y2: 20. }, IconShape::Circle { cx: 12., cy: 12., radius: 2., filled: true }];
const NODES: &[(f32, f32)] = &[(4., 12.), (12., 4.), (20., 12.), (12., 20.)];
const POLYGON4: &[IconShape] = &[IconShape::Polygon { points: NODES, filled: false }, IconShape::Line { x1: 4., y1: 12., x2: 20., y2: 12. }, IconShape::Line { x1: 12., y1: 4., x2: 12., y2: 20. }];
const MIND: &[IconShape] = &[IconShape::Circle { cx: 12., cy: 12., radius: 4., filled: true }, IconShape::Circle { cx: 4., cy: 6., radius: 2., filled: false }, IconShape::Circle { cx: 20., cy: 6., radius: 2., filled: false }, IconShape::Circle { cx: 4., cy: 18., radius: 2., filled: false }, IconShape::Circle { cx: 20., cy: 18., radius: 2., filled: false }, IconShape::Line { x1: 8., y1: 10., x2: 5., y2: 7. }, IconShape::Line { x1: 16., y1: 10., x2: 19., y2: 7. }, IconShape::Line { x1: 8., y1: 14., x2: 5., y2: 17. }, IconShape::Line { x1: 16., y1: 14., x2: 19., y2: 17. }];
const PICTURE: &[IconShape] = &[IconShape::Rect { left: 3., top: 4., right: 21., bottom: 20., filled: false }, IconShape::Circle { cx: 8., cy: 9., radius: 2., filled: true }, IconShape::Polygon { points: &[(5., 18.), (11., 12.), (14., 15.), (17., 11.), (20., 18.)], filled: true }];
const RENAME: &[IconShape] = &[IconShape::Line { x1: 5., y1: 19., x2: 19., y2: 5. }, IconShape::Polygon { points: &[(4., 20.), (5., 15.), (9., 19.)], filled: true }, IconShape::Line { x1: 13., y1: 7., x2: 17., y2: 11. }, IconShape::Line { x1: 7., y1: 7., x2: 11., y2: 7. }];
const SEARCH: &[IconShape] = &[IconShape::Circle { cx: 10., cy: 10., radius: 6., filled: false }, IconShape::Line { x1: 14., y1: 14., x2: 21., y2: 21. }];
const VIEW: &[IconShape] = &[IconShape::Polygon { points: &[(2., 12.), (7., 6.), (17., 6.), (22., 12.), (17., 18.), (7., 18.)], filled: false }, IconShape::Circle { cx: 12., cy: 12., radius: 3., filled: true }];
const PHOTO: &[IconShape] = &[IconShape::Rect { left: 3., top: 3., right: 21., bottom: 21., filled: false }, IconShape::Circle { cx: 15., cy: 8., radius: 2., filled: true }, IconShape::Polygon { points: &[(5., 18.), (10., 11.), (13., 15.), (16., 12.), (20., 18.)], filled: true }];
const CONVERT: &[IconShape] = &[IconShape::Rect { left: 4., top: 5., right: 15., bottom: 19., filled: false }, IconShape::Line { x1: 10., y1: 12., x2: 21., y2: 12. }, IconShape::Polygon { points: &[(17., 8.), (21., 12.), (17., 16.)], filled: true }];
const PDF: &[IconShape] = &[IconShape::Rect { left: 5., top: 3., right: 19., bottom: 21., filled: false }, IconShape::Line { x1: 7., y1: 16., x2: 10., y2: 8. }, IconShape::Line { x1: 10., y1: 8., x2: 13., y2: 16. }, IconShape::Line { x1: 8., y1: 13., x2: 12., y2: 13. }, IconShape::Line { x1: 15., y1: 9., x2: 18., y2: 9. }, IconShape::Line { x1: 15., y1: 9., x2: 15., y2: 16. }];
const FOLDER: &[IconShape] = &[IconShape::Polygon { points: &[(3., 6.), (10., 6.), (12., 9.), (21., 9.), (19., 19.), (4., 19.)], filled: false }, IconShape::Line { x1: 5., y1: 12., x2: 19., y2: 12. }];
const NOTE: &[IconShape] = &[IconShape::Rect { left: 4., top: 3., right: 20., bottom: 21., filled: false }, IconShape::Line { x1: 8., y1: 8., x2: 16., y2: 8. }, IconShape::Line { x1: 8., y1: 12., x2: 16., y2: 12. }, IconShape::Line { x1: 8., y1: 16., x2: 14., y2: 16. }, IconShape::Circle { cx: 18., cy: 18., radius: 2., filled: true }];
const OUTLINE: &[IconShape] = &[IconShape::Line { x1: 5., y1: 5., x2: 5., y2: 19. }, IconShape::Line { x1: 5., y1: 7., x2: 10., y2: 7. }, IconShape::Line { x1: 5., y1: 12., x2: 10., y2: 12. }, IconShape::Line { x1: 5., y1: 17., x2: 10., y2: 17. }, IconShape::Rect { left: 11., top: 5., right: 20., bottom: 9., filled: false }, IconShape::Rect { left: 11., top: 10., right: 20., bottom: 14., filled: false }, IconShape::Rect { left: 11., top: 15., right: 20., bottom: 19., filled: false }];
const BENCH: &[IconShape] = &[IconShape::Circle { cx: 12., cy: 12., radius: 8., filled: false }, IconShape::Line { x1: 12., y1: 12., x2: 17., y2: 8. }, IconShape::Circle { cx: 12., cy: 12., radius: 2., filled: true }];
pub fn icon_shapes(tab: u8) -> &'static [IconShape] {
    match tab { 0 => DOC, 1 => TABLE, 2 => GRID, 3 => POLYGON4, 4 => MIND, 5 => PICTURE, 6 => RENAME, 7 => SEARCH, 8 => VIEW, 9 => PHOTO, 10 => CONVERT, 11 => PDF, 12 => FOLDER, 13 => NOTE, 14 => OUTLINE, 15 => BENCH, _ => &[] }
}

/// 사이드바(`tab_navigation_shell.dart`)와 같은 Material Icons 글리프의 코드포인트(Flutter icons.dart 기준).
/// 독 탭 0..=11·14·15 는 사이드바 인덱스와 같고, 12(자료함)=가상 폴더(dashboard_customize_outlined),
/// 13(업무자료함/노트북)=업무자료함(menu_book_outlined)이다. tools/make_dock_icon_font.py 의 목록과 같아야 한다.
pub const ICON_CODEPOINTS: [u32; 16] = [
    0xefae,  // 0 description_outlined
    0xf41d,  // 1 table_chart_outlined
    0xf05ed, // 2 draw_outlined
    0xee36,  // 3 account_tree_outlined
    0xf0616, // 4 hub_outlined
    0xef02,  // 5 brush_outlined
    0xe20b,  // 6 drive_file_rename_outline
    0xe316,  // 7 history_toggle_off
    0xf0d7,  // 8 grid_view_outlined
    0xf29d,  // 9 photo_library_outlined
    0xe678,  // 10 transform
    0xf2a4,  // 11 picture_as_pdf_outlined
    0xefa0,  // 12 dashboard_customize_outlined (가상 폴더)
    0xf1c2,  // 13 menu_book_outlined (업무자료함)
    0xee36,  // 14 account_tree_outlined (아웃라이너, 사이드바 14)
    0xf3c3,  // 15 speed_outlined
];
/// 탭의 Material Icons 글리프 문자. 범위 밖이면 None.
pub fn icon_glyph(tab: u8) -> Option<char> { ICON_CODEPOINTS.get(tab as usize).and_then(|&c| char::from_u32(c)) }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IconCacheKey { pub tab: u8, pub dpi: u32, pub size_px: i32 }
pub fn icon_size_px(dpi: u32, logical_size: i32) -> i32 { ((dpi.max(1) as i64 * logical_size as i64 + 48) / 96) as i32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette { pub foreground: u32, pub accent: u32, pub background: u32, pub border: u32 }
pub const NORMAL_PALETTE: Palette = Palette { foreground: 0xF4F1ED, accent: 0x67B7D1, background: 0x302820, border: 0x706050 };
pub const LIGHT_PALETTE: Palette = Palette { foreground: 0x202124, accent: 0x1769AA, background: 0xF5F6F8, border: 0xC3C7CD };
pub const DARK_PALETTE: Palette = NORMAL_PALETTE;
pub fn palette_for_theme(dark: bool) -> Palette { if dark { DARK_PALETTE } else { LIGHT_PALETTE } }
pub fn palette_for_mode(normal: Palette, high_contrast: bool, window: u32, window_text: u32, highlight: u32, highlight_text: u32) -> Palette {
    if high_contrast { Palette { foreground: window_text, accent: highlight, background: window, border: highlight_text } } else { normal }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooltipPlacement { pub left: i32, pub top: i32 }
pub fn tooltip_visible(elapsed_ms: u32, delay_ms: u32, hovered: bool) -> bool { hovered && elapsed_ms >= delay_ms }
pub fn tooltip_position(anchor: (i32, i32), size: (i32, i32), screen: Rect, gap: i32) -> TooltipPlacement {
    let mut left = anchor.0 - size.0 / 2;
    let mut top = anchor.1 - size.1 - gap;
    left = left.clamp(screen.left, (screen.right - size.0).max(screen.left));
    if top < screen.top { top = anchor.1 + gap; }
    top = top.clamp(screen.top, (screen.bottom - size.1).max(screen.top));
    TooltipPlacement { left, top }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key { Left, Right, Enter, Other }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusState { pub selected: usize, pub count: usize }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction { None, Focus(usize), Activate(usize) }
pub fn keyboard_step(state: FocusState, key: Key) -> (FocusState, KeyAction) {
    if state.count == 0 { return (state, KeyAction::None); }
    let selected = state.selected.min(state.count - 1);
    match key {
        Key::Left => { let next = (selected + state.count - 1) % state.count; (FocusState { selected: next, count: state.count }, KeyAction::Focus(next)) }
        Key::Right => { let next = (selected + 1) % state.count; (FocusState { selected: next, count: state.count }, KeyAction::Focus(next)) }
        Key::Enter => (FocusState { selected, count: state.count }, KeyAction::Activate(selected)),
        Key::Other => (FocusState { selected, count: state.count }, KeyAction::None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }
    #[test]
    fn dock_layout_dump_edges_dpi_and_workarea() {
        let m = r(0, 0, 1920, 1080);
        let inputs = [
            (Edge::Bottom, r(0, 0, 1920, 1040)),
            (Edge::Top, r(0, 40, 1920, 1080)),
            (Edge::Left, r(48, 0, 1920, 1080)),
            (Edge::Right, r(0, 0, 1872, 1080)),
            (Edge::Auto, r(0, 0, 1920, 1040)),
        ];
        let dump = inputs
            .iter()
            .map(|(e, w)| {
                let d = dock_rect(*w, m, *e, 96, 400, 56, 18);
                format!(
                    "{:?}:{} {} {} {}",
                    d.edge, d.rect.left, d.rect.top, d.rect.right, d.rect.bottom
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            dump,
            "Bottom:760 966 1160 1022\nTop:760 58 1160 114\nLeft:66 340 122 740\nRight:1798 340 1854 740\nBottom:760 966 1160 1022"
        );
        assert_eq!(
            dock_rect(r(0, 0, 1920, 1040), m, Edge::Bottom, 192, 400, 56, 18).rect,
            r(560, 892, 1360, 1004)
        );
        assert_eq!(
            dock_rect(r(80, 0, 1920, 1080), m, Edge::Auto, 96, 400, 56, 18).edge,
            Edge::Left
        );
        // Auto-hidden taskbars leave rcWork equal to rcMonitor; use the documented bottom default.
        assert_eq!(dock_rect(m,m,Edge::Auto,96,400,56,18).rect,r(760,1006,1160,1062));
        // Secondary monitor coordinates may be negative; placement remains centered in its own work area.
        let secondary=r(-1920,0,0,1080); let work=r(-1920,0,0,1040);
        assert_eq!(dock_rect(work,secondary,Edge::Bottom,96,400,56,18).rect,r(-1160,966,-760,1022));
    }
    #[test]
    fn auto_hide_overlap_and_reveal_edge_decisions() {
        let dock = r(400, 950, 800, 1010);
        assert!(foreground_covers_dock(r(0, 0, 1920, 1040), dock));
        assert!(!foreground_covers_dock(r(0, 0, 1920, 940), dock));
        assert!(!foreground_covers_dock(r(0, 0, 400, 1080), dock));
        let monitor = r(0, 0, 1920, 1080);
        assert!(pointer_on_reveal_edge((900, 1079), monitor, Edge::Bottom, 2));
        assert!(!pointer_on_reveal_edge((900, 1077), monitor, Edge::Bottom, 2));
        assert!(pointer_on_reveal_edge((0, 500), monitor, Edge::Left, 2));
        assert!(!pointer_on_reveal_edge((4, 500), monitor, Edge::Left, 2));
        assert!(pointer_on_reveal_edge((900, 0), monitor, Edge::Top, 2));
        assert!(!pointer_on_reveal_edge((900, 2), monitor, Edge::Top, 2));
        assert!(pointer_on_reveal_edge((1919, 500), monitor, Edge::Right, 2));
        assert!(!pointer_on_reveal_edge((1917, 500), monitor, Edge::Right, 2));
        assert!(!pointer_on_reveal_edge((900, 1079), monitor, Edge::Auto, 2));
        assert!(!parse_auto_hide("{}"));
        assert!(!parse_auto_hide(r#"{"dock_auto_hide":"true"}"#));
        assert!(parse_auto_hide(r#"{"dock_auto_hide":true}"#));
    }
    #[test]
    fn hover_interpolation_dump() {
        assert_eq!(
            icon_scales(100., &[0., 50., 100., 150., 200.], 100., 1.8)
                .iter()
                .map(|v| format!("{v:.2}"))
                .collect::<Vec<_>>()
                .join(","),
            "1.00,1.40,1.80,1.40,1.00"
        )
    }
    #[test]
    fn icon_layout_and_hit_test_dump() {
        let a = icon_layout(r(0, 0, 300, 80), 3, 40, 12, 96, None, 80., 1.8);
        assert_eq!(
            a.iter()
                .map(|i| format!(
                    "{},{},{},{}",
                    i.hit.left, i.hit.top, i.hit.right, i.hit.bottom
                ))
                .collect::<Vec<_>>()
                .join(";"),
            "78,20,118,60;130,20,170,60;182,20,222,60"
        );
        assert_eq!(hit_test((150, 40), &a), Some(1));
        assert_eq!(hit_test((118, 40), &a), None)
    }
    #[test]
    fn menu_dump_hidden_tabs_and_required_entries() {
        assert_eq!(
            menu_items(&[0, 1, 12, 13], false)
                .iter()
                .map(|m| format!("{}:{}", m.tab, m.name))
                .collect::<Vec<_>>()
                .join("|"),
            "0:문서편집|5:붙임사진 편집|2:표그림판|3:다이어그램|4:마인드맵|7:몽땅검색|8:문서뷰|9:사진뷰|6:파일명 일괄변경|10:파일변환|11:PDF 편집|14:아웃라이너"
        );
        assert_eq!(menu_items(&[1], true).first().unwrap().tab, 0);
        assert!(menu_items(&[12, 13], true).iter().any(|i| i.tab == 12));
        assert!(menu_items(&[], true).iter().any(|i| i.tab == 15));
        assert!(
            menu_items(&[], false)
                .iter()
                .all(|i| i.tab != 12 && i.tab != 13 && i.tab != 15)
        )
    }
    #[test]
    fn reclick_state_machine_opens_then_closes_and_reopens() {
        let mut state=WindowState::Closed;
        assert_eq!(click_action(state),ClickAction::Open); state=WindowState::OpenFocused;
        assert_eq!(click_action(state),ClickAction::Close); state=WindowState::Closed;
        assert_eq!(click_action(state),ClickAction::Open);
        assert_eq!(tab_event_name(4).as_deref(),Some("docfit-open-tab-4"));
        assert_eq!(tab_close_event_name(4).as_deref(),Some("docfit-close-tab-4"));
        assert_eq!(tab_close_event_name(16),None);
    }
    #[test]
    fn sidebar_group_order_and_hidden_tabs_are_shared_with_dock() {
        let items = menu_items(&[1, 3, 8, 10], false);
        assert_eq!(items.iter().map(|item| item.tab).collect::<Vec<_>>(), [0, 5, 2, 4, 7, 9, 6, 11, 14]);
        assert_eq!(items.iter().map(|item| item.group).collect::<Vec<_>>(), [0, 0, 1, 1, 2, 2, 3, 3, 3]);
        assert_eq!(menu_items(&[], true).iter().map(|item| item.tab).collect::<Vec<_>>(), [0,1,5,2,3,4,7,8,9,12,13,6,10,11,14,15]);
    }
    #[test]
    fn routing_table_and_tab_event_contract() {
        assert_eq!(route(7), Route::Search);
        assert_eq!(route(12), Route::VirtualFolder);
        assert_eq!(route(13), Route::Clipboard);
        for tab in [0,1,2,3,4,5,6,8,9,10,11,14,15] { assert_eq!(route(tab), Route::MainTab(tab)); }
        assert_eq!(route(16), Route::Invalid);
        assert_eq!(tab_event_name(4).as_deref(), Some("docfit-open-tab-4"));
        assert_eq!(tab_event_name(15).as_deref(), Some("docfit-open-tab-15"));
        assert_eq!(tab_event_name(16), None);
    }
    #[test]
    fn hidden_tabs_json_parsing_and_fallback() {
        assert_eq!(parse_hidden_tabs(r#"{"hidden_tabs":[1,6,12,13,14,15,0,-1,"2",1]}"#), std::collections::BTreeSet::from([1,6,14,15]));
        assert_eq!(parse_hidden_tabs(r#"{"hidden_tabs":"bad"}"#), std::collections::BTreeSet::new());
        assert_eq!(parse_hidden_tabs("{"), std::collections::BTreeSet::new());
        assert_eq!(parse_hidden_tabs(r#"{"hidden_tabs":[]}"#), std::collections::BTreeSet::new());
    }
    #[test]
    fn window_click_state_machine_and_running_indicator() {
        assert_eq!(click_state(WindowState::Closed), WindowState::OpenFocused);
        assert_eq!(click_state(WindowState::OpenUnfocused), WindowState::OpenFocused);
        assert_eq!(click_state(WindowState::OpenFocused), WindowState::OpenUnfocused);
        assert!(!running_dot(WindowState::Closed));
        assert!(running_dot(WindowState::OpenUnfocused));
    }
    #[test]
    fn all_menu_icons_have_distinct_vector_geometry_and_dpi_sizes() {
        for tab in 0..16 { assert!(!icon_shapes(tab).is_empty(), "missing icon {tab}"); }
        assert!(icon_shapes(16).is_empty());
        assert_eq!(icon_size_px(96, 24), 24);
        assert_eq!(icon_size_px(144, 24), 36);
        assert_ne!(icon_shapes(0), icon_shapes(1));
    }

    #[test]
    fn icon_glyphs_match_sidebar_material_icons() {
        // 사이드바 _icons[0..16] (Flutter icons.dart 코드포인트) 와 독 탭 대응.
        let sidebar: [u32; 16] = [0xefae, 0xf41d, 0xf05ed, 0xee36, 0xf0616, 0xef02, 0xe20b, 0xe316, 0xf0d7, 0xf29d, 0xe678, 0xf2a4, 0xf36e, 0xe30b, 0xee36, 0xf3c3];
        for tab in 0..16u8 {
            let g = icon_glyph(tab).unwrap_or_else(|| panic!("missing glyph {tab}"));
            assert!(g as u32 >= 0xe000, "PUA glyph expected for tab {tab}");
            // 12·13 은 사이드바 인덱스가 아닌 '가상 폴더'·'업무자료함' 별도 항목과 대응
            if tab != 12 && tab != 13 { assert_eq!(g as u32, sidebar[tab as usize], "tab {tab}"); }
        }
        assert_eq!(icon_glyph(12).map(|c| c as u32), Some(0xefa0));
        assert_eq!(icon_glyph(13).map(|c| c as u32), Some(0xf1c2));
        assert!(icon_glyph(16).is_none());
        // 가상 폴더·업무자료함 외에는 서로 다른 메뉴(14=아웃라이너 제외)가 같은 글리프를 쓰지 않는다
        for a in 0..16usize { for b in a + 1..16usize {
            if (a, b) != (3, 14) { assert_ne!(ICON_CODEPOINTS[a], ICON_CODEPOINTS[b], "{a} vs {b}"); }
        } }
    }
    #[test]
    fn tooltip_delay_and_position_dump() {
        assert!(!tooltip_visible(399, 400, true));
        assert!(tooltip_visible(400, 400, true));
        assert!(!tooltip_visible(900, 400, false));
        assert_eq!(tooltip_position((5, 5), (80, 24), r(0, 0, 100, 100), 6), TooltipPlacement { left: 0, top: 11 });
        assert_eq!(tooltip_position((98, 80), (80, 24), r(0, 0, 100, 100), 6), TooltipPlacement { left: 20, top: 50 });
    }
    #[test]
    fn keyboard_focus_wraps_and_enter_activates() {
        let s = FocusState { selected: 0, count: 3 };
        assert_eq!(keyboard_step(s, Key::Left), (FocusState { selected: 2, count: 3 }, KeyAction::Focus(2)));
        assert_eq!(keyboard_step(s, Key::Right), (FocusState { selected: 1, count: 3 }, KeyAction::Focus(1)));
        assert_eq!(keyboard_step(FocusState { selected: 8, count: 3 }, Key::Enter), (FocusState { selected: 2, count: 3 }, KeyAction::Activate(2)));
        assert_eq!(keyboard_step(FocusState { selected: 0, count: 0 }, Key::Enter).1, KeyAction::None);
    }
    #[test]
    fn high_contrast_uses_system_palette_and_normal_keeps_theme() {
        let hc = palette_for_mode(NORMAL_PALETTE, true, 0x111, 0x222, 0x333, 0x444);
        assert_eq!(hc, Palette { foreground: 0x222, accent: 0x333, background: 0x111, border: 0x444 });
        assert_eq!(palette_for_mode(NORMAL_PALETTE, false, 0, 0, 0, 0), NORMAL_PALETTE);
    }
}
