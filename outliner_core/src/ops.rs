use crate::{
    parse_serialize::{block_from_text, refresh_metadata},
    Block, Document, LineNode, OutlinerError,
};
use unicode_segmentation::UnicodeSegmentation;

fn find_block(document: &Document, id: u64) -> Result<usize, OutlinerError> {
    document
        .blocks
        .iter()
        .position(|block| block.id == id)
        .ok_or_else(|| OutlinerError::Invalid(format!("블록을 찾을 수 없습니다: {id}")))
}

fn next_id(document: &Document) -> u64 {
    document
        .blocks
        .iter()
        .map(|block| block.id)
        .max()
        .unwrap_or(0)
        + 1
}

fn normalize_lines(document: &mut Document) {
    for (index, block) in document.blocks.iter_mut().enumerate() {
        block.line_index = index;
    }
}

/// Applies one named operation to a block and leaves the document untouched on failure.
pub fn edit(
    document: &mut Document,
    operation: &str,
    id: u64,
    text: Option<&str>,
) -> Result<(), OutlinerError> {
    edit_with_offset(document, operation, id, text, None)
}

/// Applies an operation with an optional grapheme offset for cursor based splitting.
pub fn edit_with_offset(
    document: &mut Document,
    operation: &str,
    id: u64,
    text: Option<&str>,
    offset: Option<usize>,
) -> Result<(), OutlinerError> {
    let block_index = find_block(document, id)?;
    let sequence_index = document
        .sequence
        .iter()
        .position(|node| *node == LineNode::Block(id))
        .ok_or_else(|| OutlinerError::Invalid("블록 순서를 찾을 수 없습니다".into()))?;
    let indent = document.blocks[block_index].indent;
    match operation {
        "insert" => {
            let value = text.ok_or_else(|| OutlinerError::Invalid("text가 필요합니다".into()))?;
            let new_id = next_id(document);
            let block = block_from_text(value, new_id, indent);
            document.blocks.insert(block_index + 1, block);
            document
                .sequence
                .insert(sequence_index + 1, LineNode::Block(new_id));
            let ending = document.line_endings[sequence_index].clone();
            if ending.is_empty() {
                document.line_endings[sequence_index] = document.newline.clone();
            }
            document.line_endings.insert(sequence_index + 1, ending);
        }
        "delete" => {
            document.blocks.remove(block_index);
            document.sequence.remove(sequence_index);
            document.line_endings.remove(sequence_index);
        }
        "indent" => {
            if block_index == 0 {
                return Err(OutlinerError::Invalid(
                    "첫 블록은 들여쓸 수 없습니다".into(),
                ));
            }
            document.blocks[block_index].indent += 1;
        }
        "outdent" => {
            if indent == 0 {
                return Err(OutlinerError::Invalid("이미 최상위 블록입니다".into()));
            }
            document.blocks[block_index].indent -= 1;
        }
        "fold" => document.blocks[block_index].folded = true,
        "unfold" => document.blocks[block_index].folded = false,
        "up" | "down" => {
            let other = if operation == "up" {
                sequence_index
                    .checked_sub(1)
                    .ok_or_else(|| OutlinerError::Invalid("위로 이동할 블록이 없습니다".into()))?
            } else {
                sequence_index + 1
            };
            if other >= document.sequence.len()
                || !matches!(document.sequence[other], LineNode::Block(_))
            {
                return Err(OutlinerError::Invalid("이동할 인접 블록이 없습니다".into()));
            }
            let other_id = match document.sequence[other] {
                LineNode::Block(other_id) => other_id,
                _ => unreachable!(),
            };
            let other_block_index = find_block(document, other_id)?;
            document.sequence.swap(sequence_index, other);
            document.line_endings.swap(sequence_index, other);
            document.blocks.swap(block_index, other_block_index);
        }
        "split" => {
            let source = document.blocks[block_index].text.clone();
            let (left, right) = if let Some(offset) = offset {
                let boundaries: Vec<usize> = source
                    .grapheme_indices(true)
                    .map(|(byte_index, _)| byte_index)
                    .chain(std::iter::once(source.len()))
                    .collect();
                let byte_offset = boundaries[offset.min(boundaries.len() - 1)];
                (
                    source[..byte_offset].to_owned(),
                    source[byte_offset..].to_owned(),
                )
            } else {
                let separator =
                    text.ok_or_else(|| OutlinerError::Invalid("분할 문자열이 필요합니다".into()))?;
                let (left, right) = source
                    .split_once(separator)
                    .ok_or_else(|| OutlinerError::Invalid("분할 문자열이 없습니다".into()))?;
                (left.to_owned(), right.to_owned())
            };

            // The original id stays on the front block. Indent, folded, and TODO status are
            // copied to the new block; tags/properties/links are recalculated from each fragment.
            let mut right_block = document.blocks[block_index].clone();
            let new_id = next_id(document);
            document.blocks[block_index].text = left;
            refresh_metadata(&mut document.blocks[block_index]);
            right_block.id = new_id;
            right_block.text = right;
            refresh_metadata(&mut right_block);
            document.blocks.insert(block_index + 1, right_block);
            document
                .sequence
                .insert(sequence_index + 1, LineNode::Block(new_id));
            let ending = document.line_endings[sequence_index].clone();
            if ending.is_empty() {
                document.line_endings[sequence_index] = document.newline.clone();
            }
            document.line_endings.insert(sequence_index + 1, ending);
        }
        "merge" => {
            let next_node = document
                .sequence
                .get(sequence_index + 1)
                .ok_or_else(|| OutlinerError::Invalid("병합할 다음 블록이 없습니다".into()))?;
            let next_id = match next_node {
                LineNode::Block(next_id) => *next_id,
                LineNode::Raw(_) => {
                    return Err(OutlinerError::Invalid(
                        "보존 줄을 건너 병합할 수 없습니다".into(),
                    ))
                }
            };
            let next_index = find_block(document, next_id)?;
            let next = document.blocks.remove(next_index);
            if !document.blocks[block_index].text.is_empty() && !next.text.is_empty() {
                document.blocks[block_index].text.push(' ');
            }
            document.blocks[block_index].text.push_str(&next.text);
            refresh_metadata(&mut document.blocks[block_index]);
            document.sequence.remove(sequence_index + 1);
            document.line_endings.remove(sequence_index + 1);
        }
        "set_status" => {
            let status = text.unwrap_or("");
            if !["", "TODO", "DONE"].contains(&status) {
                return Err(OutlinerError::Invalid(
                    "status는 TODO, DONE 또는 빈 문자열이어야 합니다".into(),
                ));
            }
            document.blocks[block_index].status = if status.is_empty() {
                None
            } else {
                Some(status.into())
            };
        }
        "toggle_status" => {
            document.blocks[block_index].status =
                match document.blocks[block_index].status.as_deref() {
                    Some("TODO") => Some("DONE".into()),
                    Some("DONE") | None => Some("TODO".into()),
                    Some(_) => Some("TODO".into()),
                };
        }
        "set_text" => {
            document.blocks[block_index].text = text
                .ok_or_else(|| OutlinerError::Invalid("text가 필요합니다".into()))?
                .into();
            refresh_metadata(&mut document.blocks[block_index]);
        }
        _ => {
            return Err(OutlinerError::Invalid(format!(
                "지원하지 않는 작업: {operation}"
            )))
        }
    }
    normalize_lines(document);
    document.edited = true;
    Ok(())
}

/// Applies one operation to an ordered selection as a single atomic edit.
/// IDs are interpreted in document order; on failure the input document is unchanged.
pub fn edit_many(
    document: &mut Document,
    operation: &str,
    ids: &[u64],
    text: Option<&str>,
) -> Result<(), OutlinerError> {
    let mut indices: Vec<usize> = ids.iter().map(|id| find_block(document, *id)).collect::<Result<_, _>>()?;
    indices.sort_unstable();
    indices.dedup();
    if indices.is_empty() {
        return Err(OutlinerError::Invalid("선택한 블록이 없습니다".into()));
    }
    let mut candidate = document.clone();
    let mut selected: Vec<u64> = indices.iter().map(|i| candidate.blocks[*i].id).collect();
    match operation {
        "paste" => {
            let value = text.ok_or_else(|| OutlinerError::Invalid("text가 필요합니다".into()))?;
            let after = *indices.last().unwrap();
            let anchor_id = candidate.blocks[after].id;
            let anchor_indent = candidate.blocks[after].indent;
            let pasted = crate::parse(value)?;
            let base_indent = pasted.blocks.iter().map(|b| b.indent).min().unwrap_or(0);
            let mut insert_after = anchor_id;
            for block in pasted.blocks {
                edit(&mut candidate, "insert", insert_after, Some(&block.text))?;
                let inserted = find_block(&candidate, insert_after)? + 1;
                candidate.blocks[inserted].indent = anchor_indent + block.indent.saturating_sub(base_indent);
                candidate.blocks[inserted].status = block.status;
                candidate.blocks[inserted].folded = block.folded;
                insert_after = candidate.blocks[inserted].id;
            }
        }
        "up" | "down" => {
            if operation == "up" {
                for id in &selected {
                    let i = find_block(&candidate, *id)?;
                    if i > 0 && !selected.contains(&candidate.blocks[i - 1].id) {
                        edit(&mut candidate, "up", *id, None)?;
                    }
                }
            } else {
                selected.reverse();
                for id in &selected {
                    let i = find_block(&candidate, *id)?;
                    if i + 1 < candidate.blocks.len()
                        && !selected.contains(&candidate.blocks[i + 1].id)
                    {
                        edit(&mut candidate, "down", *id, None)?;
                    }
                }
            }
        }
        "delete" => {
            selected.reverse();
            for id in &selected {
                edit(&mut candidate, operation, *id, text)?;
            }
        }
        _ => {
            for id in &selected {
                edit(&mut candidate, operation, *id, text)?;
            }
        }
    }
    *document = candidate;
    Ok(())
}

#[allow(dead_code)]
fn _block_type_anchor(_: Block) {}
