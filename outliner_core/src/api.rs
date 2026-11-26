use crate::{
    apply_key, backlinks, collect_markdown_files, daily_page, edit_many, edit_with_offset, journal_date, journal_list,
    journal_name, journal_relative, normalize_journal_name, parse, query, rename_page_in_files,
    resolve_block_ref, save_atomic_checked, search, serialize, update_page_links_text,
    KeyInput, OutlinerError, UndoHistory,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn history_from(args: &Value) -> Result<UndoHistory, OutlinerError> {
    match args.get("history") {
        None | Some(Value::Null) => Ok(UndoHistory::default()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|error| OutlinerError::Invalid(format!("args.history 형식 오류: {error}"))),
    }
}

fn history_value(history: &UndoHistory) -> Result<Value, OutlinerError> {
    serde_json::to_value(history).map_err(|error| OutlinerError::Invalid(error.to_string()))
}

fn invalid_request(id: Value, message: String) -> Value {
    json!({"id": id, "ok": false, "error": {"code": "INVALID_REQUEST", "message": message}})
}

/// Handles one JSON value and returns the standard success or coded error envelope.
pub fn handle_request(value: Value) -> Value {
    let request_id = value.get("id").cloned().unwrap_or(Value::Null);
    let Some(object) = value.as_object() else {
        return invalid_request(request_id, "요청은 JSON 객체여야 합니다".into());
    };
    let Some(operation) = object.get("op").and_then(Value::as_str) else {
        return invalid_request(request_id, "op 문자열이 필요합니다".into());
    };
    let args = object.get("args").cloned().unwrap_or(Value::Null);
    let path_arg = args.get("path").and_then(Value::as_str);
    let document_text = object.get("document").and_then(Value::as_str).unwrap_or("");
    let result: Result<Value, OutlinerError> = (|| match operation {
        "daily_page" => Ok(json!({"filename": daily_page(args["date"].as_str().unwrap_or(""))?})),
        "journal_name" => {
            let date = args["date"]
                .as_str()
                .or_else(|| args["name"].as_str())
                .unwrap_or("");
            if date.contains('_') || date.to_ascii_lowercase().ends_with(".md") {
                let name = normalize_journal_name(date)?;
                Ok(json!({"date":journal_date(date)? ,"name":name}))
            } else {
                let name = journal_name(date)?;
                Ok(
                    json!({"name":name,"yesterday":journal_name(&journal_relative(date,-1)?)?,"today":name,"tomorrow":journal_name(&journal_relative(date,1)?)?}),
                )
            }
        }
        "journal_list" => {
            let names: Vec<&str> = args["names"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            Ok(json!({"names":journal_list(names)}))
        }
        "query" => {
            let expression = args["query"].as_str().unwrap_or("");
            let pages: Vec<(String, crate::Document)> = if let Some(items) =
                args["pages"].as_array()
            {
                items
                    .iter()
                    .map(|item| {
                        let name = item["page"].as_str().ok_or_else(|| {
                            OutlinerError::Invalid("각 pages 항목에 page가 필요합니다".into())
                        })?;
                        let content = item["document"].as_str().ok_or_else(|| {
                            OutlinerError::Invalid("각 pages 항목에 document가 필요합니다".into())
                        })?;
                        Ok((name.to_owned(), parse(content)?))
                    })
                    .collect::<Result<_, OutlinerError>>()?
            } else {
                vec![(
                    args["page"].as_str().unwrap_or("<document>").to_owned(),
                    parse(document_text)?,
                )]
            };
            let refs: Vec<(&str, &crate::Document)> =
                pages.iter().map(|(n, d)| (n.as_str(), d)).collect();
            Ok(json!({"results":query(refs,expression)?}))
        }
        "load" => {
            let path =
                path_arg.ok_or_else(|| OutlinerError::Invalid("args.path가 필요합니다".into()))?;
            let content =
                fs::read_to_string(path).map_err(|error| OutlinerError::Io(error.to_string()))?;
            let document = parse(&content)?;
            Ok(json!({"document": document, "markdown": content}))
        }
        "save" => {
            let path =
                path_arg.ok_or_else(|| OutlinerError::Invalid("args.path가 필요합니다".into()))?;
            let content = args["markdown"].as_str().unwrap_or(document_text);
            save_atomic_checked(
                Path::new(path),
                content,
                args.get("expected_digest").and_then(Value::as_str),
            )?;
            Ok(json!({"path": path, "saved": true}))
        }
        "parse" => Ok(serde_json::to_value(parse(document_text)?)
            .map_err(|error| OutlinerError::Invalid(error.to_string()))?),
        "serialize" => Ok(json!({"markdown": serialize(&parse(document_text)?)})),
        "edit" => {
            let mut document = parse(document_text)?;
            let action = args["action"].as_str().unwrap_or("");
            let block_id = args["block_id"].as_u64().unwrap_or(0);
            let offset = args.get("offset").and_then(|value| {
                value.as_i64().map(|n| n.max(0) as usize).or_else(|| {
                    value
                        .as_u64()
                        .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
                })
            });
            if let Some(ids) = args.get("block_ids").and_then(Value::as_array) {
                let ids: Vec<u64> = ids.iter().filter_map(Value::as_u64).collect();
                if ids.len() != args["block_ids"].as_array().map_or(0, Vec::len) {
                    return Err(OutlinerError::Invalid("block_ids는 정수 배열이어야 합니다".into()));
                }
                edit_many(&mut document, action, &ids, args["text"].as_str())?;
            } else {
                edit_with_offset(&mut document, action, block_id, args["text"].as_str(), offset)?;
            }
            let mut result = json!({"markdown": serialize(&document), "document": document});
            // 선택 사항: args.history 가 있으면 이 편집 직전 문서를 되돌리기 단계로 기록해 돌려준다(없으면 구버전과 같다).
            if args.get("history").is_some_and(|value| !value.is_null()) {
                let mut history = history_from(&args)?;
                history.bind(args["page"].as_str(), args["version"].as_str());
                history.record(document_text);
                result["history"] = history_value(&history)?;
            }
            Ok(result)
        }
        "key" => {
            // 블록 편집 키 규칙(OUTLINEKEY1). args 는 keys::KeyInput.
            let input: KeyInput = serde_json::from_value(args.clone())
                .map_err(|error| OutlinerError::Invalid(format!("key 인자 형식 오류: {error}")))?;
            let out = apply_key(&parse(document_text)?, &input)?;
            Ok(json!({
                "markdown": out.markdown, "document": out.document,
                "handled": out.handled, "changed": out.changed, "cursor": out.cursor,
                "selection": out.selection, "folded": out.folded,
                "note": out.note, "clipboard": out.clipboard,
            }))
        }
        "history" => {
            let action = args["action"].as_str().unwrap_or("status");
            let mut history = history_from(&args)?;
            let cleared = history.bind(args["page"].as_str(), args["version"].as_str());
            let mut changed = false;
            let mut markdown = document_text.to_owned();
            match action {
                "push" => {
                    let before = args["before"].as_str().ok_or_else(|| {
                        OutlinerError::Invalid("args.before(편집 직전 마크다운)가 필요합니다".into())
                    })?;
                    history.record(before);
                }
                "undo" | "redo" => {
                    let step = if action == "undo" {
                        history.undo(document_text)
                    } else {
                        history.redo(document_text)
                    };
                    if let Some(text) = step {
                        markdown = text;
                        changed = true;
                    }
                }
                "clear" => history.clear(),
                "rebase" => history.rebase(args["version"].as_str()),
                "status" => {}
                other => {
                    return Err(OutlinerError::Invalid(format!("지원하지 않는 history 작업: {other}")))
                }
            }
            let mut result = json!({
                "history": history_value(&history)?,
                "can_undo": history.can_undo(),
                "can_redo": history.can_redo(),
                "undo_count": history.undo_len(),
                "redo_count": history.redo_len(),
                "cleared": cleared,
                "changed": changed,
                "markdown": markdown,
            });
            if changed {
                result["document"] = serde_json::to_value(parse(&markdown)?)
                    .map_err(|error| OutlinerError::Invalid(error.to_string()))?;
            }
            Ok(result)
        }
        "search" => Ok(
            json!({"block_ids": search(&parse(document_text)?, args["query"].as_str().unwrap_or(""))?}),
        ),
        "backlinks" => Ok(
            json!({"block_ids": backlinks(&parse(document_text)?, args["target"].as_str().unwrap_or(""))}),
        ),
        "resolve_block_ref" | "block_ref" => {
            let uuid = args
                .get("uuid")
                .or_else(|| args.get("id"))
                .or_else(|| args.get("block_id"))
                .or_else(|| args.get("target"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let document = if let Some(path) = path_arg {
                let content = fs::read_to_string(path)
                    .map_err(|error| OutlinerError::Io(error.to_string()))?;
                parse(&content)?
            } else {
                parse(document_text)?
            };
            let resolution = resolve_block_ref(&document, uuid);
            Ok(serde_json::to_value(&resolution)
                .map_err(|error| OutlinerError::Invalid(error.to_string()))?)
        }
        "insert_block_ref" => {
            let mut document = parse(document_text)?;
            let block_id = args["block_id"].as_u64().unwrap_or(0);
            let target = args
                .get("target")
                .or_else(|| args.get("uuid"))
                .or_else(|| args.get("id"))
                .and_then(Value::as_str)
                .ok_or_else(|| OutlinerError::Invalid("args.target 또는 uuid가 필요합니다".into()))?;
            let style = args["style"].as_str().unwrap_or("double");
            let ref_text = if style == "triple" {
                format!("((({target})))")
            } else {
                format!("(({target}))")
            };
            let block_index = document
                .blocks
                .iter()
                .position(|b| b.id == block_id)
                .ok_or_else(|| OutlinerError::Invalid(format!("블록을 찾을 수 없습니다: {block_id}")))?;
            if !document.blocks[block_index].text.is_empty() {
                document.blocks[block_index].text.push(' ');
            }
            document.blocks[block_index].text.push_str(&ref_text);
            crate::parse_serialize::refresh_metadata(&mut document.blocks[block_index]);
            document.edited = true;
            let mut result = json!({
                "markdown": serialize(&document),
                "document": document,
                "inserted": ref_text,
                "block_id": block_id
            });
            if args.get("history").is_some_and(|value| !value.is_null()) {
                let mut history = history_from(&args)?;
                history.bind(args["page"].as_str(), args["version"].as_str());
                history.record(document_text);
                result["history"] = history_value(&history)?;
            }
            Ok(result)
        }
        "block_backlinks" => {
            let target = args
                .get("target")
                .or_else(|| args.get("uuid"))
                .or_else(|| args.get("id"))
                .or_else(|| args.get("block_id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let document = if let Some(path) = path_arg {
                let content = fs::read_to_string(path)
                    .map_err(|error| OutlinerError::Io(error.to_string()))?;
                parse(&content)?
            } else {
                parse(document_text)?
            };
            let resolution = resolve_block_ref(&document, target);
            let referring_blocks: Vec<Value> = resolution.backlinks.iter().filter_map(|&ref_id| {
                document.blocks.iter().find(|b| b.id == ref_id).map(|b| {
                    json!({
                        "id": b.id,
                        "text": b.text,
                        "indent": b.indent,
                        "line_index": b.line_index,
                    })
                })
            }).collect();
            Ok(json!({
                "target": target,
                "found": resolution.found,
                "block_id": resolution.block_id,
                "text": resolution.text,
                "backlinks": resolution.backlinks,
                "referring_blocks": referring_blocks,
            }))
        }
        "check_page_references" | "page_references" => {
            let page_name = args
                .get("page")
                .or_else(|| args.get("name"))
                .and_then(Value::as_str)
                .ok_or_else(|| OutlinerError::Invalid("args.page가 필요합니다".into()))?;

            let paths = if let Some(files_array) = args.get("files").and_then(Value::as_array) {
                files_array
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(PathBuf::from)
                    .collect()
            } else if let Some(target_dir) = path_arg.or_else(|| args.get("directory").and_then(Value::as_str)) {
                collect_markdown_files(Path::new(target_dir))?
            } else {
                Vec::new()
            };

            let mut referring_files = Vec::new();
            let mut referring_blocks = Vec::new();

            for path in &paths {
                let Ok(content) = fs::read_to_string(path) else { continue };
                let Ok(doc) = parse(&content) else { continue };
                let file_display = path.display().to_string();
                let mut file_referred = false;
                for b in &doc.blocks {
                    if b.page_links.iter().any(|link| link.trim().eq_ignore_ascii_case(page_name.trim())) {
                        file_referred = true;
                        referring_blocks.push(json!({
                            "file": file_display,
                            "block_id": b.id,
                            "text": b.text,
                        }));
                    }
                }
                if file_referred {
                    referring_files.push(file_display);
                }
            }

            let count = referring_files.len();
            Ok(json!({
                "page": page_name,
                "has_references": count > 0,
                "count": count,
                "referring_files": referring_files,
                "referring_blocks": referring_blocks,
            }))
        }
        "rename_page" | "update_links" => {
            let old_name = args
                .get("old_name")
                .and_then(Value::as_str)
                .ok_or_else(|| OutlinerError::Invalid("args.old_name이 필요합니다".into()))?;
            let new_name = args
                .get("new_name")
                .and_then(Value::as_str)
                .ok_or_else(|| OutlinerError::Invalid("args.new_name이 필요합니다".into()))?;
            let preview = args
                .get("preview")
                .or_else(|| args.get("dry_run"))
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if let Some(files_array) = args.get("files").and_then(Value::as_array) {
                let paths: Vec<PathBuf> = files_array
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(PathBuf::from)
                    .collect();
                let res = rename_page_in_files(&paths, old_name, new_name, preview)?;
                return Ok(serde_json::to_value(&res)
                    .map_err(|error| OutlinerError::Invalid(error.to_string()))?);
            }

            let target_path = path_arg
                .or_else(|| args.get("directory").and_then(Value::as_str))
                .or_else(|| args.get("dir").and_then(Value::as_str));

            if let Some(target_path) = target_path {
                let path = Path::new(target_path);
                let files = collect_markdown_files(path)?;
                let res = rename_page_in_files(&files, old_name, new_name, preview)?;
                return Ok(serde_json::to_value(&res)
                    .map_err(|error| OutlinerError::Invalid(error.to_string()))?);
            }

            let (updated_doc, changed) = update_page_links_text(document_text, old_name, new_name);
            Ok(json!({
                "old_name": old_name,
                "new_name": new_name,
                "markdown": updated_doc,
                "changed": changed,
                "affected_files": if changed { vec!["<document>"] } else { vec![] },
                "files": if changed { vec!["<document>"] } else { vec![] },
                "count": if changed { 1 } else { 0 },
                "preview": preview,
            }))
        }
        _ => Err(OutlinerError::Invalid(format!(
            "지원하지 않는 명령: {operation}"
        ))),
    })();
    match result {
        Ok(result) => json!({"id": request_id, "ok": true, "result": result}),
        Err(error) => {
            json!({"id": request_id, "ok": false, "error": {"code": error.code(), "message": error.to_string()}})
        }
    }
}
