// Copyright (c) 2026 haijun93 (마포구청 공혁준). PolyForm Strict License 1.0.0.
//! 주제 빈도 순위표 (SEMI3-topicrank).
//!
//! - 파일명 낱말 + 제목·소제목 낱말 (SEMI2 문장 역할 비트맵 기반)
//! - 본문 낱말은 동시 출현(co-occurrence) 추천용 보조
//! - 순위: TF-IDF식 (이 PC에서 많지만 모든 문서에 있지 않은 말), 너무 흔한 말(계획·보고·추진 등) 자동 제외
//! - 상위 1,000개 실시간 유지, 문서 변경 시 그 문서 낱말만 증감(마이크로초, 수백 KB)
//! - 기능 API:
//!   1. 자동 완성 (빈도순)
//!   2. 빈 검색어 첫 화면 주제 1~10위
//!   3. `;` 관련어 추천 (교집합 개수)
//!   4. 기간별 급증 주제 (시간 비트맵/날짜 기반)
//!   5. 폴더 대표 주제
//!   6. 결과 순위: 드문 말 일치 가산, 흔한 말 가산 낮춤
use crate::fold::{fold, tokens};
use crate::{Error, FNV_SEED, Result, corrupt, fnv64};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub const TOPIC_LIMIT: usize = 1000;
pub const MIN_TOPIC_CHARS: usize = 2;
pub const TOPICS_FILE_NAME: &str = "topics.bin";
pub const TOPICS_TMP_PREFIX: &str = "topics.tmp-";
const MAGIC: &[u8; 8] = b"KFTOP\r\n\x1a";
const FORMAT_VERSION: u32 = 1;

/// 공문서 및 업무 보고서에서 너무 흔하게 나타나는 불용어 (자동 제외).
pub const COMMON_STOPWORDS: &[&str] = &[
    "계획", "보고", "추진", "관련", "사업", "안내", "지침", "방안",
    "대책", "결과", "개최", "운영", "관리", "지원", "현황", "작성",
    "제출", "요청", "검토", "통보", "시행", "알림", "참고", "조치",
    "협조", "내용", "사항", "개요", "목적", "방침", "기준", "방법",
    "문서", "파일", "한글", "자료", "경우", "대상", "기타", "포함",
];

/// 너무 흔한 불용어인지 확인한다.
pub fn is_common_stopword(word: &str) -> bool {
    let f = fold(word);
    if COMMON_STOPWORDS.contains(&f.as_str()) {
        return true;
    }
    // "보고서", "계획서", "안내서", "지침서" 등 접미사 "서" 제거 후 검사
    if f.ends_with("서") && f.chars().count() >= 3 {
        let base = &f[..f.len() - "서".len()];
        if COMMON_STOPWORDS.contains(&base) {
            return true;
        }
    }
    // "계획안", "추진안", "지침안" 등 접미사 "안" 제거 후 검사
    if f.ends_with("안") && f.chars().count() >= 3 {
        let base = &f[..f.len() - "안".len()];
        if COMMON_STOPWORDS.contains(&base) {
            return true;
        }
    }
    false
}

/// 단어가 순수 숫자로만 구성되어 있는지 확인한다.
fn is_pure_digits(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit())
}

/// 낱말이 유효한 주제 후보인지 판정한다 (2글자 이상, 순수 숫자 아님, 불용어 아님).
pub fn is_valid_topic_word(word: &str) -> bool {
    let f = fold(word);
    if f.chars().count() < MIN_TOPIC_CHARS {
        return false;
    }
    if is_pure_digits(&f) {
        return false;
    }
    !is_common_stopword(&f)
}

/// 문자열에서 특수문자, 언더스코어, 기호 등을 공백으로 치환하여 낱말들을 분리한다.
pub fn split_topic_tokens(s: &str) -> Vec<String> {
    let mut cleaned = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() || (0xAC00..=0xD7A3).contains(&(c as u32)) {
            cleaned.push(c);
        } else {
            cleaned.push(' ');
        }
    }
    tokens(&cleaned)
}

/// 파일명에서 주제 낱말들을 추출한다.
pub fn extract_path_terms(path: &str) -> Vec<String> {
    let p = path.replace('\\', "/");
    let filename = match p.rfind('/') {
        Some(pos) => &p[pos + 1..],
        None => &p,
    };
    // 확장자 제거
    let name_without_ext = match filename.rfind('.') {
        Some(pos) => &filename[..pos],
        None => filename,
    };
    let mut out = Vec::new();
    for tok in split_topic_tokens(name_without_ext) {
        if is_valid_topic_word(&tok) && !out.contains(&tok) {
            out.push(tok);
        }
    }
    out
}

/// 제목/소제목 문장 등에서 주제 낱말들을 추출한다.
pub fn extract_title_terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tok in split_topic_tokens(text) {
        if is_valid_topic_word(&tok) {
            out.push(tok);
        }
    }
    out
}

/// 본문 문장에서 보조 단어들을 추출한다 (중복 허용 또는 고유 목록).
pub fn extract_body_terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tok in split_topic_tokens(text) {
        let f = fold(&tok);
        if f.chars().count() >= MIN_TOPIC_CHARS && !is_pure_digits(&f) && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// 주제 항목.
#[derive(Clone, Debug, PartialEq)]
pub struct TopicItem {
    /// 주제 낱말
    pub term: String,
    /// 제목/파일명 출현 빈도 (TF)
    pub frequency: u32,
    /// 해당 주제가 나타난 문서 수 (DF)
    pub doc_count: u32,
    /// TF-IDF 순위 점수
    pub score: f32,
}

/// `;` 관련어 추천 항목.
#[derive(Clone, Debug, PartialEq)]
pub struct RelatedTopic {
    /// 관련어 낱말
    pub term: String,
    /// 함께 출현한 문서 수 (교집합 개수)
    pub co_count: u32,
    /// 연관 점수
    pub score: f32,
}

/// 기간별 급증 주제 항목.
#[derive(Clone, Debug, PartialEq)]
pub struct TrendingTopic {
    /// 주제 낱말
    pub term: String,
    /// 해당 기간 내 출현 문서 수
    pub period_count: u32,
    /// 전체 출현 문서 수
    pub total_count: u32,
    /// 급증 점수/비율
    pub surge_ratio: f32,
}

/// 단어별 통계 정보.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TermStat {
    /// 제목/파일명 출현 횟수 (TF)
    pub title_tf: u32,
    /// 제목/파일명에 출현한 문서 수 (DF)
    pub doc_count: u32,
    /// 본문 출현 횟수 (보조)
    pub body_tf: u32,
}

/// 문서별 단어 기록 (문서 변경·삭제 시 마이크로초 증감 차감용).
#[derive(Clone, Debug, PartialEq)]
pub struct DocTerms {
    pub title_terms: Vec<String>,
    pub body_terms: Vec<String>,
    pub date: i64,
    pub folder: String,
}

/// TF-IDF 점수 계산식:
/// "이 PC에서 많지만 모든 문서에 있지 않은 말"
/// - DF / N >= 0.5 (문서의 50% 이상에 출현)이면 흔한 말이므로 점수 0 (자동 제외).
/// - IDF = ln((N + 1) / (DF + 1)) + 0.1
/// - Score = TF * IDF
pub fn calculate_score(title_tf: u32, doc_count: u32, total_docs: u32) -> f32 {
    if total_docs == 0 || doc_count == 0 || title_tf == 0 {
        return 0.0;
    }
    // 전체 문서가 10개 이상일 때 50% 이상 문서에 등장하면 흔한 말로 간주하여 제외
    if total_docs >= 10 && (doc_count as f64 / total_docs as f64) >= 0.5 {
        return 0.0;
    }
    let n = total_docs as f64;
    let df = doc_count as f64;
    let idf = ((n + 1.0) / (df + 1.0)).ln() + 0.1;
    (title_tf as f64 * idf) as f32
}

/// 주제 빈도 순위표.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TopicTable {
    pub stats: HashMap<String, TermStat>,
    pub top_topics: Vec<TopicItem>,
    pub doc_entries: HashMap<u64, DocTerms>,
    pub total_docs: u32,
    pub dirty: bool,
}

impl TopicTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// 문서 단어 추가 (기존 문서가 있으면 먼저 제거 후 추가).
    pub fn add_document(
        &mut self,
        doc_id: u64,
        date: i64,
        folder: String,
        title_terms: Vec<String>,
        body_terms: Vec<String>,
    ) {
        self.remove_document(doc_id);
        self.total_docs += 1;

        let mut title_counts: HashMap<String, u32> = HashMap::new();
        for t in &title_terms {
            *title_counts.entry(t.clone()).or_default() += 1;
        }

        for (term, count) in title_counts {
            let stat = self.stats.entry(term).or_default();
            stat.title_tf += count;
            stat.doc_count += 1;
        }

        for b in &body_terms {
            let stat = self.stats.entry(b.clone()).or_default();
            stat.body_tf += 1;
        }

        self.doc_entries.insert(
            doc_id,
            DocTerms {
                title_terms,
                body_terms,
                date,
                folder,
            },
        );
        self.dirty = true;
    }

    /// 문서 단어 제거 (마이크로초 증감).
    pub fn remove_document(&mut self, doc_id: u64) -> bool {
        if let Some(entry) = self.doc_entries.remove(&doc_id) {
            self.total_docs = self.total_docs.saturating_sub(1);

            let mut title_counts: HashMap<String, u32> = HashMap::new();
            for t in &entry.title_terms {
                *title_counts.entry(t.clone()).or_default() += 1;
            }

            for (term, count) in title_counts {
                if let Some(stat) = self.stats.get_mut(&term) {
                    stat.title_tf = stat.title_tf.saturating_sub(count);
                    stat.doc_count = stat.doc_count.saturating_sub(1);
                    if stat.title_tf == 0 && stat.body_tf == 0 {
                        self.stats.remove(&term);
                    }
                }
            }

            for b in &entry.body_terms {
                if let Some(stat) = self.stats.get_mut(b) {
                    stat.body_tf = stat.body_tf.saturating_sub(1);
                    if stat.title_tf == 0 && stat.body_tf == 0 {
                        self.stats.remove(b);
                    }
                }
            }

            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// 상위 1,000개 순위표를 재계산한다.
    pub fn ensure_clean(&mut self) {
        if !self.dirty {
            return;
        }
        let total = self.total_docs;
        let mut list: Vec<TopicItem> = Vec::with_capacity(self.stats.len());

        for (term, stat) in &self.stats {
            if stat.title_tf == 0 || is_common_stopword(term) {
                continue;
            }
            let score = calculate_score(stat.title_tf, stat.doc_count, total);
            if score > 0.0 {
                list.push(TopicItem {
                    term: term.clone(),
                    frequency: stat.title_tf,
                    doc_count: stat.doc_count,
                    score,
                });
            }
        }

        list.sort_unstable_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.frequency.cmp(&a.frequency))
                .then_with(|| a.term.cmp(&b.term))
        });

        if list.len() > TOPIC_LIMIT {
            list.truncate(TOPIC_LIMIT);
        }

        self.top_topics = list;
        self.dirty = false;
    }

    /// 단어의 IDF 값을 조회한다.
    pub fn idf(&self, term: &str) -> f32 {
        let folded = fold(term);
        let df = self
            .stats
            .get(&folded)
            .map(|s| s.doc_count.max(1))
            .unwrap_or(1);
        let n = self.total_docs.max(1) as f64;
        (((n + 1.0) / (df as f64 + 1.0)).ln() + 0.1) as f32
    }

    /// 기능 1: 자동 완성 (빈도순).
    pub fn autocomplete(&mut self, prefix: &str, limit: usize) -> Vec<TopicItem> {
        let folded = fold(prefix);
        if folded.is_empty() {
            return Vec::new();
        }
        self.ensure_clean();

        let mut matched = Vec::new();
        for item in &self.top_topics {
            if item.term.starts_with(&folded) {
                matched.push(item.clone());
            }
        }

        if matched.len() < limit {
            for (term, stat) in &self.stats {
                if term.starts_with(&folded) && !matched.iter().any(|m| &m.term == term) {
                    if !is_common_stopword(term) && stat.title_tf > 0 {
                        let score = calculate_score(stat.title_tf, stat.doc_count, self.total_docs);
                        matched.push(TopicItem {
                            term: term.clone(),
                            frequency: stat.title_tf,
                            doc_count: stat.doc_count,
                            score,
                        });
                    }
                }
            }
        }

        // 빈도순 정렬
        matched.sort_unstable_by(|a, b| {
            b.frequency
                .cmp(&a.frequency)
                .then_with(|| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal))
                .then_with(|| a.term.cmp(&b.term))
        });

        if matched.len() > limit {
            matched.truncate(limit);
        }
        matched
    }

    /// 기능 2: 빈 검색어 첫 화면 주제 1~10위.
    pub fn top_topics(&mut self, limit: usize) -> Vec<TopicItem> {
        self.ensure_clean();
        let n = limit.min(self.top_topics.len());
        self.top_topics[..n].to_vec()
    }

    /// 기능 3: `;` 관련어 추천 (교집합 개수).
    pub fn related_topics(&self, word: &str, limit: usize) -> Vec<RelatedTopic> {
        let folded = fold(word);
        if folded.is_empty() {
            return Vec::new();
        }

        let mut matching_docs = Vec::new();
        for (&doc_id, entry) in &self.doc_entries {
            if entry.title_terms.contains(&folded) || entry.body_terms.contains(&folded) {
                matching_docs.push(doc_id);
            }
        }

        if matching_docs.is_empty() {
            return Vec::new();
        }

        let mut co_counts: HashMap<String, u32> = HashMap::new();
        for &doc_id in &matching_docs {
            if let Some(entry) = self.doc_entries.get(&doc_id) {
                let mut seen = std::collections::HashSet::new();
                for t in &entry.title_terms {
                    if t != &folded && !is_common_stopword(t) && seen.insert(t.as_str()) {
                        *co_counts.entry(t.clone()).or_default() += 1;
                    }
                }
                for b in &entry.body_terms {
                    if b != &folded && !is_common_stopword(b) && seen.insert(b.as_str()) {
                        *co_counts.entry(b.clone()).or_default() += 1;
                    }
                }
            }
        }

        let mut list: Vec<RelatedTopic> = co_counts
            .into_iter()
            .map(|(term, co_count)| {
                let idf = self.idf(&term);
                let score = (co_count as f32) * idf;
                RelatedTopic {
                    term,
                    co_count,
                    score,
                }
            })
            .collect();

        list.sort_unstable_by(|a, b| {
            b.co_count
                .cmp(&a.co_count)
                .then_with(|| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal))
                .then_with(|| a.term.cmp(&b.term))
        });

        if list.len() > limit {
            list.truncate(limit);
        }
        list
    }

    /// 기능 4: 기간별 급증 주제 (시간 비트맵/날짜 기반).
    pub fn trending_topics(
        &self,
        start_date: i64,
        end_date: i64,
        limit: usize,
    ) -> Vec<TrendingTopic> {
        let s_date = start_date.min(end_date);
        let e_date = start_date.max(end_date);
        let mut period_docs = 0u32;
        let mut term_period_counts: HashMap<String, u32> = HashMap::new();

        for entry in self.doc_entries.values() {
            if entry.date >= s_date && entry.date <= e_date {
                period_docs += 1;
                let mut seen = std::collections::HashSet::new();
                for t in &entry.title_terms {
                    if !is_common_stopword(t) && seen.insert(t.as_str()) {
                        *term_period_counts.entry(t.clone()).or_default() += 1;
                    }
                }
            }
        }

        if period_docs == 0 {
            return Vec::new();
        }

        let total_docs = self.total_docs.max(1) as f32;
        let mut list = Vec::new();
        for (term, p_count) in term_period_counts {
            if let Some(stat) = self.stats.get(&term) {
                let t_count = stat.doc_count.max(p_count);
                let p_rate = p_count as f32 / period_docs as f32;
                let t_rate = t_count as f32 / total_docs;
                let surge_ratio = if t_rate > 0.0 { p_rate / t_rate } else { 1.0 };
                list.push(TrendingTopic {
                    term,
                    period_count: p_count,
                    total_count: t_count,
                    surge_ratio,
                });
            }
        }

        list.sort_unstable_by(|a, b| {
            b.surge_ratio
                .partial_cmp(&a.surge_ratio)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.period_count.cmp(&a.period_count))
                .then_with(|| a.term.cmp(&b.term))
        });

        if list.len() > limit {
            list.truncate(limit);
        }
        list
    }

    /// 기능 5: 폴더 대표 주제.
    pub fn folder_topics(&self, folder: &str, limit: usize) -> Vec<TopicItem> {
        let norm_folder = folder.replace('\\', "/").trim_matches('/').to_string();
        let mut folder_docs = 0u32;
        let mut folder_tfs: HashMap<String, (u32, u32)> = HashMap::new();

        for entry in self.doc_entries.values() {
            let entry_folder = entry.folder.replace('\\', "/").trim_matches('/').to_string();
            let matches = if norm_folder.is_empty() {
                true
            } else {
                entry_folder == norm_folder || entry_folder.starts_with(&format!("{norm_folder}/"))
            };
            if matches {
                folder_docs += 1;
                let mut seen = std::collections::HashSet::new();
                for t in &entry.title_terms {
                    if !is_common_stopword(t) {
                        let e = folder_tfs.entry(t.clone()).or_insert((0, 0));
                        e.0 += 1;
                        if seen.insert(t.as_str()) {
                            e.1 += 1;
                        }
                    }
                }
            }
        }

        if folder_docs == 0 {
            return Vec::new();
        }

        let mut list = Vec::new();
        for (term, (tf, df)) in folder_tfs {
            let idf = self.idf(&term);
            let score = (tf as f32) * idf;
            list.push(TopicItem {
                term,
                frequency: tf,
                doc_count: df,
                score,
            });
        }

        list.sort_unstable_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.frequency.cmp(&a.frequency))
                .then_with(|| a.term.cmp(&b.term))
        });

        if list.len() > limit {
            list.truncate(limit);
        }
        list
    }

    /// 바이트로 직렬화한다 (KFTOP 포맷).
    pub fn to_bytes(&mut self) -> Result<Vec<u8>> {
        self.ensure_clean();
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        b.extend_from_slice(&self.total_docs.to_le_bytes());

        // stats 직렬화
        b.extend_from_slice(&(self.stats.len() as u32).to_le_bytes());
        for (term, stat) in &self.stats {
            let term_bytes = term.as_bytes();
            b.extend_from_slice(&(term_bytes.len() as u16).to_le_bytes());
            b.extend_from_slice(term_bytes);
            b.extend_from_slice(&stat.title_tf.to_le_bytes());
            b.extend_from_slice(&stat.doc_count.to_le_bytes());
            b.extend_from_slice(&stat.body_tf.to_le_bytes());
        }

        // doc_entries 직렬화
        b.extend_from_slice(&(self.doc_entries.len() as u32).to_le_bytes());
        for (&doc_id, entry) in &self.doc_entries {
            b.extend_from_slice(&doc_id.to_le_bytes());
            b.extend_from_slice(&entry.date.to_le_bytes());

            let folder_bytes = entry.folder.as_bytes();
            b.extend_from_slice(&(folder_bytes.len() as u16).to_le_bytes());
            b.extend_from_slice(folder_bytes);

            b.extend_from_slice(&(entry.title_terms.len() as u16).to_le_bytes());
            for t in &entry.title_terms {
                let tb = t.as_bytes();
                b.extend_from_slice(&(tb.len() as u16).to_le_bytes());
                b.extend_from_slice(tb);
            }

            b.extend_from_slice(&(entry.body_terms.len() as u16).to_le_bytes());
            for tb in &entry.body_terms {
                let bytes = tb.as_bytes();
                b.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                b.extend_from_slice(bytes);
            }
        }

        let h = fnv64(FNV_SEED, &b);
        b.extend_from_slice(&h.to_le_bytes());
        Ok(b)
    }

    /// 바이트로부터 역직렬화한다.
    pub fn from_bytes(b: &[u8]) -> Result<TopicTable> {
        if b.len() < 24 || &b[0..8] != MAGIC {
            return corrupt("topics: bad magic");
        }
        let stored_hash = u64::from_le_bytes(
            b[b.len() - 8..]
                .try_into()
                .map_err(|_| Error::Corrupt("topics: truncated".into()))?,
        );
        if fnv64(FNV_SEED, &b[..b.len() - 8]) != stored_hash {
            return corrupt("topics: checksum");
        }

        let mut pos = 8;
        let mut take = |n: usize| -> Result<&[u8]> {
            if pos + n <= b.len() - 8 {
                let s = &b[pos..pos + n];
                pos += n;
                Ok(s)
            } else {
                corrupt("topics: truncated")
            }
        };

        let format = u32::from_le_bytes(take(4)?.try_into().unwrap());
        if format != FORMAT_VERSION {
            return Err(Error::FormatVersion {
                found: format,
                expected: FORMAT_VERSION,
            });
        }

        let total_docs = u32::from_le_bytes(take(4)?.try_into().unwrap());
        let stats_len = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
        let mut stats = HashMap::with_capacity(stats_len);

        for _ in 0..stats_len {
            let term_len = u16::from_le_bytes(take(2)?.try_into().unwrap()) as usize;
            let term = std::str::from_utf8(take(term_len)?)
                .map_err(|_| Error::Corrupt("topics: bad utf8".into()))?
                .to_string();
            let title_tf = u32::from_le_bytes(take(4)?.try_into().unwrap());
            let doc_count = u32::from_le_bytes(take(4)?.try_into().unwrap());
            let body_tf = u32::from_le_bytes(take(4)?.try_into().unwrap());
            stats.insert(
                term,
                TermStat {
                    title_tf,
                    doc_count,
                    body_tf,
                },
            );
        }

        let docs_len = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
        let mut doc_entries = HashMap::with_capacity(docs_len);

        for _ in 0..docs_len {
            let doc_id = u64::from_le_bytes(take(8)?.try_into().unwrap());
            let date = i64::from_le_bytes(take(8)?.try_into().unwrap());
            let folder_len = u16::from_le_bytes(take(2)?.try_into().unwrap()) as usize;
            let folder = std::str::from_utf8(take(folder_len)?)
                .map_err(|_| Error::Corrupt("topics: bad utf8".into()))?
                .to_string();

            let title_terms_len = u16::from_le_bytes(take(2)?.try_into().unwrap()) as usize;
            let mut title_terms = Vec::with_capacity(title_terms_len);
            for _ in 0..title_terms_len {
                let tl = u16::from_le_bytes(take(2)?.try_into().unwrap()) as usize;
                let t = std::str::from_utf8(take(tl)?)
                    .map_err(|_| Error::Corrupt("topics: bad utf8".into()))?
                    .to_string();
                title_terms.push(t);
            }

            let body_terms_len = u16::from_le_bytes(take(2)?.try_into().unwrap()) as usize;
            let mut body_terms = Vec::with_capacity(body_terms_len);
            for _ in 0..body_terms_len {
                let bl = u16::from_le_bytes(take(2)?.try_into().unwrap()) as usize;
                let bt = std::str::from_utf8(take(bl)?)
                    .map_err(|_| Error::Corrupt("topics: bad utf8".into()))?
                    .to_string();
                body_terms.push(bt);
            }

            doc_entries.insert(
                doc_id,
                DocTerms {
                    title_terms,
                    body_terms,
                    date,
                    folder,
                },
            );
        }

        let mut table = TopicTable {
            stats,
            top_topics: Vec::new(),
            doc_entries,
            total_docs,
            dirty: true,
        };
        table.ensure_clean();
        Ok(table)
    }

    /// 파일에 원자적으로 저장한다 (임시 파일 쓰기 후 rename).
    pub fn save(&mut self, dir: &Path) -> Result<()> {
        let bytes = self.to_bytes()?;
        let tmp = dir.join(format!("{TOPICS_TMP_PREFIX}{}", std::process::id()));
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        let target = dir.join(TOPICS_FILE_NAME);
        std::fs::rename(&tmp, &target)?;
        Ok(())
    }

    /// 파일로부터 로드한다. 파일이 없으면 새 TopicTable을 반환한다.
    pub fn load(dir: &Path) -> Result<TopicTable> {
        let target = dir.join(TOPICS_FILE_NAME);
        if !target.exists() {
            return Ok(TopicTable::new());
        }
        let bytes = std::fs::read(&target)?;
        TopicTable::from_bytes(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_and_stopwords() {
        assert!(is_common_stopword("계획"));
        assert!(is_common_stopword("보고서")); // "보고서" 중 "보고"
        assert!(!is_common_stopword("도시재생"));

        let path_terms = extract_path_terms("총무과/2026/스마트_도시_추진_계획서.hwp");
        assert!(path_terms.contains(&"스마트".to_string()));
        assert!(path_terms.contains(&"도시".to_string()));
        // "추진", "계획서" 등 불용어 제외 확인
        assert!(!path_terms.contains(&"추진".to_string()));
        assert!(!path_terms.contains(&"계획".to_string()));
    }

    #[test]
    fn test_topic_table_round_trip() {
        let mut table = TopicTable::new();
        // 문서 5개 생성
        // "스마트": 문서 1, 2, 3 출현 (DF=3, TF=5)
        // "도시": 문서 1 출현 (DF=1, TF=1)
        // "모빌리티": 문서 2 출현 (DF=1, TF=1)
        // "공통단어": 문서 1~5 모두 출현 (DF=5) -> 흔한 말(DF/N >= 0.5)로 제외 또는 점수 낮음
        table.add_document(
            1,
            20260101,
            "총무과".to_string(),
            vec!["스마트".to_string(), "스마트".to_string(), "도시".to_string()],
            vec!["인프라".to_string(), "구축".to_string()],
        );
        table.add_document(
            2,
            20260102,
            "총무과".to_string(),
            vec!["스마트".to_string(), "모빌리티".to_string()],
            vec!["인프라".to_string()],
        );
        table.add_document(
            3,
            20260103,
            "기획과".to_string(),
            vec!["스마트".to_string(), "스마트".to_string()],
            vec!["혁신".to_string()],
        );
        table.add_document(
            4,
            20260104,
            "기획과".to_string(),
            vec!["재정".to_string()],
            vec!["자립".to_string()],
        );
        table.add_document(
            5,
            20260105,
            "복지과".to_string(),
            vec!["복지".to_string()],
            vec!["지원".to_string()],
        );

        let bytes = table.to_bytes().expect("to_bytes");
        let mut loaded = TopicTable::from_bytes(&bytes).expect("from_bytes");

        assert_eq!(loaded.total_docs, 5);
        let top = loaded.top_topics(10);
        assert!(!top.is_empty());
        assert_eq!(top[0].term, "스마트"); // 빈도 및 TF-IDF 최고점
    }
}
