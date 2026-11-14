//! kfind-core: 공용 본문 문장 검색 엔진.
//!
//! - 색인: 비교형 글의 2·3글자 조각 → 문장 번호 roaring 비트맵, 조각 사전 fst, 문장 → (문서, 문단, 위계 순위) 표.
//! - 저장: 조각(세그먼트) 파일은 한 번 쓰면 바뀌지 않고, 목록 파일(MANIFEST)만 임시 파일 → rename으로 교체한다.
//! - 질의: 2글자부터, 같은 문장 안 다중 낱말, 붙여 쓴 검색어 나누기, 붙어 있는 일치 우선, 상위 N건.
//! - 증분: 문서 추가·수정·삭제는 새 조각 + 지운 문장 비트맵, 합치기는 배경 스레드에서.
//! - 견고성: 공개 진입점은 패닉을 `Error::Panic`으로 바꾸고, 질의에는 시간 한도·취소가 있다.
//! - 문장 나누기: `split_sentences`(규칙 판 `SENTENCE_RULE_VERSION`, Python 기준 `tools/sentence_ref.py`).
//!
//! 규칙·형식 설명: `docs/KC1_KFIND_CORE_2026-11-12.md`.
mod docmap;
mod fold;
mod fold_table;
mod index;
mod manifest;
mod qcache;
mod query;
mod segment;
mod sentence;
mod textcode;
mod topic;
mod writer;

pub use docmap::{DateCount, DateInterval, DocumentMeta, FolderCount, SearchScope, SentenceRole};
pub use fold::{FOLD_RULE_VERSION, UNICODE_VERSION, fold, fold_into, tokens};
pub use index::{DocHit, Index, Order, Ranking, SearchCounts, SearchOptions, SearchResult, SentenceMatch, SentenceRef};
pub use qcache::{QueryCacheOptions, QueryCacheStats};
pub use query::{MIN_WORD_CHARS, ParsedQuery, QueryWord, SplitMode, parse_groups, parse_query, stem_suffixes};
pub use segment::FORMAT_VERSION;
pub use topic::{RelatedTopic, TopicItem, TopicTable, TrendingTopic, is_common_stopword};
/// 문서 범위(`SearchOptions::allow`·`deny`)의 문서 ID 집합. 호출하는 쪽이 같은 판의 roaring을 맞추지 않아도 되게 다시 낸다.
pub use roaring::RoaringTreemap;
pub use sentence::{SENTENCE_RULE_VERSION, SentenceSpan, index_sentences, is_indexable, split_sentences};
pub use writer::{MergeJob, MergeOutput, MergePolicy, Sentence, Writer, WriterOptions};

use std::fmt;

/// 엔진 오류. 종류 이름을 그대로 로그·화면 상태로 쓸 수 있게 나눈다.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// 파일이 형식에 맞지 않거나 깨졌다.
    Corrupt(String),
    /// 색인 형식 버전이 이 엔진과 다르다(다시 만들어야 함).
    FormatVersion { found: u32, expected: u32 },
    /// 비교형 규칙 버전이 다르다(다시 색인해야 함).
    RuleVersion { found: u32, expected: u32 },
    /// 다른 쓰기 쪽이 색인 폴더를 잡고 있다.
    Locked,
    /// 질의 시간 한도를 넘었다.
    Timeout,
    /// 호출한 쪽이 취소했다(새 검색어 도착).
    Cancelled,
    /// 엔진 안에서 패닉이 났다. 프로세스는 계속 돈다.
    Panic(String),
    /// 잘못된 인자(위계 순위 범위 등).
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Corrupt(m) => write!(f, "corrupt index: {m}"),
            Error::FormatVersion { found, expected } => write!(f, "index format version {found}, expected {expected}"),
            Error::RuleVersion { found, expected } => write!(f, "fold rule version {found}, expected {expected}"),
            Error::Locked => write!(f, "index is locked by another writer"),
            Error::Timeout => write!(f, "query time limit exceeded"),
            Error::Cancelled => write!(f, "query cancelled"),
            Error::Panic(m) => write!(f, "panic isolated: {m}"),
            Error::Invalid(m) => write!(f, "invalid argument: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// 패닉 격리 경계. 안쪽 패닉을 `Error::Panic`으로 바꾼다(`panic = "unwind"` 필요).
/// 엔진의 공개 진입점(검색·문서 추가·커밋·합치기)은 모두 이 경계 안에서 돈다. FFI·파이프 계층도 같은 함수를 쓰면 된다.
pub fn guard<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Err(Error::Panic(msg))
        }
    }
}

pub(crate) fn corrupt<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Corrupt(msg.into()))
}

pub const FNV_SEED: u64 = 0xcbf29ce484222325;

/// FNV-1a 64(파일 무결성 확인과 시험 지문에 쓴다). 이어서 계산하려면 앞 결과를 `seed`로 넘긴다.
pub fn fnv64(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = seed;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
