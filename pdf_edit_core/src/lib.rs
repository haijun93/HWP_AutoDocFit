//! 경량 PDF 페이지 편집 코어.
//! 렌더링은 포함하지 않고 PDF 객체의 페이지 순서·삭제·회전·분할만 처리한다.

use lopdf::{Document, Object, ObjectId};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum PdfEditError { Io(std::io::Error), Pdf(lopdf::Error), Invalid(String) }
impl From<std::io::Error> for PdfEditError { fn from(e: std::io::Error) -> Self { Self::Io(e) } }
impl From<lopdf::Error> for PdfEditError { fn from(e: lopdf::Error) -> Self { Self::Pdf(e) } }
pub type Result<T> = std::result::Result<T, PdfEditError>;

pub fn page_count(path: impl AsRef<Path>) -> Result<usize> { Ok(Document::load(path)?.get_pages().len()) }

/// 1-based 페이지 번호를 남길 순서로 새 PDF를 저장한다. 중복·범위 오류는 거부한다.
pub fn reorder(input: impl AsRef<Path>, output: impl AsRef<Path>, order: &[usize]) -> Result<()> {
    if order.is_empty() { return Err(PdfEditError::Invalid("페이지 목록이 비어 있습니다".into())); }
    let mut doc = Document::load(input)?;
    let pages = doc.get_pages();
    let ids: Vec<ObjectId> = order.iter().map(|n| pages.get(&(*n as u32)).copied().ok_or_else(|| PdfEditError::Invalid(format!("없는 페이지: {n}")))).collect::<Result<_>>()?;
    let mut seen = std::collections::HashSet::new();
    if ids.iter().any(|id| !seen.insert(*id)) { return Err(PdfEditError::Invalid("페이지가 중복되었습니다".into())); }
    let pages_id = doc.catalog()?.get(b"Pages")?.as_reference()?;
    let kids = ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>();
    doc.get_object_mut(pages_id)?.as_dict_mut()?.set("Kids", Object::Array(kids));
    doc.get_object_mut(pages_id)?.as_dict_mut()?.set("Count", Object::Integer(ids.len() as i64));
    doc.save(output)?; Ok(())
}

pub fn delete_pages(input: impl AsRef<Path>, output: impl AsRef<Path>, remove: &[usize]) -> Result<()> {
    let total = page_count(&input)?;
    let remove: std::collections::HashSet<usize> = remove.iter().copied().collect();
    let keep: Vec<usize> = (1..=total).filter(|n| !remove.contains(n)).collect();
    if keep.is_empty() { return Err(PdfEditError::Invalid("모든 페이지를 삭제할 수 없습니다".into())); }
    reorder(input, output, &keep)
}

pub fn rotate_pages(input: impl AsRef<Path>, output: impl AsRef<Path>, pages: &[usize], degrees: i64) -> Result<()> {
    let mut doc = Document::load(input)?; let ids = doc.get_pages();
    let normalized = degrees.rem_euclid(360); if normalized % 90 != 0 { return Err(PdfEditError::Invalid("회전 각도는 90도 단위여야 합니다".into())); }
    for n in pages { let id = ids.get(&(*n as u32)).ok_or_else(|| PdfEditError::Invalid(format!("없는 페이지: {n}")))?; let page = doc.get_object_mut(*id)?.as_dict_mut()?; let old = page.get(b"Rotate").ok().and_then(|v| v.as_i64().ok()).unwrap_or(0); page.set("Rotate", Object::Integer((old + normalized).rem_euclid(360))); }
    doc.save(output)?; Ok(())
}

pub fn split(input: impl AsRef<Path>, output_dir: impl AsRef<Path>, ranges: &[std::ops::RangeInclusive<usize>]) -> Result<Vec<PathBuf>> {
    let total = page_count(&input)?; let dir = output_dir.as_ref(); std::fs::create_dir_all(dir)?; let mut out = Vec::new();
    for (i, range) in ranges.iter().enumerate() { let pages: Vec<usize> = range.clone().collect(); if pages.iter().any(|p| *p == 0 || *p > total) { return Err(PdfEditError::Invalid("분할 범위가 올바르지 않습니다".into())); } let path = dir.join(format!("part-{}.pdf", i + 1)); reorder(&input, &path, &pages)?; out.push(path); }
    Ok(out)
}
