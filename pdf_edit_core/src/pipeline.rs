//! 인메모리 다중 편집 파이프라인.
//!
//! `lopdf::Document` 하나를 메모리에 열어 두고 회전 → 삭제 → 순서 변경 → 합치기를
//! 임시 파일 없이 연달아 적용한 뒤, 최종 결과를 한 번만 기록한다(`EditSession::save`).
//! 각 작업의 쪽 번호는 **직전 작업을 적용한 뒤의 현재 문서** 기준 1부터의 번호이다
//! (기존 Flutter 의 임시 파일 체이닝과 같은 의미).
use super::{
    delete_doc, inherited, invalid, load, page_info_of, rotate_doc, save, select, validate_doc,
    PageInfo, PdfEditError, Result,
};
use lopdf::{dictionary, Document, Object, ObjectId};
use std::{collections::BTreeMap, path::Path};

/// 파이프라인에 쌓는 편집 작업.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// 지정 쪽을 `degrees`(90의 배수, 음수 허용)만큼 시계 방향 누적 회전.
    Rotate { pages: Vec<usize>, degrees: i64 },
    /// 지정 쪽 삭제(전부 삭제는 오류).
    Delete { pages: Vec<usize> },
    /// 새 쪽 순서. 나열하지 않은 쪽은 제거된다(`reorder` CLI 와 같은 의미).
    Reorder { order: Vec<usize> },
}

/// 메모리에 열린 편집 문서.
pub struct EditSession {
    doc: Document,
}

impl EditSession {
    /// 파일에서 열기. 크기·암호화·서명·쪽수 규칙은 CLI 와 동일하다.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            doc: load(path.as_ref())?,
        })
    }

    /// 바이트에서 열기(같은 규칙).
    pub fn from_bytes(raw: &[u8]) -> Result<Self> {
        if raw.len() as u64 > 1024 * 1024 * 1024 {
            return Err(invalid("PDF exceeds 1 GB"));
        }
        super::check_header(raw)?;
        Ok(Self {
            doc: validate_doc(Document::load_mem(raw)?)?,
        })
    }

    pub fn page_count(&self) -> usize {
        self.doc.get_pages().len()
    }

    /// 작업 하나 적용. 검증에 실패하면 문서는 바뀌지 않는다.
    pub fn apply(&mut self, op: &EditOp) -> Result<()> {
        match op {
            EditOp::Rotate { pages, degrees } => rotate_doc(&mut self.doc, pages, *degrees),
            EditOp::Delete { pages } => delete_doc(&mut self.doc, pages).map(|_| ()),
            EditOp::Reorder { order } => select(&mut self.doc, order),
        }
    }

    /// 작업 여러 개를 순서대로 적용. 중간에 실패하면 거기서 멈추고 `(실패한 작업 번호, 오류)`를
    /// 돌려준다(앞서 적용된 작업은 유지된다).
    pub fn apply_all(&mut self, ops: &[EditOp]) -> std::result::Result<(), (usize, PdfEditError)> {
        for (i, op) in ops.iter().enumerate() {
            self.apply(op).map_err(|e| (i, e))?;
        }
        Ok(())
    }

    /// 현재 쪽 정보(번호·크기·유효 회전).
    pub fn page_info(&self) -> Result<Vec<PageInfo>> {
        page_info_of(&self.doc)
    }

    /// 결과를 새 PDF 경로에 한 번만 기록(덮어쓰기 금지, 원자적 게시).
    pub fn save(&mut self, output: impl AsRef<Path>) -> Result<()> {
        let count = self.page_count();
        save(&mut self.doc, output.as_ref(), count)
    }

    /// 결과를 메모리 바이트로 직렬화.
    pub fn to_bytes(&mut self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.doc.save_to(&mut buf)?;
        Ok(buf)
    }

    /// 여러 문서를 인자 순서대로 이어 붙인 새 세션. 각 원본의 상속 쪽 속성
    /// (Resources/MediaBox/CropBox/Rotate)은 쪽에 복사해 보존한다.
    pub fn merge(sessions: Vec<EditSession>) -> Result<Self> {
        if sessions.len() < 2 {
            return Err(invalid("Merge needs at least two PDFs"));
        }
        let mut merged = Document::with_version("1.5");
        let mut order: Vec<ObjectId> = Vec::new();
        for session in sessions {
            let mut doc = session.doc;
            let pages = doc.get_pages();
            for id in pages.values() {
                let attrs = [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"]
                    .iter()
                    .map(|key| inherited(&doc, *id, key).map(|v| (*key, v)))
                    .collect::<Result<Vec<_>>>()?;
                let page = doc.get_object_mut(*id)?.as_dict_mut()?;
                for (key, value) in attrs {
                    if let Some(value) = value {
                        page.set(key, value);
                    }
                }
            }
            let ids = doc.objects.keys().copied().collect::<Vec<_>>();
            let start = merged.max_id + 1;
            let mapping = ids
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, (start + i as u32, 0)))
                .collect::<BTreeMap<_, _>>();
            for id in pages.values() {
                order.push(mapping[id]);
            }
            for (id, mut object) in doc.objects {
                let new_id = mapping[&id];
                remap_object(&mut object, &mapping);
                merged.objects.insert(new_id, object);
            }
            merged.max_id = start + ids.len() as u32 - 1;
        }
        if order.len() > 3000 {
            return Err(invalid("PDF must contain 1..3000 pages"));
        }
        let pages_id = merged.new_object_id();
        for id in &order {
            merged
                .get_object_mut(*id)?
                .as_dict_mut()?
                .set("Parent", pages_id);
        }
        merged.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => order.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
                "Count" => order.len() as i64
            }),
        );
        let catalog_id = merged.new_object_id();
        merged.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {"Type" => "Catalog", "Pages" => pages_id}),
        );
        merged.trailer.set("Root", catalog_id);
        merged.prune_objects();
        Ok(Self { doc: merged })
    }
}

fn remap_object(object: &mut Object, mapping: &BTreeMap<ObjectId, ObjectId>) {
    match object {
        Object::Reference(id) => {
            if let Some(mapped) = mapping.get(id) {
                *id = *mapped;
            }
        }
        Object::Array(values) => values.iter_mut().for_each(|v| remap_object(v, mapping)),
        Object::Dictionary(dict) => {
            for (_, value) in dict.iter_mut() {
                remap_object(value, mapping);
            }
            if dict.get(b"Type").ok().and_then(|v| v.as_name().ok()) == Some(b"Page") {
                dict.set("Parent", (0, 0));
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter_mut() {
                remap_object(value, mapping);
            }
        }
        _ => {}
    }
}
