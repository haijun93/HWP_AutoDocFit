//! 몽땅클립보드 관리창(S18, UI1) 상태 기계 — 순수 함수, Win32 없음(창 없이 시험한다).
//!
//! Flutter `features/kclip/kclip_dialog.dart`의 `_KClipPanelState`에 흩어져 있던 암묵적 상태
//! (`busy`·`message`·`query`+디바운스 타이머·`category`·`minimized`·`maximized`·`isTop`·표 확인 집합·
//! 미리보기 대화상자·확인 대화상자)를 한곳의 [`Model`]과 [`step`]으로 드러낸다. 창(`manage.rs`)은
//! 사건을 넣고 돌아온 [`Effect`]만 실행한다(kvf `window.rs`의 "Model은 step 한 곳에서만 갱신" 구조).
//!
//! ## 상태
//! | 축 | 값 | 비고 |
//! |---|---|---|
//! | 불러오기 `load` | `Idle` · `Loading(seq)` · `Failed(글)` | 늦게 온 결과(seq 다름)는 버린다. 불러오는 중 다시 요청되면 `again`에 적어 두고 끝난 뒤 한 번 더 |
//! | 작업 `busy` | `None` · `Some(Job)` | Flutter `busy`와 같다: 작업 중에는 새 작업을 받지 않는다(무시) |
//! | 확인 `confirm` | `None` · `Clear{pinned}` · `Quit` | 확인 창이 떠 있는 동안 다른 작업 없음 |
//! | 상세 `detail` | `Closed` · `Loading(id)` · `Ready{..}` · `Failed{..}` | Flutter 항목 누름 대화상자(글·그림 + 닫기·다시 복사) |
//! | 화면 `section` | `List` · `Settings` | Flutter는 한 열에 다 있었다. 같은 기능을 두 쪽으로 나눔 |
//! | 창 | `collapsed`·`maximized`·`topmost` | 머리글 단추 |
//! | 찾기 | `typed`(입력 칸) → 200ms 뒤 `query`(불러오기에 쓰는 값) | 디바운스 |
//!
//! ## 전이(요약, 시험 `tests` 참고)
//! | 지금 | 사건 | 다음 · 효과 |
//! |---|---|---|
//! | 아무 때 | `Open` | 처음 상태로 되돌리고 `Load` |
//! | `load=Loading(s)` | `Loaded{seq:s}` | `Idle`(또는 `Failed`), `again`이면 다시 `Load`, 표 확인이 필요한 항목마다 `CheckTable` |
//! | `load=Loading(s)` | `Loaded{seq≠s}` | 무시 |
//! | `load=Loading` | `Reload`·`External` | `again=true`(겹쳐 불러오지 않음) |
//! | `busy=None`·`confirm=None` | `Do(job)` | `busy=Some(job)`, `Run(job)` |
//! | `busy=Some`·또는 `confirm=Some` | `Do(job)` | 무시(불가능한 전이) |
//! | `busy=Some(j)` | `Done{..}` | `busy=None`, 글 표시, 메모 성공이면 `ClearMemo`, `Load` |
//! | `busy=None` | `Done` | 무시(늦게 온 결과) |
//! | `confirm=None`·`busy=None` | `AskClear` | `confirm=Clear{고정 수}`, `Confirm` |
//! | `confirm=Some(c)` | `Answer(true)` | `confirm=None` 후 c의 작업을 `Do` |
//! | `detail=Closed` | `OpenDetail(i)` | `Loading(id)`, `LoadDetail(id)` |
//! | `detail=Loading(id)` | `DetailLoaded{id}` | `Ready`/`Failed`; id가 다르면 무시 |
//! | `detail≠Closed` | `Escape`·`CloseDetail` | `Closed` |
//! | `detail=Ready{id}` | `Recopy` | `Closed`, `Do(Restore{id})` |
//! | 찾기 글 있음 | `Escape` | 찾기 지움(`Load`) |

use std::collections::{HashMap, HashSet};

use crate::category::Category;
use crate::json::Value;
use crate::store::{Settings, Summary};

/// 불러올 항목 수(Flutter `list` limit 200과 같음).
pub const LIST_LIMIT: usize = 200;
/// 찾기 입력 디바운스(Flutter와 같은 200ms).
pub const DEBOUNCE_MS: u32 = 200;
/// Flutter '일시정지 시간' 메뉴(30분·1시간). 빠른 목록 설정 메뉴의 3시간도 함께 둔다.
pub const PAUSE_MINUTES: [u32; 3] = [30, 60, 180];

/// 분류 단추 순서(Flutter `kclipCategories` = Rust `Category::ALL`).
pub const CATEGORIES: [Category; 7] = [
    Category::All, Category::Document, Category::Image, Category::File, Category::Folder, Category::Web, Category::Capture,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    List,
    Settings,
}

/// 여는 길: 앱 왼쪽 '몽땅클립보드' 단추·트레이(Sidebar), 앱 안 Ctrl+Shift+V(Hotkey — 메모 칸에 포커스).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    Sidebar,
    Hotkey,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Load {
    Idle,
    Loading(u64),
    Failed(String),
}

/// 화면캡처 폴더(설정 `capture_dirs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureDir {
    pub path: String,
    pub label: String,
    /// watching·off·paused·all_off.
    pub state: String,
}

impl CaptureDir {
    pub fn state_text(&self) -> &'static str {
        match self.state.as_str() {
            "watching" => "감시 중",
            "off" => "끔",
            "paused" => "일시정지",
            "all_off" => "전체 꺼짐",
            _ => "",
        }
    }
}

/// 작업 스레드가 읽어 온 한 번의 상태(목록·설정·기록 상태).
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub items: Vec<Summary>,
    pub settings: Settings,
    pub capture_dirs: Vec<CaptureDir>,
    pub autostart: Option<bool>,
    pub count: i64,
    pub hotkey_ok: bool,
    pub resume_in_min: Option<u64>,
}

/// 작업(Flutter `_act`·`_call`이 하던 일). 한 번에 하나.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Restore { id: i64 },
    Pin { id: i64, pinned: bool },
    Delete { id: i64 },
    /// 모두 지우기(고정 항목 남김).
    Clear,
    /// 일시정지(None = 다시 켤 때까지).
    Pause(Option<u32>),
    Resume,
    /// 몽땅클립보드 끄기(kagent 종료).
    Quit,
    /// settings_set과 같은 키·단위.
    Settings(Value),
    Autostart(bool),
    AddMemo(String),
    /// 끌어 놓은 파일·폴더·그림·웹 링크.
    AddFiles(Vec<String>),
    /// 클립보드 엑셀 표를 한/글 표로(앱 서비스에 맡김).
    ExcelTable,
    /// 표 항목을 음영 없이 복사(앱 서비스에 맡김).
    CopyNoShade { id: i64 },
    OpenWeb { id: i64 },
}

impl Job {
    /// 성공했을 때 보일 글(Flutter와 같은 글). None이면 결과 글을 그대로.
    pub fn done_text(&self) -> Option<&'static str> {
        match self {
            Job::Restore { .. } => Some("클립보드에 올렸습니다. Ctrl+V로 붙여넣으세요."),
            Job::Clear => Some("고정하지 않은 항목을 지웠습니다."),
            Job::AddMemo(_) => Some("메모를 클립보드 항목에 추가했습니다."),
            Job::Quit => Some("몽땅클립보드를 껐습니다."),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    /// 모두 지우기(고정한 항목 수).
    Clear { pinned: usize },
    Quit,
}

impl Confirm {
    /// 확인 창 글(Flutter `_confirmClearAll`과 같은 글, 끄기는 빠른 목록 설정 메뉴와 같은 글).
    pub fn text(self) -> String {
        match self {
            Confirm::Clear { pinned } if pinned > 0 => {
                format!("고정한 항목 {pinned}개는 남고 나머지를 지웁니다.\n고정을 풀면 다음에 지워집니다.")
            }
            Confirm::Clear { .. } => "고정한 항목은 남고 나머지를 지웁니다.".into(),
            Confirm::Quit => "몽땅클립보드를 끌까요?\n\n복사 기록과 빠른 목록 단축키가 멈춥니다. 기록은 지워지지 않으며, \
                              앱의 몽땅클립보드 화면에서 다시 켤 수 있습니다."
                .into(),
        }
    }
    fn job(self) -> Job {
        match self {
            Confirm::Clear { .. } => Job::Clear,
            Confirm::Quit => Job::Quit,
        }
    }
}

/// 상세 보기 내용.
#[derive(Clone, Debug, PartialEq)]
pub enum DetailBody {
    Text(String),
    /// 그림(픽셀은 창이 따로 가진다).
    Image { w: u32, h: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Detail {
    Closed,
    Loading { id: i64, kind: String },
    Ready { id: i64, kind: String, body: DetailBody },
    Failed { id: i64, kind: String },
}

impl Detail {
    pub fn id(&self) -> Option<i64> {
        match self {
            Detail::Closed => None,
            Detail::Loading { id, .. } | Detail::Ready { id, .. } | Detail::Failed { id, .. } => Some(*id),
        }
    }
    pub fn is_open(&self) -> bool {
        *self != Detail::Closed
    }
}

/// 종류 이름(Flutter `_kind`).
pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "text" => "글",
        "image" => "그림",
        "files" => "파일",
        _ => "?",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Open(OpenMode),
    Loaded { seq: u64, result: Result<Box<Snapshot>, String> },
    /// 다른 곳(빠른 목록·트레이·새 복사·앱)에서 저장소나 설정이 바뀜.
    External,
    Category(Category),
    /// 찾기 칸 글이 바뀜(디바운스 시작).
    Typed(String),
    DebounceFired,
    Section(Section),
    /// 보이는 목록(분류로 거른 것) 안의 위치.
    Select(usize),
    Move(isize),
    /// 보이는 줄 수가 바뀜(창 크기).
    Rows(usize),
    Scroll(isize),
    ScrollTo(usize),
    OpenDetail(usize),
    DetailLoaded { id: i64, result: Result<DetailBody, String> },
    CloseDetail,
    Recopy,
    Do(Job),
    Done { ok: bool, message: String },
    AskClear,
    AskQuit,
    Answer(bool),
    TableChecked { id: i64, table: bool },
    ToggleTop,
    ToggleCollapse,
    ToggleMax,
    Escape,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Load { seq: u64, query: String },
    /// 디바운스 타이머(다시 걸면 이전 것은 취소).
    Debounce,
    Run(Job),
    Confirm(Confirm),
    LoadDetail { id: i64, kind: String },
    CheckTable(i64),
    Topmost(bool),
    Collapse(bool),
    Maximize(bool),
    /// 찾기 칸 비우기(Escape).
    ClearSearch,
    /// 메모 칸 비우기(메모 추가 성공).
    ClearMemo,
    /// 분류·창 위치 저장.
    Save,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub section: Section,
    /// 불러온 항목(고정 먼저, 그 안은 최근 사용 순).
    pub items: Vec<Summary>,
    pub category: Category,
    pub typed: String,
    pub query: String,
    pub load: Load,
    pub seq: u64,
    pub again: bool,
    pub busy: Option<Job>,
    pub confirm: Option<Confirm>,
    pub detail: Detail,
    pub message: String,
    /// 보이는 목록 안의 고른 줄·첫 줄·줄 수.
    pub sel: usize,
    pub top: usize,
    pub rows: usize,
    pub collapsed: bool,
    pub maximized: bool,
    pub topmost: bool,
    pub settings: Settings,
    pub capture_dirs: Vec<CaptureDir>,
    pub autostart: Option<bool>,
    pub count: i64,
    pub hotkey_ok: bool,
    pub resume_in_min: Option<u64>,
    /// 엑셀 표 확인 결과(id → 표인지)와 확인 중인 id.
    pub tables: HashMap<i64, bool>,
    pub checking: HashSet<i64>,
    /// 처음 불러오기가 끝났는지(그 전에는 '불러오는 중').
    pub loaded_once: bool,
}

impl Default for Model {
    fn default() -> Self {
        Model {
            section: Section::List,
            items: Vec::new(),
            category: Category::All,
            typed: String::new(),
            query: String::new(),
            load: Load::Idle,
            seq: 0,
            again: false,
            busy: None,
            confirm: None,
            detail: Detail::Closed,
            message: String::new(),
            sel: 0,
            top: 0,
            rows: 1,
            collapsed: false,
            maximized: false,
            topmost: false,
            settings: Settings::default(),
            capture_dirs: Vec::new(),
            autostart: None,
            count: 0,
            hotkey_ok: true,
            resume_in_min: None,
            tables: HashMap::new(),
            checking: HashSet::new(),
            loaded_once: false,
        }
    }
}

/// 고정 항목을 먼저(Flutter `items.sort(pinned)`, 안정 정렬).
pub fn order(items: Vec<Summary>) -> Vec<Summary> {
    let (mut pinned, rest): (Vec<_>, Vec<_>) = items.into_iter().partition(|i| i.pinned);
    pinned.extend(rest);
    pinned
}

/// 엑셀 표일 수 있는 항목(Flutter `_maybeTable`).
pub fn maybe_table(item: &Summary) -> bool {
    item.kind == "text" && item.preview.contains("서식 포함")
}

/// 웹 글 항목(두 번 누르면 브라우저로, Flutter `_isWebText`).
pub fn is_web_text(item: &Summary) -> bool {
    let p = item.preview.trim();
    item.preview.contains("· 웹 ·")
        || ((p.starts_with("http://") || p.starts_with("https://")) && p.len() > 8 && !p.contains(char::is_whitespace))
}

impl Model {
    /// 분류로 거른 항목 위치(Flutter `visibleItems`).
    pub fn visible(&self) -> Vec<usize> {
        self.items.iter().enumerate().filter(|(_, i)| i.cats.contains(self.category)).map(|(n, _)| n).collect()
    }
    pub fn visible_item(&self, i: usize) -> Option<&Summary> {
        self.visible().get(i).map(|n| &self.items[*n])
    }
    pub fn selected(&self) -> Option<&Summary> {
        self.visible_item(self.sel)
    }
    pub fn pinned_count(&self) -> usize {
        self.items.iter().filter(|i| i.pinned).count()
    }
    pub fn paused(&self) -> bool {
        self.settings.paused
    }
    /// 기록 상태 글(Flutter Chip).
    pub fn status_text(&self) -> String {
        if self.paused() {
            match self.resume_in_min {
                Some(m) => format!("일시정지 · {}분 뒤 다시 기록", m.max(1)),
                None => "일시정지".into(),
            }
        } else {
            format!("기록 중 · {}개", self.count)
        }
    }
    pub fn hotkey_text(&self) -> String {
        format!("{}{}", self.settings.hotkey, if self.hotkey_ok { "" } else { " (사용 불가)" })
    }
    fn clamp_sel(&mut self) {
        let len = self.visible().len();
        self.sel = self.sel.min(len.saturating_sub(1));
        let max_top = len.saturating_sub(self.rows.max(1));
        self.top = self.top.min(max_top);
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + self.rows.max(1) {
            self.top = self.sel + 1 - self.rows.max(1);
        }
    }
    fn start_load(&mut self) -> Vec<Effect> {
        if matches!(self.load, Load::Loading(_)) {
            self.again = true;
            return vec![];
        }
        self.seq += 1;
        self.load = Load::Loading(self.seq);
        vec![Effect::Load { seq: self.seq, query: self.query.clone() }]
    }
    fn table_checks(&mut self) -> Vec<Effect> {
        let ids: Vec<i64> = self
            .items
            .iter()
            .filter(|i| maybe_table(i) && !self.tables.contains_key(&i.id) && !self.checking.contains(&i.id))
            .map(|i| i.id)
            .collect();
        ids.into_iter()
            .map(|id| {
                self.checking.insert(id);
                Effect::CheckTable(id)
            })
            .collect()
    }
    fn begin(&mut self, job: Job) -> Vec<Effect> {
        if self.busy.is_some() || self.confirm.is_some() {
            return vec![];
        }
        self.busy = Some(job.clone());
        vec![Effect::Run(job)]
    }
}

/// 상태 전이 한 곳.
pub fn step(m: &mut Model, ev: Event) -> Vec<Effect> {
    match ev {
        Event::Open(mode) => {
            let keep = (m.topmost, m.category);
            let rows = m.rows;
            let seq = m.seq;
            *m = Model { topmost: keep.0, category: keep.1, rows, seq, ..Model::default() };
            let _ = mode; // 포커스 위치는 창이 정한다(Hotkey = 메모 칸).
            m.start_load()
        }
        Event::Loaded { seq, result } => {
            if m.load != Load::Loading(seq) {
                return vec![];
            }
            m.load = Load::Idle;
            match result {
                Ok(s) => {
                    let prev = m.selected().map(|i| i.id);
                    let s = *s;
                    m.items = order(s.items);
                    m.settings = s.settings;
                    m.capture_dirs = s.capture_dirs;
                    m.autostart = s.autostart;
                    m.count = s.count;
                    m.hotkey_ok = s.hotkey_ok;
                    m.resume_in_min = s.resume_in_min;
                    m.loaded_once = true;
                    // 지운 항목의 표 확인 결과는 버린다.
                    let live: HashSet<i64> = m.items.iter().map(|i| i.id).collect();
                    m.tables.retain(|id, _| live.contains(id));
                    if let Some(id) = prev {
                        if let Some(p) = m.visible().iter().position(|n| m.items[*n].id == id) {
                            m.sel = p;
                        }
                    }
                    m.clamp_sel();
                }
                Err(e) => {
                    m.load = Load::Failed(e);
                    m.loaded_once = true;
                }
            }
            let mut out = m.table_checks();
            if std::mem::take(&mut m.again) {
                out.extend(m.start_load());
            }
            out
        }
        Event::External => m.start_load(),
        Event::Category(c) => {
            if m.category == c {
                return vec![];
            }
            m.category = c;
            m.sel = 0;
            m.top = 0;
            vec![Effect::Save]
        }
        Event::Typed(s) => {
            if s == m.typed {
                return vec![];
            }
            m.typed = s;
            vec![Effect::Debounce]
        }
        Event::DebounceFired => {
            let q = m.typed.trim().to_string();
            if q == m.query {
                return vec![];
            }
            m.query = q;
            m.sel = 0;
            m.top = 0;
            m.start_load()
        }
        Event::Section(s) => {
            m.section = s;
            vec![]
        }
        Event::Select(i) => {
            if i < m.visible().len() {
                m.sel = i;
                m.clamp_sel();
            }
            vec![]
        }
        Event::Move(d) => {
            let len = m.visible().len();
            if len > 0 {
                m.sel = (m.sel as isize + d).clamp(0, len as isize - 1) as usize;
                m.clamp_sel();
            }
            vec![]
        }
        Event::Rows(r) => {
            m.rows = r.max(1);
            m.clamp_sel();
            vec![]
        }
        Event::Scroll(d) => {
            let max_top = m.visible().len().saturating_sub(m.rows.max(1));
            m.top = (m.top as isize + d).clamp(0, max_top as isize) as usize;
            vec![]
        }
        Event::ScrollTo(t) => {
            let max_top = m.visible().len().saturating_sub(m.rows.max(1));
            m.top = t.min(max_top);
            vec![]
        }
        Event::OpenDetail(i) => {
            if m.detail.is_open() {
                return vec![];
            }
            let Some(item) = m.visible_item(i) else { return vec![] };
            let (id, kind) = (item.id, item.kind.clone());
            m.sel = i;
            m.clamp_sel();
            m.detail = Detail::Loading { id, kind: kind.clone() };
            vec![Effect::LoadDetail { id, kind }]
        }
        Event::DetailLoaded { id, result } => {
            let Detail::Loading { id: want, kind } = &m.detail else { return vec![] };
            if *want != id {
                return vec![];
            }
            let kind = kind.clone();
            m.detail = match result {
                Ok(body) => Detail::Ready { id, kind, body },
                Err(_) => Detail::Failed { id, kind },
            };
            vec![]
        }
        Event::CloseDetail => {
            m.detail = Detail::Closed;
            vec![]
        }
        Event::Recopy => {
            let Detail::Ready { id, .. } = m.detail else { return vec![] };
            if m.busy.is_some() {
                return vec![];
            }
            m.detail = Detail::Closed;
            m.begin(Job::Restore { id })
        }
        Event::Do(job) => m.begin(job),
        Event::Done { ok, message } => {
            let Some(job) = m.busy.take() else { return vec![] };
            let mut out = Vec::new();
            m.message = match (ok, job.done_text()) {
                (true, Some(t)) => t.to_string(),
                _ => message,
            };
            if ok {
                if let Job::AddMemo(_) = job {
                    out.push(Effect::ClearMemo);
                }
            }
            // 열기·확인만 하는 일은 목록이 바뀌지 않는다.
            if !matches!(job, Job::OpenWeb { .. } | Job::Quit) {
                out.extend(m.start_load());
            }
            out
        }
        Event::AskClear => {
            if m.busy.is_some() || m.confirm.is_some() {
                return vec![];
            }
            let c = Confirm::Clear { pinned: m.pinned_count() };
            m.confirm = Some(c);
            vec![Effect::Confirm(c)]
        }
        Event::AskQuit => {
            if m.busy.is_some() || m.confirm.is_some() {
                return vec![];
            }
            m.confirm = Some(Confirm::Quit);
            vec![Effect::Confirm(Confirm::Quit)]
        }
        Event::Answer(yes) => {
            let Some(c) = m.confirm.take() else { return vec![] };
            if yes { m.begin(c.job()) } else { vec![] }
        }
        Event::TableChecked { id, table } => {
            m.checking.remove(&id);
            if m.items.iter().any(|i| i.id == id) {
                m.tables.insert(id, table);
            }
            vec![]
        }
        Event::ToggleTop => {
            m.topmost = !m.topmost;
            vec![Effect::Topmost(m.topmost), Effect::Save]
        }
        Event::ToggleCollapse => {
            m.collapsed = !m.collapsed;
            vec![Effect::Collapse(m.collapsed)]
        }
        Event::ToggleMax => {
            m.maximized = !m.maximized;
            vec![Effect::Maximize(m.maximized)]
        }
        Event::Escape => {
            if m.detail.is_open() {
                m.detail = Detail::Closed;
                vec![]
            } else if !m.typed.is_empty() {
                m.typed.clear();
                let mut out = vec![Effect::ClearSearch];
                if !m.query.is_empty() {
                    m.query.clear();
                    m.sel = 0;
                    m.top = 0;
                    out.extend(m.start_load());
                }
                out
            } else {
                vec![]
            }
        }
    }
}

/// 설정 화면 칸(Flutter `_settings`·`_captureFolders`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Autostart,
    MaxItems,
    MaxTotalMb,
    MaxItemMb,
    MaxAgeDays,
    Sticky,
    AlwaysOnTop,
    AutoPaste,
    ExcelTarget,
    CaptureAll,
    CaptureDir(usize),
}

/// 설정 화면에 보이는 칸 순서.
pub fn setting_rows(m: &Model) -> Vec<Setting> {
    let mut v = vec![
        Setting::Autostart, Setting::MaxItems, Setting::MaxTotalMb, Setting::MaxItemMb, Setting::MaxAgeDays,
        Setting::Sticky, Setting::AlwaysOnTop, Setting::AutoPaste, Setting::ExcelTarget, Setting::CaptureAll,
    ];
    v.extend((0..m.capture_dirs.len()).map(Setting::CaptureDir));
    v
}

/// 숫자 칸의 (설정 키, 이름, 단위, 선택지, 지금 값).
pub fn number_spec(s: Setting, cfg: &Settings) -> Option<(&'static str, &'static str, &'static [i64], i64)> {
    use crate::settings_menu::{MAX_AGE_DAYS, MAX_ITEM_MB, MAX_ITEMS, MAX_TOTAL_MB};
    Some(match s {
        Setting::MaxItems => ("max_items", "보관 개수", &MAX_ITEMS, cfg.max_items),
        Setting::MaxTotalMb => ("max_total_mb", "최대 MB", &MAX_TOTAL_MB, cfg.max_total_bytes / 1024 / 1024),
        Setting::MaxItemMb => ("max_item_mb", "항목 하나 최대 MB", &MAX_ITEM_MB, cfg.max_item_bytes / 1024 / 1024),
        Setting::MaxAgeDays => ("max_age_days", "보관 일", &MAX_AGE_DAYS, cfg.max_age_days.round() as i64),
        _ => return None,
    })
}

/// 켜고 끄는 칸의 (설정 키, 이름, 지금 값).
pub fn flag_spec(s: Setting, cfg: &Settings) -> Option<(&'static str, &'static str, bool)> {
    Some(match s {
        Setting::Sticky => ("sticky_popup", "스티커 메모처럼 화면에 유지(빠른 목록)", cfg.sticky_popup),
        Setting::AlwaysOnTop => ("always_on_top", "항상 위(빠른 목록)", cfg.always_on_top),
        Setting::AutoPaste => ("auto_paste", "고르면 바로 붙여넣기", cfg.auto_paste),
        Setting::ExcelTarget => ("excel_target_paste", "엑셀 표는 목표 결과로 붙이기", cfg.excel_target_paste),
        Setting::CaptureAll => ("watch_screenshots", "화면캡처 폴더 자동 인식(새 캡처 그림을 경로로 기록)", cfg.watch_screenshots),
        _ => return None,
    })
}

/// 설정 칸을 누른 결과 작업(숫자 칸은 값 고르기가 따로 있어 None).
pub fn toggle_job(m: &Model, s: Setting) -> Option<Job> {
    match s {
        Setting::Autostart => Some(Job::Autostart(m.autostart != Some(true))),
        Setting::CaptureDir(i) => {
            if !m.settings.watch_screenshots {
                return None; // 전체가 꺼져 있으면 폴더별 칸은 사용 안 함(Flutter와 같음)
            }
            let d = m.capture_dirs.get(i)?;
            let path = d.path.to_lowercase();
            let off: Vec<String> = m.settings.capture_dirs_off.iter().map(|p| p.to_lowercase()).collect();
            let was_on = !off.contains(&path);
            let mut next: Vec<Value> = off.iter().filter(|p| **p != path).map(|p| Value::Str(p.clone())).collect();
            if was_on {
                next.push(Value::Str(path));
            }
            Some(Job::Settings(Value::obj(vec![("capture_dirs_off", Value::Arr(next))])))
        }
        _ => {
            let (key, _, on) = flag_spec(s, &m.settings)?;
            Some(Job::Settings(Value::obj(vec![(key, (!on).into())])))
        }
    }
}

/// 폴더별 칸이 켜져 있는지.
pub fn capture_dir_on(m: &Model, i: usize) -> bool {
    m.capture_dirs.get(i).is_some_and(|d| {
        let path = d.path.to_lowercase();
        !m.settings.capture_dirs_off.iter().any(|p| p.to_lowercase() == path)
    })
}

/// 숫자 칸에서 값을 고른 작업.
pub fn number_job(s: Setting, value: i64) -> Option<Job> {
    let (key, ..) = number_spec(s, &Settings::default())?;
    Some(Job::Settings(Value::obj(vec![(key, value.into())])))
}

/// 숫자 칸 표시(선택지 단위).
pub fn number_text(s: Setting, v: i64) -> String {
    match s {
        Setting::MaxItems => format!("{v}개"),
        Setting::MaxAgeDays if v == 365 => "1년".into(),
        Setting::MaxAgeDays => format!("{v}일"),
        _ => format!("{v}MB"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::CategorySet;

    fn item(id: i64, kind: &str, preview: &str, pinned: bool, cats: &[Category]) -> Summary {
        Summary { id, kind: kind.into(), preview: preview.into(), pinned, used: 0.0, size: 10, cats: CategorySet::of(cats) }
    }

    fn snap(items: Vec<Summary>) -> Result<Box<Snapshot>, String> {
        Ok(Box::new(Snapshot {
            items,
            settings: Settings::default(),
            capture_dirs: vec![CaptureDir { path: "C:\\Shots".into(), label: "캡처 도구".into(), state: "watching".into() }],
            autostart: Some(false),
            count: 3,
            hotkey_ok: true,
            resume_in_min: None,
        }))
    }

    fn sample() -> Vec<Summary> {
        vec![
            item(1, "text", "첫 글", false, &[Category::All]),
            item(2, "image", "그림", true, &[Category::All, Category::Image]),
            item(3, "text", "서식 포함 · 표", false, &[Category::All, Category::Document]),
        ]
    }

    fn opened() -> Model {
        let mut m = Model::default();
        let fx = step(&mut m, Event::Open(OpenMode::Sidebar));
        assert_eq!(fx, vec![Effect::Load { seq: 1, query: String::new() }]);
        let fx = step(&mut m, Event::Loaded { seq: 1, result: snap(sample()) });
        assert_eq!(fx, vec![Effect::CheckTable(3)], "서식 포함 글만 표 확인");
        m
    }

    #[test]
    fn open_loads_and_orders_pinned_first() {
        let m = opened();
        assert_eq!(m.load, Load::Idle);
        assert_eq!(m.items.iter().map(|i| i.id).collect::<Vec<_>>(), [2, 1, 3]);
        assert_eq!(m.status_text(), "기록 중 · 3개");
        assert!(m.loaded_once);
    }

    #[test]
    fn stale_or_overlapping_loads_never_overwrite_newer_state() {
        let mut m = opened();
        assert_eq!(step(&mut m, Event::External), vec![Effect::Load { seq: 2, query: String::new() }]);
        // 불러오는 중 다시 요청 → 겹치지 않고 끝난 뒤 한 번 더.
        assert_eq!(step(&mut m, Event::External), vec![]);
        assert!(m.again);
        // 옛 순번 결과는 버림.
        assert_eq!(step(&mut m, Event::Loaded { seq: 1, result: snap(vec![]) }), vec![]);
        assert_eq!(m.items.len(), 3);
        let fx = step(&mut m, Event::Loaded { seq: 2, result: snap(sample()) });
        assert_eq!(fx, vec![Effect::Load { seq: 3, query: String::new() }], "3번 항목은 이미 확인 중이라 다시 묻지 않음");
        // 실패해도 이전 목록은 남긴다(빠른 목록과 같은 원칙).
        step(&mut m, Event::Loaded { seq: 3, result: Err("database is locked".into()) });
        assert_eq!(m.load, Load::Failed("database is locked".into()));
        assert_eq!(m.items.len(), 3);
    }

    #[test]
    fn busy_rejects_new_jobs_until_done_then_reloads() {
        let mut m = opened();
        assert_eq!(step(&mut m, Event::Do(Job::Pin { id: 1, pinned: true })), vec![Effect::Run(Job::Pin { id: 1, pinned: true })]);
        // 작업 중 다른 작업·확인은 받지 않는다(Flutter busy).
        assert_eq!(step(&mut m, Event::Do(Job::Delete { id: 1 })), vec![]);
        assert_eq!(step(&mut m, Event::AskClear), vec![]);
        let fx = step(&mut m, Event::Done { ok: true, message: String::new() });
        assert_eq!(fx, vec![Effect::Load { seq: 2, query: String::new() }]);
        assert!(m.busy.is_none());
        // 늦게 온 완료는 무시.
        assert_eq!(step(&mut m, Event::Done { ok: true, message: "x".into() }), vec![]);
    }

    #[test]
    fn done_messages_match_flutter_and_memo_is_cleared_only_on_success() {
        let mut m = opened();
        step(&mut m, Event::Do(Job::AddMemo("메모".into())));
        let fx = step(&mut m, Event::Done { ok: true, message: String::new() });
        assert_eq!(fx[0], Effect::ClearMemo);
        assert_eq!(m.message, "메모를 클립보드 항목에 추가했습니다.");
        step(&mut m, Event::Loaded { seq: 2, result: snap(sample()) });
        step(&mut m, Event::Do(Job::AddMemo("메모".into())));
        let fx = step(&mut m, Event::Done { ok: false, message: "기록이 일시정지되어 있어 메모를 추가하지 못했습니다.".into() });
        assert!(!fx.contains(&Effect::ClearMemo));
        assert_eq!(m.message, "기록이 일시정지되어 있어 메모를 추가하지 못했습니다.");
        step(&mut m, Event::Loaded { seq: 3, result: snap(sample()) });
        step(&mut m, Event::Do(Job::Restore { id: 1 }));
        step(&mut m, Event::Done { ok: true, message: String::new() });
        assert_eq!(m.message, "클립보드에 올렸습니다. Ctrl+V로 붙여넣으세요.");
        // 웹 열기는 목록을 다시 읽지 않는다.
        step(&mut m, Event::Loaded { seq: 4, result: snap(sample()) });
        step(&mut m, Event::Do(Job::OpenWeb { id: 1 }));
        assert_eq!(step(&mut m, Event::Done { ok: true, message: "기본 브라우저에서 웹 링크를 열었습니다.".into() }), vec![]);
    }

    #[test]
    fn clear_all_needs_confirmation_and_counts_pinned() {
        let mut m = opened();
        let fx = step(&mut m, Event::AskClear);
        assert_eq!(fx, vec![Effect::Confirm(Confirm::Clear { pinned: 1 })]);
        assert!(Confirm::Clear { pinned: 1 }.text().starts_with("고정한 항목 1개는 남고"));
        assert_eq!(Confirm::Clear { pinned: 0 }.text(), "고정한 항목은 남고 나머지를 지웁니다.");
        // 확인 창이 떠 있는 동안 다른 작업 없음.
        assert_eq!(step(&mut m, Event::Do(Job::Delete { id: 1 })), vec![]);
        assert_eq!(step(&mut m, Event::Answer(false)), vec![]);
        assert!(m.confirm.is_none() && m.busy.is_none());
        step(&mut m, Event::AskClear);
        assert_eq!(step(&mut m, Event::Answer(true)), vec![Effect::Run(Job::Clear)]);
        // 대답이 두 번 와도 한 번만.
        assert_eq!(step(&mut m, Event::Answer(true)), vec![]);
        step(&mut m, Event::Done { ok: true, message: String::new() });
        assert_eq!(m.message, "고정하지 않은 항목을 지웠습니다.");
        // 끄기도 확인 후에만.
        step(&mut m, Event::Loaded { seq: 2, result: snap(sample()) });
        assert_eq!(step(&mut m, Event::AskQuit), vec![Effect::Confirm(Confirm::Quit)]);
        assert_eq!(step(&mut m, Event::Answer(true)), vec![Effect::Run(Job::Quit)]);
    }

    #[test]
    fn search_is_debounced_and_escape_clears_it() {
        let mut m = opened();
        assert_eq!(step(&mut m, Event::Typed("회".into())), vec![Effect::Debounce]);
        assert_eq!(step(&mut m, Event::Typed("회의".into())), vec![Effect::Debounce]);
        assert_eq!(m.query, "", "디바운스 전에는 불러오지 않음");
        assert_eq!(step(&mut m, Event::DebounceFired), vec![Effect::Load { seq: 2, query: "회의".into() }]);
        // 같은 검색어면 다시 부르지 않는다.
        step(&mut m, Event::Loaded { seq: 2, result: snap(sample()) });
        step(&mut m, Event::Typed(" 회의 ".into()));
        assert_eq!(step(&mut m, Event::DebounceFired), vec![]);
        let fx = step(&mut m, Event::Escape);
        assert_eq!(fx, vec![Effect::ClearSearch, Effect::Load { seq: 3, query: String::new() }]);
        assert_eq!(step(&mut m, Event::Escape), vec![], "지울 것이 없으면 아무 일 없음(창은 닫지 않음)");
    }

    #[test]
    fn category_filters_visible_rows_and_resets_selection() {
        let mut m = opened();
        step(&mut m, Event::Select(2));
        assert_eq!(m.selected().unwrap().id, 3);
        assert_eq!(step(&mut m, Event::Category(Category::Image)), vec![Effect::Save]);
        assert_eq!(m.visible(), vec![0]);
        assert_eq!(m.sel, 0);
        assert_eq!(step(&mut m, Event::Category(Category::Image)), vec![], "같은 분류는 그대로");
        step(&mut m, Event::Category(Category::Web));
        assert!(m.visible().is_empty());
        assert_eq!(step(&mut m, Event::OpenDetail(0)), vec![], "빈 목록에서 상세 없음");
        step(&mut m, Event::Move(5));
        assert_eq!(m.sel, 0);
    }

    #[test]
    fn selection_survives_reload_and_scroll_stays_in_range() {
        let mut m = opened();
        step(&mut m, Event::Rows(2));
        step(&mut m, Event::Move(10));
        assert_eq!((m.sel, m.top), (2, 1));
        step(&mut m, Event::External);
        // 새 항목이 맨 위에 생겨도 고른 항목(3)을 따라간다.
        let mut items = sample();
        items.insert(0, item(9, "text", "새 글", false, &[Category::All]));
        step(&mut m, Event::Loaded { seq: 2, result: snap(items) });
        assert_eq!(m.selected().unwrap().id, 3);
        step(&mut m, Event::Scroll(-10));
        assert_eq!(m.top, 0);
        step(&mut m, Event::ScrollTo(99));
        assert_eq!(m.top, 2);
    }

    #[test]
    fn detail_flow_ignores_late_results_and_recopy_restores() {
        let mut m = opened();
        assert_eq!(step(&mut m, Event::OpenDetail(1)), vec![Effect::LoadDetail { id: 1, kind: "text".into() }]);
        // 다른 상세는 열리지 않는다(대화상자 하나).
        assert_eq!(step(&mut m, Event::OpenDetail(0)), vec![]);
        assert_eq!(step(&mut m, Event::DetailLoaded { id: 2, result: Ok(DetailBody::Text("x".into())) }), vec![]);
        step(&mut m, Event::DetailLoaded { id: 1, result: Ok(DetailBody::Text("첫 글 전체".into())) });
        assert!(matches!(m.detail, Detail::Ready { id: 1, .. }));
        assert_eq!(step(&mut m, Event::Recopy), vec![Effect::Run(Job::Restore { id: 1 })]);
        assert_eq!(m.detail, Detail::Closed);
        step(&mut m, Event::Done { ok: true, message: String::new() });
        step(&mut m, Event::Loaded { seq: 2, result: snap(sample()) });
        step(&mut m, Event::OpenDetail(0));
        step(&mut m, Event::DetailLoaded { id: 2, result: Err("그림을 불러오지 못했습니다.".into()) });
        assert!(matches!(m.detail, Detail::Failed { id: 2, .. }));
        assert_eq!(step(&mut m, Event::Recopy), vec![], "실패한 상세에서는 다시 복사 없음");
        step(&mut m, Event::Escape);
        assert_eq!(m.detail, Detail::Closed);
    }

    #[test]
    fn table_checks_are_asked_once_and_dropped_for_deleted_items() {
        let mut m = opened();
        assert!(m.checking.contains(&3));
        step(&mut m, Event::TableChecked { id: 3, table: true });
        assert_eq!(m.tables.get(&3), Some(&true));
        step(&mut m, Event::External);
        let fx = step(&mut m, Event::Loaded { seq: 2, result: snap(sample()) });
        assert!(fx.is_empty(), "확인한 항목은 다시 묻지 않음");
        step(&mut m, Event::External);
        step(&mut m, Event::Loaded { seq: 3, result: snap(vec![]) });
        assert!(m.tables.is_empty());
        // 지운 뒤 늦게 온 확인 결과는 넣지 않는다.
        step(&mut m, Event::TableChecked { id: 3, table: true });
        assert!(m.tables.is_empty());
    }

    #[test]
    fn window_buttons_toggle_and_open_resets_but_keeps_category_and_topmost() {
        let mut m = opened();
        assert_eq!(step(&mut m, Event::ToggleTop), vec![Effect::Topmost(true), Effect::Save]);
        assert_eq!(step(&mut m, Event::ToggleCollapse), vec![Effect::Collapse(true)]);
        assert_eq!(step(&mut m, Event::ToggleMax), vec![Effect::Maximize(true)]);
        step(&mut m, Event::Category(Category::Image));
        m.message = "이전 글".into();
        step(&mut m, Event::Open(OpenMode::Hotkey));
        assert!(m.topmost && m.category == Category::Image);
        assert!(!m.collapsed && !m.maximized && m.message.is_empty() && m.items.is_empty());
        assert_eq!(m.load, Load::Loading(2), "순번은 이어서(이전 창의 늦은 결과를 버리려고)");
    }

    #[test]
    fn settings_rows_map_to_the_same_settings_set_keys() {
        let m = opened();
        assert_eq!(setting_rows(&m).len(), 11);
        assert_eq!(toggle_job(&m, Setting::AlwaysOnTop), Some(Job::Settings(Value::obj(vec![("always_on_top", false.into())]))));
        assert_eq!(toggle_job(&m, Setting::Sticky), Some(Job::Settings(Value::obj(vec![("sticky_popup", true.into())]))));
        assert_eq!(toggle_job(&m, Setting::Autostart), Some(Job::Autostart(true)));
        assert_eq!(number_job(Setting::MaxItems, 200), Some(Job::Settings(Value::obj(vec![("max_items", 200i64.into())]))));
        assert_eq!(number_job(Setting::Sticky, 1), None);
        // 폴더별 끄기(소문자 경로), 다시 켜기.
        let Some(Job::Settings(v)) = toggle_job(&m, Setting::CaptureDir(0)) else { panic!() };
        let mut s = m.settings.clone();
        s.apply(&v);
        assert_eq!(s.capture_dirs_off, ["c:\\shots"]);
        let mut m2 = m.clone();
        m2.settings = s;
        assert!(!capture_dir_on(&m2, 0));
        let Some(Job::Settings(v)) = toggle_job(&m2, Setting::CaptureDir(0)) else { panic!() };
        m2.settings.apply(&v);
        assert!(capture_dir_on(&m2, 0));
        // 전체가 꺼져 있으면 폴더별 칸은 사용 안 함.
        m2.settings.watch_screenshots = false;
        assert_eq!(toggle_job(&m2, Setting::CaptureDir(0)), None);
        // 모든 선택지가 Settings::apply에서 받아들여진다.
        for s in [Setting::MaxItems, Setting::MaxTotalMb, Setting::MaxItemMb, Setting::MaxAgeDays] {
            let (_, _, choices, _) = number_spec(s, &Settings::default()).unwrap();
            for v in choices {
                let Some(Job::Settings(p)) = number_job(s, *v) else { panic!() };
                let mut cfg = Settings::default();
                cfg.apply(&p);
                assert_eq!(number_spec(s, &cfg).unwrap().3, *v, "{s:?} {v}");
            }
        }
        assert_eq!(number_text(Setting::MaxAgeDays, 365), "1년");
    }

    #[test]
    fn paused_status_text_and_hotkey_text() {
        let mut m = opened();
        m.settings.paused = true;
        assert_eq!(m.status_text(), "일시정지");
        m.resume_in_min = Some(12);
        assert_eq!(m.status_text(), "일시정지 · 12분 뒤 다시 기록");
        m.hotkey_ok = false;
        assert_eq!(m.hotkey_text(), "Ctrl+Shift+V (사용 불가)");
    }

    #[test]
    fn web_and_table_predicates_match_flutter() {
        assert!(is_web_text(&item(1, "text", "https://example.com/a", false, &[])));
        assert!(is_web_text(&item(1, "text", "글 · 웹 · 제목", false, &[])));
        assert!(!is_web_text(&item(1, "text", "https://a b", false, &[])));
        assert!(!is_web_text(&item(1, "text", "http://", false, &[])));
        assert!(maybe_table(&item(1, "text", "서식 포함 · 표", false, &[])));
        assert!(!maybe_table(&item(1, "image", "서식 포함", false, &[])));
        assert_eq!(kind_label("files"), "파일");
    }
}
