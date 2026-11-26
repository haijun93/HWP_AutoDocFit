//! 데이터 폴더 전체 검색·역링크(Win32 없음). 엔진 `search`·`backlinks` 를 페이지마다 호출한다.
//! 저장하지 않은 현재 페이지는 메모리 본문(`current`)으로 바꿔 넣는다.
use crate::pages::{PageStore, is_dashed_journal, is_journal};
use outliner_core::{Document, backlinks, build_index, parse, search};

pub const MAX_HITS: usize = 300;
pub const SNIPPET_CHARS: usize = 160;

/// 자동완성용 읽기 전용 스냅숏. 다른 페이지의 임시 숫자 ID는 참조 후보로 쓰지 않는다.
pub fn completion_catalog(store: &PageStore, current: Option<&str>) -> (Vec<String>, Vec<(String, String)>) {
    let mut tags = std::collections::BTreeSet::new();
    let mut blocks = std::collections::BTreeMap::new();
    let mut ambiguous = std::collections::BTreeSet::new();
    for page in store.list() {
        if current == Some(page.as_str()) { continue; }
        let Ok(path) = store.path_of(&page) else { continue; };
        let Ok((doc, _, _)) = crate::docio::load_file(&path) else { continue; };
        for b in doc.blocks {
            tags.extend(b.tags);
            if let Some(id) = b.properties.get("id").filter(|id| !id.trim().is_empty() && !id.contains(['(', ')', '\n', '\r'])) {
                if blocks.insert(id.clone(), format!("{page} · {} · {id}", snippet(&b.text))).is_some() { ambiguous.insert(id.clone()); }
            }
        }
    }
    (tags.into_iter().collect(), blocks.into_iter().filter(|(id, _)| !ambiguous.contains(id)).collect())
}

/// 빠른 검색: 빈 질의는 최근 페이지, 1글자는 이름, 2글자부터 본문. 파일 생성 없음.
pub fn quick_search(store: &PageStore, query: &str, recent: &[String], current: Option<(&str, &str)>) -> Vec<Hit> {
    let pages = store.list();
    let q = query.trim();
    let mut hits: Vec<Hit> = crate::links::candidates(&pages, recent, q, MAX_HITS)
        .into_iter().map(|page| Hit { page, block: None, text: String::new() }).collect();
    if q.chars().count() >= 2 {
        hits.extend(search_all(store, q, current).into_iter().filter(|h| h.block.is_some()).take(MAX_HITS.saturating_sub(hits.len())));
    }
    hits
}

/// 검색 결과 한 건. `block` 이 None 이면 페이지 이름이 맞은 것.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub page: String,
    /// `Document::blocks` 안의 순서.
    pub block: Option<usize>,
    pub text: String,
}

/// 역링크 한 건: 어느 페이지의 몇 번째 블록이 대상 페이지를 가리키는가.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackItem {
    pub page: String,
    pub block: usize,
    pub text: String,
}

fn snippet(s: &str) -> String {
    let one: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    one.chars().take(SNIPPET_CHARS).collect()
}

fn load_doc(store: &PageStore, page: &str, current: Option<(&str, &str)>) -> Option<Document> {
    if let Some((name, md)) = current {
        if name == page {
            return parse(md).ok();
        }
    }
    let text = std::fs::read_to_string(store.path_of(page).ok()?).ok()?;
    parse(&text).ok()
}

/// 글자 수 2 미만이면 빈 목록. 페이지 이름 일치 → 블록 일치 순.
pub fn search_all(store: &PageStore, query: &str, current: Option<(&str, &str)>) -> Vec<Hit> {
    let q = query.trim();
    if q.chars().count() < 2 {
        return Vec::new();
    }
    let needle = q.to_lowercase();
    let pages = store.list();
    let mut hits = Vec::new();
    for p in &pages {
        if p.to_lowercase().contains(&needle) {
            hits.push(Hit { page: p.clone(), block: None, text: String::new() });
        }
    }
    'outer: for p in &pages {
        let Some(doc) = load_doc(store, p, current) else { continue };
        let Ok(ids) = search(&doc, q) else { continue };
        for id in ids {
            let Some(idx) = doc.blocks.iter().position(|b| b.id == id) else { continue };
            hits.push(Hit { page: p.clone(), block: Some(idx), text: snippet(&doc.blocks[idx].text) });
            if hits.len() >= MAX_HITS {
                break 'outer;
            }
        }
    }
    hits
}

/// 같은 일지를 가리키는 두 이름(`2026_11_21`·`2026-11-21`)과 대소문자 차이를 같은 대상으로 본다.
fn same_target(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let l = s.trim().to_lowercase();
        if is_dashed_journal(&l) || is_journal(&l) { l.replace('-', "_") } else { l }
    };
    norm(a) == norm(b)
}

/// 데이터 폴더 전체에서 `target` 페이지를 `[[링크]]` 로 가리키는 블록(대상 페이지 자신은 제외).
pub fn backlinks_all(store: &PageStore, target: &str, current: Option<(&str, &str)>) -> Vec<BackItem> {
    let mut out = Vec::new();
    for p in store.list() {
        if same_target(&p, target) {
            continue;
        }
        let Some(doc) = load_doc(store, &p, current) else { continue };
        let keys: Vec<String> = build_index(&doc).backlinks.keys().filter(|k| same_target(k, target)).cloned().collect();
        let mut ids: Vec<u64> = keys.iter().flat_map(|k| backlinks(&doc, k)).collect();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            if let Some(idx) = doc.blocks.iter().position(|b| b.id == id) {
                out.push(BackItem { page: p.clone(), block: idx, text: snippet(&doc.blocks[idx].text) });
            }
        }
    }
    out
}

/// 데이터 폴더 전체에서 특정 블록 id 또는 UUID를 `((참조))` 로 가리키는 블록.
pub fn block_backlinks_all(store: &PageStore, target_uuid: &str, current: Option<(&str, &str)>) -> Vec<BackItem> {
    let mut out = Vec::new();
    let needle = target_uuid.trim();
    if needle.is_empty() {
        return out;
    }
    for p in store.list() {
        let Some(doc) = load_doc(store, &p, current) else { continue };
        let ids = outliner_core::resolve_block_ref(&doc, needle).backlinks;
        for id in ids {
            if let Some(idx) = doc.blocks.iter().position(|b| b.id == id) {
                out.push(BackItem { page: p.clone(), block: idx, text: snippet(&doc.blocks[idx].text) });
            }
        }
    }
    out
}
