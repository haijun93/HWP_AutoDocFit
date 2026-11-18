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
pub const TAB_NAMES: [&str; 15] = [
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
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub tab: u8,
    pub name: &'static str,
}
pub fn menu_items(hidden_tabs: &[u8], developer_mode: bool) -> Vec<MenuItem> {
    (0..15u8)
        .filter(|n| {
            *n == 0 || ((*n == 12 || *n == 13) && developer_mode) || !hidden_tabs.contains(n)
        })
        .filter(|n| (*n != 12 && *n != 13) || developer_mode)
        .map(|tab| MenuItem {
            tab,
            name: TAB_NAMES[tab as usize],
        })
        .collect()
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
        .filter(|n| (1..=14).contains(n) && *n != 12 && *n != 13)
        .map(|n| n as u8).collect()
}

/// Stable Win32 auto-reset event name used to request a Flutter tab.
pub fn tab_event_name(tab: u8) -> Option<String> {
    (tab < 15).then(|| format!("docfit-open-tab-{tab}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route { Search, VirtualFolder, Clipboard, MainTab(u8), Invalid }
pub fn route(tab: u8) -> Route {
    match tab { 7 => Route::Search, 12 => Route::VirtualFolder, 13 => Route::Clipboard,
        0..=14 => Route::MainTab(tab), _ => Route::Invalid }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowState { Closed, OpenUnfocused, OpenFocused }
pub fn click_state(state: WindowState) -> WindowState {
    match state { WindowState::Closed => WindowState::OpenFocused,
        WindowState::OpenUnfocused => WindowState::OpenFocused,
        WindowState::OpenFocused => WindowState::OpenUnfocused }
}
pub fn running_dot(state: WindowState) -> bool { state != WindowState::Closed }

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
        )
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
            "0:문서편집|2:표그림판|3:다이어그램|4:마인드맵|5:붙임사진 편집|6:파일명 일괄변경|7:몽땅검색|8:문서뷰|9:사진뷰|10:파일변환|11:PDF 편집|14:아웃라이너"
        );
        assert_eq!(menu_items(&[1], true).first().unwrap().tab, 0);
        assert!(menu_items(&[12, 13], true).iter().any(|i| i.tab == 12));
        assert!(
            menu_items(&[], false)
                .iter()
                .all(|i| i.tab != 12 && i.tab != 13)
        )
    }
    #[test]
    fn routing_table_and_tab_event_contract() {
        assert_eq!(route(7), Route::Search);
        assert_eq!(route(12), Route::VirtualFolder);
        assert_eq!(route(13), Route::Clipboard);
        for tab in [0,1,2,3,4,5,6,8,9,10,11,14] { assert_eq!(route(tab), Route::MainTab(tab)); }
        assert_eq!(route(15), Route::Invalid);
        assert_eq!(tab_event_name(4).as_deref(), Some("docfit-open-tab-4"));
        assert_eq!(tab_event_name(15), None);
    }
    #[test]
    fn hidden_tabs_json_parsing_and_fallback() {
        assert_eq!(parse_hidden_tabs(r#"{"hidden_tabs":[1,6,12,13,14,0,-1,"2",1]}"#), std::collections::BTreeSet::from([1,6,14]));
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
}
