use pdf_edit_core::{delete_pages, page_count, reorder, rotate_pages, split};
use std::{env, path::PathBuf, process::ExitCode};

fn usage() { eprintln!("pdf-edit count|delete IN OUT PAGES|reorder IN OUT 3,1,2|rotate IN OUT DEGREES PAGES|split IN DIR 1-2,4-5"); }
fn pages(text: &str) -> Result<Vec<usize>, String> { text.split(',').map(|v| v.trim().parse().map_err(|_| format!("잘못된 페이지: {v}"))).collect() }
fn main() -> ExitCode {
    let a: Vec<String> = env::args().skip(1).collect(); if a.is_empty() { usage(); return ExitCode::from(2); }
    let result = match a[0].as_str() {
        "count" if a.len() == 2 => page_count(&a[1]).map(|n| println!("{n}")),
        "delete" if a.len() == 4 => pages(&a[3]).map_err(pdf_edit_core::PdfEditError::Invalid).and_then(|p| delete_pages(&a[1], &a[2], &p)),
        "reorder" if a.len() == 4 => pages(&a[3]).map_err(pdf_edit_core::PdfEditError::Invalid).and_then(|p| reorder(&a[1], &a[2], &p)),
        "rotate" if a.len() == 5 => a[3].parse::<i64>().map_err(|_| pdf_edit_core::PdfEditError::Invalid("각도가 잘못되었습니다".into())).and_then(|d| pages(&a[4]).map_err(pdf_edit_core::PdfEditError::Invalid).and_then(|p| rotate_pages(&a[1], &a[2], &p, d))),
        "split" if a.len() == 4 => {
            let ranges = a[3].split(',').map(|r| { let mut i = r.split('-'); let s: usize = i.next().unwrap_or("").trim().parse().map_err(|_| pdf_edit_core::PdfEditError::Invalid("분할 범위가 잘못되었습니다".into()))?; let e: usize = i.next().unwrap_or("").trim().parse().map_err(|_| pdf_edit_core::PdfEditError::Invalid("분할 범위가 잘못되었습니다".into()))?; Ok(s..=e) }).collect::<Result<Vec<_>, pdf_edit_core::PdfEditError>>();
            ranges.and_then(|r| split(&a[1], PathBuf::from(&a[2]), &r).map(|_| ()))
        }
        _ => { usage(); return ExitCode::from(2); }
    };
    match result { Ok(_) => ExitCode::SUCCESS, Err(e) => { eprintln!("pdf-edit: {e:?}"); ExitCode::from(1) } }
}
