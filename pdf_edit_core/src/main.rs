use pdf_edit_core::{delete_pages, page_count, reorder, rotate_pages, split};
use std::{env, path::PathBuf, process::ExitCode};

fn usage() {
    eprintln!("pdf-edit count IN|delete IN OUT PAGES|reorder IN OUT 3,1,2|rotate IN OUT DEGREES PAGES|split IN DIR 1-2,4-5|merge OUT IN1 IN2...");
}
fn pages(text: &str) -> Result<Vec<usize>, String> {
    text.split(',')
        .map(|v| v.trim().parse().map_err(|_| format!("잘못된 페이지: {v}")))
        .collect()
}
fn remap_object(
    object: &mut lopdf::Object,
    mapping: &std::collections::BTreeMap<lopdf::ObjectId, lopdf::ObjectId>,
) {
    match object {
        lopdf::Object::Reference(id) => {
            if let Some(mapped) = mapping.get(id) {
                *id = *mapped;
            }
        }
        lopdf::Object::Array(values) => {
            for value in values {
                remap_object(value, mapping);
            }
        }
        lopdf::Object::Dictionary(dict) => {
            for (_, value) in dict.iter_mut() {
                remap_object(value, mapping);
            }
            if dict.get(b"Type").ok().and_then(|v| v.as_name().ok()) == Some(b"Page") {
                dict.set("Parent", (0, 0));
            }
        }
        lopdf::Object::Stream(stream) => {
            for (_, value) in stream.dict.iter_mut() {
                remap_object(value, mapping);
            }
        }
        _ => {}
    }
}
fn main() -> ExitCode {
    let a: Vec<String> = env::args().skip(1).collect();
    if a.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    let result = match a[0].as_str() {
        "count" if a.len() == 2 => page_count(&a[1]).map(|n| println!("{n}")),
        "delete" if a.len() == 4 => pages(&a[3])
            .map_err(pdf_edit_core::PdfEditError::Invalid)
            .and_then(|p| delete_pages(&a[1], &a[2], &p)),
        "reorder" if a.len() == 4 => pages(&a[3])
            .map_err(pdf_edit_core::PdfEditError::Invalid)
            .and_then(|p| reorder(&a[1], &a[2], &p)),
        "rotate" if a.len() == 5 => a[3]
            .parse::<i64>()
            .map_err(|_| pdf_edit_core::PdfEditError::Invalid("각도가 잘못되었습니다".into()))
            .and_then(|d| {
                pages(&a[4])
                    .map_err(pdf_edit_core::PdfEditError::Invalid)
                    .and_then(|p| rotate_pages(&a[1], &a[2], &p, d))
            }),
        "split" if a.len() == 4 => {
            let ranges = a[3]
                .split(',')
                .map(|r| {
                    let mut i = r.split('-');
                    let s: usize = i.next().unwrap_or("").trim().parse().map_err(|_| {
                        pdf_edit_core::PdfEditError::Invalid("분할 범위가 잘못되었습니다".into())
                    })?;
                    let e: usize = i.next().unwrap_or("").trim().parse().map_err(|_| {
                        pdf_edit_core::PdfEditError::Invalid("분할 범위가 잘못되었습니다".into())
                    })?;
                    Ok(s..=e)
                })
                .collect::<Result<Vec<_>, pdf_edit_core::PdfEditError>>();
            ranges.and_then(|r| split(&a[1], PathBuf::from(&a[2]), &r).map(|_| ()))
        }
        "merge" if a.len() >= 4 => {
            let mut merged = lopdf::Document::with_version("1.5");
            let mut order = Vec::new();
            for input in &a[2..] {
                let doc = lopdf::Document::load(input).map_err(pdf_edit_core::PdfEditError::Pdf)?;
                if doc.is_encrypted() {
                    return Err(pdf_edit_core::PdfEditError::Invalid(
                        "Encrypted PDFs are not supported".into(),
                    ));
                }
                let pages = doc.get_pages();
                let ids = doc.objects.keys().copied().collect::<Vec<_>>();
                let start = merged.max_id + 1;
                let mapping = ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (*id, (start + i as u32, 0)))
                    .collect::<std::collections::BTreeMap<_, _>>();
                for (page_no, id) in pages {
                    let _ = page_no;
                    order.push(mapping[&id]);
                }
                for (id, mut object) in doc.objects {
                    let new_id = mapping[&id];
                    remap_object(&mut object, &mapping);
                    merged.objects.insert(new_id, object);
                }
                merged.max_id = start + ids.len() as u32 - 1;
            }
            let pages_id = merged.new_object_id();
            for id in &order {
                merged
                    .get_object_mut(*id)?
                    .as_dict_mut()?
                    .set("Parent", pages_id);
            }
            merged.objects.insert(pages_id, lopdf::Object::Dictionary(lopdf::dictionary! {"Type" => "Pages", "Kids" => order.iter().map(|id| lopdf::Object::Reference(*id)).collect::<Vec<_>>(), "Count" => order.len() as i64}));
            let catalog_id = merged.new_object_id();
            merged.objects.insert(
                catalog_id,
                lopdf::Object::Dictionary(
                    lopdf::dictionary! {"Type" => "Catalog", "Pages" => pages_id},
                ),
            );
            merged.trailer.set("Root", catalog_id);
            let out = PathBuf::from(&a[1]);
            if out.exists() {
                return Err(pdf_edit_core::PdfEditError::Invalid(
                    "Output must be new".into(),
                ));
            }
            merged.save(&out).map_err(pdf_edit_core::PdfEditError::Pdf)
        }
        _ => {
            usage();
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pdf-edit: {e:?}");
            ExitCode::from(1)
        }
    }
}
