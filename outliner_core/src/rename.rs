use crate::{save_atomic, OutlinerError};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Result of a page rename and link update operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenameResult {
    pub old_name: String,
    pub new_name: String,
    pub affected_files: Vec<String>,
    pub count: usize,
    pub preview: bool,
}

fn is_fence_start(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let leading_spaces = line.len() - trimmed.len();
    if leading_spaces > 3 {
        return None;
    }
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let mut count = 1;
    for c in chars {
        if c == first {
            count += 1;
        } else {
            break;
        }
    }
    if count >= 3 {
        Some((first, count))
    } else {
        None
    }
}

fn is_fence_end(line: &str, fence_char: char, fence_count: usize) -> bool {
    let trimmed = line.trim_start();
    let leading_spaces = line.len() - trimmed.len();
    if leading_spaces > 3 {
        return false;
    }
    let mut chars = trimmed.chars();
    let mut count = 0;
    while let Some(c) = chars.next() {
        if c == fence_char {
            count += 1;
        } else {
            if !c.is_whitespace() {
                return false;
            }
            break;
        }
    }
    for c in chars {
        if !c.is_whitespace() {
            return false;
        }
    }
    count >= fence_count
}

/// Replaces a matching page target on one line while preserving wiki-link aliases.
/// Nested links and inline code are left untouched.
pub fn update_line_links(line: &str, old_name: &str, new_name: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;

    while i < n {
        // 1. Inline code spans: `...` or ```...```
        if chars[i] == '`' {
            let tick_start = i;
            while i < n && chars[i] == '`' {
                i += 1;
            }
            let tick_count = i - tick_start;
            let mut close_found = None;
            let mut j = i;
            while j < n {
                if chars[j] == '`' {
                    let c_start = j;
                    while j < n && chars[j] == '`' {
                        j += 1;
                    }
                    if j - c_start == tick_count {
                        close_found = Some(j);
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            if let Some(close_idx) = close_found {
                for k in tick_start..close_idx {
                    out.push(chars[k]);
                }
                i = close_idx;
                continue;
            } else {
                for k in tick_start..i {
                    out.push(chars[k]);
                }
                continue;
            }
        }

        // 2. Wikilinks: `[[...]]`
        if chars[i] == '[' && i + 1 < n && chars[i + 1] == '[' {
            // Nested link check: preceding character is also `[` (e.g. `[[[`)
            if i > 0 && chars[i - 1] == '[' {
                out.push(chars[i]);
                i += 1;
                continue;
            }

            // Markdown alias link check: e.g. `[alias]([[target]])`
            if i > 0 && chars[i - 1] == '(' {
                let mut prev = i - 1;
                while prev > 0 && chars[prev - 1].is_whitespace() {
                    prev -= 1;
                }
                if prev > 0 && chars[prev - 1] == ']' {
                    out.push(chars[i]);
                    i += 1;
                    continue;
                }
            }

            let content_start = i + 2;
            let mut j = content_start;
            let mut has_nested_bracket = false;
            let mut close_idx = None;

            while j < n {
                if chars[j] == '[' {
                    has_nested_bracket = true;
                }
                if chars[j] == ']' {
                    if j + 1 < n && chars[j + 1] == ']' {
                        if j + 2 < n && chars[j + 2] == ']' {
                            has_nested_bracket = true;
                        }
                        close_idx = Some(j);
                        break;
                    } else {
                        has_nested_bracket = true;
                    }
                }
                j += 1;
            }

            if let Some(c_idx) = close_idx {
                if !has_nested_bracket {
                    let target_end = chars[content_start..c_idx]
                        .iter()
                        .position(|character| *character == '|')
                        .map_or(c_idx, |offset| content_start + offset);
                    let target: String = chars[content_start..target_end].iter().collect();
                    if target == old_name {
                        out.push_str("[[");
                        out.push_str(new_name);
                        out.extend(chars[target_end..c_idx].iter());
                        out.push_str("]]");
                        i = c_idx + 2;
                        continue;
                    }
                }
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

/// Updates page links in full document text, preserving line endings
/// and ignoring fenced code blocks, inline code, nested links, and unrelated tags.
pub fn update_page_links_text(content: &str, old_name: &str, new_name: &str) -> (String, bool) {
    if old_name.is_empty() || old_name == new_name {
        return (content.to_string(), false);
    }

    let mut result = String::with_capacity(content.len());
    let mut changed = false;
    let mut in_fence: Option<(char, usize)> = None;

    for line in content.split_inclusive('\n') {
        let (line_without_ending, ending) = if let Some(without_lf) = line.strip_suffix('\n') {
            if let Some(without_cr) = without_lf.strip_suffix('\r') {
                (without_cr, "\r\n")
            } else {
                (without_lf, "\n")
            }
        } else {
            (line, "")
        };

        if let Some((f_char, f_count)) = in_fence {
            if is_fence_end(line_without_ending, f_char, f_count) {
                in_fence = None;
            }
            result.push_str(line_without_ending);
            result.push_str(ending);
        } else if let Some((f_char, f_count)) = is_fence_start(line_without_ending) {
            in_fence = Some((f_char, f_count));
            result.push_str(line_without_ending);
            result.push_str(ending);
        } else {
            let updated = update_line_links(line_without_ending, old_name, new_name);
            if updated != line_without_ending {
                changed = true;
            }
            result.push_str(&updated);
            result.push_str(ending);
        }
    }

    (result, changed)
}

/// Recursively collects `.md` files under the specified path.
pub fn collect_markdown_files(path: &Path) -> Result<Vec<PathBuf>, OutlinerError> {
    let mut files = Vec::new();
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(files);
    }
    if path.is_dir() {
        let entries = fs::read_dir(path).map_err(|e| OutlinerError::Io(e.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|e| OutlinerError::Io(e.to_string()))?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name == "target" {
                        continue;
                    }
                }
                files.extend(collect_markdown_files(&entry_path)?);
            } else if entry_path.extension().and_then(|s| s.to_str()) == Some("md") {
                files.push(entry_path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Updates page links across multiple files.
/// When `preview` is false, atomically saves modified files via temporary files.
/// Returns the list of affected file paths and count.
pub fn rename_page_in_files(
    paths: &[PathBuf],
    old_name: &str,
    new_name: &str,
    preview: bool,
) -> Result<RenameResult, OutlinerError> {
    if old_name.is_empty() {
        return Err(OutlinerError::Invalid("old_name이 필요합니다".into()));
    }
    if new_name.is_empty() {
        return Err(OutlinerError::Invalid("new_name이 필요합니다".into()));
    }

    let mut affected_files = Vec::new();

    for path in paths {
        let content = fs::read_to_string(path).map_err(|e| OutlinerError::Io(e.to_string()))?;
        let (updated, changed) = update_page_links_text(&content, old_name, new_name);
        if changed {
            let display_path = path.display().to_string();
            affected_files.push(display_path);
            if !preview {
                save_atomic(path, &updated)?;
            }
        }
    }

    let count = affected_files.len();
    Ok(RenameResult {
        old_name: old_name.to_string(),
        new_name: new_name.to_string(),
        affected_files,
        count,
        preview,
    })
}
