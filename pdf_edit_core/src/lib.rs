//! Offline PDF page editing with no-clobber output publication.
use lopdf::{dictionary, Document, Object, ObjectId};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
#[derive(Debug)]
pub enum PdfEditError {
    Io(std::io::Error),
    Pdf(lopdf::Error),
    Invalid(String),
}
impl From<std::io::Error> for PdfEditError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<lopdf::Error> for PdfEditError {
    fn from(e: lopdf::Error) -> Self {
        Self::Pdf(e)
    }
}
pub type Result<T> = std::result::Result<T, PdfEditError>;
fn invalid(message: &str) -> PdfEditError {
    PdfEditError::Invalid(message.into())
}
fn load(path: &Path) -> Result<Document> {
    if path.metadata()?.len() > 1024 * 1024 * 1024 {
        return Err(invalid("PDF exceeds 1 GB"));
    }
    let doc = Document::load(path)?;
    if doc.is_encrypted() {
        return Err(invalid(
            "Encrypted PDFs require protection-preserving editing",
        ));
    }
    if doc.objects.values().any(|o| {
        o.as_dict().ok().is_some_and(|d| {
            [b"FT".as_slice(), b"Type".as_slice()]
                .iter()
                .any(|k| d.get(k).ok().and_then(|v| v.as_name().ok()) == Some(b"Sig"))
        })
    }) {
        return Err(invalid("Editing a signed PDF invalidates its signature"));
    }
    let n = doc.get_pages().len();
    if n == 0 || n > 3000 {
        return Err(invalid("PDF must contain 1..3000 pages"));
    }
    Ok(doc)
}
fn validate_output(path: &Path) -> Result<()> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_none_or(|e| !e.eq_ignore_ascii_case("pdf"))
        || path.exists()
    {
        return Err(invalid("Output must be a new PDF path"));
    }
    Ok(())
}
fn save(doc: &mut Document, output: &Path, count: usize) -> Result<()> {
    validate_output(output)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    doc.save_to(tmp.as_file_mut())?;
    tmp.as_file_mut().sync_all()?;
    if Document::load(tmp.path())?.get_pages().len() != count {
        return Err(invalid("Output page count mismatch"));
    }
    tmp.persist_noclobber(output)
        .map_err(|e| PdfEditError::Io(e.error))?;
    Ok(())
}
fn inherited(doc: &Document, id: ObjectId, key: &[u8]) -> Result<Option<Object>> {
    let mut current = id;
    let mut seen = HashSet::new();
    while seen.insert(current) {
        let dict = doc.get_object(current)?.as_dict()?;
        if let Ok(value) = dict.get(key) {
            return Ok(Some(value.clone()));
        }
        match dict.get(b"Parent").ok().and_then(|v| v.as_reference().ok()) {
            Some(p) => current = p,
            None => return Ok(None),
        }
    }
    Err(invalid("Cyclic PDF page tree"))
}
fn select(doc: &mut Document, order: &[usize]) -> Result<()> {
    if order.is_empty() {
        return Err(invalid("Page selection is empty"));
    }
    let pages = doc.get_pages();
    let mut seen = HashSet::new();
    let ids = order
        .iter()
        .map(|n| {
            if *n == 0 || *n > pages.len() || !seen.insert(*n) {
                return Err(invalid("Invalid or duplicate page"));
            }
            Ok(pages[&(*n as u32)])
        })
        .collect::<Result<Vec<_>>>()?;
    let root = doc.new_object_id();
    for id in &ids {
        let attrs = [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"]
            .iter()
            .map(|key| inherited(doc, *id, key).map(|v| (*key, v)))
            .collect::<Result<Vec<_>>>()?;
        let page = doc.get_object_mut(*id)?.as_dict_mut()?;
        for (key, value) in attrs {
            if let Some(value) = value {
                page.set(key, value);
            }
        }
        page.set("Parent", root);
    }
    doc.objects.insert(root, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(), "Count" => ids.len() as i64 }));
    let catalog = doc.trailer.get(b"Root")?.as_reference()?;
    doc.get_object_mut(catalog)?
        .as_dict_mut()?
        .set("Pages", root);
    doc.prune_objects();
    Ok(())
}
pub fn page_count(path: impl AsRef<Path>) -> Result<usize> {
    Ok(load(path.as_ref())?.get_pages().len())
}
pub fn reorder(input: impl AsRef<Path>, output: impl AsRef<Path>, order: &[usize]) -> Result<()> {
    validate_output(output.as_ref())?;
    let mut doc = load(input.as_ref())?;
    select(&mut doc, order)?;
    save(&mut doc, output.as_ref(), order.len())
}
pub fn delete_pages(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    remove: &[usize],
) -> Result<()> {
    validate_output(output.as_ref())?;
    let mut doc = load(input.as_ref())?;
    let n = doc.get_pages().len();
    if remove.is_empty() || remove.iter().any(|p| *p == 0 || *p > n) {
        return Err(invalid("Invalid deletion selection"));
    }
    let keep = (1..=n).filter(|p| !remove.contains(p)).collect::<Vec<_>>();
    select(&mut doc, &keep)?;
    save(&mut doc, output.as_ref(), keep.len())
}
pub fn rotate_pages(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    pages: &[usize],
    degrees: i64,
) -> Result<()> {
    validate_output(output.as_ref())?;
    let mut doc = load(input.as_ref())?;
    let ids = doc.get_pages();
    if pages.is_empty() || degrees % 90 != 0 {
        return Err(invalid("Select pages and a multiple of 90 degrees"));
    }
    let mut seen = HashSet::new();
    for n in pages {
        if *n == 0 || *n > ids.len() || !seen.insert(*n) {
            return Err(invalid("Invalid or duplicate page"));
        }
        let id = ids[&(*n as u32)];
        let old = inherited(&doc, id, b"Rotate")?
            .map(|v| v.as_i64())
            .transpose()?
            .unwrap_or(0);
        doc.get_object_mut(id)?.as_dict_mut()?.set(
            "Rotate",
            (old.rem_euclid(360) + degrees.rem_euclid(360)).rem_euclid(360),
        );
    }
    save(&mut doc, output.as_ref(), ids.len())
}
pub fn split(
    input: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    ranges: &[std::ops::RangeInclusive<usize>],
) -> Result<Vec<PathBuf>> {
    let doc = load(input.as_ref())?;
    let n = doc.get_pages().len();
    if ranges.is_empty()
        || ranges.len() > n
        || ranges
            .iter()
            .any(|r| r.is_empty() || *r.start() == 0 || *r.end() > n)
    {
        return Err(invalid("Invalid split ranges"));
    }
    let dir = output_dir.as_ref();
    let paths = (1..=ranges.len())
        .map(|i| dir.join(format!("part-{i}.pdf")))
        .collect::<Vec<_>>();
    for p in &paths {
        validate_output(p)?;
    }
    std::fs::create_dir_all(dir)?;
    for (range, path) in ranges.iter().zip(&paths) {
        let mut part = doc.clone();
        let order = range.clone().collect::<Vec<_>>();
        select(&mut part, &order)?;
        save(&mut part, path, order.len())?;
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    fn fixture(path: &Path, pages: usize) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let catalog_id = doc.new_object_id();
        let mut kids = Vec::new();
        for _ in 0..pages {
            let page_id = doc.new_object_id();
            doc.objects.insert(page_id, Object::Dictionary(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()]
            }));
            kids.push(page_id.into());
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => pages as i64 },
            ),
        );
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => pages_id }),
        );
        doc.trailer.set("Root", catalog_id);
        doc.max_id = doc.objects.len() as u32;
        doc.save(path).unwrap();
    }

    #[test]
    fn page_operations_preserve_requested_order_and_count() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        fixture(&input, 4);
        reorder(&input, &output, &[4, 2, 1]).unwrap();
        assert_eq!(page_count(&output).unwrap(), 3);
        let rotated = dir.path().join("rotated.pdf");
        rotate_pages(&output, &rotated, &[1, 3], 90).unwrap();
        assert_eq!(page_count(&rotated).unwrap(), 3);
    }

    #[test]
    fn delete_and_split_reject_empty_or_invalid_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        fixture(&input, 3);
        assert!(delete_pages(&input, dir.path().join("bad.pdf"), &[1, 2, 3]).is_err());
        assert!(split(&input, dir.path().join("parts"), &[2..=4]).is_err());
    }
}
