//! 창 한 개가 가진 화면 상태 전체(그리기 입력). 문서뷰·사진뷰 공용: 격자 규칙(`GridSpec`)과 기본 썸네일 크기만 다르다.
use crate::items::{Collection, Doc};
use crate::layout::{GridSpec, Metrics};
use crate::state::{Nav, ScanView};
use crate::thumb::clamp_thumb;

#[derive(Clone, Debug)]
pub struct App {
    pub coll: Collection,
    pub scan: ScanView,
    /// "klib scan" 등 선택된 엔진 이름.
    pub engine: String,
    pub thumb: f64,
    pub scroll: i32,
    pub selected: Option<usize>,
    pub selection: crate::selection::Selection,
    /// 파일 작업이 끝난 뒤 늦게 도착하는 스캔 결과를 막는다. 새 스캔에서 비운다.
    pub retired_paths: std::collections::HashSet<String>,
    pub search_focus: bool,
    pub excluded_count: usize,
    /// 시간대 오프셋(초).
    pub tz_offset: i64,
    /// 안내 줄(오류 등).
    pub notice: String,
    /// 마지막 클릭이 누른 항목의 경로(더블클릭이 열 대상).
    pub clicked: Option<String>,
    /// 연결 앱으로 열기 실패 안내. 다음 선택 때 지운다.
    pub open_error: String,
    pub file_status: String,
    /// 격자 규칙(문서뷰·사진뷰).
    pub spec: GridSpec,
    /// 설정이 없거나 잘못됐을 때 쓰는 썸네일 크기(문서뷰 190·사진뷰 160).
    pub default_thumb: f64,
}

impl App {
    /// 문서뷰 규칙(`GridSpec::DOC`, 기본 190).
    pub fn new(thumb: f64) -> App {
        App::with_spec(GridSpec::DOC, 190.0, thumb)
    }

    pub fn with_spec(spec: GridSpec, default_thumb: f64, thumb: f64) -> App {
        App {
            coll: Collection::default(),
            scan: ScanView::default(),
            engine: String::new(),
            thumb: clamp_thumb(thumb, default_thumb),
            scroll: 0,
            selected: None,
            selection: Default::default(),
            retired_paths: Default::default(),
            search_focus: false,
            excluded_count: 0,
            tz_offset: 0,
            notice: String::new(),
            clicked: None,
            open_error: String::new(),
            file_status: String::new(),
            spec,
            default_thumb,
        }
    }

    /// 스캔 띠는 스캔 중이고 이미 항목이 있을 때만 격자 위에 붙는다(비었으면 가운데 안내로 대신).
    pub fn banner_shown(&self) -> bool {
        self.scan.busy() && !self.coll.is_empty()
    }

    pub fn metrics(&self, w: i32, h: i32, dpi: u32) -> Metrics {
        Metrics::with_spec(self.spec, w, h, dpi, self.thumb, self.banner_shown())
    }

    pub fn clamp_scroll(&mut self, m: &Metrics) {
        self.scroll = self.scroll.clamp(0, m.max_scroll(self.coll.len()));
    }

    pub fn scroll_by(&mut self, m: &Metrics, delta: i32) {
        self.scroll += delta;
        self.clamp_scroll(m);
    }

    pub fn select(&mut self, m: &Metrics, n: Option<usize>) {
        self.selection.clear();
        self.open_error.clear();
        self.selected = n.filter(|i| *i < self.coll.len());
        if let Some(i) = self.selected {
            self.selection.context(&self.coll.get(i).unwrap().path);
            self.scroll = m.scroll_to_show(i, self.scroll, self.coll.len());
        }
    }

    /// 격자 클릭: 선택하고 누른 항목을 기억한다. 선택이 화면을 스크롤해도 더블클릭은 이 항목을 연다.
    pub fn click(&mut self, m: &Metrics, n: Option<usize>) {
        self.click_modified(m, n, false, false, false);
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.selection.clear();
        self.clicked = None;
        self.open_error.clear();
    }

    pub fn is_selected(&self, n: usize) -> bool {
        // 기존 캡처 호출자는 selected를 직접 지정한다. 실제 입력에서 마지막 선택 해제는 초점도 비운다.
        self.coll.get(n).is_some_and(|d| self.selection.paths.contains(&d.path))
            || (self.selection.paths.is_empty() && self.selected == Some(n))
    }

    pub fn selected_docs(&self) -> Vec<Doc> {
        (0..self.coll.len()).filter(|i| self.is_selected(*i)).filter_map(|i| self.coll.get(i).cloned()).collect()
    }

    pub fn select_all(&mut self) {
        self.selection.all((0..self.coll.len()).filter_map(|i| self.coll.get(i).map(|d| d.path.clone())));
        self.clicked = None;
    }

    pub fn click_modified(&mut self, m: &Metrics, n: Option<usize>, ctrl: bool, shift: bool, right: bool) {
        self.open_error.clear();
        self.clicked = None;
        let Some(i) = n.filter(|i| *i < self.coll.len()) else {
            if !right && !ctrl && !shift { self.clear_selection(); }
            return;
        };
        let path = self.coll.get(i).unwrap().path.clone();
        if right {
            self.selection.context(&path);
        } else {
            // 보통 클릭에는 전체 경로 목록을 만들지 않는다.
            let order = if shift { (0..self.coll.len()).map(|i| self.coll.get(i).unwrap().path.clone()).collect() } else { Vec::new() };
            self.selection.click(&path, &order, ctrl, shift);
            if !ctrl && !shift { self.clicked = Some(path); }
        }
        self.selected = if self.selection.paths.is_empty() { None } else { Some(i) };
        if !right { self.scroll = m.scroll_to_show(i, self.scroll, self.coll.len()); }
    }

    /// Enter 로 열 경로. 검색칸 입력 중이면 없다.
    pub fn enter_target(&self) -> Option<String> {
        if self.search_focus || self.selection.paths.len() != 1 { None } else { self.selected_docs().first().map(|d| d.path.clone()) }
    }

    pub fn nav(&mut self, m: &Metrics, key: Nav) {
        let next = m.navigate(self.selected, key, self.coll.len());
        self.select(m, next);
    }

    /// 스캔 중 새 문서가 들어와 순서가 바뀌어도 선택은 같은 경로의 문서에 머문다.
    pub fn extend_docs(&mut self, more: Vec<Doc>) {
        let keep = self.selected_doc().map(|d| d.path.clone());
        self.coll.extend(more.into_iter().filter(|d| !self.retired_paths.contains(&d.path)).collect());
        self.selected = keep.and_then(|p| (0..self.coll.len()).find(|i| self.coll.get(*i).is_some_and(|d| d.path == p)));
    }

    pub fn apply_file_results(&mut self, op: crate::file_ops::Operation, results: &[crate::file_ops::FileResult], listed: impl Fn(&Doc) -> bool) {
        use crate::file_ops::Operation;
        if op == Operation::Clipboard { return; }
        let mut docs = std::mem::take(&mut self.coll.docs);
        let mut removed = std::collections::HashSet::new();
        let mut added = std::collections::HashMap::new();
        for result in results.iter().filter(|r| r.error.is_none()) {
            let path = &result.source.path;
            let was_selected = self.selection.paths.contains(path);
            if let Some(target) = &result.target {
                // 결과에서 직접 추가한 새 경로도 같은 스캔이 다시 넣지 않게 한다.
                self.retired_paths.insert(target.path.clone());
            }
            if op != Operation::Copy {
                self.retired_paths.insert(path.clone());
                removed.insert(path.clone());
                self.selection.paths.remove(path);
            }
            if let Some(target) = result.target.as_ref().filter(|d| listed(d)) {
                added.insert(target.path.clone(), target.clone());
                if op != Operation::Copy && was_selected { self.selection.paths.insert(target.path.clone()); }
            }
            if self.selection.anchor.as_ref() == Some(path) && op != Operation::Copy {
                self.selection.anchor = result.target.as_ref().filter(|d| listed(d)).map(|d| d.path.clone());
            }
        }
        // 파일마다 전체 목록을 순회하지 않고 한 번에 반영한다.
        docs.retain(|d| !removed.contains(&d.path) && !added.contains_key(&d.path));
        docs.extend(added.into_values());
        self.coll.replace(docs);
        self.selected = None;
        self.selected = (0..self.coll.len()).find(|i| self.is_selected(*i));
        self.clicked = None;
    }

    pub fn selected_doc(&self) -> Option<&Doc> {
        self.selected.and_then(|i| self.coll.get(i))
    }

    /// 썸네일 크기가 바뀌면 같은 항목이 계속 보이도록 선택 항목(없으면 맨 위 줄)을 기준으로 스크롤을 다시 맞춘다.
    pub fn set_thumb(&mut self, thumb: f64, make_metrics: impl Fn(f64) -> Metrics) {
        let old = make_metrics(self.thumb);
        let anchor = self.selected.unwrap_or_else(|| old.first_visible(self.scroll, self.coll.len()));
        self.thumb = clamp_thumb(thumb, self.default_thumb);
        let new = make_metrics(self.thumb);
        self.scroll = new.row_top(anchor / new.cols.max(1));
        self.clamp_scroll(&new);
        if let Some(i) = self.selected {
            self.scroll = new.scroll_to_show(i, self.scroll, self.coll.len());
        }
    }
}
