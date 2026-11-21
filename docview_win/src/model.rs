//! 화면 데이터 모델(창과 무관한 순수 부분): 문서 항목·검색·정렬·표시 글자·확장자 색.
use crate::layout::Metrics;

#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub path: String,
    pub name: String,
    /// 소문자, 점 없음.
    pub ext: String,
    pub size: u64,
    /// 유닉스 초(UTC).
    pub mtime: f64,
    pub cloud: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Modified,
    Size,
    Ext,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [SortKey::Name, SortKey::Modified, SortKey::Size, SortKey::Ext];
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "이름",
            SortKey::Modified => "수정일",
            SortKey::Size => "크기",
            SortKey::Ext => "종류",
        }
    }
    pub fn next(self) -> SortKey {
        let i = Self::ALL.iter().position(|k| *k == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
    /// 처음 누르면 이름·종류는 오름차순, 수정일·크기는 큰 값(최신)이 먼저.
    pub fn default_descending(self) -> bool {
        matches!(self, SortKey::Modified | SortKey::Size)
    }
}

/// 문서 목록 + 보이는 순서(검색·정렬을 적용한 인덱스 목록).
#[derive(Clone, Debug)]
pub struct Collection {
    pub docs: Vec<Doc>,
    pub query: String,
    pub sort: SortKey,
    pub descending: bool,
    view: Vec<u32>,
}

impl Default for Collection {
    fn default() -> Self {
        Collection { docs: Vec::new(), query: String::new(), sort: SortKey::Name, descending: false, view: Vec::new() }
    }
}

impl Collection {
    pub fn new(docs: Vec<Doc>) -> Collection {
        let mut c = Collection { docs, ..Collection::default() };
        c.rebuild();
        c
    }
    pub fn replace(&mut self, docs: Vec<Doc>) {
        self.docs = docs;
        self.rebuild();
    }
    pub fn extend(&mut self, more: Vec<Doc>) {
        self.docs.extend(more);
        self.rebuild();
    }
    pub fn set_query(&mut self, q: &str) {
        self.query = q.to_string();
        self.rebuild();
    }
    pub fn set_sort(&mut self, key: SortKey, descending: bool) {
        self.sort = key;
        self.descending = descending;
        self.rebuild();
    }
    /// 정렬 기준을 다음으로 돌린다(같은 기준 안에서 방향은 기준의 기본값).
    pub fn cycle_sort(&mut self) {
        let k = self.sort.next();
        self.set_sort(k, k.default_descending());
    }
    pub fn toggle_direction(&mut self) {
        self.descending = !self.descending;
        self.rebuild();
    }
    pub fn total(&self) -> usize {
        self.docs.len()
    }
    pub fn len(&self) -> usize {
        self.view.len()
    }
    pub fn is_empty(&self) -> bool {
        self.view.is_empty()
    }
    /// 보이는 순서 `n` 번째 문서.
    pub fn get(&self, n: usize) -> Option<&Doc> {
        self.view.get(n).and_then(|i| self.docs.get(*i as usize))
    }
    fn rebuild(&mut self) {
        let q = self.query.trim().to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        let mut v: Vec<u32> = (0..self.docs.len() as u32)
            .filter(|i| {
                if terms.is_empty() {
                    return true;
                }
                let d = &self.docs[*i as usize];
                let hay = format!("{} {}", d.name.to_lowercase(), d.ext);
                terms.iter().all(|t| hay.contains(t))
            })
            .collect();
        let docs = &self.docs;
        match self.sort {
            SortKey::Modified => v.sort_by(|a, b| {
                let (x, y) = (&docs[*a as usize], &docs[*b as usize]);
                x.mtime.total_cmp(&y.mtime).then_with(|| x.path.cmp(&y.path))
            }),
            SortKey::Size => v.sort_by(|a, b| {
                let (x, y) = (&docs[*a as usize], &docs[*b as usize]);
                x.size.cmp(&y.size).then_with(|| x.path.cmp(&y.path))
            }),
            // 글 키는 한 번만 만들어 수만 항목에서도 비교마다 할당하지 않는다.
            key => {
                let mut keyed: Vec<(String, u32)> = v
                    .iter()
                    .map(|i| {
                        let d = &docs[*i as usize];
                        let k = if key == SortKey::Ext { format!("{}\u{0}{}", d.ext, d.name.to_lowercase()) } else { d.name.to_lowercase() };
                        (k, *i)
                    })
                    .collect();
                keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| docs[a.1 as usize].path.cmp(&docs[b.1 as usize].path)));
                v = keyed.into_iter().map(|(_, i)| i).collect();
            }
        }
        if self.descending {
            v.reverse();
        }
        self.view = v;
    }
}

/// Flutter `_formatBytes` 와 같은 규칙.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 유닉스 초 + 시간대 오프셋(초) → `YYYY-MM-DD`.
pub fn format_date(mtime: f64, offset_secs: i64) -> String {
    let secs = mtime.floor() as i64 + offset_secs;
    let days = secs.div_euclid(86_400);
    // Howard Hinnant 의 civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// 1234567 → "1,234,567".
pub fn group_digits(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 경과 초 → "m:ss" / "h:mm:ss".
pub fn format_elapsed(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// Flutter `_docExtColor` 와 같은 색(COLORREF, BGR).
pub fn ext_color(ext: &str) -> u32 {
    let rgb = |r: u8, g: u8, b: u8| ui_theme::rgb(r, g, b);
    match ext.to_lowercase().as_str() {
        "hwp" | "hwpx" => rgb(0x25, 0x63, 0xEB),
        "xls" | "xlsx" => rgb(0x16, 0xA3, 0x4A),
        "ppt" | "pptx" => rgb(0xDC, 0x26, 0x26),
        "pdf" => rgb(0xEA, 0x58, 0x0C),
        _ => rgb(0x64, 0x74, 0x8B),
    }
}

pub fn badge_text(ext: &str) -> String {
    if ext.is_empty() { "DOC".into() } else { ext.to_uppercase() }
}

/// 스캔 상태(화면 표시용).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanState {
    Idle,
    Running,
    Cancelling,
    Complete,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct ScanView {
    pub state: ScanState,
    pub visited: usize,
    pub folder: String,
    pub elapsed_secs: u64,
}

impl Default for ScanView {
    fn default() -> Self {
        ScanView { state: ScanState::Idle, visited: 0, folder: String::new(), elapsed_secs: 0 }
    }
}

impl ScanView {
    pub fn busy(&self) -> bool {
        matches!(self.state, ScanState::Running | ScanState::Cancelling)
    }
}

/// 키보드 이동.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

/// 창 한 개가 가진 화면 상태 전체(그리기 입력).
#[derive(Clone, Debug)]
pub struct App {
    pub coll: Collection,
    pub scan: ScanView,
    /// "klib scan" 등 선택된 엔진 이름.
    pub engine: String,
    pub thumb: f64,
    pub scroll: i32,
    pub selected: Option<usize>,
    pub search_focus: bool,
    pub excluded_count: usize,
    /// 시간대 오프셋(초).
    pub tz_offset: i64,
    /// 안내 줄(오류 등).
    pub notice: String,
}

impl App {
    pub fn new(thumb: f64) -> App {
        App {
            coll: Collection::default(),
            scan: ScanView::default(),
            engine: String::new(),
            thumb: crate::settings::clamp_thumb(thumb, crate::settings::DEFAULT_DOC_THUMB),
            scroll: 0,
            selected: None,
            search_focus: false,
            excluded_count: 0,
            tz_offset: 0,
            notice: String::new(),
        }
    }

    /// 스캔 띠는 스캔 중이고 이미 항목이 있을 때만 격자 위에 붙는다(비었으면 가운데 안내로 대신).
    pub fn banner_shown(&self) -> bool {
        self.scan.busy() && !self.coll.is_empty()
    }

    pub fn metrics(&self, w: i32, h: i32, dpi: u32) -> Metrics {
        Metrics::new(w, h, dpi, self.thumb, self.banner_shown())
    }

    pub fn clamp_scroll(&mut self, m: &Metrics) {
        self.scroll = self.scroll.clamp(0, m.max_scroll(self.coll.len()));
    }

    pub fn scroll_by(&mut self, m: &Metrics, delta: i32) {
        self.scroll += delta;
        self.clamp_scroll(m);
    }

    pub fn select(&mut self, m: &Metrics, n: Option<usize>) {
        self.selected = n.filter(|i| *i < self.coll.len());
        if let Some(i) = self.selected {
            self.scroll = m.scroll_to_show(i, self.scroll, self.coll.len());
        }
    }

    pub fn nav(&mut self, m: &Metrics, key: Nav) {
        let next = m.navigate(self.selected, key, self.coll.len());
        self.select(m, next);
    }

    /// 스캔 중 새 문서가 들어와 순서가 바뀌어도 선택은 같은 경로의 문서에 머문다.
    pub fn extend_docs(&mut self, more: Vec<Doc>) {
        let keep = self.selected_doc().map(|d| d.path.clone());
        self.coll.extend(more);
        self.selected = keep.and_then(|p| (0..self.coll.len()).find(|i| self.coll.get(*i).is_some_and(|d| d.path == p)));
    }

    pub fn selected_doc(&self) -> Option<&Doc> {
        self.selected.and_then(|i| self.coll.get(i))
    }

    /// 썸네일 크기가 바뀌면 같은 항목이 계속 보이도록 선택 항목(없으면 맨 위 줄)을 기준으로 스크롤을 다시 맞춘다.
    pub fn set_thumb(&mut self, thumb: f64, make_metrics: impl Fn(f64) -> Metrics) {
        let old = make_metrics(self.thumb);
        let anchor = self.selected.unwrap_or_else(|| old.first_visible(self.scroll, self.coll.len()));
        self.thumb = crate::settings::clamp_thumb(thumb, crate::settings::DEFAULT_DOC_THUMB);
        let new = make_metrics(self.thumb);
        self.scroll = new.row_top(anchor / new.cols.max(1));
        self.clamp_scroll(&new);
        if let Some(i) = self.selected {
            self.scroll = new.scroll_to_show(i, self.scroll, self.coll.len());
        }
    }
}
