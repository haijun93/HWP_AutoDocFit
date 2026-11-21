//! 시험 공용: 임시 폴더·가짜 EDIT 호스트.
#![allow(dead_code)]
use outliner_win::edit::EditHost;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

/// 시험마다 새 임시 폴더(사용자 데이터가 아니라 시험 전용).
pub fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("outliner_win4_{tag}_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn write(dir: &std::path::Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

pub fn read(dir: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

/// 글·선택·조합 상태를 가진 가짜 EDIT.
#[derive(Default)]
pub struct Fake {
    pub text: String,
    pub sel: (usize, usize),
    pub composing: bool,
    pub candidate: bool,
    pub completes: usize,
}

impl Fake {
    /// 글을 바꾸고 캐럿을 끝에 둔다(사용자가 입력한 것처럼).
    pub fn type_text(&mut self, t: &str) {
        self.text = t.to_string();
        let n = t.encode_utf16().count();
        self.sel = (n, n);
    }
}

impl EditHost for Fake {
    fn read_text(&mut self) -> String {
        self.text.clone()
    }
    fn selection(&mut self) -> (usize, usize) {
        self.sel
    }
    fn set_text(&mut self, t: &str, caret: usize) {
        self.text = t.to_string();
        self.sel = (caret, caret);
    }
    fn composing(&mut self) -> bool {
        self.composing
    }
    fn candidate_open(&mut self) -> bool {
        self.candidate
    }
    fn request_complete(&mut self) {
        self.completes += 1;
        self.composing = false;
    }
}

pub fn fixed_clock() -> outliner_win::shell::Now {
    outliner_win::shell::Now { date_iso: "2026-11-21".into(), stamp: "20261121-101500".into() }
}
