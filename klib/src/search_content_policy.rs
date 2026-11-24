//! search-content 전용 선택·순위·저장 문단 캐시. 공용 자료함 순위는 바꾸지 않는다.
use super::*;
use std::collections::HashSet;
use std::sync::Arc;
use tantivy::{DocAddress, DocSet};
use tantivy::postings::Postings;

/// 후보 검색은 유지하고, 응답에 필요한 문서만 위치·본문을 복원한다.
pub struct ContentScope<'a> {
    pub roots: &'a [String],
    pub allowed: Option<&'a HashSet<String>>,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub offset: usize,
    pub limit: usize,
    pub hydrate: bool,
}

#[derive(Default)]
pub(super) struct ContentCache {
    generation: Option<u64>,
    documents: HashMap<DocAddress, Arc<TantivyDocument>>,
    paths: HashMap<(u32, u64), String>,
    matching_paths: HashMap<String, Arc<HashSet<String>>>,
    synonyms: Option<Arc<Vec<Vec<String>>>>,
}
impl ContentCache {
    fn use_generation(&mut self, generation: u64) {
        if self.generation != Some(generation) {
            self.documents.clear();
            self.paths.clear();
            self.matching_paths.clear();
            self.generation = Some(generation);
        }
    }
    fn use_synonyms(&mut self, synonyms: Arc<Vec<Vec<String>>>) {
        if !self.synonyms.as_ref().is_some_and(|old| Arc::ptr_eq(old, &synonyms)) {
            self.matching_paths.clear();
            self.synonyms = Some(synonyms);
        }
    }
}

impl SearchIndex {
    /// 문서 AND의 단어별 문서 집합. roots/날짜는 최종 선택 때 검사한다.
    pub fn content_matching_paths(&self, query: &str) -> tantivy::Result<Arc<HashSet<String>>> {
        let generation = self.reader.searcher().generation().generation_id();
        let synonyms = self.load_synonyms();
        {
            let mut cache = self.content_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.use_generation(generation);
            cache.use_synonyms(synonyms.clone());
            if let Some(paths) = cache.matching_paths.get(query) {
                profile_add("and_paths_cache_hits", 1.0);
                return Ok(paths.clone());
            }
        }
        let scope = ContentScope { roots:&[], allowed:None, start:None, end:None, offset:0, limit:5000, hydrate:false };
        let paths: Arc<HashSet<String>> = Arc::new(self.search_content(query, SearchOptions { limit:5000 }, &scope)?
            .into_iter().map(|h| h.path).collect());
        // 검색 중 커밋되면 이전 세대 결과를 새 세대 캐시에 넣지 않는다.
        if self.reader.searcher().generation().generation_id() == generation {
            let mut cache = self.content_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.use_generation(generation);
            cache.use_synonyms(synonyms);
            if cache.matching_paths.len() >= 16 { cache.matching_paths.clear(); }
            cache.matching_paths.insert(query.to_string(), paths.clone());
        }
        profile_add("and_paths_cache_misses", 1.0);
        Ok(paths)
    }
    pub(super) fn content_path(&self, searcher: &tantivy::Searcher, columns: &[Columns], address: DocAddress) -> tantivy::Result<String> {
        let column = &columns[address.segment_ord as usize];
        let Some(ord) = column.path_ord(address.doc_id) else { return Ok(String::new()); };
        let mut cache = self.content_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.use_generation(searcher.generation().generation_id());
        let key = (address.segment_ord, ord);
        if let Some(path) = cache.paths.get(&key) { return Ok(path.clone()); }
        let path = column.path(address.doc_id)?;
        if cache.paths.len() >= 16384 { cache.paths.clear(); }
        cache.paths.insert(key, path.clone());
        Ok(path)
    }
    pub(super) fn content_document(&self, searcher: &tantivy::Searcher, address: DocAddress) -> tantivy::Result<Arc<TantivyDocument>> {
        let phase = std::time::Instant::now();
        let mut cache = self.content_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.use_generation(searcher.generation().generation_id());
        let document = if let Some(document) = cache.documents.get(&address) {
            profile_add("stored_cache_hits", 1.0);
            document.clone()
        } else {
            let document = Arc::new(searcher.doc::<TantivyDocument>(address)?);
            profile_add("stored_reads", 1.0);
            // 검색 결과 전체를 쌓지 않고 최근 문단만 보존한다.
            if cache.documents.len() >= 4096 { cache.documents.clear(); }
            cache.documents.insert(address, document.clone());
            document
        };
        profile_elapsed("stored_read_ms", phase);
        Ok(document)
    }
}

type Candidate = (usize, Score, u8, i64, Option<u64>, DocAddress);

/// BM25 길이 정규화로 사라지는 반복 신호를 postings의 원시 term_freq에서 읽는다.
/// 두 글자 한글은 실제 발생 횟수이며 긴 어절은 bigram 발생 수의 근사값이다.
fn occurrences(searcher: &tantivy::Searcher, field: Field, query: &str, candidates: &[Candidate]) -> tantivy::Result<Vec<u64>> {
    let mut tokenizer = KoBigram;
    let mut stream = tokenizer.token_stream(query);
    let mut tokens = HashSet::new();
    while stream.advance() { tokens.insert(stream.token().text.clone()); }
    let mut totals = vec![0u64; candidates.len()];
    let mut segments = vec![Vec::new(); searcher.segment_readers().len()];
    for (i, c) in candidates.iter().enumerate() { segments[c.5.segment_ord as usize].push(i); }
    for (segment, indices) in segments.iter_mut().enumerate() {
        if indices.is_empty() { continue; }
        indices.sort_unstable_by_key(|&i| candidates[i].5.doc_id);
        let inverted = searcher.segment_reader(segment as u32).inverted_index(field)?;
        for token in &tokens {
            let term = tantivy::Term::from_field_text(field, token);
            let Some(mut postings) = inverted.read_postings(&term, IndexRecordOption::WithFreqs)? else { continue; };
            for &i in indices.iter() {
                let c = &candidates[i];
                if postings.seek(c.5.doc_id) == c.5.doc_id {
                    let weight = match c.2 { 0 => 1, 1 => 8, 2 => 4, _ => 2 };
                    totals[c.0] += u64::from(postings.term_freq()) * weight;
                }
            }
        }
    }
    Ok(totals)
}

pub(super) fn select(searcher: &tantivy::Searcher, field: Field, query: &str,
                     candidates: &[Candidate], by_path: &HashMap<String, usize>,
                     mut ranked: Vec<hierarchy::search::DocumentAggregate>, scope: &ContentScope<'_>)
                     -> tantivy::Result<Vec<hierarchy::search::DocumentAggregate>> {
    let phase = std::time::Instant::now();
    let paths: HashMap<usize, &str> = by_path.iter().map(|(path, &id)| (id, path.as_str())).collect();
    if scope.hydrate {
        let counts = occurrences(searcher, field, query, candidates)?;
        ranked.sort_by(|a, b| {
            let rank = |r| if r == 0 { u8::MAX } else { r };
            rank(a.best_rank).cmp(&rank(b.best_rank))
                .then_with(|| counts[b.document].cmp(&counts[a.document]))
                .then_with(|| b.best_score.total_cmp(&a.best_score))
                .then_with(|| b.newest.cmp(&a.newest))
                .then_with(|| paths[&a.document].cmp(paths[&b.document]))
        });
    }
    profile_elapsed("occurrence_rank_ms", phase);
    let phase = std::time::Instant::now();
    let selected = ranked.into_iter().filter(|row| {
        let path = paths[&row.document];
        if scope.allowed.is_some_and(|allowed| !allowed.contains(path)) { return false; }
        let normalized = path.to_lowercase();
        if !scope.roots.is_empty() && !scope.roots.iter().any(|root| {
            normalized.starts_with(root) && normalized.get(root.len()..)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with('\\') || tail.starts_with('/'))
        }) { return false; }
        if scope.start.is_none() && scope.end.is_none() { return true; }
        let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|t| t.as_secs_f64());
        scope.start.is_none_or(|s| mtime.is_some_and(|t| t >= s))
            && scope.end.is_none_or(|e| mtime.is_some_and(|t| t < e))
    }).skip(scope.offset).take(scope.limit).collect();
    profile_elapsed("scope_page_ms", phase);
    Ok(selected)
}
