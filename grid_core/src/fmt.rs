//! 표시 글자(Flutter `_formatBytes` 와 같은 규칙 등)와 색 섞기.

/// Flutter `_formatBytes` 와 같은 규칙.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 유닉스 초 + 시간대 오프셋(초) → `YYYY-MM-DD`.
pub fn format_date(mtime: f64, offset_secs: i64) -> String {
    let secs = mtime.floor() as i64 + offset_secs;
    let days = secs.div_euclid(86_400);
    // Howard Hinnant 의 civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// 1234567 → "1,234,567".
pub fn group_digits(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 경과 초 → "m:ss" / "h:mm:ss".
pub fn format_elapsed(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// BGR(COLORREF) 두 색을 `t`(0~1, b 쪽 비율)로 섞는다.
pub fn blend(a: u32, b: u32, t: f64) -> u32 {
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xFF) as f64, ((b >> s) & 0xFF) as f64);
        ((x + (y - x) * t).round() as u32) & 0xFF
    };
    ch(0) | (ch(8) << 8) | (ch(16) << 16)
}
