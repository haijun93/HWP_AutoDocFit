//! 미리보기 원문(Q-A6): 앱이 이미 가진 SQLite 문단 표(`timeline-content.sqlite`)를 **읽기 전용**으로 열어
//! 화면에 보일 결과의 문단 글만 가져온다. 엔진(kfind)에 원문을 두지 않는다.
//! SQLite는 Windows에 들어 있는 `winsqlite3.dll`을 System32에서만 불러 쓴다(exe 크기 0 증가, DLL 바꿔치기 방지).
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

type Db = *mut c_void;
type Stmt = *mut c_void;

#[allow(clippy::type_complexity)]
struct Api {
    open_v2: unsafe extern "system" fn(*const u8, *mut Db, i32, *const u8) -> i32,
    close_v2: unsafe extern "system" fn(Db) -> i32,
    prepare_v2: unsafe extern "system" fn(Db, *const u8, i32, *mut Stmt, *mut *const u8) -> i32,
    bind_text: unsafe extern "system" fn(Stmt, i32, *const u8, i32, isize) -> i32,
    bind_int64: unsafe extern "system" fn(Stmt, i32, i64) -> i32,
    step: unsafe extern "system" fn(Stmt) -> i32,
    column_text: unsafe extern "system" fn(Stmt, i32) -> *const u8,
    column_bytes: unsafe extern "system" fn(Stmt, i32) -> i32,
    reset: unsafe extern "system" fn(Stmt) -> i32,
    finalize: unsafe extern "system" fn(Stmt) -> i32,
    exec: unsafe extern "system" fn(Db, *const u8, *const c_void, *mut c_void, *mut *mut u8) -> i32,
    busy_timeout: unsafe extern "system" fn(Db, i32) -> i32,
    errmsg: unsafe extern "system" fn(Db) -> *const u8,
}
// 함수 포인터만 담는다.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

const SQLITE_OK: i32 = 0;
const SQLITE_ROW: i32 = 100;
const SQLITE_DONE: i32 = 101;
const OPEN_READONLY: i32 = 0x1;
const OPEN_READWRITE: i32 = 0x2;
const OPEN_CREATE: i32 = 0x4;
const OPEN_NOMUTEX: i32 = 0x8000;
const TRANSIENT: isize = -1;
/// 한 문단에서 가져올 글의 상한(화면 한 줄 강조용).
const MAX_BODY: usize = 64 * 1024;

#[cfg(windows)]
fn load() -> Result<Api, String> {
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    unsafe {
        let name: Vec<u16> = "winsqlite3.dll\0".encode_utf16().collect();
        let module = LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if module.is_null() {
            return Err("winsqlite3.dll을 불러올 수 없습니다.".into());
        }
        macro_rules! f {
            ($n:literal) => {{
                let p = GetProcAddress(module, concat!($n, "\0").as_ptr())
                    .ok_or(concat!("winsqlite3 함수 없음: ", $n))?;
                std::mem::transmute::<unsafe extern "system" fn() -> isize, _>(p)
            }};
        }
        Ok(Api {
            open_v2: f!("sqlite3_open_v2"),
            close_v2: f!("sqlite3_close_v2"),
            prepare_v2: f!("sqlite3_prepare_v2"),
            bind_text: f!("sqlite3_bind_text"),
            bind_int64: f!("sqlite3_bind_int64"),
            step: f!("sqlite3_step"),
            column_text: f!("sqlite3_column_text"),
            column_bytes: f!("sqlite3_column_bytes"),
            reset: f!("sqlite3_reset"),
            finalize: f!("sqlite3_finalize"),
            exec: f!("sqlite3_exec"),
            busy_timeout: f!("sqlite3_busy_timeout"),
            errmsg: f!("sqlite3_errmsg"),
        })
    }
}

#[cfg(not(windows))]
fn load() -> Result<Api, String> {
    Err("winsqlite3는 Windows에서만 씁니다.".into())
}

fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(load).as_ref().map_err(Clone::clone)
}

/// 앱 문단 표의 기본 위치: `%KAGENT_DATA%` 또는 `%LOCALAPPDATA%\HwpAutoDocFit\kagent\timeline-content.sqlite`
/// (Python `kagent_client.data_dir()`·`TimemachineService`와 같은 규칙).
pub fn default_path() -> Option<PathBuf> {
    let base = match std::env::var_os("KAGENT_DATA").filter(|s| !s.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(std::env::var_os("LOCALAPPDATA").filter(|s| !s.is_empty())?)
            .join("HwpAutoDocFit")
            .join("kagent"),
    };
    Some(base.join("timeline-content.sqlite"))
}

const BODY_SQL: &str = "SELECT b.body FROM content_paragraph_map m JOIN content_paragraph_fts b \
    ON b.rowid=m.para_rowid WHERE m.path=?1 AND m.paragraph_index=?2 LIMIT 1\0";

/// 읽기 전용 연결 하나와 준비된 문장 하나. 검색 일꾼 스레드 하나에서만 쓴다.
pub struct Paragraphs {
    api: &'static Api,
    db: Db,
    stmt: Stmt,
}
// 연결은 NOMUTEX로 열고 한 스레드(검색 일꾼)만 쓴다. 다른 스레드로 옮길 때만 Send가 필요하다.
unsafe impl Send for Paragraphs {}

impl Paragraphs {
    /// 파일이 없으면 오류(만들지 않는다). 쓰기 핸들을 열지 않는다(`SQLITE_OPEN_READONLY`).
    pub fn open(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Err("문단 표가 없습니다.".into());
        }
        let api = api()?;
        let mut name = path.to_string_lossy().into_owned().into_bytes();
        name.push(0);
        unsafe {
            let mut db = std::ptr::null_mut();
            let rc = (api.open_v2)(name.as_ptr(), &mut db, OPEN_READONLY | OPEN_NOMUTEX, std::ptr::null());
            let mut me = Self { api, db, stmt: std::ptr::null_mut() };
            if rc != SQLITE_OK {
                return Err(me.error("문단 표 열기"));
            }
            (api.busy_timeout)(db, 200);
            // 쉴 때 메모리를 작게: 쪽 캐시 256KB.
            me.exec("PRAGMA cache_size=-256;PRAGMA query_only=1;")?;
            let rc = (api.prepare_v2)(db, BODY_SQL.as_ptr(), -1, &mut me.stmt, std::ptr::null_mut());
            if rc != SQLITE_OK {
                return Err(me.error("문단 표 형식"));
            }
            Ok(me)
        }
    }

    fn error(&self, what: &str) -> String {
        unsafe {
            let p = (self.api.errmsg)(self.db);
            let msg = if p.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(p.cast()).to_string_lossy().into_owned()
            };
            format!("{what}: {msg}")
        }
    }

    fn exec(&self, sql: &str) -> Result<(), String> {
        let mut s = sql.as_bytes().to_vec();
        s.push(0);
        let rc = unsafe {
            (self.api.exec)(self.db, s.as_ptr(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut())
        };
        if rc == SQLITE_OK { Ok(()) } else { Err(self.error("문단 표 설정")) }
    }

    /// (경로, 문단 번호)의 문단 글. 없으면 None.
    pub fn body(&mut self, path: &str, para: u32) -> Result<Option<String>, String> {
        unsafe {
            let a = self.api;
            (a.reset)(self.stmt);
            (a.bind_text)(self.stmt, 1, path.as_ptr(), path.len() as i32, TRANSIENT);
            (a.bind_int64)(self.stmt, 2, para as i64);
            let out = match (a.step)(self.stmt) {
                SQLITE_ROW => {
                    let p = (a.column_text)(self.stmt, 0);
                    let n = ((a.column_bytes)(self.stmt, 0).max(0) as usize).min(MAX_BODY);
                    if p.is_null() {
                        Ok(None)
                    } else {
                        let bytes = std::slice::from_raw_parts(p, n);
                        Ok(Some(String::from_utf8_lossy(bytes).into_owned()))
                    }
                }
                SQLITE_DONE => Ok(None),
                _ => Err(self.error("문단 읽기")),
            };
            (a.reset)(self.stmt);
            out
        }
    }
}

impl Drop for Paragraphs {
    fn drop(&mut self) {
        unsafe {
            if !self.stmt.is_null() {
                (self.api.finalize)(self.stmt);
            }
            if !self.db.is_null() {
                (self.api.close_v2)(self.db);
            }
        }
    }
}

/// 시험·합성 자료 전용: 앱과 같은 꼴의 문단 표를 **새 파일**로 만든다(있으면 거부). 사용자 자료에는 쓰지 않는다.
#[doc(hidden)]
pub fn create_fixture(path: &Path, rows: &[(&str, u32, &str)]) -> Result<(), String> {
    if path.exists() {
        return Err("이미 있는 파일에는 쓰지 않습니다.".into());
    }
    let api = api()?;
    let mut name = path.to_string_lossy().into_owned().into_bytes();
    name.push(0);
    unsafe {
        let mut db = std::ptr::null_mut();
        let rc = (api.open_v2)(name.as_ptr(), &mut db, OPEN_READWRITE | OPEN_CREATE | OPEN_NOMUTEX, std::ptr::null());
        let me = Paragraphs { api, db, stmt: std::ptr::null_mut() };
        if rc != SQLITE_OK {
            return Err(me.error("합성 문단 표 만들기"));
        }
        me.exec(
            "CREATE VIRTUAL TABLE content_paragraph_fts USING fts5(path UNINDEXED,name UNINDEXED,\
             paragraph_index UNINDEXED,level UNINDEXED,level_rank UNINDEXED,body,location_label UNINDEXED,\
             section UNINDEXED,tokenize='trigram');\
             CREATE TABLE content_paragraph_map(path TEXT NOT NULL, paragraph_index INTEGER NOT NULL,\
             para_rowid INTEGER NOT NULL, PRIMARY KEY(path, paragraph_index, para_rowid)) WITHOUT ROWID;\
             BEGIN;",
        )?;
        let quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
        for (i, (p, para, body)) in rows.iter().enumerate() {
            let rowid = i + 1;
            me.exec(&format!(
                "INSERT INTO content_paragraph_fts(rowid,path,name,paragraph_index,level,level_rank,body,location_label)\
                 VALUES({rowid},{},'',{para},'body',8,{},'');\
                 INSERT INTO content_paragraph_map VALUES({},{para},{rowid});",
                quote(p),
                quote(body),
                quote(p)
            ))?;
        }
        me.exec("COMMIT;")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ktm-para-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_body_by_path_and_paragraph_read_only() {
        let dir = temp("read");
        let db = dir.join("timeline-content.sqlite");
        create_fixture(&db, &[("C:\\a\\가.hwp", 0, "예산집행 계획"), ("C:\\a\\가.hwp", 3, "넷째 문단 '인용'"), ("C:\\b.hwp", 0, "다른")]).unwrap();
        let before = std::fs::read(&db).unwrap();
        let mut p = Paragraphs::open(&db).unwrap();
        assert_eq!(p.body("C:\\a\\가.hwp", 3).unwrap().as_deref(), Some("넷째 문단 '인용'"));
        assert_eq!(p.body("C:\\a\\가.hwp", 0).unwrap().as_deref(), Some("예산집행 계획"));
        assert_eq!(p.body("C:\\a\\가.hwp", 9).unwrap(), None);
        assert_eq!(p.body("C:\\없음.hwp", 0).unwrap(), None);
        drop(p);
        assert_eq!(std::fs::read(&db).unwrap(), before, "읽기 전용");
        assert!(create_fixture(&db, &[]).is_err(), "있는 파일에 쓰지 않음");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_is_not_created() {
        let dir = temp("missing");
        let db = dir.join("none.sqlite");
        assert!(Paragraphs::open(&db).is_err());
        assert!(!db.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn wrong_schema_is_an_error() {
        let dir = temp("schema");
        let db = dir.join("x.sqlite");
        std::fs::write(&db, b"").unwrap();
        assert!(Paragraphs::open(&db).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
