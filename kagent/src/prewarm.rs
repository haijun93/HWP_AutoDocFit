//! 그림·화면캡처 항목 썸네일 미리 만들기(사용자 결정 2026-11-13, KCAP2).
//!
//! 썸네일 첫 생성(1080p 87ms·4K 177ms)이 미리보기 목표 50ms를 넘어, 그림·화면캡처 항목을 기록하면 대기열에
//! 넣고 쉬는 때(낮은 우선순위 배경 스레드, CPU·디스크 입출력 모두 배경 모드) 하나씩 만들어 `thumb`의 메모리·
//! 디스크 캐시에 둔다. 캐시 상한(메모리 4MB·디스크 16MB)·암호화·모두 지우기 규칙은 `thumb`가 그대로 지킨다.
//! 미리 만들기 전에 마우스를 올리면 기존 즉시 생성 경로(`preview`·파이프 `thumb`)가 그대로 만든다.
//!
//! 항목별 작업 상태 기계(`Queue`, 순수 — 스레드·캐시와 떨어져 시험한다):
//! | 지금 \ 사건 | 기록(push) | 꺼내기(take) | 끝(finish 같은 id) | 모두 지우기(clear) |
//! |---|---|---|---|---|
//! | 없음 | `Queued`(맨 앞) | — | 불가(무시) | 그대로 |
//! | `Queued` | 맨 앞으로(최근 것 우선) | 맨 앞이고 생성 중인 것이 없으면 `Generating` | 불가(무시) | 없음 |
//! | `Generating` | 그대로(이미 만드는 중) | — | 결과에 따라 `Done`/`Failed` | 그대로(결과는 `thumb` 세대 번호로 버려짐) |
//! | `Done`·`Failed` | `Queued`(다시 기록된 같은 항목) | — | 불가(무시) | 없음 |
//! - 대기열 상한 `QUEUE_CAP`: 넘치면 가장 오래된 대기 항목을 버린다(`dropped`, 그 항목은 즉시 생성 경로로).
//! - 동시에 `Generating`은 하나뿐. 이미 메모리 캐시에 있으면 만들지 않고 `Done`(건너뜀)으로 끝낸다.
//! - 마지막 기록 뒤 `SETTLE_MS` 동안 더 오는 것이 없을 때 꺼낸다(연속 캡처는 다 모은 뒤 최근 것부터).
//! 상태 요약은 `status.json`의 `thumb_prewarm`으로 앱·에이전트가 읽는다.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// 대기열 상한(메모리 캐시 4MB는 1080p 썸네일 17장 정도 — 미리 만든 것이 캐시를 다 밀어내지 않게).
pub const QUEUE_CAP: usize = 8;
/// 마지막 기록 뒤 기다리는 시간(연속 캡처 묶기, 기록 직후 저장·목록 갱신과 겹치지 않게).
pub const SETTLE_MS: u64 = 300;
/// 끝난 작업 상태를 기억하는 개수(상태 조회용).
const HISTORY: usize = 32;

/// 항목별 작업 상태(시험·진단용 조회 `Queue::state`. 실행 중 요약은 `to_json`).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Generating,
    Done,
    Failed,
}

/// 생성 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Made,
    /// 이미 메모리 캐시에 있었음(마우스를 먼저 올렸거나 같은 항목이 다시 기록됨).
    Skipped,
    Failed,
}

#[derive(Debug)]
pub struct Queue {
    /// 앞이 가장 최근 기록.
    pending: VecDeque<i64>,
    current: Option<i64>,
    finished: VecDeque<(i64, JobState)>,
    cap: usize,
    pub made: u64,
    pub skipped: u64,
    pub failed: u64,
    pub dropped: u64,
}

impl Queue {
    pub const fn new(cap: usize) -> Self {
        Queue { pending: VecDeque::new(), current: None, finished: VecDeque::new(), cap,
                made: 0, skipped: 0, failed: 0, dropped: 0 }
    }

    /// 기록된 항목을 넣는다. 상한을 넘겨 버린 항목이 있으면 돌려준다.
    pub fn push(&mut self, id: i64) -> Option<i64> {
        if self.current == Some(id) {
            return None;
        }
        self.pending.retain(|&p| p != id);
        self.finished.retain(|&(f, _)| f != id);
        self.pending.push_front(id);
        if self.pending.len() > self.cap {
            self.dropped += 1;
            return self.pending.pop_back();
        }
        None
    }

    /// 다음에 만들 항목(가장 최근 것). 생성 중인 것이 있으면 None.
    pub fn take(&mut self) -> Option<i64> {
        if self.current.is_some() {
            return None;
        }
        let id = self.pending.pop_front()?;
        self.current = Some(id);
        Some(id)
    }

    /// 생성 끝. 생성 중인 항목이 아니면(불가능한 전이) 무시하고 false.
    pub fn finish(&mut self, id: i64, outcome: Outcome) -> bool {
        if self.current != Some(id) {
            return false;
        }
        self.current = None;
        let state = match outcome {
            Outcome::Made => { self.made += 1; JobState::Done }
            Outcome::Skipped => { self.skipped += 1; JobState::Done }
            Outcome::Failed => { self.failed += 1; JobState::Failed }
        };
        self.finished.push_back((id, state));
        while self.finished.len() > HISTORY {
            self.finished.pop_front();
        }
        true
    }

    /// 모두 지우기: 대기와 끝난 기록을 비운다. 생성 중인 것은 끝날 때 결과만 기록된다.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.finished.clear();
    }

    #[allow(dead_code)]
    pub fn state(&self, id: i64) -> Option<JobState> {
        if self.current == Some(id) {
            return Some(JobState::Generating);
        }
        if self.pending.contains(&id) {
            return Some(JobState::Queued);
        }
        self.finished.iter().rev().find(|(f, _)| *f == id).map(|(_, s)| *s)
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn to_json(&self) -> crate::json::Value {
        crate::json::Value::obj(vec![
            ("queued", (self.pending.len() as i64).into()),
            ("generating", self.current.is_some().into()),
            ("made", (self.made as i64).into()),
            ("skipped", (self.skipped as i64).into()),
            ("failed", (self.failed as i64).into()),
            ("dropped", (self.dropped as i64).into()),
        ])
    }
}

// ---- 시스템 연결(에이전트 실행 중에만) ----

struct Shared {
    queue: Queue,
    last_push: Option<Instant>,
    started: bool,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { queue: Queue::new(QUEUE_CAP), last_push: None, started: false });
static WAKE: Condvar = Condvar::new();

/// 그림·화면캡처 항목을 기록한 직후 부른다(어느 스레드에서나, 바로 돌아온다).
pub fn enqueue(id: i64) {
    let enabled = { crate::app().settings.lock().unwrap().preview_images };
    if !enabled {
        return;
    }
    let mut sh = SHARED.lock().unwrap();
    sh.queue.push(id);
    sh.last_push = Some(Instant::now());
    if !sh.started {
        let spawned = std::thread::Builder::new()
            .name("kagent-thumb-prewarm".into())
            .stack_size(512 * 1024)
            .spawn(run);
        sh.started = spawned.is_ok();
    }
    drop(sh);
    WAKE.notify_one();
}

/// 모두 지우기 때(`thumb::clear_all`), 또는 미리보기 끄기 설정 때.
pub fn clear() {
    SHARED.lock().unwrap().queue.clear();
}

pub fn status_json() -> crate::json::Value {
    SHARED.lock().unwrap().queue.to_json()
}

fn run() {
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
        // CPU·디스크 입출력 모두 배경 우선순위: 다른 일이 있으면 그쪽이 먼저.
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
    let settle = Duration::from_millis(SETTLE_MS);
    loop {
        let id = {
            let mut sh = SHARED.lock().unwrap();
            loop {
                if sh.queue.has_pending() {
                    let enabled = { crate::app().settings.lock().unwrap().preview_images };
                    if !enabled {
                        sh.queue.clear();
                        continue;
                    }
                    let since = sh.last_push.map(|t| t.elapsed()).unwrap_or(settle);
                    if since < settle {
                        sh = WAKE.wait_timeout(sh, settle - since).unwrap().0;
                        continue;
                    }
                    if let Some(id) = sh.queue.take() {
                        break id;
                    }
                }
                sh = WAKE.wait(sh).unwrap();
            }
        };
        let enabled = { crate::app().settings.lock().unwrap().preview_images };
        if !enabled {
            SHARED.lock().unwrap().queue.finish(id, Outcome::Skipped);
            crate::write_status();
            continue;
        }
        let outcome = make(id);
        SHARED.lock().unwrap().queue.finish(id, outcome);
        crate::write_status();
    }
}

fn make(id: i64) -> Outcome {
    let Ok(Some(size)) = crate::app().store.lock().unwrap().size_of(id) else { return Outcome::Failed };
    if crate::thumb::is_cached(id, size) {
        return Outcome::Skipped;
    }
    if crate::thumb::get(id, size).is_some() { Outcome::Made } else { Outcome::Failed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_and_one_at_a_time() {
        let mut q = Queue::new(8);
        q.push(1);
        q.push(2);
        q.push(3);
        assert_eq!(q.state(1), Some(JobState::Queued));
        assert_eq!(q.take(), Some(3), "최근 것 먼저");
        assert_eq!(q.state(3), Some(JobState::Generating));
        assert_eq!(q.take(), None, "생성 중에는 하나만");
        assert!(q.finish(3, Outcome::Made));
        assert_eq!(q.state(3), Some(JobState::Done));
        assert_eq!(q.take(), Some(2));
        assert!(q.finish(2, Outcome::Failed));
        assert_eq!(q.state(2), Some(JobState::Failed));
        assert_eq!(q.take(), Some(1));
        assert!(q.finish(1, Outcome::Skipped));
        assert_eq!(q.state(1), Some(JobState::Done));
        assert_eq!(q.take(), None);
        assert_eq!((q.made, q.skipped, q.failed, q.dropped), (1, 1, 1, 0));
        assert_eq!(q.state(99), None);
    }

    #[test]
    fn burst_over_cap_drops_oldest() {
        let mut q = Queue::new(3);
        assert_eq!(q.push(1), None);
        q.push(2);
        q.push(3);
        assert_eq!(q.push(4), Some(1), "가장 오래된 대기 항목을 버림");
        assert_eq!(q.push(5), Some(2));
        assert_eq!(q.dropped, 2);
        assert_eq!(q.state(1), None);
        assert_eq!((q.take(), q.finish(5, Outcome::Made)), (Some(5), true));
        assert_eq!((q.take(), q.finish(4, Outcome::Made)), (Some(4), true));
        assert_eq!(q.take(), Some(3));
    }

    #[test]
    fn repush_moves_to_front_and_generating_is_kept() {
        let mut q = Queue::new(8);
        q.push(1);
        q.push(2);
        q.push(1); // 같은 그림이 다시 기록됨 → 맨 앞으로, 중복 없음
        assert_eq!(q.take(), Some(1));
        q.push(1); // 만드는 중에 다시 기록돼도 대기열에 또 넣지 않음
        assert!(q.finish(1, Outcome::Made));
        assert_eq!(q.take(), Some(2));
        assert!(q.finish(2, Outcome::Made));
        assert_eq!(q.take(), None);
        q.push(2); // 끝난 항목이 다시 기록되면 다시 대기(만들 때 캐시가 있으면 건너뜀)
        assert_eq!(q.state(2), Some(JobState::Queued));
    }

    #[test]
    fn impossible_transitions_are_ignored() {
        let mut q = Queue::new(8);
        assert!(!q.finish(7, Outcome::Made), "생성 중이 아닌 항목의 끝");
        q.push(7);
        assert!(!q.finish(7, Outcome::Made), "대기 중인 항목의 끝");
        assert_eq!(q.state(7), Some(JobState::Queued));
        assert_eq!(q.take(), Some(7));
        assert!(!q.finish(8, Outcome::Made), "다른 항목의 끝");
        assert_eq!(q.state(7), Some(JobState::Generating));
        assert_eq!(q.made, 0);
    }

    #[test]
    fn clear_empties_queue_but_keeps_generating() {
        let mut q = Queue::new(8);
        q.push(1);
        q.push(2);
        assert_eq!(q.take(), Some(2));
        q.clear();
        assert_eq!(q.state(1), None);
        assert!(!q.has_pending());
        assert_eq!(q.state(2), Some(JobState::Generating));
        assert!(q.finish(2, Outcome::Made));
        assert_eq!(q.take(), None);
    }
}
