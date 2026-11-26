//! 페이지 모드의 상태 묶음(Win32 없음): 데이터 폴더·현재 페이지·페이지 목록·최근·검색·역링크·이름 입력 줄.
//! 편집기(`Editor`)는 그대로 두고 그 위에서 "다른 페이지 열기" 같은 동작만 한다. 모든 동작은 시험에서
//! 가짜 호스트로 그대로 돌릴 수 있다(대화상자는 창 쪽에서, 여기에는 확인이 끝난 동작만 있다).
use crate::docio;
use crate::edit::{EditHost, Editor};
use crate::find::{self, BackItem, Hit};
use crate::pages::{PageStore, RECENT_MAX};
use crate::view::View;
use crate::layout::{LeftSection, LeftItem, RightItem};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visit {
    pub page: String,
    pub block: Option<usize>,
}

/// UI1 카드는 읽기 미리보기. 누르면 기존 중앙 편집기로 연다.
#[derive(Clone, Debug)]
pub struct SidebarCard {
    pub target: Visit,
    pub preview: String,
    /// 순번 블록이 편집 뒤 다른 블록을 가리키는 일을 막는다(ID·파일 형식은 그대로).
    pub version: String,
}

/// 위쪽 입력 칸이 "새 페이지 이름"·"이름 바꾸기" 입력으로 바뀐 상태.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    NewPage,
    Rename,
    Property,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode { Legacy, Quick, Commands }

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
    pub search_mode: SearchMode,
    pub create_page: Option<String>,
    pub command_hits: Vec<crate::commands::Action>,
    pub hit_sel: usize,
    pub hit_first: usize,
    pub back: Vec<BackItem>,
    pub back_first: usize,
    pub left_first: usize,
    pub prompt: Option<PromptKind>,
    pub clock: fn() -> Now,
    pub last_rename: Option<(String, String)>,
    pub left_open: bool,
    pub right_open: bool,
    pub section: LeftSection,
    /// 저장 형식·사용자 설정을 바꾸지 않는 실행 중 즐겨찾기.
    pub favorites: Vec<String>,
    pub cards: Vec<SidebarCard>,
    pub visits: Vec<Visit>,
    pub visit_at: usize,
    pub toggle_prefix: Option<u64>,
    pub graph: std::collections::BTreeMap<String, outliner_core::Document>,
    pub unlinked: Vec<BackItem>,
    pub property_block: Option<u64>,
    pub reference_revision: Option<(String, Option<u64>)>,
    pub presentation_revision: String,
    pub presentation_cache: std::collections::BTreeMap<u64, Vec<Vec<outliner_core::references::DisplaySpan>>>,
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
            search_mode: SearchMode::Legacy,
            create_page: None,
            command_hits: Vec::new(),
            hit_sel: 0,
            hit_first: 0,
            back: Vec::new(),
            back_first: 0,
            left_first: 0,
            prompt: None,
            clock: fixed_clock,
            last_rename: None,
            left_open: true,
            right_open: true,
            section: LeftSection::All,
            favorites: Vec::new(),
            cards: Vec::new(),
            visits: Vec::new(),
            visit_at: 0,
            toggle_prefix: None,
            graph: Default::default(),
            unlinked: Vec::new(),
            property_block: None,
            reference_revision: None,
            presentation_revision: String::new(),
            presentation_cache: Default::default(),
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
        (self.editor.tag_names, self.editor.graph_blocks) = find::completion_catalog(&store, self.page.as_deref());
        self.editor.today = (self.clock)().date_iso;
        self.refresh_reference_graph();
    }

    /// 왼쪽 패널에 그릴 목록(검색 칸 글로 거른 것). 이름 입력 줄이 열려 있는 동안은 거르지 않는다.
    pub fn left_list(&self) -> Vec<crate::layout::LeftItem> {
        let q = if self.prompt.is_some() { "" } else { self.query.as_str() };
        if self.section == LeftSection::All {
            return crate::layout::left_items_filtered(&self.pages, &self.recent, q);
        }
        let names: Vec<String> = match self.section {
            LeftSection::Journals => self.pages.iter().filter(|p| crate::pages::looks_like_journal(p)).cloned().collect(),
            LeftSection::Favorites => self.favorites.iter().filter(|p| self.pages.contains(p)).cloned().collect(),
            LeftSection::Recent => self.recent.iter().filter(|p| self.pages.contains(p)).cloned().collect(),
            LeftSection::All => unreachable!(),
        };
        let q = q.trim().to_lowercase();
        let mut result = vec![LeftItem::Label(format!("{} ({})", self.section.label(), names.len()))];
        result.extend(names.into_iter().filter(|p| p.to_lowercase().contains(&q)).map(LeftItem::Page));
        result
    }

    pub fn select_section(&mut self, section: LeftSection) {
        self.section = section;
        self.left_first = 0;
    }

    pub fn toggle_favorite(&mut self) {
        let Some(page) = self.page.clone() else { return };
        if self.favorites.contains(&page) { self.favorites.retain(|p| p != &page); }
        else { self.favorites.push(page); }
        self.left_first = 0;
        self.say("즐겨찾기는 이번 실행 동안 유지됩니다");
    }

    pub fn can_back(&self) -> bool { !self.visits.is_empty() && self.visit_at > 0 }
    pub fn can_forward(&self) -> bool { self.visit_at + 1 < self.visits.len() }

    fn remember_position(&mut self) {
        if let Some(v) = self.visits.get_mut(self.visit_at) {
            v.block = self.editor.view.rows.get(self.editor.view.selected).map(|r| r.block);
        }
    }

    /// 저장/대상 읽기에 실패하면 방문 스택과 현재 페이지를 그대로 둔다. 삭제된 방문을 재생성하지 않는다.
    pub fn navigate(&mut self, forward: bool, page_rows: usize) -> Result<(), String> {
        if !(if forward { self.can_forward() } else { self.can_back() }) { return Ok(()); }
        self.persist()?;
        self.remember_position();
        let at = if forward { self.visit_at + 1 } else { self.visit_at - 1 };
        let target = self.visits[at].clone();
        let history = self.visits.clone();
        self.load_page(&target.page)?;
        self.visits = history;
        self.visit_at = at;
        if let Some(block) = target.block { self.editor.view.reveal_block(block, page_rows); }
        Ok(())
    }

    pub fn right_list(&self) -> Vec<RightItem> {
        let mut items = Vec::new();
        for i in 0..self.cards.len() { items.extend([RightItem::Card(i), RightItem::Preview(i)]); }
        if !self.cards.is_empty() && !self.back.is_empty() { items.push(RightItem::Group(format!("연결 참조 ({})", self.back.len()))); }
        items.extend(crate::layout::right_items(&self.back));
        items.push(RightItem::Group(format!("미연결 언급 ({})", self.unlinked.len())));
        items.extend((0..self.unlinked.len()).map(RightItem::Unlinked));
        items.push(RightItem::AddProperty);
        if let Some(b) = self.editor.view.rows.get(self.editor.view.selected).and_then(|r| self.editor.view.doc.blocks.get(r.block)) {
            items.extend(b.properties.iter().map(|(k, v)| RightItem::Property(k.clone(), v.clone())));
            let tags: std::collections::BTreeSet<_> = crate::references::block_targets(b).into_iter().filter_map(|t| {
                if let outliner_core::references::RefTarget::Tag(tag) = t { Some(tag) } else { None }
            }).collect();
            items.extend(tags.into_iter().map(RightItem::Tag));
        }
        items
    }

    /// 페이지/블록을 새 카드로 연다. 같은 대상은 맨 위로 옮기며 원문·ID·방문 이력은 건드리지 않는다.
    pub fn open_card(&mut self, page: &str, block: Option<usize>) -> Result<(), String> {
        let page = crate::pages::validate_name(page)?;
        if !self.editor.flush_for_close() { return Err("편집 중인 글을 반영하지 못했습니다".into()); }
        let doc = if self.page.as_deref() == Some(page.as_str()) { self.editor.view.doc.clone() }
        else {
            let path = self.store()?.path_of(&page)?;
            docio::load_file(&path)?.0
        };
        if block.is_some_and(|b| b >= doc.blocks.len()) { return Err("블록을 찾을 수 없습니다".into()); }
        let preview = block.and_then(|b| doc.blocks.get(b)).map(|b| b.text.clone())
            .unwrap_or_else(|| doc.blocks.iter().take(3).map(|b| b.text.as_str()).collect::<Vec<_>>().join(" · "));
        let target = Visit { page, block };
        self.cards.retain(|c| c.target != target);
        self.cards.insert(0, SidebarCard { target, preview, version: outliner_core::content_digest(&outliner_core::serialize(&doc)) });
        self.right_open = true;
        self.back_first = 0;
        Ok(())
    }

    pub fn close_card(&mut self, index: usize) {
        if index < self.cards.len() { self.cards.remove(index); self.back_first = 0; }
    }

    pub fn activate_card(&mut self, index: usize, page_rows: usize) -> Result<(), String> {
        let card = self.cards.get(index).ok_or("카드가 없습니다")?.clone();
        let target = card.target;
        // 미리보기는 없는 페이지를 생성하지 않는다.
        if !self.store()?.exists(&target.page) { return Err("카드의 페이지가 없습니다".into()); }
        if target.block.is_some() {
            if !self.editor.flush_for_close() { return Err("편집 중인 글을 반영하지 못했습니다".into()); }
            let markdown = if self.page.as_deref() == Some(target.page.as_str()) { self.editor.markdown() }
                else { outliner_core::serialize(&docio::load_file(&self.store()?.path_of(&target.page)?)?.0) };
            if outliner_core::content_digest(&markdown) != card.version {
                return Err("블록이 바뀌었습니다. Shift+클릭으로 카드를 다시 열어 주세요".into());
            }
        }
        self.goto(&target.page, target.block, page_rows)
    }

    pub fn card_preview(&self, index: usize) -> String {
        self.cards[index].preview.clone()
    }

    fn remap_page(&mut self, old: &str, new: &str) {
        for page in &mut self.favorites { if page == old { *page = new.to_string(); } }
        for visit in &mut self.visits { if visit.page == old { visit.page = new.to_string(); } }
        for card in &mut self.cards { if card.target.page == old { card.target.page = new.to_string(); } }
    }

    pub fn refresh_backlinks(&mut self) {
        let selected_id = self.editor.view.rows.get(self.editor.view.selected).map(|r| self.editor.view.doc.blocks[r.block].id);
        let revision = (outliner_core::content_digest(&self.editor.markdown()), selected_id);
        if self.reference_revision.as_ref() == Some(&revision) { return; }
        self.reference_revision = Some(revision);
        let Some(page) = self.page.as_deref() else {
            self.back.clear();
            return;
        };
        let mut list = Vec::new();
        let selected = self.editor.view.rows.get(self.editor.view.selected).map(|r| &self.editor.view.doc.blocks[r.block]);
        let canonical = |p: &str| outliner_core::normalize_journal_name(p).unwrap_or_else(|_| p.to_lowercase());
        for source in &self.pages {
            let source = source.as_str();
            let doc = if source == page { &self.editor.view.doc }
                else { let Some(doc) = self.graph.get(source) else { continue; }; doc };
            for (i, b) in doc.blocks.iter().enumerate() {
                use outliner_core::references::RefTarget;
                let targets = crate::references::block_targets(b);
                let page_ref = source != page && targets.iter().any(|t| matches!(t, RefTarget::Page(p) | RefTarget::Tag(p) if canonical(p) == canonical(page)));
                let block_ref = selected.is_some_and(|sel| targets.iter().any(|t| {
                    matches!(t, RefTarget::Block(id) if sel.properties.get("id") == Some(id) || (source == page && sel.id.to_string() == *id))
                }));
                if page_ref || block_ref {
                    list.push(BackItem { page: source.into(), block: i, text: b.text.chars().take(find::SNIPPET_CHARS).collect() });
                    if list.len() >= find::MAX_HITS { break; }
                }
            }
            if list.len() >= find::MAX_HITS { break; }
        }
        if self.back != list { self.back_first = 0; self.back = list; }
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
        self.remember_position();
        self.editor.replace_document(view, Some(info), base);
        self.page = Some(stem.to_string());
        // The pending property input belongs to a block of the previous page.
        if self.prompt == Some(PromptKind::Property) {
            self.cancel_prompt();
        }
        self.recent.retain(|r| r != stem);
        self.recent.insert(0, stem.to_string());
        self.recent.truncate(RECENT_MAX);
        store.save_recent(&self.recent);
        self.refresh_lists();
        self.refresh_backlinks();
        self.search_open = false;
        self.search_mode = SearchMode::Legacy;
        self.create_page = None;
        self.command_hits.clear();
        self.hits.clear();
        self.query.clear();
        self.visits.truncate(if self.visits.is_empty() { 0 } else { self.visit_at + 1 });
        if self.visits.last().is_none_or(|v| v.page != stem) {
            self.visits.push(Visit { page: stem.to_string(), block: None });
        }
        self.visit_at = self.visits.len() - 1;
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
        self.remap_page(&old, &stem);
        self.last_rename = Some((old, stem.clone()));
        self.load_page(&stem)?;
        self.say(format!("이름을 '{stem}' 로 바꿨습니다(링크를 고친 파일 {n}개)"));
        Ok(())
    }

    /// 이름 바꾸기 실행 취소.
    pub fn undo_rename(&mut self) -> Result<(), String> {
        let store = self.store()?;
        let (old, new) = self.last_rename.take().ok_or("되돌릴 이름 변경 내역이 없습니다")?;
        self.persist()?;
        let n = store.undo_rename(&old, &new)?;
        self.remap_page(&new, &old);
        self.load_page(&old)?;
        self.say(format!("이름을 다시 '{old}' 로 되돌렸습니다(링크를 고친 파일 {n}개)"));
        Ok(())
    }

    /// 현재 페이지를 `.trash` 로 옮기고(되돌릴 수 있다) 오늘의 일지를 연다. 참조가 있으면 경고 메시지를 포함한다.
    pub fn delete_current(&mut self) -> Result<(), String> {
        let store = self.store()?;
        let old = self.page.clone().ok_or("열린 페이지가 없습니다")?;
        let referring = store.check_references(&old).unwrap_or_default();
        self.persist()?;
        let now = (self.clock)();
        store.delete(&old, &now.stamp)?;
        self.page = None;
        self.recent.retain(|r| r != &old);
        self.refresh_lists();
        let today = store.journal_today(&now.date_iso)?;
        self.load_page(&today)?;
        if referring.is_empty() {
            self.say(format!("'{old}' 를 .trash 로 옮겼습니다"));
        } else {
            self.say(format!("'{old}' 를 .trash 로 옮겼습니다 (참조하는 페이지 {}개 있음: {})", referring.len(), referring.join(", ")));
        }
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

    pub fn begin_search(&mut self, mode: SearchMode) -> bool {
        if self.store.is_none() || self.prompt.is_some() || !self.editor.finish_edit() { return false; }
        self.search_mode = mode;
        self.set_query("");
        true
    }

    /// 검색어가 바뀔 때마다(Enter 불필요) 부른다. 2글자 미만이면 결과를 닫는다.
    pub fn set_query(&mut self, q: &str) {
        self.query = q.to_string();
        self.left_first = 0;
        self.hit_sel = 0;
        self.hit_first = 0;
        self.create_page = None;
        self.command_hits.clear();
        if self.search_mode == SearchMode::Commands {
            let entries = crate::commands::palette(q);
            self.command_hits = entries.iter().map(|(a, _)| *a).collect();
            self.hits = entries.into_iter().map(|(_, label)| Hit { page: label, block: None, text: "명령 실행".into() }).collect();
            self.search_open = true;
            return;
        }
        if self.search_mode == SearchMode::Quick {
            if let Some(store) = &self.store {
                let md = self.editor.markdown();
                self.hits = find::quick_search(store, q, &self.recent, self.page.as_deref().map(|p| (p, md.as_str())));
                if let Ok(name) = crate::pages::validate_name(q) {
                    if !store.list().iter().any(|p| p.eq_ignore_ascii_case(&name)) {
                        self.create_page = Some(name.clone());
                        self.hits.push(Hit { page: name, block: None, text: "새 페이지 생성 (Enter)".into() });
                    }
                }
            }
            self.search_open = true;
            return;
        }
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
        self.search_mode = SearchMode::Legacy;
        self.create_page = None;
        self.command_hits.clear();
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
        if self.search_mode == SearchMode::Commands {
            let action = *self.command_hits.get(i).ok_or("명령이 없습니다")?;
            return self.run_palette(action);
        }
        if self.search_mode == SearchMode::Quick && i + 1 == self.hits.len() && self.create_page.as_ref() == Some(&hit.page) {
            self.new_page(&hit.page)?;
            self.editor.start_edit_selected();
            return Ok(());
        }
        if !self.store()?.exists(&hit.page) { return Err("검색 대상이 사라졌습니다. 다시 검색해 주세요".into()); }
        self.goto(&hit.page, hit.block, page_rows)
    }

    fn run_palette(&mut self, action: crate::commands::Action) -> Result<(), String> {
        use crate::commands::Action;
        use crate::edit::Cmd;
        // 실패하는 명령도 팔레트를 닫고 기존 Editor 오류 문구를 보여 준다.
        self.close_search();
        match action {
            Action::Save => if !self.editor.save() { return Err(self.editor.message.clone().unwrap_or("저장 실패".into())); },
            Action::Undo => { self.editor.history_step(true); }
            Action::Redo => { self.editor.history_step(false); }
            Action::Left => self.left_open = !self.left_open,
            Action::Right => self.right_open = !self.right_open,
            Action::Today => self.open_today()?,
            Action::New => { self.begin_prompt(PromptKind::NewPage); }
            Action::Rename => { self.begin_prompt(PromptKind::Rename); }
            Action::Todo => self.editor.nav_command(Cmd::Toggle),
            Action::Collapse => self.editor.nav_command(Cmd::Collapse),
            Action::Expand => self.editor.nav_command(Cmd::Expand),
        }
        Ok(())
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
        if kind == PromptKind::Property {
            if self.editor.host.composing() || self.editor.host.candidate_open() { return None; }
            if !self.editor.finish_edit() { return None; }
            self.property_block = self.editor.view.rows.get(self.editor.view.selected).map(|r| self.editor.view.doc.blocks[r.block].id);
            self.property_block?;
        }
        self.prompt = Some(kind);
        self.search_open = false;
        self.search_mode = SearchMode::Legacy;
        self.create_page = None;
        self.command_hits.clear();
        Some(if kind == PromptKind::Rename { self.page.clone().unwrap_or_default() } else { String::new() })
    }

    pub fn cancel_prompt(&mut self) {
        self.prompt = None;
        self.property_block = None;
    }

    /// Enter: 입력 줄 확정. 실패하면 줄을 그대로 둔다(고쳐 쓸 수 있게).
    pub fn finish_prompt(&mut self, text: &str) -> Result<(), String> {
        let Some(kind) = self.prompt else { return Ok(()) };
        let r = match kind {
            PromptKind::NewPage => self.new_page(text),
            PromptKind::Rename => self.rename_current(text),
            PromptKind::Property => {
                let (key, value) = text.split_once("::").ok_or("이름:: 값 형식으로 입력해 주세요")?;
                self.editor.set_property(self.property_block.ok_or("속성 대상 블록이 없습니다")?, key.trim(), value.trim())
            }
        };
        if r.is_ok() {
            self.prompt = None;
            self.property_block = None;
            self.refresh_backlinks();
        }
        r
    }
}
