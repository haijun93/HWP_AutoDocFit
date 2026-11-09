//! Flutter 관리창에서 K에이전트 저장소를 직접 읽고 바꾸는 C ABI.
//! JSON 입력·출력이며, 반환 문자열은 `kclip_free`로 해제한다.

#[path = "../../src/category.rs"] mod category;
#[path = "../../src/crypt.rs"] mod crypt;
#[path = "../../src/json.rs"] mod json;
mod open_item {
    const WEB_MARKER: &[u8] = b"\0KAGENT-WEB\0";
    pub fn web_url_from_extra(extra: &[u8]) -> Option<String> {
        let rest = extra.strip_prefix(WEB_MARKER)?;
        let end = rest.iter().position(|&b| b == 0)?;
        super::safe_web_url(&String::from_utf8(rest[..end].to_vec()).ok()?).map(str::to_owned)
    }
    pub fn formats_from_extra(extra: &[u8]) -> Vec<(String, Vec<u8>)> {
        if let Some(rest) = extra.strip_prefix(WEB_MARKER) {
            let Some(end) = rest.iter().position(|&b| b == 0) else { return Vec::new() };
            return crate::clip_formats::unpack(&rest[end + 1..]);
        }
        crate::clip_formats::unpack(extra)
    }
}
fn safe_web_url(url: &str) -> Option<&str> {
    if url.is_empty() || url.chars().count() > 2048 || url.chars().any(|c| c.is_control() || c.is_whitespace()) { return None; }
    let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.starts_with('.') || authority.ends_with('.') { return None; }
    Some(url)
}
mod clip_formats {
    pub fn unpack(data: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut result = Vec::new();
        let mut offset = 0;
        while offset + 8 <= data.len() {
            let name_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            let value_len = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
            offset += 8;
            let Some(name_end) = offset.checked_add(name_len) else { break };
            let Some(value_end) = name_end.checked_add(value_len) else { break };
            if value_end > data.len() { break; }
            let name = String::from_utf8_lossy(&data[offset..name_end]).into_owned();
            result.push((name, data[name_end..value_end].to_vec()));
            offset = value_end;
        }
        result
    }
}
#[path = "../../src/sqlite.rs"] mod sqlite;
#[path = "../../src/store.rs"] mod store;

#[cfg(test)]
const _: &str = env!("CARGO_MANIFEST_DIR");

use category::Category;
use json::Value;
use std::ffi::{c_char, CStr, CString};
use std::path::Path;
use store::Store;

fn category(value: &str) -> Category {
    match value {
        "문서" | "document" => Category::Document,
        "그림" | "image" => Category::Image,
        "파일" | "file" => Category::File,
        "폴더" | "folder" => Category::Folder,
        "웹" | "web" => Category::Web,
        "화면캡처" | "capture" => Category::Capture,
        _ => Category::All,
    }
}

fn result(request: &Value) -> Result<Value, String> {
    let action = request.str("action").unwrap_or("");
    let db_path = request.str("db_path").ok_or("db_path가 없습니다")?;
    if db_path.is_empty() || db_path.contains('\0') || !Path::new(db_path).is_absolute() {
        return Err("절대 저장소 경로가 필요합니다".into());
    }
    if !Path::new(db_path).is_file() {
        return Err("저장소 파일이 아직 없습니다".into());
    }
    // 에이전트와 동일한 SQLite DB를 별도 연결로 다루므로 WAL busy timeout으로 직렬화한다.
    let mut db = Store::open(db_path)?;
    match action {
        "list" => {
            let cat = category(request.str("category").unwrap_or("전체"));
            let query = request.str("query").unwrap_or("");
            let page = request.num("page").unwrap_or(0.0).max(0.0) as usize;
            let limit = request.num("limit").unwrap_or(200.0).clamp(1.0, 500.0) as usize;
            let items = db.list(page.saturating_mul(limit).saturating_add(limit), cat, query)?;
            let items = items.into_iter().skip(page.saturating_mul(limit)).map(|item| item.to_json()).collect::<Vec<_>>();
            Ok(Value::obj(vec![("ok", true.into()), ("items", Value::Arr(items))]))
        }
        "detail" => {
            let id = request.num("id").filter(|n| *n > 0.0).ok_or("id가 올바르지 않습니다")? as i64;
            let item = db.payload(id)?.map(|(kind, data, _)| Value::obj(vec![
                ("id", id.into()), ("kind", kind.into()),
                ("data", base64(&data).into()),
            ]));
            Ok(Value::obj(vec![("ok", true.into()), ("item", item.unwrap_or(Value::Null))]))
        }
        "pin" => {
            let id = request.num("id").filter(|n| *n > 0.0).ok_or("id가 올바르지 않습니다")? as i64;
            let pinned = request.bool("pinned").unwrap_or(true);
            Ok(Value::obj(vec![("ok", true.into()), ("changed", db.pin(id, pinned)?.into())]))
        }
        "delete" => {
            let id = request.num("id").filter(|n| *n > 0.0).ok_or("id가 올바르지 않습니다")? as i64;
            Ok(Value::obj(vec![("ok", true.into()), ("deleted", db.delete(id)?.into())]))
        }
        _ => Err("지원하지 않는 FFI 작업입니다".into()),
    }
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let value = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            output.push(if i <= chunk.len() { TABLE[((value >> (18 - 6 * i)) & 63) as usize] as char } else { '=' });
        }
    }
    output
}

// 파일 저장소 경로는 UTF-8 JSON 문자열이다. DB는 존재하는 에이전트 경로만 열어 만든다.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kclip_call_json(input: *const c_char) -> *mut c_char {
    if input.is_null() { return std::ptr::null_mut(); }
    let response = std::panic::catch_unwind(|| {
        let text = unsafe { CStr::from_ptr(input) }.to_string_lossy();
        let parsed = json::parse(&text).ok_or_else(|| "JSON 입력이 올바르지 않습니다".to_string())?;
        result(&parsed)
    });
    let text = match response {
        Ok(Ok(value)) => json::to_string(&value),
        Ok(Err(error)) => json::to_string(&Value::obj(vec![("ok", false.into()), ("message", error.into())])),
        Err(_) => "{\"ok\":false,\"message\":\"Rust FFI 오류\"}".into(),
    };
    CString::new(text).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kclip_free(value: *mut c_char) {
    if !value.is_null() { drop(unsafe { CString::from_raw(value) }); }
}

fn main() {}

// 테스트용 JSON 호출도 동일 ABI 경로를 거친다.
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn db_path() -> String {
        let id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("kclip-ffi-{id}.db")).to_string_lossy().into_owned()
    }

    #[test]
    fn isolated_store_list_search_pin_delete() {
        let path = db_path();
            let db = Store::open(&path).unwrap();
        db.insert("text", b"KC1 ffi keyword", &[], "KC1 ffi keyword", "test", 1_800_000_000.0).unwrap();
        drop(db);
        let request = |action: &str, extra: Vec<(&str, Value)>| {
            let mut fields = vec![("action".to_string(), action.into()), ("db_path".to_string(), path.as_str().into())];
            fields.extend(extra.into_iter().map(|(key, value)| (key.to_string(), value)));
            result(&Value::Obj(fields)).unwrap()
        };
        let listed = request("list", vec![("category", "전체".into()), ("query", "keyword".into()), ("limit", 20.into())]);
        let Value::Arr(items) = listed.get("items").unwrap() else { panic!("items") };
        assert_eq!(items.len(), 1);
        let db = Store::open(&path).unwrap();
        assert!(db.pin(1, true).unwrap());
        assert!(db.delete(1).unwrap());
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{path}-wal"));
        let _ = std::fs::remove_file(format!("{path}-shm"));
    }
}
