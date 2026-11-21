//! 읽기 전용 파일 이름 미리보기의 순수 계산부.

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
        if row.status == RowStatus::Unchanged {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
