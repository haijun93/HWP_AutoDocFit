//! 문서 단위 실행 취소/다시 실행 이력.
//!
//! 엔진은 요청마다 문서를 새로 파싱하는 무상태 방식이므로, 이력도 "직렬화된 마크다운 스냅숏"의 스택으로 둔다.
//! 스냅숏 방식은 편집 연산(set_text·insert·delete·indent·outdent·up/down·fold·status·split·merge)이
//! 무엇이든 역연산을 따로 만들지 않고 정확히 되돌리며, 마크다운은 원문 줄(Raw)까지 보존하므로 되돌린 결과가 바이트 단위로 같다.
//! 편집 묶음(Enter 의 저장+삽입 등)은 호출한 쪽이 묶음이 시작되기 전 마크다운을 `record` 로 한 번만 넣어 한 단계로 만든다.
use serde::{Deserialize, Serialize};

/// 되돌리기 단계 한도(가장 오래된 것부터 버린다).
pub const MAX_UNDO_DEPTH: usize = 200;
/// 이력이 가질 수 있는 스냅숏 글자(바이트) 합계 한도. 넘으면 오래된 단계부터 버린다.
pub const MAX_UNDO_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoHistory {
    undo: Vec<String>,
    redo: Vec<String>,
    /// 이력이 쌓인 페이지(다른 페이지가 오면 비운다).
    #[serde(default)]
    pub page: Option<String>,
    /// 이력이 쌓인 파일 버전(다르면 외부 변경으로 보고 비운다).
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default = "default_depth")]
    max_depth: usize,
    #[serde(default = "default_bytes")]
    max_bytes: usize,
}

fn default_depth() -> usize {
    MAX_UNDO_DEPTH
}
fn default_bytes() -> usize {
    MAX_UNDO_BYTES
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::with_limits(MAX_UNDO_DEPTH, MAX_UNDO_BYTES)
    }
}

impl UndoHistory {
    pub fn with_limits(max_depth: usize, max_bytes: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), page: None, version: None, max_depth, max_bytes }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
    /// 이력이 쥔 스냅숏 바이트 합계.
    pub fn bytes(&self) -> usize {
        self.undo.iter().chain(&self.redo).map(String::len).sum()
    }

    /// 이력을 모두 비운다(페이지 전환·외부 변경·불러오기).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// 페이지·파일 버전이 이력을 쌓은 때와 다르면 비우고 true. 처음 묶을 때나 같으면 false.
    pub fn bind(&mut self, page: Option<&str>, version: Option<&str>) -> bool {
        let mut cleared = false;
        if let Some(p) = page {
            if self.page.as_deref().is_some_and(|old| old != p) {
                cleared = true;
            }
            self.page = Some(p.to_owned());
        }
        if let Some(v) = version {
            if self.version.as_deref().is_some_and(|old| old != v) {
                cleared = true;
            }
            self.version = Some(v.to_owned());
        }
        if cleared {
            self.clear();
        }
        cleared
    }

    /// 파일 버전만 갱신한다(자기 저장 뒤: 외부 변경이 아니므로 이력을 비우지 않는다).
    pub fn rebase(&mut self, version: Option<&str>) {
        self.version = version.map(str::to_owned);
    }

    /// 새 편집 직전 마크다운 `before` 를 되돌리기 단계로 넣는다. 다시 실행 단계는 모두 지운다.
    /// 스냅숏 하나가 바이트 한도보다 크면 이력을 비우고 기록하지 않는다(false).
    pub fn record(&mut self, before: &str) -> bool {
        self.redo.clear();
        if before.len() > self.max_bytes {
            self.undo.clear();
            return false;
        }
        self.undo.push(before.to_owned());
        self.trim();
        true
    }

    /// 한 단계 되돌린다. `current` 는 지금 마크다운(다시 실행용으로 보관). 되돌릴 것이 없으면 None.
    pub fn undo(&mut self, current: &str) -> Option<String> {
        let previous = self.undo.pop()?;
        self.redo.push(current.to_owned());
        self.trim();
        Some(previous)
    }

    /// 한 단계 다시 실행한다.
    pub fn redo(&mut self, current: &str) -> Option<String> {
        let next = self.redo.pop()?;
        self.undo.push(current.to_owned());
        self.trim();
        Some(next)
    }

    fn trim(&mut self) {
        while self.undo.len() > self.max_depth {
            self.undo.remove(0);
        }
        while self.redo.len() > self.max_depth {
            self.redo.remove(0);
        }
        while self.bytes() > self.max_bytes {
            if !self.undo.is_empty() {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else {
                break;
            }
        }
    }
}
