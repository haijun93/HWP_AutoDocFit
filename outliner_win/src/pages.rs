//! 데이터 폴더의 `.md` 페이지 관리(Win32 없음, 시험은 임시 폴더). 규칙은 `docfit_app/outliner_service.py` 와 같다:
//! 페이지 이름은 파일 한 조각(경로 구분자·`.`·`..` 거부), `.md` 확장 자동, 새 페이지·0바이트 파일은 `- \n`.
//!
//! 차이(사용자 확정 2026-11-21): 이름 바꾸기는 `[[링크]]` 도 함께 갱신(엔진 `rename_page_in_files`),
//! 삭제는 폴더 안 `.trash` 로 옮김(복원 가능), 일지 파일 이름은 `YYYY_MM_DD`(Flutter 의 `YYYY-MM-DD` 는 열 때 정리).
use std::fs;
use std::path::{Path, PathBuf};

pub const TRASH_DIR: &str = ".trash";
pub const RECENT_MAX: usize = 20;

#[derive(Clone, Debug)]
pub struct PageStore {
    pub root: PathBuf,
    /// 최근 페이지 목록 파일(JSON 문자열 배열, Flutter 의 `outliner_recent.json` 과 같은 형식). None 이면 저장 안 함.
    pub recent_path: Option<PathBuf>,
}

/// 페이지 이름 검증. 통과하면 `.md` 를 뗀 이름(파일 이름 줄기)을 돌려준다.
pub fn validate_name(name: &str) -> Result<String, String> {
    let mut s = name.trim();
    if s.len() > 3 && s.get(s.len() - 3..).is_some_and(|t| t.eq_ignore_ascii_case(".md")) {
        s = s[..s.len() - 3].trim_end();
    }
    if s.is_empty() {
        return Err("페이지 이름을 입력해 주세요".into());
    }
    if s == "." || s == ".." {
        return Err("페이지 이름으로 쓸 수 없는 이름입니다".into());
    }
    if s.chars().any(|c| c.is_control() || "\\/:*?\"<>|".contains(c)) {
        return Err("페이지 이름에 \\ / : * ? \" < > | 는 쓸 수 없습니다".into());
    }
    if s.ends_with('.') || s.ends_with(' ') || s.starts_with('.') {
        return Err("페이지 이름은 점으로 시작하거나 점·공백으로 끝날 수 없습니다".into());
    }
    if s.chars().count() > 100 {
        return Err("페이지 이름이 너무 깁니다(100자 이내)".into());
    }
    Ok(s.to_string())
}

/// `YYYY-MM-DD`(Flutter 가 만든 옛 일지 이름)인가.
pub fn is_dashed_journal(name: &str) -> bool {
    name.len() == 10 && name.as_bytes()[4] == b'-' && outliner_core::journal_name(name).is_ok()
}

/// `YYYY_MM_DD`(엔진 규칙 일지 이름)인가.
pub fn is_journal(name: &str) -> bool {
    outliner_core::journal_date(name).is_ok()
}

/// 일지로 인식되는가(두 이름 규칙 모두).
pub fn looks_like_journal(name: &str) -> bool {
    is_journal(name) || is_dashed_journal(name)
}

/// 페이지 목록 정렬: 일반 페이지(가나다·대소문자 무시) 먼저, 일지는 최신순으로 뒤에.
pub fn sort_pages(names: &mut Vec<String>) {
    names.sort_by(|a, b| {
        let (ja, jb) = (looks_like_journal(a), looks_like_journal(b));
        match (ja, jb) {
            (false, false) => a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)),
            (true, true) => b.replace('-', "_").cmp(&a.replace('-', "_")),
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
        }
    });
}

impl PageStore {
    pub fn new(root: impl Into<PathBuf>, recent_path: Option<PathBuf>) -> PageStore {
        PageStore { root: root.into(), recent_path }
    }

    pub fn ensure_root(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root).map_err(|e| format!("{}: {e}", self.root.display()))
    }

    pub fn path_of(&self, name: &str) -> Result<PathBuf, String> {
        Ok(self.root.join(format!("{}.md", validate_name(name)?)))
    }

    pub fn exists(&self, name: &str) -> bool {
        self.path_of(name).map(|p| p.is_file()).unwrap_or(false)
    }

    /// 데이터 폴더 바로 아래 `.md` 파일(하위 폴더·`.trash` 는 제외).
    pub fn files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(rd) = fs::read_dir(&self.root) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() && p.extension().and_then(|s| s.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("md")) {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// 페이지 이름 목록(정렬됨).
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> =
            self.files().iter().filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned())).collect();
        sort_pages(&mut names);
        names
    }

    /// 새 페이지 파일(`- \n`). 이미 있으면 오류. 만든 이름을 돌려준다.
    pub fn create(&self, name: &str) -> Result<String, String> {
        use std::io::Write;
        let stem = validate_name(name)?;
        self.ensure_root()?;
        let path = self.path_of(&stem)?;
        let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                "같은 이름의 페이지가 이미 있습니다".to_string()
            } else {
                format!("{}: {e}", path.display())
            }
        })?;
        f.write_all(b"- \n").map_err(|e| e.to_string())?;
        Ok(stem)
    }

    /// 없으면 만들고, 0바이트면 `- \n` 로 채운다(엔진 `insert` 는 기준 블록이 필요하다).
    pub fn ensure_page(&self, name: &str) -> Result<String, String> {
        let stem = validate_name(name)?;
        let path = self.path_of(&stem)?;
        if !path.exists() {
            return match self.create(&stem) {
                Ok(s) => Ok(s),
                Err(_) if path.exists() => Ok(stem),
                Err(e) => Err(e),
            };
        }
        if fs::metadata(&path).map(|m| m.len() == 0).unwrap_or(false) {
            fs::write(&path, b"- \n").map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(stem)
    }

    /// 이름 바꾸기 + 모든 페이지의 `[[옛 이름]]` 링크 갱신. 링크를 고친 파일 수를 돌려준다.
    pub fn rename(&self, old: &str, new: &str) -> Result<usize, String> {
        let old = validate_name(old)?;
        let new = validate_name(new)?;
        if old == new {
            return Err("같은 이름입니다".into());
        }
        let (src, dst) = (self.path_of(&old)?, self.path_of(&new)?);
        if !src.is_file() {
            return Err("페이지를 찾을 수 없습니다".into());
        }
        let case_only = old.to_lowercase() == new.to_lowercase();
        if dst.exists() && !case_only {
            return Err("같은 이름의 페이지가 이미 있습니다".into());
        }
        fs::rename(&src, &dst).map_err(|e| format!("이름을 바꾸지 못했습니다: {e}"))?;
        match outliner_core::rename_page_in_files(&self.files(), &old, &new, false) {
            Ok(r) => Ok(r.count),
            Err(e) => Err(format!("이름은 바꿨지만 링크를 갱신하지 못했습니다: {e}")),
        }
    }

    /// 삭제 = `.trash/<이름>.<시각>.md` 로 옮김. 옮긴 경로를 돌려준다.
    pub fn delete(&self, name: &str, stamp: &str) -> Result<PathBuf, String> {
        let stem = validate_name(name)?;
        let src = self.path_of(&stem)?;
        if !src.is_file() {
            return Err("페이지를 찾을 수 없습니다".into());
        }
        let trash = self.root.join(TRASH_DIR);
        fs::create_dir_all(&trash).map_err(|e| format!("{}: {e}", trash.display()))?;
        let mut dst = trash.join(format!("{stem}.{stamp}.md"));
        let mut n = 1;
        while dst.exists() {
            dst = trash.join(format!("{stem}.{stamp}-{n}.md"));
            n += 1;
        }
        fs::rename(&src, &dst).map_err(|e| format!("삭제하지 못했습니다: {e}"))?;
        Ok(dst)
    }

    /// `.trash` 안 파일 이름(최근 것이 뒤).
    pub fn list_trash(&self) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.root.join(TRASH_DIR))
            .map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.ends_with(".md")).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// `.trash` 의 파일을 원래 이름으로 되돌린다(같은 이름 페이지가 있으면 오류). 되돌린 이름을 돌려준다.
    pub fn restore(&self, trash_file: &str) -> Result<String, String> {
        if Path::new(trash_file).file_name().map(|n| n.to_string_lossy() != trash_file).unwrap_or(true) {
            return Err("복원할 파일 이름을 확인해 주세요".into());
        }
        let src = self.root.join(TRASH_DIR).join(trash_file);
        if !src.is_file() {
            return Err("휴지통에서 파일을 찾을 수 없습니다".into());
        }
        // `<이름>.<시각>.md` 또는 `<이름>.<시각>-N.md` → 이름은 뒤에서 두 번째 점 앞까지.
        let stem = trash_file.trim_end_matches(".md");
        let name = stem.rsplit_once('.').map(|(a, _)| a).unwrap_or(stem).to_string();
        let dst = self.path_of(&name)?;
        if dst.exists() {
            return Err("같은 이름의 페이지가 이미 있어 복원하지 못했습니다".into());
        }
        fs::rename(&src, &dst).map_err(|e| format!("복원하지 못했습니다: {e}"))?;
        Ok(name)
    }

    /// 옛 `YYYY-MM-DD` 일지 이름이면 `YYYY_MM_DD` 로 정리(같은 이름이 이미 있으면 그대로 둔다). 정리된 이름을 돌려준다.
    pub fn normalize_journal(&self, name: &str) -> Result<String, String> {
        let stem = validate_name(name)?;
        if !is_dashed_journal(&stem) || !self.path_of(&stem)?.is_file() {
            return Ok(stem);
        }
        let target = outliner_core::journal_name(&stem).map_err(|e| e.to_string())?;
        if self.path_of(&target)?.exists() {
            return Ok(stem);
        }
        self.rename(&stem, &target)?;
        Ok(target)
    }

    /// 오늘의 일지(`YYYY_MM_DD.md`)를 만들고(없으면) 이름을 돌려준다. `date_iso` 는 `YYYY-MM-DD`.
    pub fn journal_today(&self, date_iso: &str) -> Result<String, String> {
        let name = outliner_core::journal_name(date_iso).map_err(|e| e.to_string())?;
        if !self.path_of(&name)?.exists() && self.path_of(date_iso)?.is_file() {
            self.normalize_journal(date_iso)?;
        }
        self.ensure_page(&name)
    }

    // ---------- 최근 페이지 ----------

    /// 저장된 최근 목록 중 실제로 있는 페이지만(최대 20).
    pub fn load_recent(&self, available: &[String]) -> Vec<String> {
        let Some(p) = &self.recent_path else { return Vec::new() };
        let Ok(text) = fs::read_to_string(p) else { return Vec::new() };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
        let mut out: Vec<String> = Vec::new();
        for s in v.as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
            if available.iter().any(|a| a == s) && !out.iter().any(|o| o == s) {
                out.push(s.to_string());
            }
            if out.len() >= RECENT_MAX {
                break;
            }
        }
        out
    }

    pub fn save_recent(&self, recent: &[String]) {
        let Some(p) = &self.recent_path else { return };
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let list: Vec<&String> = recent.iter().take(RECENT_MAX).collect();
        let _ = fs::write(p, serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()));
    }
}
