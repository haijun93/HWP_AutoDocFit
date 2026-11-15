//! 패널 배치와 패널별 문맥. 렌더링과 화면 좌표를 다루지 않는다.
use crate::json::{self, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    Single,
    Columns,
    LeftOneRightTwo,
    Grid,
}
impl Template {
    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Columns => "columns",
            Self::LeftOneRightTwo => "left_one_right_two",
            Self::Grid => "grid",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "single" => Self::Single,
            "columns" => Self::Columns,
            "left_one_right_two" => Self::LeftOneRightTwo,
            "grid" => Self::Grid,
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    pub id: String,
    pub box_id: Option<i64>,
    pub space: Option<String>,
    pub virtual_folder_id: Option<String>,
    pub view: String,
    pub sort: String,
    pub query: String,
    pub selection: Vec<String>,
    pub scroll: i64,
}
impl Panel {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.into(),
            box_id: None,
            space: None,
            virtual_folder_id: None,
            view: "details".into(),
            sort: "name_asc".into(),
            query: String::new(),
            selection: Vec::new(),
            scroll: 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub version: u32,
    pub panel_count: u8,
    pub template: Template,
    pub active_panel: String,
    pub column_splits: Vec<u16>,
    pub row_splits: Vec<u16>,
    pub panels: Vec<Panel>,
}
impl Default for Layout {
    fn default() -> Self {
        Self::for_count(1)
    }
}
impl Layout {
    pub fn for_count(count: u8) -> Self {
        let n = count.clamp(1, 4);
        let template = match n {
            1 => Template::Single,
            2 => Template::Columns,
            3 => Template::LeftOneRightTwo,
            _ => Template::Grid,
        };
        let (column_splits, row_splits) = match template {
            Template::Single => (vec![10000], vec![10000]),
            Template::Columns => (vec![5000, 5000], vec![10000]),
            Template::LeftOneRightTwo => (vec![5000, 5000], vec![10000, 5000, 5000]),
            Template::Grid => (vec![5000, 5000], vec![5000, 5000]),
        };
        let panels = (1..=n).map(|i| Panel::new(&format!("p{i}"))).collect();
        Self {
            version: 1,
            panel_count: n,
            template,
            active_panel: "p1".into(),
            column_splits,
            row_splits,
            panels,
        }
    }
    pub fn set_count(&mut self, count: u8) {
        let next = Self::for_count(count);
        let old = std::mem::take(&mut self.panels);
        self.panels = next
            .panels
            .into_iter()
            .map(|mut p| {
                if let Some(existing) = old.iter().find(|x| x.id == p.id) {
                    p = existing.clone()
                }
                p
            })
            .collect();
        self.panel_count = count.clamp(1, 4);
        self.template = next.template;
        self.column_splits = next.column_splits;
        self.row_splits = next.row_splits;
        if !self.panels.iter().any(|p| p.id == self.active_panel) {
            self.active_panel = "p1".into();
        }
    }
    pub fn normalize_splits(values: &[u16], slots: usize) -> Option<Vec<u16>> {
        if slots == 0 || values.len() != slots || values.iter().any(|x| *x == 0) {
            return None;
        }
        let sum: u32 = values.iter().map(|x| *x as u32).sum();
        if sum == 0 {
            return None;
        }
        let mut out = Vec::with_capacity(slots);
        let mut used = 0u32;
        for v in values.iter().take(slots - 1) {
            let n = ((*v as u32 * 10000) / sum).max(1);
            out.push(n as u16);
            used += n;
        }
        if used >= 10000 {
            return None;
        }
        out.push((10000 - used) as u16);
        Some(out)
    }
    pub fn value(&self) -> Value {
        let panels = self
            .panels
            .iter()
            .map(|p| {
                Value::obj(vec![
                    ("id", p.id.clone().into()),
                    ("box_id", p.box_id.map(Value::from).unwrap_or(Value::Null)),
                    (
                        "space",
                        p.space.clone().map(Value::from).unwrap_or(Value::Null),
                    ),
                    (
                        "virtual_folder_id",
                        p.virtual_folder_id
                            .clone()
                            .map(Value::from)
                            .unwrap_or(Value::Null),
                    ),
                    ("view", p.view.clone().into()),
                    ("sort", p.sort.clone().into()),
                    ("query", p.query.clone().into()),
                    (
                        "selection",
                        Value::Arr(p.selection.iter().cloned().map(Value::from).collect()),
                    ),
                    ("scroll", Value::from(p.scroll)),
                ])
            })
            .collect();
        Value::obj(vec![
            ("layout_version", Value::from(self.version as i64)),
            ("panel_count", Value::from(self.panel_count as i64)),
            ("layout", self.template.name().into()),
            ("active_panel", self.active_panel.clone().into()),
            (
                "split",
                Value::obj(vec![
                    (
                        "columns",
                        Value::Arr(
                            self.column_splits
                                .iter()
                                .map(|v| Value::from(*v as i64))
                                .collect(),
                        ),
                    ),
                    (
                        "rows",
                        Value::Arr(
                            self.row_splits
                                .iter()
                                .map(|v| Value::from(*v as i64))
                                .collect(),
                        ),
                    ),
                ]),
            ),
            ("panels", Value::Arr(panels)),
        ])
    }
    pub fn parse(text: &str) -> Self {
        let Some(v) = json::parse(text) else {
            return Self::default();
        };
        let Some(raw) = v.num("panel_count") else {
            return Self::default();
        };
        if raw.fract() != 0.0 || !(1.0..=4.0).contains(&raw) {
            return Self::default();
        }
        let count = raw as u8;
        if v.num("layout_version").is_some_and(|n| n > 1.0) {
            return Self::default();
        }
        let mut out = Self::for_count(count);
        if let Some(t) = v.str("layout").and_then(Template::parse) {
            if matches!(
                (count, t),
                (1, Template::Single)
                    | (2, Template::Columns)
                    | (3, Template::LeftOneRightTwo)
                    | (4, Template::Grid)
            ) {
                out.template = t
            }
        }
        if let Some(Value::Obj(_)) = v.get("split") {
            if let Some(cols) = v.get("split").and_then(|s| array_u16(s.get("columns"))) {
                let expected=if count==1 {1}else{2};
                if let Some(x) = Self::normalize_splits(&cols, expected) {
                    out.column_splits = x
                }
            }
            if let Some(rows) = v.get("split").and_then(|s| array_u16(s.get("rows"))) {
                let expected=match count{1=>1,2=>1,3=>2,_=>2};
                if let Some(x) = Self::normalize_splits(&rows, expected) {
                    out.row_splits = x
                }
            }
        }
        if let Some(Value::Arr(panels)) = v.get("panels") {
            for p in panels {
                let Some(id) = p.str("id").filter(|s| s.len() <= 40 && s.starts_with('p')) else {
                    continue;
                };
                if let Some(target) = out.panels.iter_mut().find(|x| x.id == id) {
                    target.box_id = p
                        .num("box_id")
                        .filter(|n| n.fract() == 0.0 && *n > 0.0)
                        .map(|n| n as i64);
                    target.space = p
                        .str("space")
                        .filter(|s| ["core", "basis", "reference", "temp"].contains(s))
                        .map(str::to_owned);
                    target.virtual_folder_id = p.str("virtual_folder_id").map(str::to_owned);
                    if let Some(s) = p.str("view").filter(|s| ["details", "icons"].contains(s)) {
                        target.view = s.into()
                    }
                    if let Some(s) = p.str("sort").filter(|s| s.len() <= 40) {
                        target.sort = s.into()
                    }
                    if let Some(s) = p.str("query").filter(|s| s.len() <= 4096) {
                        target.query = s.into()
                    }
                    if let Some(Value::Arr(items)) = p.get("selection") {
                        target.selection = items
                            .iter()
                            .filter_map(|v| {
                                if let Value::Str(s) = v {
                                    Some(s.clone())
                                } else {
                                    None
                                }
                            })
                            .take(5000)
                            .collect()
                    }
                    target.scroll = p
                        .num("scroll")
                        .filter(|n| n.fract() == 0.0 && *n >= 0.0)
                        .unwrap_or(0.0) as i64;
                }
            }
        }
        out.active_panel = v
            .str("active_panel")
            .filter(|id| out.panels.iter().any(|p| p.id == **id))
            .unwrap_or("p1")
            .into();
        out
    }
    /// 기존 설정에는 레이아웃이 없으므로 기존 단일 패널을 p1로 연다.
    pub fn migrate_legacy(text: &str) -> Self {
        let Some(v) = json::parse(text) else {
            return Self::default();
        };
        let mut l = Self::default();
        let mut p = Panel::new("p1");
        p.box_id = v
            .num("last_box")
            .filter(|n| n.fract() == 0.0 && *n > 0.0)
            .map(|n| n as i64);
        l.panels[0] = p;
        l
    }
}
fn array_u16(v: Option<&Value>) -> Option<Vec<u16>> {
    let Value::Arr(a) = v? else { return None };
    a.iter()
        .map(|x| {
            if let Value::Num(n) = x {
                if n.fract() == 0.0 && (1.0..=10000.0).contains(n) {
                    Some(*n as u16)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_templates_and_three_default() {
        for n in 1..=4 {
            assert_eq!(Layout::for_count(n).panel_count, n);
        }
        assert_eq!(Layout::for_count(1).template, Template::Single);
        assert_eq!(Layout::for_count(2).template, Template::Columns);
        assert_eq!(Layout::for_count(3).template, Template::LeftOneRightTwo);
        assert_eq!(Layout::for_count(4).template, Template::Grid);
    }
    #[test]
    fn layout_roundtrip_preserves_panel_context_and_ratios() {
        let mut l = Layout::for_count(4);
        l.panels[2].query = "한글".into();
        l.panels[2].selection = vec!["x".into()];
        l.column_splits = vec![6000, 4000];
        let got = Layout::parse(&json::to_string(&l.value()));
        assert_eq!(got, l);
    }
    #[test]
    fn invalid_values_recover_to_safe_defaults() {
        for s in [
            r#"{"panel_count":0}"#,
            r#"{"panel_count":5}"#,
            r#"{"layout_version":3,"panel_count":2}"#,
        ] {
            assert_eq!(Layout::parse(s), Layout::default());
        }
        let l = Layout::parse(
            r#"{"panel_count":3,"layout":"grid","active_panel":"bad","split":{"columns":[0,1]},"panels":[{"id":"p9"}]}"#,
        );
        assert_eq!(l.template, Template::LeftOneRightTwo);
        assert_eq!(l.active_panel, "p1");
    }
    #[test]
    fn old_settings_migrate_to_p1() {
        let l = Layout::migrate_legacy(r#"{"last_box":42,"hotkey":"Win+Shift+F"}"#);
        assert_eq!(l.panel_count, 1);
        assert_eq!(l.panels[0].id, "p1");
        assert_eq!(l.panels[0].box_id, Some(42));
    }
    #[test]
    fn split_normalization_is_exact() {
        assert_eq!(
            Layout::normalize_splits(&[1, 2, 1], 3)
                .unwrap()
                .iter()
                .map(|x| *x as u32)
                .sum::<u32>(),
            10000
        );
        assert!(Layout::normalize_splits(&[0, 1], 2).is_none());
    }
}
