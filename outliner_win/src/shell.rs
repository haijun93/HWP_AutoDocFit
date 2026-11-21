//! 페이지 모드의 상태 묶음(Win32 없음): 데이터 폴더·현재 페이지·페이지 목록·최근·검색·역링크·이름 입력 줄.
//! 편집기(`Editor`)는 그대로 두고 그 위에서 "다른 페이지 열기" 같은 동작만 한다. 모든 동작은 시험에서
//! 가짜 호스트로 그대로 돌릴 수 있다(대화상자는 창 쪽에서, 여기에는 확인이 끝난 동작만 있다).
use crate::docio;
use crate::edit::{EditHost, Editor};
use crate::find::{self, BackItem, Hit};
use crate::pages::{PageStore, RECENT_MAX};
use crate::view::View;

/// 위쪽 입력 칸이 "새 페이지 이름"·"이름 바꾸기" 입력으로 바뀐 상태.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    NewPage,
    Rename,
}

/// 오늘 날짜·시각 문자열(시험에서는 고정값으로 바꾼다).
#[derive(Clone, Debug)]
pub struct Now {
    /// `YYYY-MM-DD`
    pub date_iso: String,
    /// `YYYYMMDD-HHMMSS`
    pub stamp: String,
}

pub struct Shell<H: EditHost> {
    pub editor: Editor<H>,
    /// None 이면 한 파일만 여는 옛 방식(패널 없음).
    pub store: Option<PageStore>,
    pub page: Option<String>,
    pub pages: Vec<String>,
    pub recent: Vec<String>,
    pub query: String,
    pub hits: Vec<Hit>,
    pub search_open: bool,
    pub hit_sel: usize,
    pub hit_first: usize,
    pub back: Vec<BackItem>,
    pub back_first: usize,
    pub left_first: usize,
    pub prompt: Option<PromptKind>,
    pub clock: fn() -> Now,
}

fn fixed_clock() -> Now {
    Now { date_iso: "1970-01-01".into(), stamp: "19700101-000000".into() }
}

impl<H: EditHost> Shell<H> {
    /// 한 파일만 여는 방식(패널 없음).
    pub fn single(editor: Editor<H>) -> Shell<H> {
        Shell {
            editor,
            store: None,
            page: None,
            pages: Vec::new(),
            recent: Vec::new(),
            query: String::new(),
            hits: Vec::new(),
            search_open: false,
            hit_sel: 0,
            hit_first: 0,
            back: Vec::new(),
            back_first: 0,
            left_first: 0,
            prompt: None,
            clock: fixed_clock,
        }
    }

    /// 데이터 폴더 방식. 아직 아무 페이지도 열지 않았다(`start` 로 오늘의 일지를 연다).
    pub fn with_store(host: H, store: PageStore, clock: fn() -> Now) -> Result<Shell<H>, String> {
        store.ensure_root()?;
        let view = View::from_markdown("- \n")?;
        let base = outliner_core::serialize(&view.doc);
        let mut shell = Shell::single(Editor::new(view, host, None, base));
        shell.store = Some(store);
        shell.clock = clock;
        shell.refresh_lists();
        Ok(shell)
    }

    pub fn panels(&self) -> bool {
        self.store.is_some()
    }

    fn store(&self) -> Result<PageStore, String> {
        self.store.clone().ok_or_else(|| "데이터 폴더가 없습니다".to_string())
    }

    fn say(&mut self, m: impl Into<String>) {
        self.editor.message = Some(m.into());
    }

    /// 페이지·최근 목록을 다시 읽고 자동완성 후보도 맞춘다.
    pub fn refresh_lists(&mut self) {
        let Some(store) = self.store.clone() else { return };
        self.pages = store.list();
        let keep: Vec<String> = self.recent.iter().filter(|r| self.pages.contains(r)).cloned().collect();
        self.recent = if keep.is_empty() { store.load_recent(&self.pages) } else { keep };
        self.editor.page_names = self.pages.clone();
        self.editor.recent_names = self.recent.clone();
    }

    /// 왼쪽 패널에 그릴 목록(검색 칸 글로 거른 것). 이름 입력 줄이 열려 있는 동안은 거르지 않는다.
    pub fn left_list(&self) -> Vec<crate::layout::LeftItem> {
        let q = if self.prompt.is_some() { "" } else { self.query.as_str() };
        crate::layout::left_items_filtered(&self.pages, &self.recent, q)
    }

    pub fn refresh_backlinks(&mut self) {
        self.back_first = 0;
        let (Some(store), Some(page)) = (self.store.clone(), self.page.clone()) else {
            self.back.clear();
            return;
        };
        let md = self.editor.markdown();
        self.back = find::backlinks_all(&store, &page, Some((&page, &md)));
    }

    /// 편집 중 글을 확정하고 변경이 있으면 저장한다. 실패하면 이유를 돌려주고 아무것도 바꾸지 않는다.
    pub fn persist(&mut self) -> Result<(), String> {
        if self.page.is_none() && self.editor.file().is_none() {
            return Ok(());
        }
        if !self.editor.flush_for_close() {
            return Err(self.editor.message.clone().unwrap_or_else(|| "편집 중인 글을 반영하지 못했습니다".into()));
        }
        if self.editor.dirty() && !self.editor.save() {
            return Err(self.editor.message.clone().unwrap_or_else(|| "저장하지 못했습니다".into()));
        }
        Ok(())
    }

    /// 파일에서 페이지를 읽어 편집기에 싣는다(저장은 하지 않는다).
    fn load_page(&mut self, stem: &str) -> Result<(), String> {
        let store = self.store()?;
        let path = store.path_of(stem)?;
        let (_doc, text, info) = docio::load_file(&path)?;
        let view = View::from_markdown(&text)?;
        let base = outliner_core::serialize(&view.doc);
        self.editor.replace_document(view, Some(info), base);
        self.page = Some(stem.to_string());
        self.recent.retain(|r| r != stem);
        self.recent.insert(0, stem.to_string());
        self.recent.truncate(RECENT_MAX);
        store.save_recent(&self.recent);
        self.refresh_lists();
        self.refresh_backlinks();
        self.search_open = false;
        self.hits.clear();
        Ok(())
    }

    /// 페이지 열기(없으면 만든다 — `[[링크]]` 를 따라갈 때). 열기 전에 현재 페이지를 저장한다.
    pub fn open_page(&mut self, name: &str) -> Result<(), String> {
        let store = self.store()?;
        let target = crate::pages::validate_name(name)?;
        if self.page.as_deref() == Some(target.as_str()) {
            self.search_open = false;
            return Ok(());
        }
        self.persist()?;
        let stem = store.normalize_journal(&target)?;
        let stem = store.ensure_page(&stem)?;
        self.load_page(&stem)
    }

    pub fn open_today(&mut self) -> Result<(), String> {
        let store = self.store()?;
        let now = (self.clock)();
        if self.page.is_some() || self.editor.file().is_some() {
            self.persist()?;
        }
        let name = store.journal_today(&now.date_iso)?;
        if self.page.as_deref() == Some(name.as_str()) {
            return Ok(());
        }
        self.load_page(&name)
    }

    /// 처음 열기: 오늘의 일지.
    pub fn start(&mut self) -> Result<(), String> {
        self.open_today()
    }

    pub fn new_page(&mut self, name: &str) -> Result<(), String> {
        let store = self.store()?;
        self.persist()?;
        let stem = store.create(name)?;
        self.load_page(&stem)?;
        self.say(format!("새 페이지 '{stem}' 를 만들었습니다"));
        Ok(())
    }

    /// 현재 페이지 이름 바꾸기. 다른 페이지의 `[[옛 이름]]` 링크도 함께 고친다.
    pub fn rename_current(&mut self, new_name: &str) -> Result<(), String> {
        let store = self.store()?;
        let old = self.page.clone().ok_or("열린 페이지가 없습니다")?;
        self.persist()?;
        let n = store.rename(&old, new_name)?;
        let stem = crate::pages::validate_name(new_name)?;
        self.load_page(&stem)?;
        self.say(format!("이름을 '{stem}' 로 바꿨습니다(링크를 고친 파일 {n}개)"));
        Ok(())
    }

    /// 현재 페이지를 `.trash` 로 옮기고(되돌릴 수 있다) 오늘의 일지를 연다.
    pub fn delete_current(&mut self) -> Result<(), String> {
        let store = self.store()?;
        let old = self.page.clone().ok_or("열린 페이지가 없습니다")?;
        self.persist()?;
        let now = (self.clock)();
        store.delete(&old, &now.stamp)?;
        self.page = None;
        self.recent.retain(|r| r != &old);
        self.refresh_lists();
        let today = store.journal_today(&now.date_iso)?;
        self.load_page(&today)?;
        self.say(format!("'{old}' 를 .trash 로 옮겼습니다"));
        Ok(())
    }

    /// 마지막으로 지운 페이지를 되돌려 연다.
    pub fn restore_last(&mut self) -> Result<(), String> {
        let store = self.store()?;
        let last = store.list_trash().pop().ok_or("휴지통이 비어 있습니다")?;
        let name = store.restore(&last)?;
        self.refresh_lists();
        self.open_page(&name)
    }

    /// `[[링크]]` 따라가기(클릭·Ctrl+클릭).
    pub fn follow_link(&mut self, name: &str) -> Result<(), String> {
        self.open_page(name)
    }

    // ---------- 검색 ----------

    /// 검색어가 바뀔 때마다(Enter 불필요) 부른다. 2글자 미만이면 결과를 닫는다.
    pub fn set_query(&mut self, q: &str) {
        self.query = q.to_string();
        self.left_first = 0;
        self.hit_sel = 0;
        self.hit_first = 0;
        if q.trim().chars().count() < 2 {
            self.hits.clear();
            self.search_open = false;
            return;
        }
        if let Some(store) = self.store.clone() {
            let md = self.editor.markdown();
            let cur = self.page.clone();
            self.hits = find::search_all(&store, q, cur.as_deref().map(|p| (p, md.as_str())));
        }
        self.search_open = true;
    }

    pub fn close_search(&mut self) {
        self.search_open = false;
        self.hits.clear();
        self.query.clear();
    }

    pub fn move_hit(&mut self, down: bool) {
        if self.hits.is_empty() {
            return;
        }
        let n = self.hits.len();
        self.hit_sel = if down { (self.hit_sel + 1).min(n - 1) } else { self.hit_sel.saturating_sub(1) };
    }

    /// 검색 결과로 이동: 페이지를 열고 해당 블록을 선택한다.
    pub fn open_hit(&mut self, i: usize, page_rows: usize) -> Result<(), String> {
        let hit = self.hits.get(i).cloned().ok_or("결과가 없습니다")?;
        self.goto(&hit.page, hit.block, page_rows)
    }

    pub fn goto_backlink(&mut self, i: usize, page_rows: usize) -> Result<(), String> {
        let item = self.back.get(i).cloned().ok_or("역링크가 없습니다")?;
        self.goto(&item.page, Some(item.block), page_rows)
    }

    fn goto(&mut self, page: &str, block: Option<usize>, page_rows: usize) -> Result<(), String> {
        self.open_page(page)?;
        if let Some(b) = block {
            self.editor.view.reveal_block(b, page_rows);
        }
        self.search_open = false;
        Ok(())
    }

    // ---------- 이름 입력 줄 ----------

    /// 입력 줄을 열고 처음 글을 돌려준다(이름 바꾸기는 현재 이름).
    pub fn begin_prompt(&mut self, kind: PromptKind) -> Option<String> {
        if self.store.is_none() || (kind == PromptKind::Rename && self.page.is_none()) {
            return None;
        }
        self.prompt = Some(kind);
        self.search_open = false;
        Some(if kind == PromptKind::Rename { self.page.clone().unwrap_or_default() } else { String::new() })
    }

    pub fn cancel_prompt(&mut self) {
        self.prompt = None;
    }

    /// Enter: 입력 줄 확정. 실패하면 줄을 그대로 둔다(고쳐 쓸 수 있게).
    pub fn finish_prompt(&mut self, text: &str) -> Result<(), String> {
        let Some(kind) = self.prompt else { return Ok(()) };
        let r = match kind {
            PromptKind::NewPage => self.new_page(text),
            PromptKind::Rename => self.rename_current(text),
        };
        if r.is_ok() {
            self.prompt = None;
        }
        r
    }
}
