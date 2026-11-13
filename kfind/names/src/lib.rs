//! kfind-names: 드라이브 전체 파일명 색인과 질의(FN1, `docs/FN1_DESIGN_2026-11-13.md`).
//!
//! - 저장: 항목을 열(列)별 배열로 둔다(부모 번호·비교형 위치·원래 이름 위치·정렬 키·확장자·표지·하위 트리 끝).
//!   비교형 이름(NFC·casefold·글자/숫자만, `kfind_core::fold`)은 `0`으로 구분한 **한 덩어리 버퍼**에 이어 붙인다.
//! - 순서: 항목은 폴더 트리 **전위(DFS) 순서**, 형제는 비교형 이름 순. 그래서 한 폴더의 하위 트리가 연속 번호 구간
//!   `[id, end[id])`이 되어 경로 조건·제외 폴더가 구간 계산으로 끝나고, 번호 순서가 곧 경로 순서다.
//! - 질의: 가장 긴 낱말로 버퍼 전체를 SIMD 부분 문자열 찾기(`memchr::memmem`), 여러 스레드로 구간 분할,
//!   찾은 위치 → 항목 번호는 위치 배열 위 질주 탐색. 나머지 낱말·제외·확장자·종류는 그 항목에서만 확인.
//!   상위 N은 미리 계산한 정렬 키(u64)로 고르고 전체 개수는 따로 센다. 입력이 앞 질의의 연장이면 앞 결과 안에서만 거른다.
//! - 갱신: USN 기록으로 만들기·지우기·이름 바꾸기를 반영(지운 표지 + 뒤에 덧붙이기). 많이 쌓이면 `compact`.
//! - 공유: `seg`로 `names.seg`에 쓰고 다른 프로세스가 메모리 매핑으로 복사 없이 같은 질의 코드를 돌린다.
mod build;
mod crc;
mod index;
#[cfg(windows)]
pub mod ntfs;
pub mod query;
pub mod seg;
pub mod source;
#[cfg(windows)]
pub mod walk;

pub use build::Builder;
pub use index::{NONE, Names, View, flag};
pub use query::{Hit, Options, Query, Results, Searcher, Sort, parse};
pub use seg::NamesSeg;
pub use source::{Change, ChangeKind};

/// 엔진 오류.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// 파일이 형식에 맞지 않거나 깨졌다.
    Corrupt(String),
    /// 관리자 권한이 없어 볼륨을 열 수 없다(MFT·USN).
    AccessDenied,
    /// USN 저널이 없거나 잘려 나가 처음부터 다시 열거해야 한다.
    Rescan(String),
    /// 질의가 취소되었다.
    Cancelled,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Corrupt(s) => write!(f, "corrupt: {s}"),
            Error::AccessDenied => write!(f, "access denied (administrator required)"),
            Error::Rescan(s) => write!(f, "rescan needed: {s}"),
            Error::Cancelled => write!(f, "cancelled"),
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
