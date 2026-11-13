//! 색인 원천에 공통인 변경 기록(USN 기록·디렉터리 순회 대조 결과를 같은 꼴로 받는다).

/// Windows 파일 속성 값(`FILE_ATTRIBUTE_*`). 운영체제와 무관한 시험에서도 쓰려고 여기 둔다.
pub mod attr {
    pub const READONLY: u32 = 0x1;
    pub const HIDDEN: u32 = 0x2;
    pub const SYSTEM: u32 = 0x4;
    pub const DIRECTORY: u32 = 0x10;
    pub const REPARSE_POINT: u32 = 0x400;
}

/// USN 이유 값(`USN_REASON_*`).
pub mod reason {
    pub const FILE_CREATE: u32 = 0x100;
    pub const FILE_DELETE: u32 = 0x200;
    pub const RENAME_OLD_NAME: u32 = 0x1000;
    pub const RENAME_NEW_NAME: u32 = 0x2000;
    pub const BASIC_INFO_CHANGE: u32 = 0x8000;
    pub const CLOSE: u32 = 0x8000_0000;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// 만들기·새 이름·(놓쳤을 수 있는) 그 밖 변경: 있으면 이름·부모를 맞추고 없으면 더한다.
    Upsert,
    /// 지우기.
    Remove,
    /// 옛 이름(새 이름 기록이 따로 온다): 무시.
    Ignore,
}

/// 한 항목의 변경. `rec`·`parent`는 볼륨 안 MFT 레코드 번호(파일 참조 번호의 아래 48비트).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub vol: u16,
    pub rec: u64,
    pub parent: u64,
    pub name: String,
    pub attrs: u32,
    pub kind: ChangeKind,
}

/// USN 이유 → 변경 종류.
pub fn kind_from_reason(r: u32) -> ChangeKind {
    if r & reason::FILE_DELETE != 0 {
        ChangeKind::Remove
    } else if r & reason::RENAME_OLD_NAME != 0
        && r & (reason::RENAME_NEW_NAME | reason::FILE_CREATE) == 0
    {
        ChangeKind::Ignore
    } else {
        ChangeKind::Upsert
    }
}

/// 파일 참조 번호(64비트) → 레코드 번호(아래 48비트).
#[inline]
pub fn rec_of(frn: u64) -> u64 {
    frn & 0x0000_FFFF_FFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons() {
        assert_eq!(
            kind_from_reason(reason::FILE_CREATE | reason::CLOSE),
            ChangeKind::Upsert
        );
        assert_eq!(
            kind_from_reason(reason::FILE_DELETE | reason::CLOSE),
            ChangeKind::Remove
        );
        assert_eq!(
            kind_from_reason(reason::FILE_CREATE | reason::FILE_DELETE),
            ChangeKind::Remove
        );
        assert_eq!(
            kind_from_reason(reason::RENAME_OLD_NAME),
            ChangeKind::Ignore
        );
        assert_eq!(
            kind_from_reason(reason::RENAME_NEW_NAME),
            ChangeKind::Upsert
        );
        assert_eq!(
            kind_from_reason(reason::BASIC_INFO_CHANGE),
            ChangeKind::Upsert
        );
        assert_eq!(rec_of(0x0005_0000_0000_0123), 0x123);
    }
}
