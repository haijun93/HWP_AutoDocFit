//! 본문에서 시행일·작성일 후보를 추출하고 문맥별 신뢰도를 계산한다.
use crate::date::{Date, ymd_to_days};

/// 본문 날짜 후보와 신뢰도. 점수 범위는 0.0~1.0이다.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentDateCandidate {
    pub date: Date,
    pub score: f32,
    pub start: usize,
    pub end: usize,
    pub context: DateContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateContext {
    TitleArea,
    ApprovalArea,
    Labeled,
    Body,
    Quoted,
}

/// 본문에서 인식한 날짜 후보를 신뢰도 높은 순으로 돌려준다.
/// 제목 첫 5줄, 문서 끝 결재란, 날짜 라벨을 우선하고 인용문은 낮게 평가한다.
pub fn extract_document_dates(text: &str) -> Vec<DocumentDateCandidate> {
    let lines: Vec<&str> = text.lines().collect();
    let tail_start = lines.len().saturating_sub(8);
    let mut offsets = Vec::with_capacity(lines.len());
    let mut pos = 0;
    for line in &lines {
        offsets.push(pos);
        pos += line.len() + 1;
    }
    let mut out = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        let mut cursor = 0;
        while cursor < line.len() {
            let Some((date, end)) = parse_at(line, cursor) else {
                cursor += line[cursor..].chars().next().unwrap().len_utf8();
                continue;
            };
            let before = &line[..cursor];
            let after = &line[end..];
            let quoted = before.contains('“')
                || before.contains('「')
                || before.contains('"')
                || before.contains('‘')
                || after.contains('”')
                || after.contains('」')
                || after.contains('"')
                || after.contains('’');
            let labeled = [
                "시행일",
                "시행일자",
                "작성일",
                "작성일자",
                "일자",
                "작성",
                "발령일",
                "공포일",
            ]
            .iter()
            .any(|s| before.ends_with(s) || before.trim_end().ends_with(s));
            let approval = li >= tail_start
                && ["결재", "담당", "검토", "협조", "승인"]
                    .iter()
                    .any(|s| line.contains(s));
            let context = if quoted {
                DateContext::Quoted
            } else if li < 5 {
                DateContext::TitleArea
            } else if approval {
                DateContext::ApprovalArea
            } else if labeled {
                DateContext::Labeled
            } else {
                DateContext::Body
            };
            let score = match context {
                DateContext::TitleArea => 0.94,
                DateContext::ApprovalArea => 0.92,
                DateContext::Labeled => 0.90,
                DateContext::Body => 0.48,
                DateContext::Quoted => 0.16,
            };
            let start = offsets[li] + cursor;
            out.push(DocumentDateCandidate {
                date,
                score,
                start,
                end: offsets[li] + end,
                context,
            });
            cursor = end;
        }
    }
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.start.cmp(&b.start))
    });
    out
}

/// 날짜 후보와 파일 수정일 중 문서 날짜를 우선 적용할지 선택한다.
pub fn preferred_document_date(text: &str, modified: Date, prefer_document_date: bool) -> Date {
    if prefer_document_date {
        if let Some(c) = extract_document_dates(text)
            .into_iter()
            .find(|c| c.score >= 0.45)
        {
            return c.date;
        }
    }
    modified
}

fn parse_at(s: &str, start: usize) -> Option<(Date, usize)> {
    let b = s.as_bytes();
    if !b.get(start)?.is_ascii_digit() {
        return None;
    }
    let (year, mut i) = if s[start..].starts_with("20") && digits(b, start, 4) {
        (number(b, start, 4)? as i32, start + 4)
    } else {
        return None;
    };
    let month;
    let day;
    if s[i..].starts_with('년') {
        i += '년'.len_utf8();
        i = skip_space(s, i);
        let mstart = i;
        month = read_num(b, &mut i)?;
        i = skip_space(s, i);
        if !s[i..].starts_with('월') {
            return None;
        }
        i += '월'.len_utf8();
        i = skip_space(s, i);
        day = read_num(b, &mut i)?;
        i = skip_space(s, i);
        if s[i..].starts_with('일') {
            i += '일'.len_utf8();
        }
        let _ = mstart;
    } else {
        let sep = if b.get(i) == Some(&b'.') || b.get(i) == Some(&b'-') || b.get(i) == Some(&b'/') {
            let c = b[i];
            i += 1;
            i = skip_space(s, i);
            Some(c)
        } else {
            None
        };
        if sep.is_none() {
            return None;
        }
        month = read_num(b, &mut i)?;
        if sep == Some(b'.') {
            while b.get(i) == Some(&b'.') || b.get(i) == Some(&b' ') {
                i += 1;
            }
        } else if b.get(i) == Some(&sep.unwrap()) {
            i += 1;
            i = skip_space(s, i);
        } else {
            return None;
        }
        day = read_num(b, &mut i)?;
        if sep == Some(b'.') && b.get(i) == Some(&b'.') {
            i += 1;
        }
    }
    if i < b.len() && b[i].is_ascii_digit() {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = ymd_to_days(year, month, day);
    let (y, m, d) = crate::date::days_to_ymd(days);
    if (y, m, d) != (year, month, day) {
        return None;
    }
    Some((Date::new(year, month, day), i))
}
fn digits(b: &[u8], s: usize, n: usize) -> bool {
    b.get(s..s + n)
        .is_some_and(|x| x.iter().all(u8::is_ascii_digit))
}
fn number(b: &[u8], s: usize, n: usize) -> Option<u32> {
    std::str::from_utf8(b.get(s..s + n)?).ok()?.parse().ok()
}
fn read_num(b: &[u8], i: &mut usize) -> Option<u32> {
    let s = *i;
    while b.get(*i).is_some_and(u8::is_ascii_digit) {
        *i += 1;
    }
    if *i == s { None } else { number(b, s, *i - s) }
}
fn skip_space(s: &str, mut i: usize) -> usize {
    while s.as_bytes().get(i) == Some(&b' ') {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_twenty_plus_synthetic_forms_and_ranks_context() {
        let forms = [
            "2026. 3. 5.",
            "2026.03.05",
            "2026-03-05",
            "2026/03/05",
            "2026년 3월 5일",
            "2026년3월5일",
            "2026.3.5",
            "2026. 03. 05.",
            "2026-3-5",
            "2026/3/5",
            "시행일자 2026. 3. 5.",
            "시행일: 2026.03.05",
            "작성일 2026년 3월 5일",
            "작성일자: 2026-03-05",
            "발령일 2026.3.5.",
            "공포일자 2026.03.05",
            "2026. 03.05.",
            "2026.03. 05.",
            "2026. 3.05.",
            "2026년 03월 05일",
            "2026년 3월 05일",
            "2026-03-5",
            "2026/3/05",
        ];
        for f in forms {
            assert!(!extract_document_dates(f).is_empty(), "unparsed: {f}");
        }
        let c = extract_document_dates(
            "시행일: 2026. 3. 5.\n본문\n인용: “2024년 1월 2일”\n\n결재 담당 2026.03.06",
        );
        assert!(c.len() >= 3);
        assert_eq!(c[0].date, Date::new(2026, 3, 5));
        assert!(
            c.iter()
                .find(|x| x.context == DateContext::Quoted)
                .unwrap()
                .score
                < c[0].score
        );
    }
    #[test]
    fn modified_date_policy_is_explicit() {
        let body = "시행일자: 2026년 3월 5일";
        let modified = Date::new(2026, 4, 1);
        assert_eq!(
            preferred_document_date(body, modified, true),
            Date::new(2026, 3, 5)
        );
        assert_eq!(preferred_document_date(body, modified, false), modified);
    }
}
