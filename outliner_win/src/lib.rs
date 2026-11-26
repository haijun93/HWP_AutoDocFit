//! 아웃라이너 Rust+Win32 2단계 첫 카드: 읽기 전용 블록 목록 창 + 한글 IME 순서 탐침.
pub mod capture;
pub mod clock;
pub mod docio;
pub mod edit;
pub mod find;
pub mod layout;
pub mod links;
pub mod pages;
pub mod paint_shell;
pub mod shell;
pub mod paint;
pub mod png;
pub mod sha256;
pub mod probe_fmt;
pub mod view;
pub mod window;
pub mod bullet;

/// 시험·캡처 기본 문서(저장소 안 작은 .md).
pub const SAMPLE_MD: &str = include_str!("../tests/sample.md");
