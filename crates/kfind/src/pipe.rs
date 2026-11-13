use crate::Result;
use kfind_core::{Index, SearchOptions};
use serde::Deserialize;
use serde_json::{Value, json};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::Authorization::*,
    Security::*,
    System::{Pipes::*, Threading::*},
};
pub const MAX_REQUEST: usize = 65536;
#[derive(Deserialize, Debug)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Query {
        id: u64,
        text: String,
        #[serde(default = "default_top")]
        top: usize,
    },
    Cancel {
        id: u64,
    },
    Status {},
}
fn default_top() -> usize {
    20
}
pub fn decode(bytes: &[u8]) -> Result<Request> {
    if bytes.len() > MAX_REQUEST {
        return Err("request exceeds 64KB".into());
    }
    Ok(serde_json::from_slice(bytes)?)
}
/// SID comes exclusively from the connected process token, never from request JSON.
pub fn process_sid(pid: u32) -> Result<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let _process = OwnedHandle::from_raw_handle(process);
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let _token = OwnedHandle::from_raw_handle(token);
        let mut size = 0;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        if GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut sid = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut sid) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut n = 0;
        while *sid.add(n) != 0 {
            n += 1;
        }
        let result = String::from_utf16(std::slice::from_raw_parts(sid, n));
        LocalFree(sid.cast());
        Ok(result?)
    }
}
pub fn current_sid() -> Result<String> {
    process_sid(std::process::id())
}
pub fn create(name: &str, first: bool) -> Result<NamedPipeServer> {
    // Protected DACL: SYSTEM and interactive users only. Network logons are also rejected by pipe mode.
    let sddl: Vec<u16> = "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)\0".encode_utf16().collect();
    unsafe {
        let mut descriptor = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let result = ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, &attrs as *const _ as *mut _);
        LocalFree(descriptor);
        Ok(result?)
    }
}
type Active = Arc<Mutex<HashMap<(String, u64), Arc<AtomicBool>>>>;
#[derive(Clone)]
pub struct Handler {
    root: PathBuf,
    active: Active,
    indices: Arc<Mutex<HashMap<String, Arc<Mutex<Index>>>>>,
}
struct QueryLease {
    active: Active,
    key: (String, u64),
}
impl Drop for QueryLease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.key);
        }
    }
}
impl Handler {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            active: Arc::new(Mutex::new(HashMap::new())),
            indices: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn handle(&self, sid: &str, request: Request) -> Result<Value> {
        if !valid_sid(sid) {
            return Err("invalid client SID".into());
        }
        match request {
            Request::Status {} => Ok(
                json!({"state":"ready","version":env!("CARGO_PKG_VERSION"),"sid":sid,"index_available":self.root.join(sid).is_dir()}),
            ),
            Request::Cancel { id } => {
                let active = self.active.lock().map_err(|_| "query registry poisoned")?;
                let found = active.get(&(sid.to_owned(), id));
                if let Some(flag) = found {
                    flag.store(true, Ordering::Relaxed);
                }
                Ok(json!({"id":id,"cancelled":found.is_some()}))
            }
            Request::Query { id, text, top } => {
                if !(1..=1000).contains(&top) {
                    return Err("top must be 1..1000".into());
                }
                let key = (sid.to_owned(), id);
                let flag = Arc::new(AtomicBool::new(false));
                {
                    let mut active = self.active.lock().map_err(|_| "query registry poisoned")?;
                    if active.contains_key(&key) {
                        return Err("query id already active".into());
                    }
                    if active.len() >= 64 {
                        return Err("too many active queries".into());
                    }
                    active.insert(key.clone(), flag.clone());
                }
                let _lease = QueryLease {
                    active: self.active.clone(),
                    key,
                };
                let dir = self.root.join(sid);
                if !dir.is_dir() {
                    return Err("index unavailable".into());
                }
                let view = {
                    let mut indices = self.indices.lock().map_err(|_| "index registry poisoned")?;
                    if let Some(index) = indices.get(sid) {
                        index.clone()
                    } else {
                        if indices.len() >= 64 {
                            return Err("too many user indices".into());
                        }
                        let index = Arc::new(Mutex::new(Index::open(&dir)?));
                        indices.insert(sid.to_owned(), index.clone());
                        index
                    }
                };
                let mut index = view.lock().map_err(|_| "user index poisoned")?;
                index.reload()?;
                let result = index.search(
                    &text,
                    &SearchOptions {
                        top,
                        cancel: Some(flag),
                        ..Default::default()
                    },
                )?;
                let hits:Vec<_>=result.hits.iter().map(|h| json!({"doc_id":h.doc_id,"count":h.count,"level":h.level,"adjacent":h.adjacent,"sentences":h.sentences.iter().map(|s|json!({"ord":s.ord,"para":s.para,"level":s.level,"adjacent":s.adjacent})).collect::<Vec<_>>()})).collect();
                Ok(
                    json!({"id":id,"generation":result.generation,"total_docs":result.total_docs,"total_sentences":result.total_sentences,"hits":hits}),
                )
            }
        }
    }
}
fn valid_sid(sid: &str) -> bool {
    sid.starts_with("S-1-")
        && sid.len() < 185
        && sid[4..]
            .split('-')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}
pub async fn serve_client(mut pipe: NamedPipeServer, handler: Handler) -> Result<()> {
    let mut pid = 0;
    if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle().cast(), &mut pid) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let sid = process_sid(pid)?;
    let operation = async {
        let size = pipe.read_u32_le().await? as usize;
        if size > MAX_REQUEST {
            return Err("request exceeds 64KB".into());
        }
        let mut bytes = vec![0; size];
        pipe.read_exact(&mut bytes).await?;
        let request = decode(&bytes)?;
        let response = tokio::task::spawn_blocking(move || {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler.handle(&sid, request)
            })) {
                Ok(Ok(value)) => json!({"ok":true,"result":value}),
                Ok(Err(e)) => json!({"ok":false,"error":e.to_string()}),
                Err(_) => json!({"ok":false,"error":"query panic isolated"}),
            }
        })
        .await?;
        let bytes = serde_json::to_vec(&response)?;
        pipe.write_u32_le(bytes.len() as u32).await?;
        pipe.write_all(&bytes).await?;
        Ok(())
    };
    tokio::time::timeout(Duration::from_secs(10), operation).await?
}
pub async fn run(
    name: &str,
    root: PathBuf,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let pipe = create(name, true)?;
    run_bound(pipe, name, root, stop).await
}
pub async fn run_bound(
    mut pipe: NamedPipeServer,
    name: &str,
    root: PathBuf,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let handler = Handler::new(root);
    let permits = Arc::new(tokio::sync::Semaphore::new(32));
    let mut clients = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _=stop.changed()=>break,
            result=pipe.connect()=> {
                result?; let next=create(name,false)?; let connected=std::mem::replace(&mut pipe,next);
                if let Ok(permit)=permits.clone().try_acquire_owned() { let handler=handler.clone(); clients.spawn(async move { let _permit=permit; let _=serve_client(connected,handler).await; }); }
            }
            Some(_)=clients.join_next(), if !clients.is_empty()=>{}
        }
    }
    clients.abort_all();
    while clients.join_next().await.is_some() {}
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_limits() {
        assert!(decode(&vec![b' '; MAX_REQUEST + 1]).is_err());
        let prefix = br#"{"command":"query","id":1,"text":""#;
        let suffix = br#""}"#;
        let mut boundary = prefix.to_vec();
        boundary.extend(vec![b'a'; MAX_REQUEST - prefix.len() - suffix.len()]);
        boundary.extend(suffix);
        assert_eq!(boundary.len(), MAX_REQUEST);
        assert!(decode(&boundary).is_ok());
        assert!(decode(br#"{"command":"status","sid":"S-1-5-18"}"#).is_err());
        assert!(decode(br#"{"command":"unknown"}"#).is_err());
        assert!(decode(br#"{"command":"status"}"#).is_ok());
    }
    #[test]
    fn rejects_path_in_sid() {
        for s in ["../x", "S-1-5/../../x", "S-1--5"] {
            assert!(!valid_sid(s));
        }
        assert!(valid_sid("S-1-5-21-123"));
    }
    #[test]
    fn cancellation_is_sid_scoped() {
        let h = Handler::new(PathBuf::new());
        let flag = Arc::new(AtomicBool::new(false));
        h.active
            .lock()
            .unwrap()
            .insert(("S-1-5-18".into(), 1), flag.clone());
        h.handle("S-1-5-19", Request::Cancel { id: 1 }).unwrap();
        assert!(!flag.load(Ordering::Relaxed));
        h.handle("S-1-5-18", Request::Cancel { id: 1 }).unwrap();
        assert!(flag.load(Ordering::Relaxed));
    }
    #[test]
    fn missing_index_does_not_create_files() {
        let d = tempfile::tempdir().unwrap();
        let h = Handler::new(d.path().to_owned());
        assert!(
            h.handle(
                "S-1-5-18",
                Request::Query {
                    id: 1,
                    text: "예산".into(),
                    top: 20
                }
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
        assert!(h.active.lock().unwrap().is_empty());
    }
    #[test]
    fn query_reloads_new_generation() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("S-1-5-18");
        let mut w = kfind_core::Writer::create(&dir, Default::default()).unwrap();
        let sentences = [kfind_core::Sentence {
            para: 0,
            level: 0,
            text: "예산 집행",
        }];
        w.add_document(1, 0, &sentences).unwrap();
        w.commit().unwrap();
        let h = Handler::new(d.path().to_owned());
        let query = || Request::Query {
            id: 1,
            text: "예산".into(),
            top: 20,
        };
        assert_eq!(h.handle("S-1-5-18", query()).unwrap()["total_docs"], 1);
        w.add_document(2, 0, &sentences).unwrap();
        w.commit().unwrap();
        assert_eq!(h.handle("S-1-5-18", query()).unwrap()["total_docs"], 2);
        assert!(h.handle("S-1-5-19", query()).is_err());
    }
    #[tokio::test]
    async fn oversized_pipe_frame_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        let name = format!(r"\\.\pipe\kfind-oversize-{}", std::process::id());
        let server = create(&name, true).unwrap();
        let h = Handler::new(d.path().to_owned());
        let task = tokio::spawn(async move {
            server.connect().await.unwrap();
            serve_client(server, h).await
        });
        let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(&name)
            .unwrap();
        client.write_u32_le((MAX_REQUEST + 1) as u32).await.unwrap();
        assert!(task.await.unwrap().is_err());
    }
    #[tokio::test]
    async fn accept_loop_stops_without_a_client() {
        let d = tempfile::tempdir().unwrap();
        let name = format!(r"\\.\pipe\kfind-stop-{}", std::process::id());
        let server = create(&name, true).unwrap();
        let (send, recv) = tokio::sync::watch::channel(false);
        send.send(true).unwrap();
        tokio::time::timeout(
            Duration::from_secs(1),
            run_bound(server, &name, d.path().to_owned(), recv),
        )
        .await
        .unwrap()
        .unwrap();
    }
    #[tokio::test]
    async fn real_pipe_query_status_cancel() {
        let d = tempfile::tempdir().unwrap();
        let sid = current_sid().unwrap();
        let dir = d.path().join(&sid);
        let mut w = kfind_core::Writer::create(&dir, Default::default()).unwrap();
        w.add_document(
            42,
            0,
            &[kfind_core::Sentence {
                para: 1,
                level: 0,
                text: "예산 집행 계획",
            }],
        )
        .unwrap();
        w.commit().unwrap();
        drop(w);
        let name = format!(r"\\.\pipe\kfind-test-{}-{}", std::process::id(), sid);
        let handler = Handler::new(d.path().to_owned());
        for req in [
            json!({"command":"query","id":1,"text":"예산","top":20}),
            json!({"command":"status"}),
            json!({"command":"cancel","id":1}),
        ] {
            let server = create(&name, true).unwrap();
            let h = handler.clone();
            let task = tokio::spawn(async move {
                server.connect().await.unwrap();
                serve_client(server, h).await.unwrap();
            });
            let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
                .open(&name)
                .unwrap();
            let bytes = serde_json::to_vec(&req).unwrap();
            client.write_u32_le(bytes.len() as u32).await.unwrap();
            client.write_all(&bytes).await.unwrap();
            let n = client.read_u32_le().await.unwrap();
            let mut bytes = vec![0; n as usize];
            client.read_exact(&mut bytes).await.unwrap();
            let response: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(response["ok"], true);
            if req["command"] == "query" {
                assert_eq!(response["result"]["hits"][0]["doc_id"], 42);
            }
            task.await.unwrap();
        }
    }
}
