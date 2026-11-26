//! 전체 종료(사이드바 '전체 종료', 설치·제거의 `--close-all`)의 순수 판단.
//!
//! 상태: 요청 중(창마다 닫기 한 번) → 모두 닫힘 | 남음(사용자가 취소했거나 대화 상자가 떠 있거나 작업 중).
//! 불가능한 전이: 같은 창에 닫기를 두 번 보내지 않는다(앱은 확인 창이 떠 있을 때 두 번째 닫기를 강제 종료로 볼 수 있다).
//! 어떤 경우에도 프로세스를 강제 종료하지 않는다.

use std::collections::BTreeSet;
use std::path::Path;

/// 사이드바 '전체 종료'를 기다리는 한도. 지나면 평소 상태로 돌아간다(한참 뒤 창을 닫았을 때 사이드바가 갑자기 끝나지 않게).
pub const QUIT_TIMEOUT_MS: u64 = 120_000;

/// `--close-all` 종료 코드.
pub const EXIT_CLOSED: i32 = 0;
pub const EXIT_REMAINING: i32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub handle: isize,
    pub title: String,
    /// 대화 상자가 떠 있으면 false: 그 창에는 닫기를 보내지 않고 사용자가 답한 뒤에 보낸다.
    pub enabled: bool,
}

/// 이번에 닫기를 보낼 창. 보낸 창은 `asked` 에 남는다.
pub fn to_ask(windows: &[Window], asked: &mut BTreeSet<isize>) -> Vec<isize> {
    windows.iter().filter(|w| w.enabled && asked.insert(w.handle)).map(|w| w.handle).collect()
}

/// `image` 가 설치 폴더 `root` 아래의 실행 파일인지(대소문자·구분자 무시). 빈 root 는 아무것도 아니다.
pub fn under(image: &Path, root: &Path) -> bool {
    let normal = |p: &Path| {
        let s = p.to_string_lossy().replace('/', "\\").to_lowercase();
        s.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(s).trim_end_matches('\\').to_owned()
    };
    let (image, root) = (normal(image), normal(root));
    // 드라이브 전체(`C:`)를 설치 폴더로 받지 않는다.
    root.contains('\\') && image.starts_with(&format!("{root}\\"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn window(handle: isize, enabled: bool) -> Window {
        Window { handle, title: String::new(), enabled }
    }

    #[test]
    fn each_window_is_asked_once_and_dialog_owners_wait() {
        let mut asked = BTreeSet::new();
        assert_eq!(to_ask(&[window(1, true), window(2, false)], &mut asked), [1]);
        assert!(to_ask(&[window(1, true), window(2, false)], &mut asked).is_empty());
        assert_eq!(to_ask(&[window(1, true), window(2, true)], &mut asked), [2], "대화 상자를 닫은 뒤");
    }

    #[test]
    fn only_executables_inside_the_install_folder() {
        let root = PathBuf::from(r"C:\Users\a\Programs\HWP_AutoDocFit");
        assert!(under(&root.join(r"_internal\resources\sidebar.exe"), &root));
        assert!(under(Path::new(r"c:\users\a\programs\hwp_autodocfit\X.EXE"), &PathBuf::from(r"C:\Users\a\Programs\HWP_AutoDocFit\")));
        assert!(!under(Path::new(r"C:\Users\a\Programs\HWP_AutoDocFit2\x.exe"), &root));
        assert!(!under(Path::new(r"C:\Windows\explorer.exe"), Path::new("C:\\")));
        assert!(!under(Path::new(r"C:\Windows\explorer.exe"), Path::new("")));
    }
}
