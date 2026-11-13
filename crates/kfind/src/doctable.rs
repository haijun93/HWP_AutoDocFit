//! 서비스 문서 표(KFS1): 앱의 문서 열쇠(doc_key) ↔ kfind-core 문서 ID와 문서 정보(경로·크기·수정 시각·날짜·범위·
//! 위계 출처·위계 규칙 판·문장 위치)를 사용자 색인 폴더의 `doctable.jsonl`에 둔다.
//!
//! 저장: 줄마다 JSON 하나인 추가 기록(`put`·`del`·`next`). 열 때 처음부터 다시 읽고, 기록 줄이 살아 있는 문서의
//! 2배 + 1,000을 넘으면 임시 파일에 다시 써서 이름을 바꾼다(원자적 교체). 마지막 줄이 깨졌으면(쓰다가 꺼짐) 그 줄만
//! 버린다. 중간 줄이 깨졌으면 손상으로 본다.
//!
//! 순서 규칙(색인과 표가 어긋날 때 안전한 쪽): 색인 커밋 → 표 기록. 사이에서 꺼지면 색인에만 있는 문서가 남는데,
//! 범위 질의는 표로 허용 집합을 만들므로 보이지 않고, 같은 ID를 다시 받으면 덮어쓴다. 표가 예전 정보(크기·수정 시각)를
//! 들고 있으면 앱이 낡았다고 보고 다시 보낸다.
//!
//! 위계 출처 전이(Q-A2): `py`(앱 Python 규칙)는 무엇이든 덮는다. `rs`(서비스 자체 추출)는 같은 크기·수정 시각의
//! `py` 문서를 덮지 못한다(거절). 파일이 바뀌었으면(크기나 수정 시각이 다르면) 덮을 수 있다.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "doctable.jsonl";
const TMP_NAME: &str = "doctable.jsonl.tmp";

/// 문서 위계를 누가 정했나.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelSrc {
    /// 앱 Python 규칙(`classify_paragraph_levels`).
    Py,
    /// 서비스 자체 추출(klib `classify_levels.rs`). 앱이 켜지면 `py`로 바꾼다.
    Rs,
}

impl LevelSrc {
    pub fn name(self) -> &'static str {
        match self {
            LevelSrc::Py => "py",
            LevelSrc::Rs => "rs",
        }
    }
}

/// 문서 정보(문장 위치 빼고). `ingest` 요청의 머리와 같다.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocMeta {
    /// 원본 경로(메모는 빈 글).
    pub path: String,
    pub size: u64,
    /// 수정 시각(앱이 쓰는 값 그대로, 같음 비교에만 쓴다).
    pub mtime: i64,
    /// 순위 날짜(밀리초, 클수록 최신, 0 = 없음).
    pub date: i64,
    /// 기준 날짜 글("2026-11-13"·"2026-11"·"2026", `after`·`before` 거르기). 없으면 None.
    #[serde(default)]
    pub ref_date: Option<String>,
    /// 범위: `tm`, `box:<id>`, `nb:<id>:src:<sid>` 등.
    pub scopes: Vec<String>,
    pub level_src: LevelSrc,
    pub level_rule: u32,
}

/// 표 한 줄.
#[derive(Clone, Debug)]
pub struct DocEntry {
    pub id: u64,
    pub meta: DocMeta,
    /// 문장 순번(ord)마다 (문단, 문단 안 시작 코드포인트, 길이) — 세 개씩 이어 붙인 것.
    pub spans: Vec<u32>,
}

impl DocEntry {
    /// 문장 순번의 (문단, 시작, 길이).
    pub fn span(&self, ord: u32) -> Option<(u32, u32, u32)> {
        let i = ord as usize * 3;
        self.spans.get(i..i + 3).map(|s| (s[0], s[1], s[2]))
    }

    /// 확장자(소문자, 점 없음). 경로가 없으면 빈 글.
    pub fn ext(&self) -> String {
        Path::new(&self.meta.path)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Line {
    Put {
        key: String,
        id: u64,
        meta: DocMeta,
        spans: Vec<u32>,
    },
    Del {
        key: String,
    },
    Next {
        id: u64,
    },
}

/// 위계 출처 전이 규칙. 거절이면 이유.
pub fn check_replace(
    old: Option<&DocMeta>,
    new: &DocMeta,
) -> std::result::Result<(), &'static str> {
    match old {
        Some(o)
            if o.level_src == LevelSrc::Py
                && new.level_src == LevelSrc::Rs
                && o.size == new.size
                && o.mtime == new.mtime =>
        {
            Err("level_src rs cannot replace py for the same file version")
        }
        _ => Ok(()),
    }
}

/// 범위 글자 검사: 1~200바이트, 공백·제어 글자 없음.
pub fn valid_scope(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// 문서 범위 `doc`이 요청 범위 `want` 안인가: 같거나, `want` 뒤에 `:`로 이어지는 하위 범위.
/// `nb:5`는 `nb:5:src:7`을 포함하고 `nb:50`은 포함하지 않는다.
pub fn scope_contains(want: &str, doc: &str) -> bool {
    doc == want
        || (doc.len() > want.len() && doc.starts_with(want) && doc.as_bytes()[want.len()] == b':')
}

/// 질의 거르개(Python `parse_query` 결과 그대로, 해석은 앱이 한다).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Filters {
    /// 확장자 중 하나(소문자, 앞 점은 무시).
    pub ext: Vec<String>,
    /// 경로에 모두 들어 있어야 하는 글(대소문자 무시).
    pub path: Vec<String>,
    /// 기준 날짜 글이 이 값 이상(글자 비교, 날짜 없는 문서는 뺀다).
    pub after: Option<String>,
    /// 기준 날짜 글이 이 값 미만.
    pub before: Option<String>,
    /// 제외어(이 낱말이 붙어 있는 그대로 있는 문서를 뺀다).
    pub exclude: Vec<String>,
    /// 구문(나누지 않고 붙어 있는 그대로 찾는 낱말).
    pub phrases: Vec<String>,
}

impl Filters {
    /// 문서 표로 판정하는 거르개가 있는가(제외어·구문은 엔진이 한다).
    pub fn uses_table(&self) -> bool {
        !self.ext.is_empty()
            || !self.path.is_empty()
            || self.after.is_some()
            || self.before.is_some()
    }

    pub fn admits(&self, e: &DocEntry) -> bool {
        if !self.ext.is_empty() {
            let ext = e.ext();
            if !self
                .ext
                .iter()
                .any(|x| x.trim_start_matches('.').to_lowercase() == ext)
            {
                return false;
            }
        }
        if !self.path.is_empty() {
            let p = e.meta.path.to_lowercase();
            if !self.path.iter().all(|v| p.contains(&v.to_lowercase())) {
                return false;
            }
        }
        let rd = e.meta.ref_date.as_deref().filter(|d| !d.is_empty());
        if let Some(a) = &self.after {
            if rd.is_none_or(|d| d < a.as_str()) {
                return false;
            }
        }
        if let Some(b) = &self.before {
            if rd.is_none_or(|d| d >= b.as_str()) {
                return false;
            }
        }
        true
    }
}

pub struct DocTable {
    path: PathBuf,
    docs: BTreeMap<String, DocEntry>,
    by_id: HashMap<u64, String>,
    next_id: u64,
    /// 기록 파일의 줄 수.
    lines: usize,
}

impl DocTable {
    /// 폴더의 문서 표를 연다(없으면 빈 표, 파일은 처음 기록할 때 만든다).
    pub fn load(dir: &Path) -> Result<DocTable> {
        let path = dir.join(FILE_NAME);
        let mut t = DocTable {
            path,
            docs: BTreeMap::new(),
            by_id: HashMap::new(),
            next_id: 1,
            lines: 0,
        };
        let file = match File::open(&t.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(t),
            Err(e) => return Err(e.into()),
        };
        let mut raw: Vec<Vec<u8>> = Vec::new();
        let mut reader = BufReader::new(file);
        loop {
            let mut buf = Vec::new();
            if reader.read_until(b'\n', &mut buf)? == 0 {
                break;
            }
            raw.push(buf);
        }
        let n = raw.len();
        // 마지막 줄이 줄바꿈 없이 끝났으면(쓰다가 꺼짐) 다음 기록이 그 뒤에 붙지 않게 다시 쓴다.
        let mut rewrite = raw.last().is_some_and(|b| b.last() != Some(&b'\n'));
        for (i, buf) in raw.into_iter().enumerate() {
            match serde_json::from_slice::<Line>(&buf) {
                Ok(line) => t.apply(line),
                // 마지막 줄만 깨진 채로 둘 수 있다(쓰다가 꺼짐). 그 줄을 버리고 다시 쓴다.
                Err(_) if i + 1 == n => {
                    rewrite = true;
                    break;
                }
                Err(e) => return Err(format!("doctable line {} is corrupt: {e}", i + 1).into()),
            }
            t.lines += 1;
        }
        if rewrite {
            t.compact()?;
        }
        Ok(t)
    }

    fn apply(&mut self, line: Line) {
        match line {
            Line::Put {
                key,
                id,
                meta,
                spans,
            } => {
                self.next_id = self.next_id.max(id + 1);
                if let Some(old) = self.docs.insert(key.clone(), DocEntry { id, meta, spans }) {
                    if old.id != id {
                        self.by_id.remove(&old.id);
                    }
                }
                self.by_id.insert(id, key);
            }
            Line::Del { key } => {
                if let Some(old) = self.docs.remove(&key) {
                    self.by_id.remove(&old.id);
                }
            }
            Line::Next { id } => self.next_id = self.next_id.max(id),
        }
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&DocEntry> {
        self.docs.get(key)
    }

    pub fn key_of(&self, id: u64) -> Option<&str> {
        self.by_id.get(&id).map(String::as_str)
    }

    pub fn entry_of(&self, id: u64) -> Option<(&str, &DocEntry)> {
        let key = self.by_id.get(&id)?;
        Some((key.as_str(), self.docs.get(key)?))
    }

    /// 열쇠 순서로 전부.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &DocEntry)> {
        self.docs.iter().map(|(k, e)| (k.as_str(), e))
    }

    /// `after` 다음 열쇠부터(열쇠 순서).
    pub fn iter_after<'a>(
        &'a self,
        after: Option<&'a str>,
    ) -> impl Iterator<Item = (&'a str, &'a DocEntry)> + 'a {
        use std::ops::Bound;
        let lower = match after {
            Some(a) => Bound::Excluded(a.to_owned()),
            None => Bound::Unbounded,
        };
        self.docs
            .range((lower, Bound::Unbounded))
            .map(|(k, e)| (k.as_str(), e))
    }

    /// 이 열쇠의 문서 ID: 있으면 그 ID, 없으면 새 ID(기록은 `put`에서).
    pub fn id_for(&mut self, key: &str) -> u64 {
        if let Some(e) = self.docs.get(key) {
            return e.id;
        }
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 문서를 적는다(색인 커밋 뒤에 부른다).
    pub fn put(&mut self, key: &str, id: u64, meta: DocMeta, spans: Vec<u32>) -> Result<()> {
        let line = Line::Put {
            key: key.to_owned(),
            id,
            meta,
            spans,
        };
        self.append(&line)?;
        self.apply(line);
        Ok(())
    }

    /// 문서를 지운다(색인 커밋 뒤에 부른다). 있었으면 그 ID.
    pub fn remove(&mut self, key: &str) -> Result<Option<u64>> {
        let Some(id) = self.docs.get(key).map(|e| e.id) else {
            return Ok(None);
        };
        let line = Line::Del {
            key: key.to_owned(),
        };
        self.append(&line)?;
        self.apply(line);
        Ok(Some(id))
    }

    /// 표를 비운다(색인을 새로 만들 때).
    pub fn clear(&mut self) -> Result<()> {
        self.docs.clear();
        self.by_id.clear();
        self.compact()
    }

    fn append(&mut self, line: &Line) -> Result<()> {
        if self.lines > 2 * self.docs.len() + 1000 {
            self.compact()?;
        }
        let mut bytes = serde_json::to_vec(line)?;
        bytes.push(b'\n');
        let mut f = File::options().create(true).append(true).open(&self.path)?;
        f.write_all(&bytes)?;
        f.flush()?;
        self.lines += 1;
        Ok(())
    }

    /// 살아 있는 문서만 임시 파일에 쓰고 이름을 바꾼다.
    fn compact(&mut self) -> Result<()> {
        let tmp = self.path.with_file_name(TMP_NAME);
        let mut out = Vec::new();
        let mut push = |line: &Line| -> Result<()> {
            out.extend(serde_json::to_vec(line)?);
            out.push(b'\n');
            Ok(())
        };
        push(&Line::Next { id: self.next_id })?;
        for (key, e) in &self.docs {
            push(&Line::Put {
                key: key.clone(),
                id: e.id,
                meta: e.meta.clone(),
                spans: e.spans.clone(),
            })?;
        }
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&out)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;
        self.lines = self.docs.len() + 1;
        Ok(())
    }

    /// 위계 규칙 판별 문서 수: `py4`·`rs` 같은 이름(출처 + 판)별.
    pub fn level_rule_counts(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for e in self.docs.values() {
            let name = match e.meta.level_src {
                LevelSrc::Py => format!("py{}", e.meta.level_rule),
                LevelSrc::Rs => "rs".to_owned(),
            };
            *out.entry(name).or_insert(0) += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(src: LevelSrc, size: u64, mtime: i64) -> DocMeta {
        DocMeta {
            path: r"C:\문서\보고.HWP".into(),
            size,
            mtime,
            date: 5,
            ref_date: Some("2026-11-13".into()),
            scopes: vec!["tm".into()],
            level_src: src,
            level_rule: 4,
        }
    }

    #[test]
    fn level_src_transitions() {
        let py = meta(LevelSrc::Py, 10, 1);
        let rs = meta(LevelSrc::Rs, 10, 1);
        assert!(check_replace(None, &rs).is_ok());
        assert!(check_replace(Some(&rs), &py).is_ok());
        assert!(check_replace(Some(&py), &py).is_ok());
        assert!(check_replace(Some(&rs), &rs).is_ok());
        assert!(check_replace(Some(&py), &rs).is_err());
        // 파일이 바뀌었으면 rs가 들어갈 수 있다.
        assert!(check_replace(Some(&py), &meta(LevelSrc::Rs, 11, 1)).is_ok());
        assert!(check_replace(Some(&py), &meta(LevelSrc::Rs, 10, 2)).is_ok());
    }

    #[test]
    fn scope_matching() {
        assert!(scope_contains("nb:5", "nb:5"));
        assert!(scope_contains("nb:5", "nb:5:src:7"));
        assert!(!scope_contains("nb:5", "nb:50"));
        assert!(!scope_contains("nb:5:src:7", "nb:5"));
        assert!(
            valid_scope("box:3")
                && !valid_scope("")
                && !valid_scope("a b")
                && !valid_scope(&"x".repeat(201))
        );
    }

    #[test]
    fn filters_follow_python_semantics() {
        let e = DocEntry {
            id: 1,
            meta: meta(LevelSrc::Py, 1, 1),
            spans: vec![],
        };
        let f = |j: &str| -> Filters { serde_json::from_str(j).unwrap() };
        assert!(f(r#"{"ext":[".hwp"]}"#).admits(&e));
        assert!(!f(r#"{"ext":["docx"]}"#).admits(&e));
        assert!(f(r#"{"path":["문서","보고"]}"#).admits(&e));
        assert!(!f(r#"{"path":["문서","예산"]}"#).admits(&e));
        assert!(f(r#"{"after":"2026-11"}"#).admits(&e));
        assert!(!f(r#"{"before":"2026-11"}"#).admits(&e));
        assert!(f(r#"{"before":"2027"}"#).admits(&e));
        let none = DocEntry {
            id: 2,
            meta: DocMeta {
                ref_date: None,
                ..meta(LevelSrc::Py, 1, 1)
            },
            spans: vec![],
        };
        assert!(!f(r#"{"after":"2000"}"#).admits(&none));
        assert!(serde_json::from_str::<Filters>(r#"{"near":3}"#).is_err());
    }

    #[test]
    fn persists_reloads_and_compacts() {
        let d = tempfile::tempdir().unwrap();
        let mut t = DocTable::load(d.path()).unwrap();
        let a = t.id_for("a");
        t.put("a", a, meta(LevelSrc::Py, 1, 1), vec![0, 0, 3])
            .unwrap();
        let b = t.id_for("b");
        assert_ne!(a, b);
        t.put("b", b, meta(LevelSrc::Rs, 1, 1), vec![]).unwrap();
        assert_eq!(t.id_for("a"), a);
        assert_eq!(t.remove("b").unwrap(), Some(b));
        assert_eq!(t.remove("b").unwrap(), None);
        let t2 = DocTable::load(d.path()).unwrap();
        assert_eq!(t2.len(), 1);
        assert_eq!(t2.get("a").unwrap().span(0), Some((0, 0, 3)));
        assert_eq!(t2.key_of(a), Some("a"));
        assert_eq!(t2.key_of(b), None);
        // 지운 문서의 ID를 다시 쓰지 않는다.
        let mut t2 = t2;
        assert!(t2.id_for("c") > b);
        // 기록이 쌓이면 다시 써서 줄인다.
        for i in 0..1100 {
            t2.put(
                "a",
                a,
                DocMeta {
                    size: i,
                    ..meta(LevelSrc::Py, 1, 1)
                },
                vec![],
            )
            .unwrap();
        }
        let lines = std::fs::read_to_string(d.path().join(FILE_NAME))
            .unwrap()
            .lines()
            .count();
        assert!(lines < 1010, "{lines}");
        let t3 = DocTable::load(d.path()).unwrap();
        assert_eq!(t3.get("a").unwrap().meta.size, 1099);
        assert!(t3.next_id > b);
        assert_eq!(t3.level_rule_counts().get("py4"), Some(&1));
    }

    #[test]
    fn torn_last_line_is_dropped_but_middle_corruption_fails() {
        let d = tempfile::tempdir().unwrap();
        let mut t = DocTable::load(d.path()).unwrap();
        t.put("a", 1, meta(LevelSrc::Py, 1, 1), vec![]).unwrap();
        let p = d.path().join(FILE_NAME);
        let mut text = std::fs::read(&p).unwrap();
        text.extend_from_slice(br#"{"op":"put","key":"b","id":2,"me"#);
        std::fs::write(&p, &text).unwrap();
        let t = DocTable::load(d.path()).unwrap();
        assert_eq!(t.len(), 1);
        assert!(
            !std::fs::read_to_string(&p)
                .unwrap()
                .contains(r#""key":"b""#)
        );
        let mut bad = b"garbage\n".to_vec();
        bad.extend(std::fs::read(&p).unwrap());
        std::fs::write(&p, bad).unwrap();
        assert!(DocTable::load(d.path()).is_err());
    }
}
