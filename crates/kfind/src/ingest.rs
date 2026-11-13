//! `ingest` 요청 처리(KFS1): 64KB 넘는 문서의 조각(part) 모으기와 문단 → 색인 문장 만들기.
//!
//! 조각 모으기 상태(사용자 SID·doc_key마다 하나):
//! - 없음 → (part 0 / parts n) → 모으는 중(next = 1) → … → (part n-1) → 완성(넣기) → 없음
//! - 불가능한 전이(거절하고 모으던 것을 버림): 순서가 틀린 조각, 머리(경로·크기 등)나 parts가 다른 조각,
//!   상한 초과(문서 하나 64MB, 사용자당 8건, 전체 64건). part 0이 다시 오면 처음부터 다시 모은다.
//! - 60초 동안 다음 조각이 없으면 버린다(다음 요청 때 정리).
//!
//! 문단: `[{p, lv, t}]`. 이어진 두 항목의 `p`가 같으면 한 문단의 이어진 글로 본다(64KB 넘는 문단을 조각 경계에서
//! 나눠 보낼 수 있게). 이때 `lv`도 같아야 한다. 문장은 `kfind_core::index_sentences`(기호만 있는 문장 제외)로 나눈다.
use crate::Result;
use crate::doctable::DocMeta;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 문서 하나의 글 상한(조각 합).
pub const MAX_DOC_BYTES: usize = 64 << 20;
pub const MAX_PARTS: u32 = 4096;
pub const MAX_LEVEL: u8 = 8;
const MAX_UPLOADS_PER_USER: usize = 8;
const MAX_UPLOADS: usize = 64;
const UPLOAD_IDLE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Para {
    pub p: u32,
    pub lv: u8,
    pub t: String,
}

/// doc_key 검사: 1~1024바이트, 제어 글자 없음.
pub fn valid_doc_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 1024 && !k.chars().any(char::is_control)
}

/// 이어진 같은 문단 번호를 하나로 합치고 위계 범위를 검사한다.
pub fn merge_paras(paras: Vec<Para>) -> Result<Vec<Para>> {
    let mut out: Vec<Para> = Vec::with_capacity(paras.len());
    for para in paras {
        if para.lv > MAX_LEVEL {
            return Err(format!("paragraph level {} exceeds {MAX_LEVEL}", para.lv).into());
        }
        match out.last_mut() {
            Some(last) if last.p == para.p => {
                if last.lv != para.lv {
                    return Err(format!("paragraph {} continues with a different level", para.p).into());
                }
                last.t.push_str(&para.t);
            }
            _ => out.push(para),
        }
    }
    Ok(out)
}

/// 문단 → 색인 문장과 문장 위치(문장 순번마다 문단·시작·길이 세 개씩).
pub fn sentences(paras: &[Para]) -> (Vec<kfind_core::Sentence<'_>>, Vec<u32>) {
    let mut out = Vec::new();
    let mut spans = Vec::new();
    for para in paras {
        for sp in kfind_core::index_sentences(&para.t) {
            out.push(kfind_core::Sentence { para: para.p, level: para.lv, text: sp.text(&para.t) });
            spans.extend([para.p, sp.start, sp.len]);
        }
    }
    (out, spans)
}

struct Upload {
    meta: DocMeta,
    parts: u32,
    next: u32,
    paras: Vec<Para>,
    bytes: usize,
    touched: Instant,
}

/// 조각 모으기 표.
#[derive(Default)]
pub struct Uploads {
    map: HashMap<(String, String), Upload>,
}

/// 조각 하나를 받은 결과.
#[derive(Debug, PartialEq)]
pub enum Accepted {
    /// 더 받아야 한다(받은 조각 수, 전체).
    Waiting { received: u32, parts: u32 },
    /// 다 모였다.
    Complete { meta: DocMeta, paras: Vec<Para> },
}

fn text_bytes(paras: &[Para]) -> usize {
    paras.iter().map(|p| p.t.len()).sum()
}

impl Uploads {
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 조각 하나를 받는다. `part`·`parts`가 없으면 한 번에 온 문서.
    pub fn accept(&mut self, sid: &str, key: &str, meta: DocMeta, paras: Vec<Para>, part: Option<u32>, parts: Option<u32>, now: Instant) -> Result<Accepted> {
        self.map.retain(|_, u| now.duration_since(u.touched) < UPLOAD_IDLE);
        let bytes = text_bytes(&paras);
        let (part, parts) = match (part, parts) {
            (None, None) => (0, 1),
            (Some(a), Some(b)) => (a, b),
            _ => return Err("part and parts must be given together".into()),
        };
        if parts == 0 || parts > MAX_PARTS || part >= parts {
            return Err(format!("part {part} of {parts} is out of range (parts 1..={MAX_PARTS})").into());
        }
        if bytes > MAX_DOC_BYTES {
            return Err("document exceeds 64MB".into());
        }
        let id = (sid.to_owned(), key.to_owned());
        if parts == 1 {
            self.map.remove(&id);
            return Ok(Accepted::Complete { meta, paras });
        }
        if part == 0 {
            self.map.remove(&id);
            let mine = self.map.keys().filter(|(s, _)| s == sid).count();
            if mine >= MAX_UPLOADS_PER_USER || self.map.len() >= MAX_UPLOADS {
                return Err("too many documents being uploaded in parts".into());
            }
            self.map.insert(id, Upload { meta, parts, next: 1, paras, bytes, touched: now });
            return Ok(Accepted::Waiting { received: 1, parts });
        }
        let Some(up) = self.map.get_mut(&id) else { return Err(format!("part {part} without part 0").into()) };
        let problem = if up.next != part {
            Some(format!("expected part {}, got {part}", up.next))
        } else if up.parts != parts || up.meta != meta {
            Some("part header differs from part 0".to_owned())
        } else if up.bytes + bytes > MAX_DOC_BYTES {
            Some("document exceeds 64MB".to_owned())
        } else {
            None
        };
        if let Some(p) = problem {
            self.map.remove(&id);
            return Err(p.into());
        }
        up.paras.extend(paras);
        up.bytes += bytes;
        up.next += 1;
        up.touched = now;
        if up.next < up.parts {
            return Ok(Accepted::Waiting { received: up.next, parts: up.parts });
        }
        let up = self.map.remove(&id).expect("upload exists");
        Ok(Accepted::Complete { meta: up.meta, paras: up.paras })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctable::LevelSrc;

    fn meta() -> DocMeta {
        DocMeta { path: "a.txt".into(), size: 1, mtime: 1, date: 0, ref_date: None, scopes: vec!["tm".into()], level_src: LevelSrc::Py, level_rule: 4 }
    }
    fn para(p: u32, lv: u8, t: &str) -> Para {
        Para { p, lv, t: t.into() }
    }

    #[test]
    fn paragraphs_merge_and_split_into_indexable_sentences() {
        let merged = merge_paras(vec![para(0, 0, "예산 계획"), para(1, 8, "집행한다. ※"), para(1, 8, "\n다음 문장.")]).unwrap();
        assert_eq!(merged, vec![para(0, 0, "예산 계획"), para(1, 8, "집행한다. ※\n다음 문장.")]);
        let (s, spans) = sentences(&merged);
        let texts: Vec<_> = s.iter().map(|s| (s.para, s.level, s.text)).collect();
        assert_eq!(texts, [(0, 0, "예산 계획"), (1, 8, "집행한다."), (1, 8, "다음 문장.")]);
        assert_eq!(spans, [0, 0, 5, 1, 0, 5, 1, 8, 6]);
        assert!(merge_paras(vec![para(1, 8, "a"), para(1, 7, "b")]).is_err());
        assert!(merge_paras(vec![para(1, 9, "a")]).is_err());
        assert!(valid_doc_key(r"c:\x\y.hwp") && !valid_doc_key("") && !valid_doc_key("a\nb"));
    }

    #[test]
    fn parts_assemble_in_order() {
        let mut u = Uploads::default();
        let t = Instant::now();
        assert_eq!(u.accept("S", "k", meta(), vec![para(0, 0, "가")], Some(0), Some(3), t).unwrap(), Accepted::Waiting { received: 1, parts: 3 });
        assert_eq!(u.accept("S", "k", meta(), vec![para(0, 0, "나")], Some(1), Some(3), t).unwrap(), Accepted::Waiting { received: 2, parts: 3 });
        match u.accept("S", "k", meta(), vec![para(1, 8, "다")], Some(2), Some(3), t).unwrap() {
            Accepted::Complete { paras, .. } => assert_eq!(merge_paras(paras).unwrap(), vec![para(0, 0, "가나"), para(1, 8, "다")]),
            other => panic!("{other:?}"),
        }
        assert!(u.is_empty());
        // 한 번에 온 문서.
        assert!(matches!(u.accept("S", "k", meta(), vec![], None, None, t).unwrap(), Accepted::Complete { .. }));
    }

    #[test]
    fn impossible_part_transitions_are_rejected() {
        let mut u = Uploads::default();
        let t = Instant::now();
        assert!(u.accept("S", "k", meta(), vec![], Some(1), Some(2), t).is_err(), "no part 0");
        u.accept("S", "k", meta(), vec![], Some(0), Some(3), t).unwrap();
        assert!(u.accept("S", "k", meta(), vec![], Some(2), Some(3), t).is_err(), "skipped part");
        assert!(u.is_empty(), "rejected upload is dropped");
        u.accept("S", "k", meta(), vec![], Some(0), Some(3), t).unwrap();
        let other = DocMeta { size: 2, ..meta() };
        assert!(u.accept("S", "k", other, vec![], Some(1), Some(3), t).is_err(), "header changed");
        u.accept("S", "k", meta(), vec![], Some(0), Some(3), t).unwrap();
        assert!(u.accept("S", "k", meta(), vec![], Some(1), Some(4), t).is_err(), "parts changed");
        // 다른 SID의 조각은 섞이지 않는다.
        u.accept("S", "k", meta(), vec![], Some(0), Some(2), t).unwrap();
        assert!(u.accept("T", "k", meta(), vec![], Some(1), Some(2), t).is_err());
        // 오래된 조각은 버린다.
        assert!(u.accept("S", "k", meta(), vec![], Some(1), Some(2), t + Duration::from_secs(61)).is_err());
        assert!(u.accept("S", "k", meta(), vec![], Some(0), Some(0), t).is_err());
        assert!(u.accept("S", "k", meta(), vec![], Some(0), None, t).is_err());
        assert!(u.accept("S", "k", meta(), vec![], Some(0), Some(MAX_PARTS + 1), t).is_err());
    }

    #[test]
    fn upload_limits() {
        let mut u = Uploads::default();
        let t = Instant::now();
        for i in 0..MAX_UPLOADS_PER_USER {
            u.accept("S", &i.to_string(), meta(), vec![], Some(0), Some(2), t).unwrap();
        }
        assert!(u.accept("S", "x", meta(), vec![], Some(0), Some(2), t).is_err());
        u.accept("T", "x", meta(), vec![], Some(0), Some(2), t).unwrap();
        let big = "가".repeat(MAX_DOC_BYTES / 3 / 2 + 1);
        u.accept("T", "y", meta(), vec![para(0, 0, &big)], Some(0), Some(3), t).unwrap();
        assert!(u.accept("T", "y", meta(), vec![para(0, 0, &big)], Some(1), Some(3), t).is_err(), "over 64MB");
    }
}
