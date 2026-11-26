//! 창끼리 파일 넘기기 규칙(CTXMENU1 과 같은 형식·개수 표). std 만 쓴다:
//! `dock`(sidebar.exe)이 이 파일을 `#[path]` 로 그대로 포함하므로 다른 의존성을 넣지 않는다.
//! 같은 표: `flutter_ui/lib/features/common/file_handoff.dart`, `docfit_app/handoff.py`
//! (`tests/test_window_handoff.py` 가 셋을 대조한다).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// 받는 메뉴 번호(앱 탭 번호).
    pub menu: u8,
    pub label: &'static str,
    /// 소문자·점 없음. 비었으면 형식 제한 없음.
    pub extensions: &'static [&'static str],
    pub limit: usize,
}

pub const TARGETS: [Target; 5] = [
    Target { menu: 0, label: "문서편집", extensions: &["hwp", "hwpx", "pdf", "doc", "docx", "txt", "md"], limit: 1000 },
    Target { menu: 11, label: "PDF 편집", extensions: &["pdf"], limit: 100 },
    Target {
        menu: 10,
        label: "파일 변환",
        extensions: &["hwp", "hwpx", "pdf", "docx", "txt", "md", "png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"],
        limit: 1000,
    },
    Target { menu: 12, label: "가상 폴더에 추가", extensions: &[], limit: 200 },
    Target {
        menu: 13,
        label: "업무자료함에 넣기",
        extensions: &["hwpx", "hwp", "docx", "pdf", "xlsx", "pptx", "txt", "md"],
        limit: 200,
    },
];

pub fn target(menu: u8) -> Option<&'static Target> {
    TARGETS.iter().find(|t| t.menu == menu)
}

fn extension(path: &str) -> String {
    let name = path.rsplit(['\\', '/']).next().unwrap_or("");
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

/// 이름만으로 본 거절 이유(메뉴 비활성 표시와 보내기 직전 검사에 함께 쓴다). 받을 수 있으면 None.
pub fn rejection<S: AsRef<str>>(menu: u8, paths: &[S]) -> Option<String> {
    let Some(target) = target(menu) else { return Some("파일을 받을 수 없는 화면입니다".into()) };
    if paths.is_empty() {
        return Some("선택한 파일이 없습니다".into());
    }
    if paths.len() > target.limit {
        return Some(format!("한 번에 {}개까지 넘길 수 있습니다", target.limit));
    }
    if !target.extensions.is_empty() && paths.iter().any(|p| !target.extensions.contains(&extension(p.as_ref()).as_str())) {
        return Some(if menu == 11 { "PDF 파일만 편집할 수 있습니다".into() } else { "지원하지 않는 형식입니다".into() });
    }
    None
}

/// 문서뷰·사진뷰 오른쪽 메뉴에 보이는 대상. 사진뷰는 CTXMENU1 과 같이 파일 변환·가상 폴더만.
pub const DOCVIEW_MENUS: [u8; 5] = [0, 11, 10, 12, 13];
pub const PHOTOVIEW_MENUS: [u8; 2] = [10, 12];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_follow_ctxmenu1() {
        assert_eq!(rejection(0, &[r"C:\a\보고.HWPX"]), None);
        assert!(rejection(0, &[r"C:\a\b.xlsx"]).is_some());
        assert_eq!(rejection(13, &[r"C:\a\b.xlsx"]), None);
        assert_eq!(rejection(12, &[r"C:\a\확장자없음"]), None);
        assert!(rejection(11, &["a.pdf", "b.hwp"]).unwrap().contains("PDF"));
        assert!(rejection(10, &[r"C:\a.pdf\b"]).is_some(), "폴더 이름의 점은 확장자가 아니다");
        assert!(rejection(11, &vec!["a.pdf"; 101]).unwrap().contains("100"));
        assert!(rejection::<&str>(0, &[]).is_some());
        assert!(rejection(7, &["a.pdf"]).is_some(), "받지 않는 화면");
    }
}
