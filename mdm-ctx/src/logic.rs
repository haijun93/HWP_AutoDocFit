//! 순수 로직: 인자 해석·경로 거르기·자연 정렬·명령줄 나누기. 시스템 호출이 없어 단위 시험이 쉽다.
use std::cmp::Ordering;
use std::path::Path;

/// 파일변환 입력 형식(`docfit_core/file_conversion.py`의 CONVERSION_INPUTS와 같아야 한다 — tests/test_mdm_ctx.py가 대조).
pub const INPUT_EXTENSIONS: [&str; 20] = [
    ".bmp", ".docx", ".gif", ".hml", ".htm", ".html", ".hwp", ".hwpx", ".jpeg", ".jpg",
    ".md", ".pdf", ".png", ".pptx", ".tif", ".tiff", ".txt", ".webp", ".xls", ".xlsx",
];

/// 명령줄 길이 한계(CreateProcess 32,767자)에서 여유를 둔 값.
pub const MAX_COMMAND_CHARS: usize = 30_000;

#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    pub app: Option<String>,
    pub target: Option<String>,
    pub paths: Vec<String>,
    /// 앱 `convert`에 그대로 넘길 옵션(`--notify` 등).
    pub passthrough: Vec<String>,
}

/// `mdm-ctx --app <exe> convert <경로…> --to <형식> [--notify] [-o <폴더>]`
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut parsed = Parsed::default();
    let mut iter = args.iter();
    let mut seen_convert = false;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--app" => parsed.app = Some(iter.next().ok_or("--app 값이 없습니다")?.clone()),
            "--to" => parsed.target = Some(iter.next().ok_or("--to 값이 없습니다")?.clone()),
            "-o" | "--output-dir" | "--timeout" => {
                let value = iter.next().ok_or("옵션 값이 없습니다")?.clone();
                parsed.passthrough.push(arg.clone());
                parsed.passthrough.push(value);
            }
            "--notify" | "--quiet" | "--json" => parsed.passthrough.push(arg.clone()),
            "convert" if !seen_convert => seen_convert = true,
            other if other.starts_with("--") => return Err(format!("알 수 없는 옵션: {other}")),
            other => {
                if !seen_convert {
                    return Err("첫 명령은 convert 여야 합니다".into());
                }
                parsed.paths.push(other.to_string());
            }
        }
    }
    if !seen_convert { return Err("convert 명령이 없습니다".into()); }
    if parsed.target.is_none() { return Err("--to 가 없습니다".into()); }
    if parsed.app.is_none() { return Err("--app 이 없습니다".into()); }
    Ok(parsed)
}

pub fn supported(path: &str) -> bool {
    match Path::new(path).extension() {
        Some(ext) => {
            let dotted = format!(".{}", ext.to_string_lossy().to_lowercase());
            INPUT_EXTENSIONS.contains(&dotted.as_str())
        }
        None => false,
    }
}

fn natural_key(path: &str) -> Vec<(String, u128)> {
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut key = Vec::new();
    let (mut text, mut digits) = (String::new(), String::new());
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            if !digits.is_empty() {
                key.push((std::mem::take(&mut text), digits.parse().unwrap_or(u128::MAX)));
                digits.clear();
            }
            text.push(ch);
        }
    }
    key.push((text, digits.parse().unwrap_or(0)));
    key
}

/// 파일 이름 자연 정렬(1, 2, 10 순) — `shell_batch.natural_sort_key`와 같은 규칙.
pub fn natural_cmp(a: &str, b: &str) -> Ordering { natural_key(a).cmp(&natural_key(b)) }

/// 대소문자 무시 중복 제거 + 자연 정렬. 입력 순서와 무관하게 결과가 같다.
pub fn dedup_sort(paths: Vec<String>) -> Vec<String> {
    let mut sorted = paths;
    sorted.sort();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = sorted.into_iter().filter(|p| seen.insert(p.to_lowercase())).collect();
    out.sort_by(|a, b| natural_cmp(a, b).then_with(|| a.cmp(b)));
    out
}

/// 명령줄이 한계를 넘지 않게 경로를 묶음으로 나눈다. `fixed` 는 경로 말고 고정으로 붙는 글자 수.
pub fn chunk_paths(paths: &[String], fixed: usize) -> Vec<Vec<String>> {
    let mut chunks: Vec<Vec<String>> = vec![Vec::new()];
    let mut used = fixed;
    for p in paths {
        let cost = p.chars().count() + 3;
        if used + cost > MAX_COMMAND_CHARS && !chunks.last().unwrap().is_empty() {
            chunks.push(Vec::new());
            used = fixed;
        }
        chunks.last_mut().unwrap().push(p.clone());
        used += cost;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(items: &[&str]) -> Vec<String> { items.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn parses_menu_command() {
        let p = parse(&v(&["--app", r"C:\앱 폴더\a.exe", "convert", r"C:\문 서\가 나.hwp", "--to", "pdf", "--notify"])).unwrap();
        assert_eq!(p.app.as_deref(), Some(r"C:\앱 폴더\a.exe"));
        assert_eq!(p.target.as_deref(), Some("pdf"));
        assert_eq!(p.paths, v(&[r"C:\문 서\가 나.hwp"]));
        assert_eq!(p.passthrough, v(&["--notify"]));
    }
    #[test]
    fn rejects_bad_args() {
        assert!(parse(&v(&["convert", "a.hwp", "--to", "pdf"])).is_err()); // --app 없음
        assert!(parse(&v(&["--app", "x", "convert", "a.hwp"])).is_err()); // --to 없음
        assert!(parse(&v(&["--app", "x", "a.hwp", "--to", "pdf"])).is_err()); // convert 없음
        assert!(parse(&v(&["--app", "x", "convert", "a.hwp", "--bogus"])).is_err());
    }
    #[test]
    fn extension_filter_is_case_insensitive() {
        assert!(supported(r"C:\a\b.HWP"));
        assert!(supported("x.xlsx"));
        assert!(!supported("x.exe"));
        assert!(!supported("noext"));
        assert!(!supported("x.doc"));
    }
    #[test]
    fn natural_sort_and_dedup() {
        let out = dedup_sort(v(&[r"C:\d\a10.pdf", r"C:\d\A2.PDF", r"C:\d\a2.pdf", r"C:\d\a1.pdf"]));
        assert_eq!(out, v(&[r"C:\d\a1.pdf", r"C:\d\A2.PDF", r"C:\d\a10.pdf"]));
    }
    #[test]
    fn chunks_respect_limit() {
        let long: Vec<String> = (0..200).map(|i| format!("C:\\{}{}.pdf", "가".repeat(240), i)).collect();
        let chunks = chunk_paths(&long, 200);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), 200);
        for c in &chunks {
            let total: usize = c.iter().map(|p| p.chars().count() + 3).sum::<usize>() + 200;
            assert!(total <= MAX_COMMAND_CHARS);
        }
        assert_eq!(chunk_paths(&[], 10), vec![Vec::<String>::new()]);
    }
}
