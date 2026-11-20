//! 엔진 호출(`outliner_core::handle_request` 라이브러리 함수 직접 호출)과 파일 읽기·저장.
//!
//! 저장 충돌: 읽을 때의 파일 바이트 SHA-256 을 기억하고, 저장 직전에 파일을 다시 읽어 비교한다.
//! (엔진 main 에 `expected_digest` 가 아직 없어 크레이트 안에서 구현 — 엔진에 들어오면 교체 가능)
use crate::sha256::{Digest, sha256};
use outliner_core::Document;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// 엔진 요청 하나. 오류 봉투는 `코드: 메시지` 문자열로 돌려준다.
pub fn request(op: &str, args: Value, document: &str) -> Result<Value, String> {
    let resp = outliner_core::handle_request(json!({"id": 1, "op": op, "args": args, "document": document}));
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp["result"].clone())
    } else {
        Err(format!("{}: {}", resp["error"]["code"].as_str().unwrap_or("ERROR"), resp["error"]["message"].as_str().unwrap_or("")))
    }
}

/// 마크다운 → 문서(엔진 `parse`).
pub fn parse_doc(markdown: &str) -> Result<Document, String> {
    let v = request("parse", Value::Null, markdown)?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// 엔진 `edit` 로 한 동작을 적용하고, 결과 마크다운을 다시 파싱한 "정규 문서"를 돌려준다.
/// 다시 파싱하는 이유: 엔진은 요청마다 문서를 새로 파싱해 블록 번호를 1.. 로 새로 매기므로,
/// 화면 쪽 문서도 같은 번호 규칙으로 맞춰 두어야 다음 요청의 `block_id` 가 틀어지지 않는다.
pub fn edit_doc(doc: &Document, action: &str, block_id: u64, text: Option<&str>) -> Result<Document, String> {
    let mut args = json!({"action": action, "block_id": block_id});
    if let Some(t) = text {
        args["text"] = Value::String(t.to_string());
    }
    let v = request("edit", args, &outliner_core::serialize(doc))?;
    let markdown = v["markdown"].as_str().ok_or("엔진 응답에 markdown 이 없습니다")?;
    parse_doc(markdown)
}

/// 열어 둔 파일의 상태.
#[derive(Clone, Debug)]
pub struct FileInfo {
    pub path: PathBuf,
    /// 읽었거나 마지막으로 저장한 시점의 파일 바이트 SHA-256.
    pub sha: Digest,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    /// 파일이 읽은 뒤 다른 곳에서 바뀌었다 — 저장하지 않았다.
    Conflict(String),
    Other(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Conflict(m) | SaveError::Other(m) => f.write_str(m),
        }
    }
}

/// 파일을 한 번만 읽어(바이트 SHA-256 과 파싱이 같은 내용을 본다) 문서와 파일 정보를 돌려준다.
pub fn load_file(path: &Path) -> Result<(Document, String, FileInfo), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sha = sha256(&bytes);
    let text = String::from_utf8(bytes).map_err(|_| format!("{}: UTF-8 문서가 아닙니다", path.display()))?;
    let doc = parse_doc(&text)?;
    Ok((doc, text, FileInfo { path: path.to_path_buf(), sha }))
}

/// 저장 직전 SHA-256 비교 → 같을 때만 엔진 `save`(원자적 교체). 성공하면 기억한 SHA-256 을 새 내용으로 갱신.
pub fn save_checked(info: &mut FileInfo, markdown: &str) -> Result<(), SaveError> {
    match std::fs::read(&info.path) {
        Ok(now) => {
            if sha256(&now) != info.sha {
                return Err(SaveError::Conflict(format!(
                    "{} 이(가) 열어 둔 뒤 다른 곳에서 바뀌어 저장하지 않았습니다(SHA-256 불일치)",
                    info.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                )));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SaveError::Conflict("파일이 사라져 저장하지 않았습니다".into()));
        }
        Err(e) => return Err(SaveError::Other(format!("파일을 다시 읽지 못했습니다: {e}"))),
    }
    request("save", json!({"path": info.path.to_string_lossy(), "markdown": markdown}), "").map_err(SaveError::Other)?;
    info.sha = sha256(markdown.as_bytes());
    Ok(())
}
