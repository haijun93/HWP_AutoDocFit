//! 읽기 전용 파일 이름 미리보기의 순수 계산부. 디스크 접근은 모두 호출자가 주입한다.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewRow {
    pub original: String,
    pub proposed: String,
    pub status: RowStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowStatus {
    Changed,
    Unchanged,
    Conflict,
    /// 윈도 파일 이름 규칙에 어긋남(빈 이름·금지 문자·예약어 등).
    Invalid,
}

/// 이름의 stem에 접두사와 번호 규칙을 적용한다. 파일 시스템에는 접근하지 않는다.
pub fn preview_name(name: &str, prefix: &str, number: usize, digits: usize) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    format!("{prefix}{stem}_{number:0digits$}{ext}")
}

/// Windows 대소문자 무시 기준으로 제안 이름 충돌을 판정한다.
pub fn mark_collisions(rows: &mut [PreviewRow], existing: &[&str]) {
    let mut seen: Vec<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    for row in rows {
        if matches!(row.status, RowStatus::Unchanged | RowStatus::Invalid) {
            continue;
        }
        let key = row.proposed.to_lowercase();
        if seen.contains(&key) {
            row.status = RowStatus::Conflict;
        } else {
            seen.push(key);
        }
    }
}

/// 사용자가 입력한 규칙. 모두 순수 문자열 규칙이며 정규식은 쓰지 않는다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    pub prefix: String,
    pub suffix: String,
    /// 0이면 순번을 붙이지 않는다. 최대 9.
    pub digits: usize,
    pub find: String,
    pub replace: String,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            prefix: String::new(),
            suffix: String::new(),
            digits: 3,
            find: String::new(),
            replace: String::new(),
        }
    }
}

/// 자릿수 입력 문자열을 0..=9로 해석한다. 비었거나 숫자가 아니면 0.
pub fn parse_digits(text: &str) -> usize {
    text.trim().parse::<usize>().map(|n| n.min(9)).unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub dir: String,
    pub name: String,
}

/// `\` 또는 `/` 기준으로 폴더와 파일 이름을 나눈다.
pub fn split_path(path: &str) -> FileEntry {
    match path.rfind(['\\', '/']) {
        Some(i) => FileEntry {
            dir: path[..i].to_string(),
            name: path[i + 1..].to_string(),
        },
        None => FileEntry {
            dir: String::new(),
            name: path.to_string(),
        },
    }
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// 규칙 전체를 한 이름에 적용한다(찾아 바꾸기 → 접두사·접미사 → 순번). 확장자는 유지.
pub fn apply_rules(name: &str, rules: &Rules, number: usize) -> String {
    let (stem, ext) = split_ext(name);
    let mut stem = if rules.find.is_empty() {
        stem.to_string()
    } else {
        stem.replace(&rules.find, &rules.replace)
    };
    stem = format!("{}{}{}", rules.prefix, stem, rules.suffix);
    if rules.digits > 0 {
        let d = rules.digits;
        stem = format!("{stem}_{number:0d$}");
    }
    format!("{stem}{ext}")
}

/// 이름이 윈도 규칙에 어긋나면 이유를 돌려준다.
pub fn invalid_reason(name: &str) -> Option<&'static str> {
    let (stem, _) = split_ext(name);
    if name.is_empty() || stem.is_empty() {
        return Some("이름이 비어 있음");
    }
    if name.encode_utf16().count() > 255 {
        return Some("이름이 너무 김");
    }
    if name
        .chars()
        .any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
    {
        return Some("쓸 수 없는 문자");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Some("끝에 점·공백");
    }
    let base = stem.trim_end().to_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit()
            && base.as_bytes()[3] != b'0');
    if reserved {
        return Some("예약된 이름");
    }
    None
}

/// 선택한 파일들의 미리보기 행을 만든다. 디스크는 읽지 않고 `dirs`(폴더별 기존 이름)만 본다.
/// 선택된 파일의 현재 이름은 비워질 자리로 보고, 바뀌지 않는 선택 파일은 그대로 자리를 차지한다.
pub fn analyze(
    files: &[FileEntry],
    rules: &Rules,
    dirs: &HashMap<String, Vec<String>>,
) -> Vec<PreviewRow> {
    let mut rows: Vec<PreviewRow> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let proposed = apply_rules(&f.name, rules, i + 1);
            let status = if proposed == f.name {
                RowStatus::Unchanged
            } else if invalid_reason(&proposed).is_some() {
                RowStatus::Invalid
            } else {
                RowStatus::Changed
            };
            PreviewRow {
                original: f.name.clone(),
                proposed,
                status,
            }
        })
        .collect();
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let key = f.dir.to_lowercase();
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    for (_, idx) in groups {
        let dir = &files[idx[0]].dir;
        let selected: Vec<String> = idx.iter().map(|&i| files[i].name.to_lowercase()).collect();
        let mut existing: Vec<String> = dirs
            .get(dir)
            .map(|v| {
                v.iter()
                    .filter(|n| !selected.contains(&n.to_lowercase()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        existing.extend(
            idx.iter()
                .filter(|&&i| rows[i].status == RowStatus::Unchanged)
                .map(|&i| rows[i].original.clone()),
        );
        let existing_ref: Vec<&str> = existing.iter().map(String::as_str).collect();
        let mut group_rows: Vec<PreviewRow> = idx.iter().map(|&i| rows[i].clone()).collect();
        mark_collisions(&mut group_rows, &existing_ref);
        for (k, &i) in idx.iter().enumerate() {
            rows[i] = group_rows[k].clone();
        }
    }
    rows
}

/// `GetOpenFileNameW` 다중 선택 버퍼(`폴더<NUL>파일1<NUL>파일2<NUL><NUL>`, 한 개면 `전체경로<NUL><NUL>`)를 전체 경로 목록으로 푼다.
pub fn parse_open_buffer(buf: &[u16]) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for chunk in buf.split(|&c| c == 0) {
        if chunk.is_empty() {
            break;
        }
        parts.push(String::from_utf16_lossy(chunk));
    }
    match parts.len() {
        0 => Vec::new(),
        1 => parts,
        _ => {
            let dir = parts[0].trim_end_matches('\\').to_string();
            parts[1..].iter().map(|f| format!("{dir}\\{f}")).collect()
        }
    }
}

/// 화면이 들고 있는 상태. 파일 시스템 접근(파일 여부·폴더 목록)은 호출자가 주입한다.
#[derive(Default)]
pub struct Model {
    pub files: Vec<FileEntry>,
    pub rules: Rules,
    pub rows: Vec<PreviewRow>,
    pub dirs: HashMap<String, Vec<String>>,
    pub scroll: usize,
}

impl Model {
    /// 경로를 목록에 더한다. 파일이 아닌 것·이미 있는 것은 건너뛰고 더한 수를 돌려준다.
    pub fn add_paths(
        &mut self,
        paths: &[String],
        is_file: impl Fn(&str) -> bool,
        list_dir: impl Fn(&str) -> Vec<String>,
    ) -> usize {
        let mut added = 0;
        for p in paths {
            if !is_file(p) {
                continue;
            }
            let e = split_path(p);
            if self.files.iter().any(|f| {
                f.dir.to_lowercase() == e.dir.to_lowercase()
                    && f.name.to_lowercase() == e.name.to_lowercase()
            }) {
                continue;
            }
            if !self.dirs.contains_key(&e.dir) {
                let names = list_dir(&e.dir);
                self.dirs.insert(e.dir.clone(), names);
            }
            self.files.push(e);
            added += 1;
        }
        self.recompute();
        added
    }
    pub fn clear(&mut self) {
        self.files.clear();
        self.dirs.clear();
        self.scroll = 0;
        self.recompute();
    }
    pub fn recompute(&mut self) {
        self.rows = analyze(&self.files, &self.rules, &self.dirs);
        self.scroll = self.scroll.min(self.rows.len().saturating_sub(1));
    }
    pub fn count(&self, s: RowStatus) -> usize {
        self.rows.iter().filter(|r| r.status == s).count()
    }
    pub fn summary(&self) -> String {
        if self.files.is_empty() {
            return "파일을 끌어 놓거나 ‘파일 열기’로 고르세요".to_string();
        }
        format!(
            "{}개 · 변경 예정 {} · 변화 없음 {} · 충돌 {} · 이름 오류 {}",
            self.files.len(),
            self.count(RowStatus::Changed),
            self.count(RowStatus::Unchanged),
            self.count(RowStatus::Conflict),
            self.count(RowStatus::Invalid)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(dir: &str, name: &str) -> FileEntry {
        FileEntry {
            dir: dir.into(),
            name: name.into(),
        }
    }
    fn dirs(list: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        list.iter()
            .map(|(d, v)| (d.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn preview_keeps_extension_and_formats_number() {
        assert_eq!(
            preview_name("사진 1.JPG", "행사_", 7, 3),
            "행사_사진 1_007.JPG"
        );
        assert_eq!(preview_name("이름", "", 12, 2), "이름_12");
    }

    #[test]
    fn collisions_are_case_insensitive_and_mark_only_colliding_rows() {
        let mut rows = vec![
            PreviewRow {
                original: "a.txt".into(),
                proposed: "New.txt".into(),
                status: RowStatus::Changed,
            },
            PreviewRow {
                original: "b.txt".into(),
                proposed: "new.TXT".into(),
                status: RowStatus::Changed,
            },
        ];
        mark_collisions(&mut rows, &["NEW.txt"]);
        assert!(rows.iter().all(|r| r.status == RowStatus::Conflict));
    }

    #[test]
    fn same_proposal_in_one_folder_marks_only_the_later_row() {
        let mut rows = vec![
            PreviewRow {
                original: "a.txt".into(),
                proposed: "Same.txt".into(),
                status: RowStatus::Changed,
            },
            PreviewRow {
                original: "b.txt".into(),
                proposed: "same.txt".into(),
                status: RowStatus::Changed,
            },
        ];
        mark_collisions(&mut rows, &[]);
        assert_eq!(rows[0].status, RowStatus::Changed);
        assert_eq!(rows[1].status, RowStatus::Conflict);
    }

    #[test]
    fn rules_apply_find_replace_prefix_suffix_number() {
        let r = Rules {
            prefix: "행사_".into(),
            suffix: "_완".into(),
            digits: 2,
            find: "사진".into(),
            replace: "img".into(),
        };
        assert_eq!(apply_rules("사진 3.JPG", &r, 5), "행사_img 3_완_05.JPG");
        let none = Rules {
            digits: 0,
            ..Rules::default()
        };
        assert_eq!(apply_rules("a.b.txt", &none, 1), "a.b.txt");
        assert_eq!(apply_rules(".hidden", &none, 1), ".hidden");
        assert_eq!(parse_digits(" 4 "), 4);
        assert_eq!(parse_digits("99"), 9);
        assert_eq!(parse_digits("가"), 0);
        assert_eq!(parse_digits(""), 0);
    }

    #[test]
    fn split_path_handles_both_separators() {
        assert_eq!(split_path("C:\\a\\b.txt"), entry("C:\\a", "b.txt"));
        assert_eq!(split_path("/x/y.md"), entry("/x", "y.md"));
        assert_eq!(split_path("z"), entry("", "z"));
    }

    #[test]
    fn invalid_names_are_detected() {
        assert_eq!(invalid_reason(""), Some("이름이 비어 있음"));
        assert_eq!(invalid_reason("a?b.txt"), Some("쓸 수 없는 문자"));
        assert_eq!(invalid_reason("a. "), Some("끝에 점·공백"));
        assert_eq!(invalid_reason("con.txt"), Some("예약된 이름"));
        assert_eq!(invalid_reason("LPT3"), Some("예약된 이름"));
        assert_eq!(invalid_reason("com0.txt"), None);
        assert_eq!(invalid_reason("보고서 1.hwpx"), None);
        assert_eq!(invalid_reason(&"가".repeat(256)), Some("이름이 너무 김"));
    }

    #[test]
    fn rename_onto_unchanged_selected_file_conflicts() {
        let files = [entry("D", "a1.txt"), entry("D", "a2.txt")];
        let r = Rules {
            digits: 0,
            find: "a1".into(),
            replace: "a2".into(),
            ..Rules::default()
        };
        let rows = analyze(&files, &r, &dirs(&[("D", &["a1.txt", "a2.txt"])]));
        assert_eq!(rows[0].status, RowStatus::Conflict);
        assert_eq!(rows[1].status, RowStatus::Unchanged);
    }

    #[test]
    fn collision_with_unselected_existing_file_is_case_insensitive() {
        let files = [entry("D", "a.txt")];
        let r = Rules {
            prefix: "x_".into(),
            digits: 0,
            ..Rules::default()
        };
        let rows = analyze(&files, &r, &dirs(&[("D", &["a.txt", "X_A.TXT"])]));
        assert_eq!(rows[0].status, RowStatus::Conflict);
        let rows = analyze(&files, &r, &dirs(&[("D", &["a.txt", "other.txt"])]));
        assert_eq!(rows[0].status, RowStatus::Changed);
    }

    #[test]
    fn same_name_in_different_folders_does_not_conflict() {
        let files = [entry("D1", "a.txt"), entry("D2", "a.txt")];
        let r = Rules {
            prefix: "n_".into(),
            digits: 0,
            ..Rules::default()
        };
        let rows = analyze(&files, &r, &dirs(&[("D1", &["a.txt"]), ("D2", &["a.txt"])]));
        assert!(rows.iter().all(|x| x.status == RowStatus::Changed));
    }

    #[test]
    fn two_files_becoming_same_name_conflict_and_case_only_change_is_ok() {
        let files = [entry("D", "a1.txt"), entry("D", "a2.txt")];
        let r = Rules {
            digits: 0,
            find: "a".into(),
            replace: "z".into(),
            ..Rules::default()
        };
        let rows = analyze(&files, &r, &dirs(&[("D", &["a1.txt", "a2.txt"])]));
        assert_eq!(rows[0].status, RowStatus::Changed);
        assert_eq!(rows[1].status, RowStatus::Changed);
        let r = Rules {
            digits: 0,
            find: "a1".into(),
            replace: "q".into(),
            ..Rules::default()
        };
        let same = [entry("D", "a1.txt"), entry("D", "q.txt")];
        let rows = analyze(&same, &r, &dirs(&[("D", &["a1.txt", "q.txt"])]));
        assert_eq!(rows[0].status, RowStatus::Conflict);
        let up = Rules {
            digits: 0,
            find: "a".into(),
            replace: "A".into(),
            ..Rules::default()
        };
        let rows = analyze(&[entry("D", "a.txt")], &up, &dirs(&[("D", &["a.txt"])]));
        assert_eq!(rows[0].proposed, "A.txt");
        assert_eq!(rows[0].status, RowStatus::Changed);
    }

    #[test]
    fn numbering_is_applied_and_invalid_is_flagged() {
        let files = [entry("D", "a.txt"), entry("D", "b.txt")];
        let r = Rules {
            prefix: "p".into(),
            ..Rules::default()
        };
        let rows = analyze(&files, &r, &dirs(&[("D", &["a.txt", "b.txt"])]));
        assert_eq!(rows[0].proposed, "pa_001.txt");
        assert_eq!(rows[1].proposed, "pb_002.txt");
        let bad = Rules {
            prefix: "a/".into(),
            ..Rules::default()
        };
        assert!(
            analyze(&files, &bad, &dirs(&[]))
                .iter()
                .all(|x| x.status == RowStatus::Invalid)
        );
    }

    #[test]
    fn open_buffer_parses_single_and_multiple_selection() {
        let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let single = [w("C:\\d\\가.txt"), vec![0, 0]].concat();
        assert_eq!(parse_open_buffer(&single), ["C:\\d\\가.txt"]);
        let multi = [
            w("C:\\d"),
            vec![0],
            w("a.txt"),
            vec![0],
            w("나.hwp"),
            vec![0, 0, 0],
        ]
        .concat();
        assert_eq!(parse_open_buffer(&multi), ["C:\\d\\a.txt", "C:\\d\\나.hwp"]);
        let root = [w("C:\\"), vec![0], w("a.txt"), vec![0, 0]].concat();
        assert_eq!(parse_open_buffer(&root), ["C:\\a.txt"]);
        assert!(parse_open_buffer(&[0, 0]).is_empty());
    }

    #[test]
    fn model_adds_only_new_files_and_summarizes() {
        let mut m = Model::default();
        let ok = |p: &str| !p.ends_with("dir");
        let ls = |_: &str| vec!["a.txt".to_string(), "b.txt".to_string()];
        let n = m.add_paths(
            &[
                "D\\a.txt".into(),
                "D\\dir".into(),
                "D\\A.TXT".into(),
                "D\\b.txt".into(),
            ],
            ok,
            ls,
        );
        assert_eq!(n, 2);
        assert_eq!(m.files.len(), 2);
        assert!(m.summary().starts_with("2개 · 변경 예정 2"));
        m.clear();
        assert!(m.rows.is_empty());
        assert!(m.summary().contains("끌어 놓"));
    }
}
