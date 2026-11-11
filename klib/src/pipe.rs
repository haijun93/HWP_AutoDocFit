//! Win32 Named Pipe 서버 및 한 줄 JSON 프로토콜 처리 모듈.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use std::sync::{Arc, Mutex, OnceLock};

use windows_sys::Win32::Foundation::{

    CloseHandle, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, HLOCAL,

    INVALID_HANDLE_VALUE, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,

};

use windows_sys::Win32::Security::Authorization::{

    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,

};

use windows_sys::Win32::Security::{

    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,

};

use windows_sys::Win32::Storage::FileSystem::{

    FILE_FLAG_OVERLAPPED, FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,

};

use windows_sys::Win32::System::IO::{CancelIo, GetOverlappedResult, OVERLAPPED};

use windows_sys::Win32::System::Pipes::{

    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,

    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,

};

use windows_sys::Win32::System::Threading::{

    CreateEventW, GetCurrentProcess, OpenProcessToken, WaitForMultipleObjects,

};

use crate::db::Database;

use crate::json::{self, Value};

use crate::util::{now_ms, wide};

const MAX_REQUEST: usize = 64 * 1024;

pub fn user_sid() -> String {

    static SID: OnceLock<String> = OnceLock::new();

    SID.get_or_init(|| unsafe {

        let mut token: HANDLE = std::ptr::null_mut();

        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {

            return String::new();

        }

        let mut buf = vec![0u8; 256];

        let mut len = 0u32;

        let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr() as _, buf.len() as u32, &mut len);

        CloseHandle(token);

        if ok == 0 {

            return String::new();

        }

        let user = &*(buf.as_ptr() as *const TOKEN_USER);

        let mut text: *mut u16 = std::ptr::null_mut();

        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {

            return String::new();

        }

        let mut n = 0;

        while *text.add(n) != 0 {

            n += 1;

        }

        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));

        LocalFree(text as HLOCAL);

        sid

    })

    .clone()

}

pub fn pipe_name() -> String {

    let suffix = crate::util::instance_suffix().unwrap_or_default();

    format!("\\\\.\\pipe\\HwpAutoDocFit.klib{suffix}.{}", user_sid())

}

/// 동시에 열어 두는 파이프 인스턴스 수(연결된 클라이언트 + 다음 연결을 기다리는 1개). CreateNamedPipeW의 최대 인스턴스 수와 같다.
const MAX_CONNS: usize = 4;

/// 한 요청(줄)이 다 오기까지 기다리는 한도. 넘기면 연결을 끊는다.
const LINE_TIMEOUT_MS: u64 = 30_000;

const READ_CHUNK: usize = 4096;

/// 받은 바이트에서 요청 한 줄을 꺼낸 결과.
#[derive(Debug, PartialEq)]
pub enum Line {
    /// 줄바꿈까지 온 요청 한 줄(줄바꿈 제외). 버퍼에서는 줄바꿈까지 지워진다.
    Ready(String),
    /// 아직 줄바꿈이 오지 않았다.
    More,
    /// 64 KiB 초과 또는 UTF-8이 아닌 요청: 응답 없이 연결을 끊는다.
    Reject,
}

/// 줄바꿈 전 요청 길이 기준 64 KiB 제한(지적 9 반영). 한 연결에서 여러 줄이 이어 올 수 있어 남은 바이트는 버퍼에 둔다.
pub fn take_line(data: &mut Vec<u8>) -> Line {
    match data.iter().position(|&b| b == b'\n') {
        Some(pos) if pos > MAX_REQUEST => Line::Reject,
        Some(pos) => {
            let line = String::from_utf8(data[..pos].to_vec());
            data.drain(..=pos);
            line.map(Line::Ready).unwrap_or(Line::Reject)
        }
        None if data.len() > MAX_REQUEST => Line::Reject,
        None => Line::More,
    }
}

/// 서버 루프가 연결마다 넘겨 쓰는 공용 상태. 요청은 이 스레드 하나에서 차례로만 처리한다.
struct Ctx<'a> {
    db: &'a Mutex<Database>,
    running: &'a AtomicBool,
    last_activity: &'a AtomicU64,
    /// 응답 쓰기용 이벤트(쓰기는 끝날 때까지 기다리므로 연결들이 함께 쓴다).
    write_event: HANDLE,
}

/// 파이프 인스턴스 하나. OVERLAPPED와 읽기 버퍼는 입출력이 걸려 있는 동안 주소가 바뀌면 안 되므로 Box에 둔다.
struct Conn {
    h: HANDLE,
    event: HANDLE,
    ov: Box<OVERLAPPED>,
    buf: Box<[u8; READ_CHUNK]>,
    data: Vec<u8>,
    /// false면 클라이언트 연결을 기다리는 중(ConnectNamedPipe).
    connected: bool,
    /// 끝나지 않은 겹친 입출력(연결 대기 또는 읽기)이 있다.
    pending: bool,
    /// 응답을 하나라도 썼다(종료 때 클라이언트가 읽을 때까지 비운다).
    replied: bool,
    /// 지금 기다리는 요청 줄의 시작 시각(연결 시각 또는 직전 응답 시각).
    line_start: u64,
}

impl Conn {
    /// 새 인스턴스를 만들고 연결 대기를 건다. 이미 붙은 클라이언트가 있으면 connected=true로 돌려준다.
    fn listen(name: &[u16], sa: &SECURITY_ATTRIBUTES) -> Option<Conn> {
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                MAX_CONNS as u32,
                65536,
                65536,
                0,
                sa,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return None;
        }
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            unsafe { CloseHandle(h) };
            return None;
        }
        let mut ov: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
        ov.hEvent = event;
        let mut conn = Conn { h, event, ov, buf: Box::new([0u8; READ_CHUNK]), data: Vec::new(), connected: false, pending: false, replied: false, line_start: now_ms() };
        if unsafe { ConnectNamedPipe(h, &mut *conn.ov) } != 0 {
            conn.connected = true;
        } else {
            match unsafe { GetLastError() } {
                ERROR_PIPE_CONNECTED => conn.connected = true,
                ERROR_IO_PENDING => conn.pending = true,
                _ => {
                    conn.close(false);
                    return None;
                }
            }
        }
        Some(conn)
    }

    /// 이벤트가 켜진 연결의 겹친 입출력 결과를 받아 처리한다. false면 연결을 닫아야 한다.
    fn service(&mut self, ctx: &Ctx) -> bool {
        let mut n = 0u32;
        if unsafe { GetOverlappedResult(self.h, &*self.ov, &mut n, 0) } == 0 {
            if unsafe { GetLastError() } == ERROR_IO_INCOMPLETE {
                return true;
            }
            self.pending = false;
            return false;
        }
        self.pending = false;
        if !self.connected {
            self.connected = true;
            self.line_start = now_ms();
            ctx.last_activity.store(self.line_start, Ordering::Relaxed);
            return self.read_more(ctx);
        }
        n != 0 && self.on_bytes(n as usize, ctx) && self.read_more(ctx)
    }

    /// 다음 읽기를 건다. 바로 끝난 읽기는 이어서 처리하고, 대기로 넘어가면 돌아온다(폴링 없음). false면 연결을 닫아야 한다.
    fn read_more(&mut self, ctx: &Ctx) -> bool {
        loop {
            if !ctx.running.load(Ordering::Relaxed) {
                return true;
            }
            if unsafe { ReadFile(self.h, self.buf.as_mut_ptr(), READ_CHUNK as u32, std::ptr::null_mut(), &mut *self.ov) } == 0 {
                if unsafe { GetLastError() } == ERROR_IO_PENDING {
                    self.pending = true;
                    return true;
                }
                return false;
            }
            let mut n = 0u32;
            if unsafe { GetOverlappedResult(self.h, &*self.ov, &mut n, 0) } == 0 || n == 0 {
                return false;
            }
            if !self.on_bytes(n as usize, ctx) {
                return false;
            }
        }
    }

    /// 방금 읽은 n바이트를 쌓고, 완성된 요청 줄을 차례로 처리해 응답한다. false면 연결을 닫아야 한다.
    fn on_bytes(&mut self, n: usize, ctx: &Ctx) -> bool {
        // 활동 시각 갱신 (지적 8 반영)
        ctx.last_activity.store(now_ms(), Ordering::Relaxed);
        self.data.extend_from_slice(&self.buf[..n]);
        while ctx.running.load(Ordering::Relaxed) {
            match take_line(&mut self.data) {
                Line::Ready(req_line) => {
                    let resp = handle_request(&req_line, ctx.db, ctx.running);
                    let mut out = resp.into_bytes();
                    out.push(b'\n');
                    self.replied = true;
                    let written = write_all(self.h, ctx.write_event, &out);
                    self.line_start = now_ms();
                    ctx.last_activity.store(self.line_start, Ordering::Relaxed);
                    if !written {
                        return false;
                    }
                }
                Line::More => break,
                Line::Reject => return false,
            }
        }
        true
    }

    /// 걸린 입출력을 취소하고 핸들을 닫는다. flush면 보낸 응답을 클라이언트가 읽을 때까지 기다린 뒤 끊는다(서버 종료 때).
    fn close(self, flush: bool) {
        unsafe {
            if self.pending {
                CancelIo(self.h);
                let mut n = 0u32;
                GetOverlappedResult(self.h, &*self.ov, &mut n, 1);
            }
            if flush && self.replied {
                FlushFileBuffers(self.h);
            }
            if self.connected {
                DisconnectNamedPipe(self.h);
            }
            CloseHandle(self.event);
            CloseHandle(self.h);
        }
    }
}

pub fn serve(db: Arc<Mutex<Database>>, running: Arc<AtomicBool>, last_activity: Arc<AtomicU64>) {
    serve_on(&pipe_name(), &db, &running, &last_activity);
}

/// 이름을 받아 파이프 서버를 돈다(시험은 사용자 파이프와 겹치지 않는 이름으로 직접 띄운다).
/// 스레드 하나가 여러 연결의 이벤트를 함께 기다린다: 요청은 한 번에 하나씩만 처리하므로 상태 경합이 없고,
/// 긴 요청 중에도 대기 인스턴스가 있어 다음 클라이언트가 연결해 차례를 기다릴 수 있다.
pub fn serve_on(name: &str, db: &Mutex<Database>, running: &AtomicBool, last_activity: &AtomicU64) {
    let sid = user_sid();
    if sid.is_empty() {
        return;
    }
    let sddl = wide(&format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)"));
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) } == 0 {
        return;
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let name = wide(name);
    let write_event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    let ctx = Ctx { db, running, last_activity, write_event };
    let mut conns: Vec<Conn> = Vec::new();
    while !write_event.is_null() && running.load(Ordering::Relaxed) {
        // 연결을 기다리는 인스턴스를 항상 하나 둔다(자리가 있을 때).
        if conns.len() < MAX_CONNS {
            match Conn::listen(&name, &sa) {
                Some(mut conn) => {
                    if conn.connected {
                        last_activity.store(now_ms(), Ordering::Relaxed);
                        if !conn.read_more(&ctx) {
                            conn.close(false);
                            continue;
                        }
                    }
                    conns.push(conn);
                    continue;
                }
                None if conns.is_empty() => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                None => {}
            }
        }
        // 종료 신호(running == false)와 읽기 시간 초과는 200ms 주기로 점검한다 (지적 7 반영)
        let events: Vec<HANDLE> = conns.iter().map(|c| c.event).collect();
        let waited = unsafe { WaitForMultipleObjects(events.len() as u32, events.as_ptr(), 0, 200) };
        let index = waited.wrapping_sub(WAIT_OBJECT_0) as usize;
        if index < conns.len() {
            // 처리한 연결은 맨 뒤로 보내 다른 연결이 굶지 않게 한다.
            let mut conn = conns.remove(index);
            if conn.service(&ctx) {
                conns.push(conn);
            } else {
                conn.close(false);
            }
        } else if waited == WAIT_TIMEOUT {
            // 읽기 타임아웃 (30초 동안 요청 한 줄이 완성되지 않으면 연결 해제).
            // 기다리는 이벤트가 하나도 없을 때만 본다: 긴 요청 뒤에 이미 도착해 있는 요청을 끊지 않기 위해서다.
            let now = now_ms();
            let mut i = 0;
            while i < conns.len() {
                if conns[i].connected && now.saturating_sub(conns[i].line_start) > LINE_TIMEOUT_MS {
                    conns.remove(i).close(false);
                } else {
                    i += 1;
                }
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    for conn in conns {
        conn.close(true);
    }
    unsafe {
        if !write_event.is_null() {
            CloseHandle(write_event);
        }
        LocalFree(sd as HLOCAL);
    }
}

/// OVERLAPPED로 연 핸들에 응답을 끝까지 쓴다. 클라이언트가 읽지 않아 밀리면 끝날 때까지 기다린다. 실패하면 false.
fn write_all(h: HANDLE, event: HANDLE, mut data: &[u8]) -> bool {
    while !data.is_empty() {
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = event;
        let chunk = data.len().min(1 << 20) as u32;
        if unsafe { WriteFile(h, data.as_ptr(), chunk, std::ptr::null_mut(), &mut ov) } == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
            return false;
        }
        let mut n = 0u32;
        if unsafe { GetOverlappedResult(h, &ov, &mut n, 1) } == 0 || n == 0 {
            return false;
        }
        data = &data[n as usize..];
    }
    true
}

/// 노트북 출처 문단을 JSON 행에서 만든다(Python 추출 경로). text·level·table(또는 kind="table")·location(또는 page).
fn nb_paragraphs_from_json(rows: &[Value]) -> Result<Vec<crate::notebook::Paragraph>, String> {
    let mut out = Vec::new();
    for row in rows {
        let Some(text) = row.str("text").or_else(|| row.str("body")) else { return Err("문단마다 text가 필요합니다.".into()) };
        let text = text.trim();
        if text.is_empty() { continue; }
        let table = row.bool("table").unwrap_or(false) || row.str("kind") == Some("table");
        let location = row.str("location").or_else(|| row.str("page")).unwrap_or("").to_string();
        out.push(crate::notebook::Paragraph { source_id: 0, ordinal: out.len() as i64, location, level: row.i64("level").unwrap_or(0).clamp(0, 9), kind: if table { "table" } else { "text" }.into(), body: text.to_string() });
    }
    Ok(out)
}

/// 출처 추가: 기본은 klib가 직접 추출한다(txt·md·hwpx·docx·pptx·xlsx, hwpx·docx는 개요 수준·표 행 포함).
/// 요청에 paragraphs가 있으면(구형 hwp·pdf를 Python이 추출한 경우) 그 문단을 첫 묶음으로 쓴다. 나머지 묶음은 nb_index로 덧붙인다.
fn nb_source_add(val: &Value, db: &Mutex<Database>) -> Value {
    let Some(nb) = val.i64("notebook_id") else { return err_val("notebook_id가 필요합니다.") };
    let Some(source_path) = val.str("source_path").or_else(|| val.str("origin")) else { return err_val("source_path가 필요합니다.") };
    let path = std::path::Path::new(source_path);
    if crate::util::is_or_has_reparse_point(path) { return err_val("심볼릭 링크나 정션 경로는 출처로 추가할 수 없습니다."); }
    if !path.is_file() { return err_val("출처 파일을 찾을 수 없습니다."); }
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let paragraphs = match val.arr("paragraphs") {
        Some(rows) => {
            if !matches!(ext.as_str(), "txt" | "md" | "hwpx" | "hwp" | "docx" | "pdf" | "pptx" | "xlsx") { return err_val(&format!("지원하지 않는 출처 형식입니다: {ext}")); }
            match nb_paragraphs_from_json(rows) { Ok(p) => p, Err(e) => return err_val(&e) }
        }
        None => {
            if !matches!(ext.as_str(), "txt" | "md" | "hwpx" | "docx" | "pptx" | "xlsx") { return err_val(&format!("지원하지 않는 출처 형식입니다: {ext}")); }
            match crate::content::extract_blocks(path, &ext) {
                Ok(blocks) => blocks.into_iter().enumerate().map(|(i, b)| crate::notebook::Paragraph { source_id: 0, ordinal: i as i64, location: String::new(), level: b.level, kind: if b.table { "table" } else { "text" }.into(), body: b.text }).collect(),
                Err(e) => return err_val(&e),
            }
        }
    };
    if paragraphs.is_empty() { return err_val("출처에서 본문 문단을 찾지 못했습니다."); }
    let metadata = match std::fs::metadata(path) { Ok(m) => m, Err(e) => return err_val(&e.to_string()) };
    let mtime = metadata.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
    let display = val.str("display_name").unwrap_or_else(|| path.file_name().and_then(|s| s.to_str()).unwrap_or(source_path));
    let source = {
        let db_guard = db.lock().unwrap();
        let c = db_guard.connection();
        let id = match crate::notebook::add_source(c, nb, &ext, source_path, display) { Ok(id) => id, Err(e) => return err_val(&e.to_string()) };
        if let Err(e) = crate::notebook::replace_paragraphs(c, id, &paragraphs) { return err_val(&e.to_string()); }
        let _ = c.execute("UPDATE nb_source SET original_size=?1,original_mtime=?2,updated_at=?3 WHERE id=?4", rusqlite::params![metadata.len() as i64, mtime, crate::util::now() as i64, id]);
        match crate::notebook::source_by_id(c, id) { Ok(Some(s)) => s, Ok(None) => return err_val("등록한 출처를 찾을 수 없습니다."), Err(e) => return err_val(&e.to_string()) }
    };
    match crate::search_index().and_then(|ix| ix.replace_notebook_source(nb, &source, &paragraphs).map_err(|e| e.to_string())) {
        Ok(()) => ok_val(vec![("id", source.id.into()), ("paragraph_count", (paragraphs.len() as i64).into())]),
        Err(e) => err_val(&e),
    }
}

// 경로가 실제 바뀐 최초 이전·영역 이동 때만 기존 본문 색인을 교체한다.
fn sync_item_index(old: Option<&str>, it: &crate::db::ItemRow) -> Option<String> {
    if old == Some(it.source_path.as_str()) || !matches!(it.kind.as_str(), "file" | "text" | "rich_text" | "note") { return None; }
    (|| {
        let ix = crate::search_index()?;
        // 옛 경로 삭제와 새 경로 색인을 한 번에 커밋한다(오류로 빠져나가도 묶음이 버려지며 커밋).
        let batch = ix.batch();
        if let Some(old) = old { crate::content::remove_file(ix, old)?; }
        crate::content::index_file(ix, &it.source_path, &it.display_name, it.box_id)?;
        batch.finish().map_err(|e| e.to_string())
    })().err()
}

fn prepare_library_item(db: &Database, id: i64) -> Result<crate::db::ItemRow, String> {
    let old = db.item_get(id).map_err(|e| e.to_string())?.ok_or("해당 자료 항목을 찾을 수 없습니다.")?;
    let it = db.item_prepare(id)?;
    let _ = sync_item_index(Some(&old.source_path), &it);
    Ok(it)
}

/// 자료함 항목을 노트북 출처로 추가한다. 같은 항목이 이미 출처면 새로 만들지 않고 그 출처를 돌려준다(reused:true).
/// klib가 못 읽는 구형 hwp·pdf는 요청의 paragraphs(Python 추출)가 필요하다.
fn nb_source_from_item(val: &Value, db: &Mutex<Database>) -> Value {
    let Some(nb) = val.i64("notebook_id") else { return err_val("notebook_id가 필요합니다.") };
    let Some(item_id) = val.i64("item_id") else { return err_val("item_id가 필요합니다.") };
    let item = {
        let g = db.lock().unwrap();
        if !g.connection().query_row("SELECT EXISTS(SELECT 1 FROM notebook WHERE id=?1)", [nb], |r| r.get::<_, bool>(0)).unwrap_or(false) { return err_val("노트북을 찾을 수 없습니다."); }
        match g.item_get(item_id).map_err(|e| e.to_string()).and_then(|row| row.map(|_| prepare_library_item(&g, item_id)).transpose()) { Ok(Some(it)) => it, Ok(None) => return err_val("해당 자료 항목을 찾을 수 없습니다."), Err(e) => return err_val(&e.to_string()) }
    };
    {
        let g = db.lock().unwrap();
        match crate::notebook::source_for_item(g.connection(), nb, item_id) {
            Ok(Some(id)) => {
                let count = crate::notebook::source_by_id(g.connection(), id).ok().flatten().map(|s| s.paragraph_count).unwrap_or(0);
                return ok_val(vec![("id", id.into()), ("paragraph_count", count.into()), ("reused", true.into())]);
            }
            Ok(None) => {}
            Err(e) => return err_val(&e.to_string()),
        }
    }
    if item.source_path.is_empty() { return err_val("이 자료 항목에는 파일 경로가 없어 출처로 추가할 수 없습니다."); }
    let mut fields = vec![("cmd", Value::from("nb_source_add")), ("notebook_id", nb.into()), ("source_path", item.source_path.clone().into()), ("display_name", val.str("display_name").unwrap_or(&item.display_name).to_string().into())];
    if let Some(rows) = val.arr("paragraphs") { fields.push(("paragraphs", Value::Arr(rows.to_vec()))); }
    let added = nb_source_add(&Value::obj(fields), db);
    if added.bool("ok") != Some(true) { return added; }
    let Some(id) = added.i64("id") else { return added };
    {
        let g = db.lock().unwrap();
        if let Err(e) = crate::notebook::set_source_item(g.connection(), id, item_id) { return err_val(&e.to_string()); }
    }
    ok_val(vec![("id", id.into()), ("paragraph_count", added.i64("paragraph_count").unwrap_or(0).into()), ("reused", false.into())])
}

/// 노트북 출처 → 원본 자료함 항목(자료함에서 보기). 항목이 지워졌거나 연결이 없으면 found:false.
fn nb_source_item(val: &Value, db: &Mutex<Database>) -> Value {
    let Some(nb) = val.i64("notebook_id") else { return err_val("notebook_id가 필요합니다.") };
    let Some(sid) = val.i64("source_id") else { return err_val("source_id가 필요합니다.") };
    let g = db.lock().unwrap();
    let linked = match crate::notebook::source_item_id(g.connection(), nb, sid) { Ok(Some(l)) => l, Ok(None) => return err_val("출처를 찾을 수 없습니다."), Err(e) => return err_val(&e.to_string()) };
    let Some(item_id) = linked else { return ok_val(vec![("found", false.into()), ("reason", "자료함 항목과 연결되지 않은 출처입니다.".into())]) };
    match g.item_get(item_id).map_err(|e| e.to_string()).and_then(|row| row.map(|_| prepare_library_item(&g, item_id)).transpose()) {
        Ok(Some(it)) => ok_val(vec![("found", true.into()), ("item_id", it.id.into()), ("box_id", it.box_id.into()), ("display_name", it.display_name.into()), ("source_path", it.source_path.into())]),
        Ok(None) => ok_val(vec![("found", false.into()), ("item_id", item_id.into()), ("reason", "자료함 항목이 삭제되었습니다.".into())]),
        Err(e) => err_val(&e.to_string()),
    }
}

/// 출처 다시 색인. paragraphs가 있으면 먼저 그 문단을 끝에 덧붙인다(64 KiB 요청 한도 때문에 나눠 보낸 묶음).
fn nb_index(val: &Value, db: &Mutex<Database>) -> Value {
    let Some(id) = val.i64("source_id") else { return err_val("source_id가 필요합니다.") };
    let Some(nb) = val.i64("notebook_id") else { return err_val("notebook_id가 필요합니다.") };
    let extra = match val.arr("paragraphs").map(nb_paragraphs_from_json) { Some(Ok(p)) => Some(p), Some(Err(e)) => return err_val(&e), None => None };
    let data = {
        let db_guard = db.lock().unwrap();
        let c = db_guard.connection();
        crate::notebook::source_by_id(c, id).map_err(|e| e.to_string())
            .and_then(|s| s.ok_or_else(|| "출처를 찾을 수 없습니다.".to_string()))
            .and_then(|s| {
                if s.notebook_id != nb { return Err("출처가 해당 노트북에 속하지 않습니다.".to_string()); }
                if let Some(extra) = &extra { crate::notebook::append_paragraphs(c, id, extra).map_err(|e| e.to_string())?; }
                let ps = crate::notebook::paragraphs(c, id).map_err(|e| e.to_string())?;
                let s = crate::notebook::source_by_id(c, id).map_err(|e| e.to_string())?.unwrap_or(s);
                Ok((s, ps))
            })
    };
    match data.and_then(|(s, ps)| crate::search_index()?.replace_notebook_source(nb, &s, &ps).map_err(|e| e.to_string()).map(|()| ps.len())) {
        Ok(count) => ok_val(vec![("indexed_paragraphs", (count as i64).into()), ("paragraph_count", (count as i64).into()), ("status", "indexed".into())]),
        Err(e) => err_val(&e),
    }
}

/// 정수 배열 값(source_ids·tags 등)을 읽는다. 키가 없으면 None.
fn nb_id_list(val: &Value, key: &str) -> Result<Option<Vec<i64>>, String> {
    let Some(rows) = val.arr(key) else { return Ok(None) };
    let mut ids = Vec::new();
    for v in rows {
        match v { Value::Num(n) if n.fract() == 0.0 => ids.push(*n as i64), _ => return Err(format!("{key}는 정수 배열이어야 합니다.")) }
    }
    Ok(Some(ids))
}

/// 문자열 배열 값. 키가 없으면 None(태그를 바꾸지 않음).
fn nb_str_list(val: &Value, key: &str) -> Result<Option<Vec<String>>, String> {
    let Some(rows) = val.arr(key) else { return Ok(None) };
    rows.iter().map(|v| match v { Value::Str(s) => Ok(s.clone()), _ => Err(format!("{key}는 문자열 배열이어야 합니다.")) }).collect::<Result<Vec<_>, _>>().map(Some)
}

fn nb_note_save(val: &Value, db: &Mutex<Database>) -> Value {
    let Some(nb) = val.i64("notebook_id") else { return err_val("notebook_id가 필요합니다.") };
    let mut citations = Vec::new();
    for row in val.arr("citations").unwrap_or(&[]) {
        let (Some(s), Some(o), Some(q)) = (row.i64("source_id"), row.i64("paragraph_ordinal"), row.str("quote")) else { return err_val("인용에는 source_id, paragraph_ordinal, quote가 필요합니다.") };
        citations.push((s, o, q.to_string()));
    }
    let tags = match nb_str_list(val, "tags") { Ok(t) => t, Err(e) => return err_val(&e) };
    let db_guard = db.lock().unwrap();
    match crate::notebook::save_note_with_citations(db_guard.connection(), val.i64("id"), nb, val.str("title").unwrap_or(""), val.str("body").unwrap_or(""), &citations, tags.as_deref()) {
        Ok(note) => match crate::notebook::note_tags(db_guard.connection(), note) {
            Ok(tags) => ok_val(vec![("id", note.into()), ("tags", Value::Arr(tags.into_iter().map(Into::into).collect()))]),
            Err(e) => err_val(&e.to_string()),
        },
        Err(e) => err_val(&e.to_string()),
    }
}

/// 노트 목록. tag가 있으면 그 태그가 달린 노트만 낸다. 노트북 전체 태그(노트 수)도 함께 낸다.
fn nb_note_list(val: &Value, db: &Mutex<Database>) -> Value {
    let Some(nb) = val.i64("notebook_id") else { return err_val("notebook_id가 필요합니다.") };
    let db_guard = db.lock().unwrap();
    let c = db_guard.connection();
    let result = (|| -> rusqlite::Result<Value> {
        let mut notes = Vec::new();
        for (id, title, body, updated_at) in crate::notebook::notes(c, nb, val.str("tag"))? {
            let citations = crate::notebook::citations(c, id)?.into_iter().map(|(source_id, paragraph_ordinal, citation_id, quote)| Value::obj(vec![("id", citation_id.into()), ("source_id", source_id.into()), ("paragraph_ordinal", paragraph_ordinal.into()), ("quote", quote.into())])).collect();
            let tags = crate::notebook::note_tags(c, id)?.into_iter().map(Into::into).collect();
            notes.push(Value::obj(vec![("id", id.into()), ("title", title.into()), ("body", body.into()), ("updated_at", updated_at.into()), ("tags", Value::Arr(tags)), ("citations", Value::Arr(citations))]));
        }
        let tags = crate::notebook::notebook_tags(c, nb)?.into_iter().map(|(tag, count)| Value::obj(vec![("tag", tag.into()), ("count", count.into())])).collect();
        Ok(ok_val(vec![("notes", Value::Arr(notes)), ("tags", Value::Arr(tags))]))
    })();
    result.unwrap_or_else(|e| err_val(&e.to_string()))
}

/// 같은 검색어로 출처마다 근거를 찾아 출처별로 묶는다(nb_compare, 내보내기의 출처 비교 절).
fn nb_compare_groups(db: &Mutex<Database>, nb: i64, query: &str, scope: Option<&[i64]>, per_source: usize) -> Result<(Vec<String>, Vec<crate::notebook::CompareGroup>), String> {
    let terms = crate::nb_transform::query_terms(query);
    let search_query = if terms.is_empty() { query.to_string() } else { terms.join(" ") };
    let sources = { let db_guard = db.lock().unwrap(); crate::notebook::sources(db_guard.connection(), nb).map_err(|e| e.to_string())? };
    let ix = crate::search_index()?;
    let groups = crate::notebook::compare_sources(&sources, scope, per_source, |id, limit| ix.search_notebook(nb, &search_query, Some(&[id]), limit).map_err(|e| e.to_string()))?;
    Ok((terms, groups))
}

fn nb_compare_args(val: &Value, query_key: &str, ids_key: &str) -> Result<Option<(String, Option<Vec<i64>>, usize)>, String> {
    let query = val.str(query_key).unwrap_or("").trim().to_string();
    if query.is_empty() { return Ok(None); }
    let ids = nb_id_list(val, ids_key)?;
    let per_source = val.i64("per_source").unwrap_or(3).clamp(1, 20) as usize;
    Ok(Some((query, ids, per_source)))
}

fn nb_compare(val: &Value, db: &Mutex<Database>) -> Value {
    let Some(nb) = val.i64("notebook_id") else { return err_val("notebook_id가 필요합니다.") };
    let (query, ids, per_source) = match nb_compare_args(val, "query", "source_ids") { Ok(Some(a)) => a, Ok(None) => return err_val("query 문자열이 필요합니다."), Err(e) => return err_val(&e) };
    match nb_compare_groups(db, nb, &query, ids.as_deref(), per_source) {
        Ok((terms, groups)) => ok_val(vec![
            ("terms", Value::Arr(terms.into_iter().map(Into::into).collect())),
            ("groups", Value::Arr(groups.into_iter().map(|g| Value::obj(vec![
                ("source_id", g.source.id.into()), ("display_name", g.source.display_name.into()), ("origin", g.source.origin.into()), ("kind", g.source.kind.into()),
                ("hits", Value::Arr(g.hits.into_iter().map(|h| Value::obj(vec![("source_id", h.source_id.into()), ("ordinal", h.ordinal.into()), ("display_name", h.display_name.into()), ("location", h.location.into()), ("score", (h.score as f64).into()), ("body", h.body.into())])).collect())),
            ])).collect())),
        ]),
        Err(e) => err_val(&e),
    }
}

/// 노트 Markdown 내보내기. compare_query가 있으면 그 검색어의 출처 비교 절(compare_source_ids 범위)을 뒤에 붙인다.
fn nb_export_md(val: &Value, db: &Mutex<Database>) -> Value {
    let Some(nb) = val.i64("notebook_id") else { return err_val("notebook_id가 필요합니다.") };
    let Some(id) = val.i64("note_id") else { return err_val("note_id가 필요합니다.") };
    let compare = match nb_compare_args(val, "compare_query", "compare_source_ids") { Ok(c) => c, Err(e) => return err_val(&e) };
    let markdown = { let db_guard = db.lock().unwrap(); crate::notebook::export_md(db_guard.connection(), nb, id) };
    let mut markdown = match markdown { Ok(Some(m)) => m, Ok(None) => return err_val("노트를 찾을 수 없습니다."), Err(e) => return err_val(&e.to_string()) };
    if let Some((query, ids, per_source)) = compare {
        match nb_compare_groups(db, nb, &query, ids.as_deref(), per_source) {
            Ok((_, groups)) => { markdown.push_str("\n\n"); markdown.push_str(&crate::notebook::compare_md(&query, &groups)); }
            Err(e) => return err_val(&e),
        }
    }
    ok_val(vec![("markdown", markdown.into())])
}

fn ok_val(mut extra: Vec<(&str, Value)>) -> Value {

    extra.insert(0, ("ok", true.into()));

    Value::obj(extra)

}

fn scan_status_val() -> Value {
    let s = crate::scan::GLOBAL.snapshot();
    let n = |v: u64| Value::from(v as i64);
    ok_val(vec![
        ("state", s.state.into()), ("phase", s.phase.into()), ("box_id", s.box_id.into()), ("root", s.root.into()),
        ("discovered", n(s.discovered)), ("total", n(s.total)), ("processed", n(s.processed)),
        ("added", n(s.added)), ("updated", n(s.updated)), ("unchanged", n(s.unchanged)), ("removed", n(s.removed)),
        ("skipped_errors", n(s.skipped_errors)), ("error_samples", Value::Arr(s.error_samples.into_iter().map(Into::into).collect())),
        ("message", s.message.into()), ("elapsed_ms", n(s.elapsed_ms)),
    ])
}

fn err_val(msg: &str) -> Value {

    Value::obj(vec![("ok", false.into()), ("message", msg.into())])

}

/// 메모 편집기 코어(editor_core) 문서들. klib 프로세스 수명 동안 유지되며 자료함 DB 잠금과 무관하다.
static EDITOR: OnceLock<Mutex<editor_core::api::Engine>> = OnceLock::new();

/// klib가 파싱한 값을 편집 코어의 JSON 값으로 옮긴다(문자열은 복사하지 않고 넘겨준다).
/// klib 파서가 받아들이는 요청은 코어 파서도 같은 값으로 읽으므로(klib 쪽이 더 엄격하다) 원문을 다시 파싱한 것과 결과가 같다.
fn editor_json(val: Value) -> editor_core::json::Json {
    use editor_core::json::Json;
    match val {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(b),
        Value::Num(n) => Json::Num(n),
        Value::Str(s) => Json::Str(s),
        Value::Arr(items) => Json::Arr(items.into_iter().map(editor_json).collect()),
        Value::Obj(items) => Json::Obj(items.into_iter().map(|(k, v)| (k, editor_json(v))).collect()),
    }
}

/// edit_* 명령을 편집 코어에 넘긴다. 이미 파싱한 값을 넘겨 큰 text가 실린 요청(붙여넣기)을 두 번 파싱하지 않는다.
/// 응답은 코어가 만든 한 줄 JSON 그대로다(ok·message 규약 동일).
fn editor_request(val: Value) -> String {
    let request = editor_json(val);
    let engine = EDITOR.get_or_init(|| Mutex::new(editor_core::api::Engine::new()));
    engine.lock().unwrap_or_else(|e| e.into_inner()).handle_value(&request)
}

pub fn handle_request(req_str: &str, db: &Mutex<Database>, running: &AtomicBool) -> String {

    let Some(val) = json::parse(req_str) else {

        return json::to_string(&err_val("요청 JSON 구문 분석에 실패했습니다."));

    };

    if val.str("cmd").is_some_and(|c| c.starts_with("edit_")) {
        return editor_request(val);
    }

    let cmd = val.str("cmd").unwrap_or("");

    let resp = match cmd {

        "ping" => ok_val(vec![("message", "pong".into()), ("version", env!("CARGO_PKG_VERSION").into())]),

        "nb_create" => {
            let name=val.str("name").unwrap_or(""); let description=val.str("description").unwrap_or(""); let db_guard=db.lock().unwrap();
            match crate::notebook::create(db_guard.connection(),name,description){Ok(id)=>ok_val(vec![("id",id.into())]),Err(e)=>err_val(&e.to_string())}
        }
        "nb_list" => { let db_guard=db.lock().unwrap(); match crate::notebook::list(db_guard.connection()){Ok(rows)=>ok_val(vec![("notebooks",Value::Arr(rows.into_iter().map(|(id,name,description,created,updated)|Value::obj(vec![("id",id.into()),("name",name.into()),("description",description.into()),("created_at",created.into()),("updated_at",updated.into())])).collect()))]),Err(e)=>err_val(&e.to_string())} }
        "nb_update" => { let Some(id)=val.i64("id") else{return json::to_string(&err_val("노트북 id가 필요합니다."))};let db_guard=db.lock().unwrap();match crate::notebook::update(db_guard.connection(),id,val.str("name").unwrap_or(""),val.str("description").unwrap_or("")){Ok(true)=>ok_val(vec![]),Ok(false)=>err_val("노트북을 찾을 수 없습니다."),Err(e)=>err_val(&e.to_string())} }
        "nb_delete" => { let Some(id)=val.i64("id") else{return json::to_string(&err_val("노트북 id가 필요합니다."))};if let Err(e)=crate::search_index().and_then(|ix|ix.remove_notebook(id).map_err(|e|e.to_string())){return json::to_string(&err_val(&e))}let db_guard=db.lock().unwrap();match crate::notebook::delete(db_guard.connection(),id){Ok(true)=>ok_val(vec![]),Ok(false)=>err_val("노트북을 찾을 수 없습니다."),Err(e)=>err_val(&e.to_string())} }
        "nb_source_add" => nb_source_add(&val, db),
        "nb_source_from_item" => nb_source_from_item(&val, db),
        "nb_source_item" => nb_source_item(&val, db),
        "nb_source_list" => {let Some(nb)=val.i64("notebook_id") else{return json::to_string(&err_val("notebook_id가 필요합니다."))};let db_guard=db.lock().unwrap();match crate::notebook::sources(db_guard.connection(),nb){Ok(rows)=>ok_val(vec![("sources",Value::Arr(rows.into_iter().map(|s|Value::obj(vec![("id",s.id.into()),("notebook_id",s.notebook_id.into()),("kind",s.kind.into()),("origin",s.origin.into()),("display_name",s.display_name.into()),("status",s.status.into()),("reason",s.reason.into()),("paragraph_count",s.paragraph_count.into()),("original_size",s.original_size.into()),("original_mtime",s.original_mtime.into()),("item_id",db_guard.connection().query_row("SELECT item_id FROM nb_source WHERE id=?1",[s.id],|r|r.get::<_,Option<i64>>(0)).ok().flatten().map(Value::from).unwrap_or(Value::Null))])).collect()))]),Err(e)=>err_val(&e.to_string())} }
        "nb_source_remove" => {let Some(nb)=val.i64("notebook_id") else{return json::to_string(&err_val("notebook_id가 필요합니다."))};let Some(id)=val.i64("source_id") else{return json::to_string(&err_val("source_id가 필요합니다."))};{let db_guard=db.lock().unwrap();match crate::notebook::source_by_id(db_guard.connection(),id){Ok(Some(s)) if s.notebook_id==nb=>{},Ok(Some(_))=>return json::to_string(&err_val("출처가 해당 노트북에 속하지 않습니다.")),Ok(None)=>return json::to_string(&err_val("출처를 찾을 수 없습니다.")),Err(e)=>return json::to_string(&err_val(&e.to_string()))}}match crate::search_index().and_then(|ix|ix.remove_notebook_source(id).map_err(|e|e.to_string())){Err(e)=>err_val(&e),Ok(())=>{let db_guard=db.lock().unwrap();match crate::notebook::remove_source(db_guard.connection(),id){Ok(true)=>ok_val(vec![]),Ok(false)=>err_val("출처를 찾을 수 없습니다."),Err(e)=>err_val(&e.to_string())}}} }
        "nb_source_paragraphs" => {let Some(nb)=val.i64("notebook_id") else{return json::to_string(&err_val("notebook_id가 필요합니다."))};let Some(id)=val.i64("source_id") else{return json::to_string(&err_val("source_id가 필요합니다."))};let db_guard=db.lock().unwrap();match crate::notebook::source_paragraphs(db_guard.connection(),nb,id){Ok(rows)=>ok_val(vec![("paragraphs",Value::Arr(rows.into_iter().map(|p|Value::obj(vec![("source_id",p.source_id.into()),("ordinal",p.ordinal.into()),("location",p.location.into()),("level",p.level.into()),("kind",p.kind.into()),("body",p.body.into())])).collect()))]),Err(e)=>err_val(&e.to_string())} }
        "nb_index" => nb_index(&val, db),
        "nb_search" => {let Some(nb)=val.i64("notebook_id") else{return json::to_string(&err_val("notebook_id가 필요합니다."))};let Some(query)=val.str("query") else{return json::to_string(&err_val("query 문자열이 필요합니다."))};let ids=match val.arr("source_ids"){Some(a)=>{let mut ids=Vec::new();for v in a{if let Value::Num(n)=v{if n.fract()!=0.0{return json::to_string(&err_val("source_ids는 정수 배열이어야 합니다."))}ids.push(*n as i64)}else{return json::to_string(&err_val("source_ids는 정수 배열이어야 합니다."))}}Some(ids)},None=>None};let limit=val.i64("limit").unwrap_or(20).clamp(1,200) as usize;let terms=crate::nb_transform::query_terms(query);let search_query=if terms.is_empty(){query.to_string()}else{terms.join(" ")};match crate::search_index().and_then(|ix|ix.search_notebook(nb,&search_query,ids.as_deref(),limit).map_err(|e|e.to_string())){Ok(hits)=>ok_val(vec![("terms",Value::Arr(terms.into_iter().map(Into::into).collect())),("results",Value::Arr(hits.into_iter().map(|h|Value::obj(vec![("source_id",h.source_id.into()),("ordinal",h.ordinal.into()),("display_name",h.display_name.into()),("location",h.location.into()),("score",(h.score as f64).into()),("body",h.body.into())])).collect()))]),Err(e)=>err_val(&e)} }
        "nb_transform" => {let Some(nb)=val.i64("notebook_id") else{return json::to_string(&err_val("notebook_id가 필요합니다."))};let Some(transform_type)=val.str("type") else{return json::to_string(&err_val("type 문자열이 필요합니다."))};let ids=val.arr("source_ids").map(|a|a.iter().filter_map(|v|if let Value::Num(n)=v{Some(*n as i64)}else{None}).collect::<Vec<_>>());let limit=val.i64("limit").unwrap_or(20).clamp(1,200) as usize;let db_guard=db.lock().unwrap();let sources=match crate::notebook::sources(db_guard.connection(),nb){Ok(v)=>v,Err(e)=>return json::to_string(&err_val(&e.to_string()))};let mut paragraphs=Vec::new();for source in sources{if ids.as_ref().is_some_and(|set|!set.contains(&source.id)){continue}match crate::notebook::paragraphs(db_guard.connection(),source.id){Ok(mut p)=>paragraphs.append(&mut p),Err(e)=>return json::to_string(&err_val(&e.to_string()))}}match crate::notebook::transform(&paragraphs,transform_type,limit){Ok(rows)=>ok_val(vec![("results",Value::Arr(rows.into_iter().map(|r|Value::obj(vec![("source_id",r.source_id.into()),("ordinal",r.ordinal.into()),("location",r.location.into()),("level",r.level.into()),("kind",r.kind.into()),("text",r.text.into()),("score",r.score.into())])).collect()))]),Err(e)=>err_val(&e)} }
        "nb_note_save" => nb_note_save(&val, db),
        "nb_note_list" => nb_note_list(&val, db),
        "nb_note_delete" => {let Some(id)=val.i64("id") else{return json::to_string(&err_val("note id가 필요합니다."))};let Some(nb)=val.i64("notebook_id") else{return json::to_string(&err_val("notebook_id가 필요합니다."))};let db_guard=db.lock().unwrap();match db_guard.connection().execute("DELETE FROM nb_note WHERE id=?1 AND notebook_id=?2",[id,nb]){Ok(n) if n>0=>ok_val(vec![]),Ok(_)=>err_val("해당 노트북의 노트를 찾을 수 없습니다."),Err(e)=>err_val(&e.to_string())} }
        "nb_export_md" => nb_export_md(&val, db),
        "nb_compare" => nb_compare(&val, db),

        "box_create" => {

            let name = val.str("name").unwrap_or("");

            let desc = val.str("description").unwrap_or("");

            if name.is_empty() {

                err_val("자료함 이름을 입력하세요.")

            } else {

                let db_guard = db.lock().unwrap();

                match db_guard.box_create(name, desc) {

                    Ok(id) => ok_val(vec![("id", id.into())]),

                    Err(e) => err_val(&e.to_string()),

                }

            }

        }

        "box_list" => {

            let db_guard = db.lock().unwrap();

            match db_guard.box_list() {

                Ok(list) => {

                    let items: Vec<Value> = list

                        .into_iter()

                        .map(|b| {

                            Value::obj(vec![

                                ("id", b.id.into()),

                                ("name", b.name.into()),

                                ("description", b.description.into()),

                                ("created_at", b.created_at.into()),

                                ("updated_at", b.updated_at.into()),

                                ("export_path", b.export_path.map(Value::Str).unwrap_or(Value::Null)),

                                ("item_count", b.item_count.into()),

                            ])

                        })

                        .collect();

                    ok_val(vec![("boxes", Value::Arr(items))])

                }

                Err(e) => err_val(&e.to_string()),

            }

        }

        "box_get" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("자료함 id가 누락되었습니다."));

            };

            let db_guard = db.lock().unwrap();

            match db_guard.box_get(id) {

                Ok(Some((b, items, rels))) => {

                    let b_val = Value::obj(vec![

                        ("id", b.id.into()),

                        ("name", b.name.into()),

                        ("description", b.description.into()),

                        ("created_at", b.created_at.into()),

                        ("updated_at", b.updated_at.into()),

                        ("export_path", b.export_path.map(Value::Str).unwrap_or(Value::Null)),

                        ("item_count", b.item_count.into()),

                    ]);

                    let it_vals: Vec<Value> = items

                        .into_iter()

                        .map(|it| {

                            Value::obj(vec![

                                ("id", it.id.into()),

                                ("box_id", it.box_id.into()),

                                ("kind", it.kind.into()),

                                ("source_path", it.source_path.into()),
                    ("origin_path", it.origin_path.into()),
                    ("managed_path", it.managed_path.into()),

                                ("display_name", it.display_name.into()),

                                ("space", it.space.into()),

                                ("x", it.x.into()),

                                ("y", it.y.into()),

                                ("created_at", it.created_at.into()),

                            ])

                        })

                        .collect();

                    let rel_vals: Vec<Value> = rels

                        .into_iter()

                        .map(|r| {

                            Value::obj(vec![

                                ("id", r.id.into()),

                                ("box_id", r.box_id.into()),

                                ("from_item", r.from_item.into()),

                                ("to_item", r.to_item.into()),

                                ("label", r.label.into()),

                            ])

                        })

                        .collect();

                    ok_val(vec![

                        ("box", b_val),

                        ("items", Value::Arr(it_vals)),

                        ("relations", Value::Arr(rel_vals)),

                    ])

                }

                Ok(None) => err_val("해당 자료함을 찾을 수 없습니다."),

                Err(e) => err_val(&e.to_string()),

            }

        }

        "box_update" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("자료함 id가 누락되었습니다."));

            };

            let name = val.str("name").unwrap_or("");

            let desc = val.str("description").unwrap_or("");

            let db_guard = db.lock().unwrap();

            match db_guard.box_update(id, name, desc) {

                Ok(true) => ok_val(vec![]),

                Ok(false) => err_val("해당 자료함이 존재하지 않습니다."),

                Err(e) => err_val(&e.to_string()),

            }

        }

        "box_rename" => {

            let Some(id) = val.i64("id") else { return json::to_string(&err_val("box id required")); };

            let name = val.str("name").unwrap_or("");

            match db.lock().unwrap().box_rename(id, name) { Ok(true) => ok_val(vec![]), Ok(false) => err_val("box not found"), Err(e) => err_val(&e.to_string()) }

        }

        "box_delete" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("자료함 id가 누락되었습니다."));

            };

            let db_guard = db.lock().unwrap();

            match db_guard.box_delete(id) {

                Ok(true) => ok_val(vec![]),

                Ok(false) => err_val("해당 자료함이 존재하지 않습니다."),

                Err(e) => err_val(&e.to_string()),

            }

        }

        "item_add" => {

            let Some(box_id) = val.i64("box_id") else {

                return json::to_string(&err_val("box_id가 누락되었습니다."));

            };

            let kind = val.str("kind").unwrap_or("");

            let source_path = val.str("source_path").unwrap_or("");

            let display_name = val.str("display_name").unwrap_or("");

            let space = val.str("space").unwrap_or("temp");

            let x = val.num("x").unwrap_or(0.0);

            let y = val.num("y").unwrap_or(0.0);

            let content = val.str("content");

            let db_guard = db.lock().unwrap();

            match db_guard.item_add(box_id, kind, source_path, display_name, space, x, y, content) {

                Ok(id) => {

                    let indexing = if matches!(kind, "file" | "text" | "rich_text" | "note") {

                        let source = db_guard.item_get(id).ok().flatten().map(|it| it.source_path).unwrap_or_default();

                        crate::search_index().and_then(|ix| crate::content::index_file(ix, &source, display_name, box_id).map_err(|e| format!("본문 색인 실패: {e}"))).err()

                    } else { None };

                    let mut extra = vec![("id", id.into())];

                    if let Some(message) = indexing { extra.push(("index_warning", message.into())); }

                    ok_val(extra)

                },

                Err(e) => err_val(&e),

            }

        }

        "search" => {

            let query = val.str("query").unwrap_or("");

            let box_id = val.i64("box_id");

            let limit = val.i64("limit").unwrap_or(20).clamp(1, 200) as usize;

            match crate::search_index().and_then(|ix| {

                if let Some(id) = box_id { ix.search_box(id, query, limit).map_err(|e| e.to_string()) }

                else { ix.search(query, crate::search::SearchOptions { limit: limit.saturating_mul(20).min(4000) }).map_err(|e| e.to_string()) }

            }) {

                Ok(hits) => {

                    let mut filtered: Vec<Value> = hits.into_iter()

                        .filter(|hit| box_id.map_or(true, |id| hit.box_ids.contains(&id.to_string())))

                        .map(|hit| {

                            let snippets: Vec<Value> = hit.snippets.iter().map(|s| s.clone().into()).collect();

                            let highlights: Vec<Value> = hit.highlighted.iter().map(|s| s.clone().into()).collect();

                            let pages: Vec<Value> = hit.hit_pages.iter()

                                .map(|p| p.map(|v| (v as i64).into()).unwrap_or(Value::Null)).collect();

                            Value::obj(vec![

                                ("path", hit.path.into()), ("name", hit.name.into()),

                                ("snippet", snippets.first().cloned().unwrap_or(Value::Str(String::new()))),

                                ("highlights", Value::Arr(highlights)), ("hits", (hit.paragraph_count as i64).into()),

                                ("page", hit.page.map(|p| (p as i64).into()).unwrap_or(Value::Null)),

                                ("pages", Value::Arr(pages)), ("score", (hit.score as f64).into()),

                            ])

                        }).collect();

                    filtered.truncate(limit);

                    ok_val(vec![("results", Value::Arr(filtered))])

                }

                Err(e) => err_val(&e),

            }

        }

        "index_file" => {

            let path = val.str("path").unwrap_or("");

            let name = val.str("name").unwrap_or_else(|| std::path::Path::new(path).file_name().and_then(|v| v.to_str()).unwrap_or(""));

            let Some(box_id) = val.i64("box_id") else { return json::to_string(&err_val("box_id가 필요합니다.")); };

            match crate::search_index().and_then(|ix| crate::content::index_file(ix, path, name, box_id).map_err(|e| e.to_string())) {

                Ok(()) => ok_val(vec![]), Err(e) => err_val(&e)

            }

        }

        "remove_file" => {

            let path = val.str("path").unwrap_or("");

            match crate::search_index().and_then(|ix| crate::content::remove_file(ix, path).map_err(|e| e.to_string())) {

                Ok(()) => ok_val(vec![]), Err(e) => err_val(&e)

            }

        }

        "item_get" | "item_prepare" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("item id가 누락되었습니다."));

            };

            let db_guard = db.lock().unwrap();

            let old_source = if cmd == "item_prepare" { db_guard.item_get(id).ok().flatten().map(|it| it.source_path) } else { None };
            match if cmd == "item_prepare" { db_guard.item_prepare(id).map(Some).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e)))) } else { db_guard.item_get(id) } {

                Ok(Some(it)) => {
                    let warning = if cmd == "item_prepare" { sync_item_index(old_source.as_deref(), &it) } else { None };
                    let mut fields = vec![

                    ("id", it.id.into()),

                    ("box_id", it.box_id.into()),

                    ("kind", it.kind.into()),

                    ("source_path", it.source_path.into()),
                    ("origin_path", it.origin_path.into()),
                    ("managed_path", it.managed_path.into()),

                    ("display_name", it.display_name.into()),

                    ("space", it.space.into()),

                    ("x", it.x.into()),

                    ("y", it.y.into()),

                    ("created_at", it.created_at.into()),

                    ];
                    if let Some(warning) = warning { fields.push(("index_warning", warning.into())); }
                    ok_val(fields)
                },

                Ok(None) => err_val("해당 자료 항목을 찾을 수 없습니다."),

                Err(e) => {
                    let row = db_guard.item_get(id).ok().flatten();
                    let code = match row { None => "NOT_REGISTERED", Some(ref it) if it.managed_path.is_empty() => "SOURCE_MISSING", _ => "MANAGED_MISSING" };
                    Value::obj(vec![("ok", false.into()), ("code", code.into()), ("message", e.to_string().into())])
                },

            }

        }

        "item_update" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("item id가 누락되었습니다."));

            };

            let display_name = val.str("display_name").unwrap_or("");

            let db_guard = db.lock().unwrap();

            match db_guard.item_update(id, display_name) {

                Ok(true) => ok_val(vec![]),

                Ok(false) => err_val("해당 자료 항목이 존재하지 않습니다."),

                Err(e) => err_val(&e.to_string()),

            }

        }

        "item_rename" => {

            let Some(id) = val.i64("id") else { return json::to_string(&err_val("item id required")); };

            let name = val.str("display_name").unwrap_or("");

            match db.lock().unwrap().item_rename(id, name) { Ok(true) => ok_val(vec![]), Ok(false) => err_val("item not found"), Err(e) => err_val(&e.to_string()) }

        }

        // 자료함 '+메모'·글 자료 더블클릭: 메모 편집기로 글 자료(text·note) 사본을 읽고 고친다. 경로 인자는 받지 않는다.
        "item_text_get" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("item id가 누락되었습니다."));

            };

            let result = {
                let guard = db.lock().unwrap();
                prepare_library_item(&guard, id).and_then(|_| guard.item_text_get(id))
            };
            match result {

                Ok((it, content)) => ok_val(vec![

                    ("id", it.id.into()),

                    ("kind", it.kind.into()),

                    ("display_name", it.display_name.into()),

                    ("content", content.into()),

                ]),

                Err(e) => err_val(&e),

            }

        }

        "item_text_set" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("item id가 누락되었습니다."));

            };

            let Some(content) = val.str("content") else {

                return json::to_string(&err_val("content가 누락되었습니다."));

            };

            let result = {
                let guard = db.lock().unwrap();
                prepare_library_item(&guard, id).and_then(|_| guard.item_text_set(id, content, val.str("display_name")))
            };

            match result {

                Ok(it) => {

                    // 본문 검색 색인도 새 내용으로 갈아 끼운다. 실패해도 저장은 된 것이므로 경고로만 알린다.
                    let indexing = crate::search_index()
                        .and_then(|ix| crate::content::index_file(ix, &it.source_path, &it.display_name, it.box_id).map_err(|e| format!("본문 색인 실패: {e}")))
                        .err();

                    let mut extra = vec![("display_name", Value::from(it.display_name))];

                    if let Some(message) = indexing { extra.push(("index_warning", message.into())); }

                    ok_val(extra)

                }

                Err(e) => err_val(&e),

            }

        }

        "item_move" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("item id가 누락되었습니다."));

            };

            let space = val.str("space").unwrap_or("temp");

            let x = val.num("x").unwrap_or(0.0);

            let y = val.num("y").unwrap_or(0.0);

            let db_guard = db.lock().unwrap();

            let old = db_guard.item_get(id).ok().flatten().map(|it| it.source_path);
            match db_guard.item_move(id, space, x, y) {

                Ok(true) => {
                    let warning = match db_guard.item_get(id) {
                        Ok(Some(it)) => sync_item_index(old.as_deref(), &it),
                        Ok(None) => Some("항목 없음".to_string()),
                        Err(e) => Some(e.to_string()),
                    };
                    ok_val(warning.map(|w| vec![("index_warning", w.into())]).unwrap_or_default())
                },

                Ok(false) => err_val("해당 자료 항목이 존재하지 않습니다."),

                Err(e) => err_val(&e),

            }

        }

        "item_delete" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("item id가 누락되었습니다."));

            };

            let db_guard = db.lock().unwrap();

            let path = db_guard.item_get(id).ok().flatten().map(|it| it.source_path);

            match db_guard.item_delete(id) {

                Ok(true) => {

                    if let Some(path) = path {

                        if let Ok(ix) = crate::search_index() { let _ = crate::content::remove_file(ix, &path); }

                    }

                    ok_val(vec![])

                },

                Ok(false) => err_val("해당 자료 항목이 존재하지 않습니다."),

                Err(e) => err_val(&e.to_string()),

            }

        }

        "item_search" => {

            let box_id = val.i64("box_id");

            let query = val.str("query").unwrap_or("");

            if !query.is_empty() {

                if let Some(box_id) = box_id {

                    let limit = val.i64("limit").unwrap_or(20).clamp(1, 200) as usize;

                    let hits = match crate::search_index().and_then(|ix| ix.search_box(box_id, query, limit).map_err(|e| e.to_string())) {

                        Ok(hits) => hits,

                        Err(e) => return json::to_string(&err_val(&e)),

                    };

                    let mut items = Vec::new();

                    for hit in hits {

                        let db_guard = db.lock().unwrap();

                        if let Ok(Some(item)) = db_guard.item_search(Some(box_id), "").map(|rows| rows.into_iter().find(|row| row.source_path.eq_ignore_ascii_case(&hit.path))) {

                            items.push(Value::obj(vec![

                                ("id", item.id.into()), ("box_id", item.box_id.into()),

                                ("kind", item.kind.into()), ("source_path", item.source_path.into()), ("origin_path", item.origin_path.into()), ("managed_path", item.managed_path.into()),

                                ("display_name", item.display_name.into()), ("space", item.space.into()),

                                ("x", item.x.into()), ("y", item.y.into()), ("created_at", item.created_at.into()),

                                ("snippet", hit.snippets.first().cloned().unwrap_or_default().into()),

                            ]));

                        }

                    }

                    return json::to_string(&ok_val(vec![("items", Value::Arr(items))]));

                }

            }

            let db_guard = db.lock().unwrap();

            match db_guard.item_search(box_id, query) {

                Ok(items) => {

                    let it_vals: Vec<Value> = items

                        .into_iter()

                        .map(|it| {

                            Value::obj(vec![

                                ("id", it.id.into()),

                                ("box_id", it.box_id.into()),

                                ("kind", it.kind.into()),

                                ("source_path", it.source_path.into()),
                    ("origin_path", it.origin_path.into()),
                    ("managed_path", it.managed_path.into()),

                                ("display_name", it.display_name.into()),

                                ("space", it.space.into()),

                                ("x", it.x.into()),

                                ("y", it.y.into()),

                                ("created_at", it.created_at.into()),

                            ])

                        })

                        .collect();

                    ok_val(vec![("items", Value::Arr(it_vals))])

                }

                Err(e) => err_val(&e.to_string()),

            }

        }

        "relation_add" => {

            let Some(box_id) = val.i64("box_id") else {

                return json::to_string(&err_val("box_id가 누락되었습니다."));

            };

            let Some(from_item) = val.i64("from_item") else {

                return json::to_string(&err_val("from_item이 누락되었습니다."));

            };

            let Some(to_item) = val.i64("to_item") else {

                return json::to_string(&err_val("to_item이 누락되었습니다."));

            };

            let label = val.str("label").unwrap_or("");

            let db_guard = db.lock().unwrap();

            match db_guard.relation_add(box_id, from_item, to_item, label) {

                Ok(id) => ok_val(vec![("id", id.into())]),

                Err(e) => err_val(&e),

            }

        }

        "relation_delete" => {

            let Some(id) = val.i64("id") else {

                return json::to_string(&err_val("relation id가 누락되었습니다."));

            };

            let db_guard = db.lock().unwrap();

            match db_guard.relation_delete(id) {

                Ok(true) => ok_val(vec![]),

                Ok(false) => err_val("해당 관계선이 존재하지 않습니다."),

                Err(e) => err_val(&e.to_string()),

            }

        }

        "relation_update" => {

            let Some(id) = val.i64("id") else { return json::to_string(&err_val("relation id required")); };

            let label = val.str("label").unwrap_or("");

            match db.lock().unwrap().relation_update(id, label) {

                Ok(true) => ok_val(vec![]), Ok(false) => err_val("relation not found"), Err(e) => err_val(&e.to_string()),

            }

        }

        "relation_list" => {

            let Some(box_id) = val.i64("box_id") else {

                return json::to_string(&err_val("box_id가 누락되었습니다."));

            };

            let db_guard = db.lock().unwrap();

            match db_guard.relation_list(box_id) {

                Ok(list) => {

                    let rel_vals: Vec<Value> = list

                        .into_iter()

                        .map(|r| {

                            Value::obj(vec![

                                ("id", r.id.into()),

                                ("box_id", r.box_id.into()),

                                ("from_item", r.from_item.into()),

                                ("to_item", r.to_item.into()),

                                ("label", r.label.into()),

                            ])

                        })

                        .collect();

                    ok_val(vec![("relations", Value::Arr(rel_vals))])

                }

                Err(e) => err_val(&e.to_string()),

            }

        }

        "box_scan" => {
            let Some(box_id) = val.i64("box_id") else { return json::to_string(&err_val("box_id가 필요합니다.")); };
            let Some(path) = val.str("path") else { return json::to_string(&err_val("path(스캔할 폴더)가 필요합니다.")); };
            let db_path = db.lock().unwrap().connection().path().map(|p| p.to_string()).unwrap_or_default();
            if db_path.is_empty() { return json::to_string(&err_val("DB 파일 경로를 알 수 없습니다.")); }
            let exts: Vec<String> = val.arr("extensions").unwrap_or(&[]).iter()
                .filter_map(|v| if let Value::Str(e) = v { Some(e.trim().to_lowercase()) } else { None })
                .filter(|e| !e.is_empty()).map(|e| if e.starts_with('.') { e } else { format!(".{e}") }).collect();
            match crate::scan::start(&db_path, box_id, path, exts) {
                Ok(()) => ok_val(vec![("started", true.into())]),
                Err(e) => err_val(&e),
            }
        }
        "box_scan_status" => scan_status_val(),
        "box_scan_files" => {
            let Some(box_id) = val.i64("box_id") else { return json::to_string(&err_val("box_id가 필요합니다.")); };
            let offset = val.i64("offset").unwrap_or(0).max(0);
            let limit = val.i64("limit").unwrap_or(1000).clamp(1, 5000);
            let guard = db.lock().unwrap();
            match crate::scan::list_files(guard.connection(), box_id, val.str("path"), offset, limit) {
                Ok(rows) => ok_val(vec![("files", Value::Arr(rows.into_iter().map(|(p, s, m)| Value::obj(vec![("path", p.into()), ("size", s.into()), ("mtime", Value::Num(m as f64 / 1e9))])).collect()))]),
                Err(e) => err_val(&e.to_string()),
            }
        }
        "box_scan_cancel" => ok_val(vec![("cancelling", crate::scan::GLOBAL.request_cancel().into())]),

        "box_export" => {

            let Some(box_id) = val.i64("box_id") else {

                return json::to_string(&err_val("box_id가 누락되었습니다."));

            };

            let target_dir = val.str("target_dir").unwrap_or("");

            if target_dir.is_empty() {

                err_val("대상 폴더 경로(target_dir)를 지정하세요.")

            } else {

                let db_guard = db.lock().unwrap();

                let before = db_guard.item_search(Some(box_id), "").unwrap_or_default();
                let result = db_guard.box_export(box_id, target_dir);
                // 실패한 export도 그 전에 완료된 안전한 항목 이전은 유지되므로 변경된 경로만 동기화한다.
                // 여러 항목의 색인 교체를 묶어 끝에서 한 번만 커밋한다.
                let index_batch = if before.is_empty() { None } else { crate::search_index().ok().map(|ix| ix.batch()) };
                for old in before {
                    if let Ok(Some(it)) = db_guard.item_get(old.id) { let _ = sync_item_index(Some(&old.source_path), &it); }
                }
                drop(index_batch);
                match result {

                    Ok(count) => ok_val(vec![("count", count.into())]),

                    Err(e) => err_val(&e),

                }

            }

        }

        "quit" => {

            running.store(false, Ordering::Relaxed);

            ok_val(vec![("message", "종료합니다.".into())])

        }

        _ => err_val(&format!("알 수 없는 명령입니다: {cmd}")),

    };

    json::to_string(&resp)

}
