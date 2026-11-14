//! 날짜·시각 및 ISO 8601 주차 계산 모듈 (순수 Rust 구현).
use serde::{Deserialize, Serialize};

/// 한국 표준시(KST = UTC+9) 오프셋 (초).
pub const KST_OFFSET_SECS: i64 = 9 * 3600;
/// 하루의 초 수.
pub const SECS_PER_DAY: i64 = 86400;

/// 연·월·일 날짜 구조체.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub const fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    /// 타임스탬프(초)와 오프셋(초)으로부터 날짜를 생성한다 (기본 KST).
    pub fn from_timestamp(ts: i64, offset_secs: i64) -> Self {
        let local_secs = ts + offset_secs;
        let days = local_secs.div_euclid(SECS_PER_DAY);
        let (y, m, d) = days_to_ymd(days);
        Self {
            year: y,
            month: m,
            day: d,
        }
    }

    /// 날짜를 타임스탬프 시작(00:00:00)으로 변환한다.
    pub fn to_timestamp(&self, offset_secs: i64) -> i64 {
        let days = ymd_to_days(self.year, self.month, self.day);
        days * SECS_PER_DAY - offset_secs
    }

    /// YYYY-MM-DD 형식 문자열로 반환한다.
    pub fn to_string_ymd(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// "YYYY-MM-DD", "YYYY.MM.DD", "YYYYMMDD" 형식 문자열을 파싱한다.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.len() == 10 {
            let b = s.as_bytes();
            if (b[4] == b'-' && b[7] == b'-') || (b[4] == b'.' && b[7] == b'.') {
                let y: i32 = s[0..4].parse().ok()?;
                let m: u32 = s[5..7].parse().ok()?;
                let d: u32 = s[8..10].parse().ok()?;
                if (1..=12).contains(&m) && (1..=31).contains(&d) {
                    return Some(Self::new(y, m, d));
                }
            }
        } else if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
            let y: i32 = s[0..4].parse().ok()?;
            let m: u32 = s[4..6].parse().ok()?;
            let d: u32 = s[6..8].parse().ok()?;
            if (1..=12).contains(&m) && (1..=31).contains(&d) {
                return Some(Self::new(y, m, d));
            }
        }
        None
    }
}

/// ISO 8601 주차 구조체 (연도와 주 번호).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IsoWeek {
    pub year: i32,
    pub week: u32,
}

impl IsoWeek {
    pub const fn new(year: i32, week: u32) -> Self {
        Self { year, week }
    }
}

/// 분석 대상 기간.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Period {
    /// 시작 시각 타임스탬프 (초, 포함).
    pub start: i64,
    /// 종료 시각 타임스탬프 (초, 포함).
    pub end: i64,
}

impl Period {
    pub const fn new(start: i64, end: i64) -> Self {
        Self { start, end }
    }

    /// 연도 전체(1월 1일 ~ 12월 31일)를 KST 기준으로 생성한다.
    pub fn full_year(year: i32) -> Self {
        let start = Date::new(year, 1, 1).to_timestamp(KST_OFFSET_SECS);
        let end = Date::new(year, 12, 31).to_timestamp(KST_OFFSET_SECS) + SECS_PER_DAY - 1;
        Self { start, end }
    }

    /// 연초(1월 1일)부터 오늘(지정 타임스탬프)까지의 기간을 생성한다.
    pub fn year_to_date(now_ts: i64, offset_secs: i64) -> Self {
        let today = Date::from_timestamp(now_ts, offset_secs);
        let start = Date::new(today.year, 1, 1).to_timestamp(offset_secs);
        Self { start, end: now_ts }
    }

    /// 타임스탬프가 기간에 포함되는지 확인한다.
    #[inline]
    pub fn contains(&self, ts: i64) -> bool {
        ts >= self.start && ts <= self.end
    }
}

/// 유닉스 일수(1970-01-01 = 0)로부터 (년, 월, 일)을 계산한다 (Hinnant 알고리즘).
pub fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// (년, 월, 일)로부터 유닉스 일수를 계산한다.
pub fn ymd_to_days(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + (d as i64) - 1;
    let doe = (yoe as i64) * 365 + (yoe as i64) / 4 - (yoe as i64) / 100 + doy;
    era * 146097 + doe - 719468
}

/// 유닉스 일수로부터 요일을 구한다 (1 = 월요일, 2 = 화요일, ..., 7 = 일요일).
#[inline]
pub fn days_to_weekday(days: i64) -> u32 {
    // 1970-01-01은 목요일(4)
    ((days + 3).rem_euclid(7) + 1) as u32
}

/// 타임스탬프로부터 요일을 구한다 (1 = 월요일, ..., 7 = 일요일).
#[inline]
pub fn timestamp_to_weekday(ts: i64, offset_secs: i64) -> u32 {
    let days = (ts + offset_secs).div_euclid(SECS_PER_DAY);
    days_to_weekday(days)
}

/// 유닉스 일수로부터 ISO 8601 주차를 계산한다.
pub fn days_to_iso_week(days: i64) -> IsoWeek {
    let weekday = days_to_weekday(days) as i64;
    // 해당 주의 목요일 일수
    let thu_days = days + (4 - weekday);
    let (thu_year, _, _) = days_to_ymd(thu_days);

    // 해당 연도 1월 1일의 목요일 일수
    let jan1_days = ymd_to_days(thu_year, 1, 1);
    let jan1_weekday = days_to_weekday(jan1_days) as i64;
    let first_thu_days = jan1_days + ((4 - jan1_weekday + 7) % 7);

    let week = ((thu_days - first_thu_days) / 7 + 1) as u32;
    IsoWeek {
        year: thu_year,
        week,
    }
}

/// 타임스탬프로부터 ISO 8601 주차를 계산한다.
#[inline]
pub fn timestamp_to_iso_week(ts: i64, offset_secs: i64) -> IsoWeek {
    let days = (ts + offset_secs).div_euclid(SECS_PER_DAY);
    days_to_iso_week(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ymd_conversion() {
        let cases = [
            (1970, 1, 1, 0),
            (2026, 11, 14, 20771),
            (2000, 2, 29, 11016),
            (2024, 1, 1, 19723),
        ];
        for (y, m, d, expected_days) in cases {
            let days = ymd_to_days(y, m, d);
            assert_eq!(days, expected_days, "ymd_to_days mismatch for {y}-{m}-{d}");
            let (ry, rm, rd) = days_to_ymd(days);
            assert_eq!((ry, rm, rd), (y, m, d), "days_to_ymd mismatch");
        }
    }

    #[test]
    fn test_weekday() {
        // 1970-01-01: Thursday (4)
        assert_eq!(days_to_weekday(0), 4);
        // 2026-11-14: Saturday (6)
        let days = ymd_to_days(2026, 11, 14);
        assert_eq!(days_to_weekday(days), 6);
        // 2026-11-16: Monday (1)
        let days_mon = ymd_to_days(2026, 11, 16);
        assert_eq!(days_to_weekday(days_mon), 1);
        // 2026-11-15: Sunday (7)
        let days_sun = ymd_to_days(2026, 11, 15);
        assert_eq!(days_to_weekday(days_sun), 7);
    }

    #[test]
    fn test_iso_week_calculation() {
        // 2026-11-14 -> 2026년 46주차
        let days = ymd_to_days(2026, 11, 14);
        let iso = days_to_iso_week(days);
        assert_eq!(iso, IsoWeek::new(2026, 46));

        // 2023-01-01 (일요일) -> 2022년 52주차
        let days = ymd_to_days(2023, 1, 1);
        let iso = days_to_iso_week(days);
        assert_eq!(iso, IsoWeek::new(2022, 52));

        // 2024-01-01 (월요일) -> 2024년 1주차
        let days = ymd_to_days(2024, 1, 1);
        let iso = days_to_iso_week(days);
        assert_eq!(iso, IsoWeek::new(2024, 1));
    }

    #[test]
    fn test_period_contains() {
        let p = Period::full_year(2026);
        let ts_mid = Date::new(2026, 6, 15).to_timestamp(KST_OFFSET_SECS);
        let ts_before = Date::new(2025, 12, 31).to_timestamp(KST_OFFSET_SECS);
        let ts_after = Date::new(2027, 1, 1).to_timestamp(KST_OFFSET_SECS);

        assert!(p.contains(ts_mid));
        assert!(!p.contains(ts_before));
        assert!(!p.contains(ts_after));
    }
}
