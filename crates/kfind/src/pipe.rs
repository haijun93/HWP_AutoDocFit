use crate::Result;
use crate::doctable::{
    DocMeta, DocTable, Filters, LevelSrc, check_replace, scope_contains, valid_scope,
};
use crate::ingest::{Accepted, Para, Uploads, merge_paras, sentences, valid_doc_key};
use kfind_core::{Index, MergePolicy, RoaringTreemap, SearchOptions, SplitMode, Writer};
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
    /// 간단 형(시험·내부용, 나머지는 기본값). 파이프에서 온 `query`는 `Find`로 읽는다.
    #[serde(skip)]
    Query {
        id: u64,
        text: String,
        top: usize,
    },
    #[serde(rename = "query")]
    Find(QueryArgs),
    Count {
        id: u64,
        text: String,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        sources: Option<Vec<String>>,
        #[serde(default)]
        split: Option<String>,
        #[serde(default)]
        filters: Filters,
    },
    Cancel {
        id: u64,
    },
    Status {},
    Ingest {
        doc_key: String,
        path: String,
        size: u64,
        mtime: i64,
        date: i64,
        #[serde(default)]
        ref_date: Option<String>,
        scopes: Vec<String>,
        level_src: LevelSrc,
        level_rule: u32,
        paras: Vec<Para>,
        #[serde(default)]
        part: Option<u32>,
        #[serde(default)]
        parts: Option<u32>,
    },
    Remove {
        doc_keys: Vec<String>,
    },
    Stale {
        #[serde(default)]
        scope: Option<String>,
        level_rule: u32,
        /// 이 열쇠 다음부터(앞 응답의 마지막 열쇠).
        #[serde(default)]
        after: Option<String>,
        #[serde(default = "default_stale_limit")]
        limit: usize,
    },
    /// FN1: 드라이브 전체 파일명 검색.
    NamesQuery {
        id: u64,
        text: String,
        #[serde(default = "default_names_limit")]
        limit: usize,
        /// `relevance`(기본)·`name`·`path`.
        #[serde(default)]
        sort: Option<String>,
        /// 숨김·시스템 항목 포함(기본 참).
        #[serde(default = "default_true")]
        hidden: bool,
    },
    /// FN1: 파일명 색인 (다시) 만들기. 서비스 모드는 roots 없이(고정 드라이브 전체), 사용자 모드는 roots 필수.
    NamesBuild {
        #[serde(default)]
        roots: Option<Vec<String>>,
    },
    NamesStatus {},
}
/// `query` 요청.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct QueryArgs {
    pub id: u64,
    pub text: String,
    #[serde(default = "default_top")]
    pub top: usize,
    /// 문서 범위(`tm`, `box:<id>`, `nb:<id>`). 없으면 범위 거르기 없음.
    #[serde(default)]
    pub scope: Option<String>,
    /// `nb:<id>` 범위의 출처(`nb:<id>:src:<sid>`만 허용).
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    /// 문서마다 돌려줄 일치 문장 수(0..=100).
    #[serde(default = "default_refs")]
    pub refs: usize,
    /// 붙여 쓴 검색어 나누기: `dict`(기본)·`bigram`·`off`.
    #[serde(default)]
    pub split: Option<String>,
    #[serde(default)]
    pub filters: Filters,
}
fn default_top() -> usize {
    20
}
fn default_refs() -> usize {
    3
}
fn default_stale_limit() -> usize {
    1000
}
fn default_names_limit() -> usize {
    100
}
fn default_true() -> bool {
    true
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
/// 사용자 하나의 쓰기 쪽: 문서 표와 (처음 쓸 때 여는) 색인 Writer. 질의는 Writer를 열지 않는다
/// (다른 쓰기 쪽이 잡고 있어도 질의는 된다).
struct Store {
    table: DocTable,
    writer: Option<Writer>,
}
#[derive(Clone)]
pub struct Handler {
    root: PathBuf,
    active: Active,
    indices: Arc<Mutex<HashMap<String, Arc<Mutex<Index>>>>>,
    stores: Arc<Mutex<HashMap<String, Arc<Mutex<Store>>>>>,
    uploads: Arc<Mutex<Uploads>>,
    mode: &'static str,
    last_error: Arc<Mutex<Option<String>>>,
    names: Arc<crate::names::NamesService>,
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
fn split_mode(s: Option<&str>) -> Result<SplitMode> {
    Ok(match s.unwrap_or("dict") {
        "dict" => SplitMode::DictLongest,
        "bigram" => SplitMode::Bigram,
        "off" => SplitMode::Off,
        other => return Err(format!("unknown split mode {other}").into()),
    })
}
/// 범위·출처 → 문서 표에서 찾을 범위 글들. 둘 다 없으면 None(범위 거르기 없음).
fn scope_wants(scope: Option<&str>, sources: Option<&[String]>) -> Result<Option<Vec<String>>> {
    let wants = match (scope, sources) {
        (None, None) => return Ok(None),
        (None, Some(_)) => return Err("sources need a scope".into()),
        (Some(s), None) => vec![s.to_owned()],
        (Some(s), Some(src)) => src.iter().map(|x| format!("{s}:src:{x}")).collect(),
    };
    if wants.len() > 1000 || !wants.iter().all(|w| valid_scope(w)) {
        return Err("invalid scope".into());
    }
    Ok(Some(wants))
}
fn in_scope(wants: Option<&[String]>, scopes: &[String]) -> bool {
    wants.is_none_or(|w| {
        w.iter()
            .any(|w| scopes.iter().any(|d| scope_contains(w, d)))
    })
}
/// 문서 표로 허용 문서 집합을 만든다. 범위도 표 거르개도 없으면 None(전부 허용).
fn allow_set(
    table: &DocTable,
    wants: Option<&[String]>,
    filters: &Filters,
) -> Option<RoaringTreemap> {
    if wants.is_none() && !filters.uses_table() {
        return None;
    }
    let mut allow = RoaringTreemap::new();
    for (_, e) in table.iter() {
        if in_scope(wants, &e.meta.scopes) && filters.admits(e) {
            allow.insert(e.id);
        }
    }
    Some(allow)
}
fn check_filters(filters: &Filters) -> Result<()> {
    if filters.exclude.len() + filters.phrases.len() + filters.ext.len() + filters.path.len() > 64 {
        return Err("too many filters".into());
    }
    Ok(())
}
fn words_json(words: &[kfind_core::QueryWord]) -> Value {
    Value::Array(
        words
            .iter()
            .map(|w| json!({"text":w.text,"pieces":w.pieces}))
            .collect(),
    )
}
impl Handler {
    pub fn new(root: PathBuf) -> Self {
        Self::with_mode(root, "user")
    }
    /// `mode`는 `status`에 그대로 싣는다(`service`·`user`).
    pub fn with_mode(root: PathBuf, mode: &'static str) -> Self {
        Self {
            root,
            active: Arc::new(Mutex::new(HashMap::new())),
            indices: Arc::new(Mutex::new(HashMap::new())),
            stores: Arc::new(Mutex::new(HashMap::new())),
            uploads: Arc::new(Mutex::new(Uploads::default())),
            mode,
            last_error: Arc::new(Mutex::new(None)),
            names: crate::names::NamesService::new(mode),
        }
    }
    /// FN1 파일명 색인(서비스 시작 때 만들기·멈출 때 USN 따라가기 정리에 쓴다).
    pub fn names(&self) -> &Arc<crate::names::NamesService> {
        &self.names
    }
    fn note_error(&self, e: &str) {
        if let Ok(mut last) = self.last_error.lock() {
            *last = Some(e.to_owned());
        }
    }
    /// 열려 있는 사용자 색인(읽기). 폴더가 없으면 오류(파일을 만들지 않는다).
    fn index(&self, sid: &str) -> Result<Arc<Mutex<Index>>> {
        let dir = self.root.join(sid);
        if !dir.is_dir() {
            return Err("index unavailable".into());
        }
        let mut indices = self.indices.lock().map_err(|_| "index registry poisoned")?;
        if let Some(index) = indices.get(sid) {
            return Ok(index.clone());
        }
        if indices.len() >= 64 {
            return Err("too many user indices".into());
        }
        let index = Arc::new(Mutex::new(Index::open(&dir)?));
        indices.insert(sid.to_owned(), index.clone());
        Ok(index)
    }
    /// 사용자 문서 표. `create`가 거짓이면 폴더가 없을 때 None(파일을 만들지 않는다).
    fn store(&self, sid: &str, create: bool) -> Result<Option<Arc<Mutex<Store>>>> {
        let dir = self.root.join(sid);
        let mut stores = self.stores.lock().map_err(|_| "store registry poisoned")?;
        if let Some(s) = stores.get(sid) {
            return Ok(Some(s.clone()));
        }
        if !dir.is_dir() {
            if !create {
                return Ok(None);
            }
            std::fs::create_dir_all(&dir)?;
        }
        if stores.len() >= 64 {
            return Err("too many user indices".into());
        }
        let s = Arc::new(Mutex::new(Store {
            table: DocTable::load(&dir)?,
            writer: None,
        }));
        stores.insert(sid.to_owned(), s.clone());
        Ok(Some(s))
    }
    /// 쓰기 쪽을 연다. 형식·규칙 판이 다르면 색인과 문서 표를 비우고 새로 만든다(앱이 다시 보낸다).
    fn writer<'a>(&self, sid: &str, store: &'a mut Store) -> Result<&'a mut Writer> {
        if store.writer.is_none() {
            let dir = self.root.join(sid);
            let w = match Writer::open(&dir, Default::default()) {
                Ok(w) => w,
                Err(
                    e @ (kfind_core::Error::FormatVersion { .. }
                    | kfind_core::Error::RuleVersion { .. }),
                ) => {
                    self.note_error(&format!("rebuilt index: {e}"));
                    let w = Writer::create(&dir, Default::default())?;
                    store.table.clear()?;
                    w
                }
                Err(e) => return Err(e.into()),
            };
            store.writer = Some(w);
        }
        Ok(store.writer.as_mut().expect("writer was just opened"))
    }
    /// 질의 등록(취소용). 같은 SID·id가 진행 중이면 거절.
    fn lease(&self, sid: &str, id: u64) -> Result<(QueryLease, Arc<AtomicBool>)> {
        let key = (sid.to_owned(), id);
        let flag = Arc::new(AtomicBool::new(false));
        let mut active = self.active.lock().map_err(|_| "query registry poisoned")?;
        if active.contains_key(&key) {
            return Err("query id already active".into());
        }
        if active.len() >= 64 {
            return Err("too many active queries".into());
        }
        active.insert(key.clone(), flag.clone());
        Ok((
            QueryLease {
                active: self.active.clone(),
                key,
            },
            flag,
        ))
    }
    /// 질의 옵션: 나누기·구문·범위(문서 표)·제외어(엔진 문서 집합).
    #[allow(clippy::too_many_arguments)]
    fn options(
        &self,
        sid: &str,
        index: &Index,
        scope: Option<&str>,
        sources: Option<&[String]>,
        split: Option<&str>,
        filters: &Filters,
        flag: Arc<AtomicBool>,
    ) -> Result<SearchOptions> {
        check_filters(filters)?;
        let wants = scope_wants(scope, sources)?;
        let allow = if wants.is_some() || filters.uses_table() {
            match self.store(sid, false)? {
                Some(s) => allow_set(
                    &s.lock().map_err(|_| "store poisoned")?.table,
                    wants.as_deref(),
                    filters,
                ),
                None => Some(RoaringTreemap::new()),
            }
        } else {
            None
        };
        let mut deny = RoaringTreemap::new();
        for w in &filters.exclude {
            deny |= index.doc_set(w)?;
        }
        Ok(SearchOptions {
            split: split_mode(split)?,
            cancel: Some(flag),
            allow: allow.map(Arc::new),
            deny: (!deny.is_empty()).then(|| Arc::new(deny)),
            phrases: filters.phrases.clone(),
            ..Default::default()
        })
    }
    pub fn handle(&self, sid: &str, request: Request) -> Result<Value> {
        if !valid_sid(sid) {
            return Err("invalid client SID".into());
        }
        match request {
            Request::Status {} => self.status(sid),
            Request::Cancel { id } => {
                let active = self.active.lock().map_err(|_| "query registry poisoned")?;
                let found = active.get(&(sid.to_owned(), id));
                if let Some(flag) = found {
                    flag.store(true, Ordering::Relaxed);
                }
                Ok(json!({"id":id,"cancelled":found.is_some()}))
            }
            Request::Query { id, text, top } => self.query(
                sid,
                id,
                &text,
                top,
                None,
                None,
                default_refs(),
                None,
                &Filters::default(),
            ),
            Request::Find(q) => self.query(
                sid,
                q.id,
                &q.text,
                q.top,
                q.scope.as_deref(),
                q.sources.as_deref(),
                q.refs,
                q.split.as_deref(),
                &q.filters,
            ),
            Request::Count {
                id,
                text,
                scope,
                sources,
                split,
                filters,
            } => {
                let started = std::time::Instant::now();
                let (_lease, flag) = self.lease(sid, id)?;
                let view = self.index(sid)?;
                let mut index = view.lock().map_err(|_| "user index poisoned")?;
                index.reload()?;
                let opts = self.options(
                    sid,
                    &index,
                    scope.as_deref(),
                    sources.as_deref(),
                    split.as_deref(),
                    &filters,
                    flag,
                )?;
                let c = index.count(&text, &opts)?;
                Ok(
                    json!({"id":id,"generation":c.generation,"total_docs":c.total_docs,"total_sentences":c.total_sentences,"words":words_json(&c.words),"dropped":c.dropped,"ms":started.elapsed().as_secs_f64()*1000.0}),
                )
            }
            Request::Ingest {
                doc_key,
                path,
                size,
                mtime,
                date,
                ref_date,
                scopes,
                level_src,
                level_rule,
                paras,
                part,
                parts,
            } => {
                let meta = DocMeta {
                    path,
                    size,
                    mtime,
                    date,
                    ref_date,
                    scopes,
                    level_src,
                    level_rule,
                };
                self.ingest(sid, doc_key, meta, paras, part, parts)
            }
            Request::Remove { doc_keys } => self.remove(sid, &doc_keys),
            Request::Stale {
                scope,
                level_rule,
                after,
                limit,
            } => self.stale(sid, scope.as_deref(), level_rule, after.as_deref(), limit),
            Request::NamesQuery {
                id,
                text,
                limit,
                sort,
                hidden,
            } => {
                let (_lease, flag) = self.lease(sid, id)?;
                let mut r = self
                    .names
                    .query(sid, &text, limit, sort.as_deref(), hidden, flag)?;
                r["id"] = json!(id);
                Ok(r)
            }
            Request::NamesBuild { roots } => self.names.build(roots),
            Request::NamesStatus {} => self.names.status(),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn query(
        &self,
        sid: &str,
        id: u64,
        text: &str,
        top: usize,
        scope: Option<&str>,
        sources: Option<&[String]>,
        refs: usize,
        split: Option<&str>,
        filters: &Filters,
    ) -> Result<Value> {
        if !(1..=1000).contains(&top) {
            return Err("top must be 1..1000".into());
        }
        if refs > 100 {
            return Err("refs must be 0..100".into());
        }
        let started = std::time::Instant::now();
        let (_lease, flag) = self.lease(sid, id)?;
        let view = self.index(sid)?;
        let mut index = view.lock().map_err(|_| "user index poisoned")?;
        index.reload()?;
        let mut opts = self.options(sid, &index, scope, sources, split, filters, flag)?;
        opts.top = top;
        opts.refs_per_doc = refs;
        let result = index.search(text, &opts)?;
        drop(index);
        let store = self.store(sid, false)?;
        let store = match &store {
            Some(s) => Some(s.lock().map_err(|_| "store poisoned")?),
            None => None,
        };
        let hits: Vec<_> = result
            .hits
            .iter()
            .map(|h| {
                // 문서 표에 없는 문서(표 밖에서 넣었거나 커밋과 표 기록 사이에 꺼진 경우)는 doc_key·path·위치가 null.
                let entry = store.as_ref().and_then(|s| s.table.entry_of(h.doc_id));
                let sentences: Vec<_> = h
                    .sentences
                    .iter()
                    .map(|s| {
                        let span = entry.and_then(|(_, e)| e.span(s.ord)).filter(|sp| sp.0 == s.para);
                        json!({"ord":s.ord,"para":s.para,"level":s.level,"adjacent":s.adjacent,"start":span.map(|x| x.1),"len":span.map(|x| x.2)})
                    })
                    .collect();
                json!({"doc_id":h.doc_id,"doc_key":entry.map(|e| e.0),"path":entry.map(|e| e.1.meta.path.as_str()),"count":h.count,"level":h.level,"adjacent":h.adjacent,"date":h.date,"sentences":sentences})
            })
            .collect();
        Ok(json!({
            "id":id,"generation":result.generation,"total_docs":result.total_docs,"total_sentences":result.total_sentences,
            "adjacent_docs":result.adjacent_docs,"words":words_json(&result.words),"dropped":result.dropped,"hits":hits,
            "ms":started.elapsed().as_secs_f64()*1000.0
        }))
    }
    fn ingest(
        &self,
        sid: &str,
        doc_key: String,
        meta: DocMeta,
        paras: Vec<Para>,
        part: Option<u32>,
        parts: Option<u32>,
    ) -> Result<Value> {
        if !valid_doc_key(&doc_key) {
            return Err("invalid doc_key".into());
        }
        if meta.scopes.len() > 64 || !meta.scopes.iter().all(|s| valid_scope(s)) {
            return Err("invalid scopes".into());
        }
        let store = self.store(sid, true)?.ok_or("store unavailable")?;
        // 위계 출처 전이는 첫 조각에서도 확인한다(거절될 문서의 조각을 모으지 않는다).
        if let Err(e) = check_replace(
            store
                .lock()
                .map_err(|_| "store poisoned")?
                .table
                .get(&doc_key)
                .map(|e| &e.meta),
            &meta,
        ) {
            return Err(format!("rejected: {e}").into());
        }
        let accepted = self
            .uploads
            .lock()
            .map_err(|_| "upload registry poisoned")?
            .accept(
                sid,
                &doc_key,
                meta,
                paras,
                part,
                parts,
                std::time::Instant::now(),
            )?;
        let (meta, paras) = match accepted {
            Accepted::Waiting { received, parts } => {
                return Ok(
                    json!({"doc_key":doc_key,"indexed":false,"received":received,"parts":parts}),
                );
            }
            Accepted::Complete { meta, paras } => (meta, paras),
        };
        let paras = merge_paras(paras)?;
        let (sents, spans) = sentences(&paras);
        let mut guard = store.lock().map_err(|_| "store poisoned")?;
        let s = &mut *guard;
        if let Err(e) = check_replace(s.table.get(&doc_key).map(|e| &e.meta), &meta) {
            return Err(format!("rejected: {e}").into());
        }
        let id = s.table.id_for(&doc_key);
        let n = sents.len();
        let generation = self.write(sid, s, |w| w.add_document(id, meta.date, &sents))?;
        s.table.put(&doc_key, id, meta, spans)?;
        Ok(
            json!({"doc_key":doc_key,"indexed":true,"doc_id":id,"sentences":n,"generation":generation}),
        )
    }
    fn remove(&self, sid: &str, doc_keys: &[String]) -> Result<Value> {
        if doc_keys.len() > 1000 {
            return Err("too many doc_keys (max 1000)".into());
        }
        let Some(store) = self.store(sid, false)? else {
            return Ok(json!({"removed":0,"missing":doc_keys.len()}));
        };
        let mut guard = store.lock().map_err(|_| "store poisoned")?;
        let s = &mut *guard;
        let mut ids: Vec<(&str, u64)> = doc_keys
            .iter()
            .filter_map(|k| s.table.get(k).map(|e| (k.as_str(), e.id)))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Ok(json!({"removed":0,"missing":doc_keys.len()}));
        }
        let generation = self.write(sid, s, |w| {
            ids.iter()
                .try_for_each(|(_, id)| w.delete_document(*id).map(|_| ()))
        })?;
        for (k, _) in &ids {
            s.table.remove(k)?;
        }
        Ok(json!({"removed":ids.len(),"missing":doc_keys.len()-ids.len(),"generation":generation}))
    }
    /// 위계를 다시 보내야 하는 문서: 범위 안이고 출처가 `py`가 아니거나 규칙 판이 `level_rule`보다 낮은 것(열쇠 순서).
    fn stale(
        &self,
        sid: &str,
        scope: Option<&str>,
        level_rule: u32,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Value> {
        if !(1..=10000).contains(&limit) {
            return Err("limit must be 1..10000".into());
        }
        let wants = scope_wants(scope, None)?;
        let Some(store) = self.store(sid, false)? else {
            return Ok(json!({"doc_keys":[],"more":false}));
        };
        let s = store.lock().map_err(|_| "store poisoned")?;
        let mut keys = Vec::new();
        let mut more = false;
        for (k, e) in s.table.iter_after(after) {
            if in_scope(wants.as_deref(), &e.meta.scopes)
                && (e.meta.level_src != LevelSrc::Py || e.meta.level_rule < level_rule)
            {
                if keys.len() == limit {
                    more = true;
                    break;
                }
                keys.push(k.to_owned());
            }
        }
        Ok(json!({"doc_keys":keys,"more":more}))
    }
    /// 쓰기 한 번: 바꾸고 커밋하고(조각이 많으면 합치기) 새 세대를 돌려준다. 실패하면 Writer를 버린다
    /// (디스크의 이전 세대는 그대로이고 다음 쓰기에서 다시 연다).
    fn write(
        &self,
        sid: &str,
        store: &mut Store,
        change: impl FnOnce(&mut Writer) -> kfind_core::Result<()>,
    ) -> Result<u64> {
        let result = (|| -> Result<u64> {
            let w = self.writer(sid, store)?;
            change(w)?;
            w.commit()?;
            if w.num_segments() > MergePolicy::default().max_segments {
                w.merge(&MergePolicy::default())?;
            }
            Ok(w.generation())
        })();
        if let Err(e) = &result {
            store.writer = None;
            self.note_error(&e.to_string());
        }
        result
    }
    fn status(&self, sid: &str) -> Result<Value> {
        let available = self.root.join(sid).is_dir();
        let (mut generation, mut sentences) = (0u64, 0u64);
        if available {
            let view = self.index(sid)?;
            let mut index = view.lock().map_err(|_| "user index poisoned")?;
            index.reload()?;
            (generation, sentences) = (index.generation(), index.num_sentences());
        }
        let (docs, counts) = match self.store(sid, false)? {
            Some(s) => {
                let s = s.lock().map_err(|_| "store poisoned")?;
                (s.table.len(), json!(s.table.level_rule_counts()))
            }
            None => (0, json!({})),
        };
        let last_error = self
            .last_error
            .lock()
            .map_err(|_| "status poisoned")?
            .clone();
        let uploads = self
            .uploads
            .lock()
            .map_err(|_| "upload registry poisoned")?
            .len();
        Ok(json!({
            "state":"ready","version":env!("CARGO_PKG_VERSION"),"sid":sid,"index_available":available,
            "mode":self.mode,"generation":generation,"docs":docs,"sentences":sentences,
            "format":kfind_core::FORMAT_VERSION,"fold_rule":kfind_core::FOLD_RULE_VERSION,"sentence_rule":kfind_core::SENTENCE_RULE_VERSION,
            "level_rule_counts":counts,"uploads":uploads,"last_error":last_error
        }))
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
    pipe: NamedPipeServer,
    name: &str,
    root: PathBuf,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    run_bound_mode(pipe, name, root, stop, "user").await
}
/// `mode`(`service`·`user`)는 `status` 응답에 실린다.
pub async fn run_bound_mode(
    mut pipe: NamedPipeServer,
    name: &str,
    root: PathBuf,
    mut stop: tokio::sync::watch::Receiver<bool>,
    mode: &'static str,
) -> Result<()> {
    let handler = Handler::with_mode(root, mode);
    if mode == "service" {
        // 서비스는 시작하자마자 드라이브 전체 이름 색인을 만든다(그동안 names_query는 상태를 알려 줌).
        if let Err(e) = handler.names().build(None) {
            handler.note_error(&e.to_string());
        }
    }
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
    handler.names().stop_followers();
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
    // ---- KFS1: ingest·remove·stale·count, 문서 표, query 응답의 path·문장 위치 ----
    const SID: &str = "S-1-5-21-7";
    fn req(v: Value) -> Request {
        decode(&serde_json::to_vec(&v).unwrap()).unwrap()
    }
    fn ingest(
        h: &Handler,
        key: &str,
        path: &str,
        scopes: &[&str],
        src: &str,
        mtime: i64,
        paras: Value,
    ) -> Result<Value> {
        h.handle(
            SID,
            req(json!({"command":"ingest","doc_key":key,"path":path,"size":10,"mtime":mtime,"date":mtime,"ref_date":"2026-11-13",
                "scopes":scopes,"level_src":src,"level_rule":4,"paras":paras})),
        )
    }
    fn query(h: &Handler, text: &str, extra: Value) -> Value {
        let mut v = json!({"command":"query","id":1,"text":text,"top":20});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        h.handle(SID, req(v)).unwrap()
    }
    fn count(h: &Handler, text: &str, extra: Value) -> Value {
        let mut v = json!({"command":"count","id":2,"text":text});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        h.handle(SID, req(v)).unwrap()
    }
    fn keys(r: &Value) -> Vec<String> {
        r["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["doc_key"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn ingest_then_query_returns_path_and_sentence_positions() {
        let d = tempfile::tempdir().unwrap();
        let h = Handler::new(d.path().to_owned());
        let r = ingest(
            &h,
            "k1",
            r"D:\자료\예산.hwp",
            &["tm"],
            "py",
            1,
            json!([
                {"p":0,"lv":0,"t":"※"},
                {"p":3,"lv":8,"t":"사업 개요. 예산을 집행한다.\n※ - -\n예산 집행 결과"}
            ]),
        )
        .unwrap();
        assert_eq!(r["indexed"], true);
        assert_eq!(r["sentences"], 3, "symbol-only sentences are not indexed");
        let q = query(&h, "예산 집행", json!({"refs":5}));
        assert_eq!(q["total_docs"], 1);
        let hit = &q["hits"][0];
        assert_eq!(hit["doc_key"], "k1");
        assert_eq!(hit["path"], r"D:\자료\예산.hwp");
        assert_eq!(hit["date"], 1);
        let text = "사업 개요. 예산을 집행한다.\n※ - -\n예산 집행 결과";
        let got: Vec<(u64, u64, String)> = hit["sentences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let (a, n) = (s["start"].as_u64().unwrap(), s["len"].as_u64().unwrap());
                (
                    s["ord"].as_u64().unwrap(),
                    s["para"].as_u64().unwrap(),
                    text.chars().skip(a as usize).take(n as usize).collect(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                (1, 3, "예산을 집행한다.".to_owned()),
                (2, 3, "예산 집행 결과".to_owned())
            ]
        );
        assert_eq!(q["words"][0]["text"], "예산");
        assert!(q["ms"].as_f64().unwrap() >= 0.0);
        // 다시 넣으면 같은 ID로 바뀐다.
        let r2 = ingest(
            &h,
            "k1",
            r"D:\자료\예산.hwp",
            &["tm"],
            "py",
            2,
            json!([{"p":0,"lv":0,"t":"결산 보고"}]),
        )
        .unwrap();
        assert_eq!(r2["doc_id"], r["doc_id"]);
        assert_eq!(query(&h, "예산", json!({}))["total_docs"], 0);
        assert_eq!(query(&h, "결산", json!({}))["total_docs"], 1);
    }

    #[test]
    fn scopes_sources_and_filters_limit_query_and_count_alike() {
        let d = tempfile::tempdir().unwrap();
        let h = Handler::new(d.path().to_owned());
        let p = |t: &str| json!([{"p":0,"lv":8,"t":t}]);
        ingest(
            &h,
            "a",
            r"C:\x\a.hwp",
            &["tm", "box:1"],
            "py",
            1,
            p("예산 집행 계획"),
        )
        .unwrap();
        ingest(
            &h,
            "b",
            r"C:\y\b.docx",
            &["tm"],
            "py",
            2,
            p("예산 집행 보고"),
        )
        .unwrap();
        ingest(&h, "m", "", &["nb:5:src:7"], "py", 3, p("예산 집행 메모")).unwrap();
        ingest(
            &h,
            "n",
            "",
            &["nb:50:src:7"],
            "py",
            4,
            p("예산 집행 다른 노트"),
        )
        .unwrap();
        let cases = [
            (json!({}), vec!["n", "m", "b", "a"]),
            (json!({"scope":"tm"}), vec!["b", "a"]),
            (json!({"scope":"box:1"}), vec!["a"]),
            (json!({"scope":"box:2"}), vec![]),
            (json!({"scope":"nb:5"}), vec!["m"]),
            (json!({"scope":"nb:5","sources":["7"]}), vec!["m"]),
            (json!({"scope":"nb:5","sources":["8"]}), vec![]),
            (json!({"scope":"tm","filters":{"ext":["docx"]}}), vec!["b"]),
            (json!({"filters":{"path":["\\x\\"]}}), vec!["a"]),
            (
                json!({"scope":"tm","filters":{"exclude":["보고"]}}),
                vec!["a"],
            ),
            (json!({"filters":{"phrases":["집행 계획"]}}), vec!["a"]),
            (json!({"filters":{"after":"2027"}}), vec![]),
        ];
        for (extra, want) in cases {
            let q = query(&h, "예산", extra.clone());
            assert_eq!(keys(&q), want, "{extra}");
            let c = count(&h, "예산", extra.clone());
            assert_eq!(c["total_docs"], want.len(), "{extra}");
            assert_eq!(c["total_sentences"], q["total_sentences"], "{extra}");
        }
        assert!(
            h.handle(
                SID,
                req(json!({"command":"query","id":1,"text":"예산","sources":["7"]}))
            )
            .is_err(),
            "sources without scope"
        );
        assert!(
            h.handle(
                SID,
                req(json!({"command":"query","id":1,"text":"예산","scope":"a b"}))
            )
            .is_err()
        );
        assert!(
            h.handle(
                SID,
                req(json!({"command":"query","id":1,"text":"예산","split":"x"}))
            )
            .is_err()
        );
        assert!(
            h.handle(
                SID,
                req(json!({"command":"query","id":1,"text":"예산","refs":101}))
            )
            .is_err()
        );
        assert!(decode(br#"{"command":"query","id":1,"text":"a","filters":{"near":2}}"#).is_err());
    }

    #[test]
    fn level_source_transitions_through_the_handler() {
        let d = tempfile::tempdir().unwrap();
        let h = Handler::new(d.path().to_owned());
        let p = json!([{"p":0,"lv":1,"t":"예산 집행"}]);
        ingest(&h, "k", "a.txt", &["tm"], "rs", 1, p.clone()).unwrap();
        ingest(&h, "k", "a.txt", &["tm"], "py", 1, p.clone()).unwrap();
        let e = ingest(&h, "k", "a.txt", &["tm"], "rs", 1, p.clone()).unwrap_err();
        assert!(e.to_string().starts_with("rejected:"), "{e}");
        // 거절된 쓰기는 색인·문서 표를 바꾸지 않는다.
        assert_eq!(
            h.handle(SID, Request::Status {}).unwrap()["level_rule_counts"],
            json!({"py4":1})
        );
        // 파일이 바뀌었으면(수정 시각이 다름) rs가 들어간다.
        ingest(&h, "k", "a.txt", &["tm"], "rs", 2, p).unwrap();
        assert_eq!(
            h.handle(SID, Request::Status {}).unwrap()["level_rule_counts"],
            json!({"rs":1})
        );
    }

    #[test]
    fn stale_pages_and_remove_hides_documents() {
        let d = tempfile::tempdir().unwrap();
        let h = Handler::new(d.path().to_owned());
        let p = json!([{"p":0,"lv":8,"t":"예산 집행"}]);
        for (k, src, scope) in [
            ("a", "py", "tm"),
            ("b", "rs", "tm"),
            ("c", "rs", "tm"),
            ("d", "rs", "box:1"),
            ("e", "py", "tm"),
        ] {
            ingest(&h, k, "", &[scope], src, 1, p.clone()).unwrap();
        }
        let stale = |extra: Value| {
            let mut v = json!({"command":"stale","level_rule":4});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            h.handle(SID, req(v)).unwrap()
        };
        assert_eq!(
            stale(json!({})),
            json!({"doc_keys":["b","c","d"],"more":false})
        );
        assert_eq!(
            stale(json!({"scope":"tm","limit":1})),
            json!({"doc_keys":["b"],"more":true})
        );
        assert_eq!(
            stale(json!({"scope":"tm","limit":1,"after":"b"})),
            json!({"doc_keys":["c"],"more":false})
        );
        // 규칙 판이 올라가면 py 문서도 낡았다.
        assert_eq!(
            stale(json!({"scope":"tm","level_rule":5}))["doc_keys"],
            json!(["a", "b", "c", "e"])
        );
        let r = h
            .handle(
                SID,
                req(json!({"command":"remove","doc_keys":["a","b","zz","a"]})),
            )
            .unwrap();
        assert_eq!(
            (r["removed"].as_u64(), r["missing"].as_u64()),
            (Some(2), Some(2))
        );
        assert_eq!(keys(&query(&h, "예산", json!({}))), ["c", "d", "e"]);
        assert_eq!(stale(json!({}))["doc_keys"], json!(["c", "d"]));
        let s = h.handle(SID, Request::Status {}).unwrap();
        assert_eq!(
            (s["docs"].as_u64(), s["sentences"].as_u64()),
            (Some(3), Some(3))
        );
        // 새 Handler(서비스 다시 시작)도 같은 표를 읽는다.
        drop(h);
        let h2 = Handler::new(d.path().to_owned());
        assert_eq!(keys(&query(&h2, "예산", json!({"scope":"tm"}))), ["c", "e"]);
    }

    #[test]
    fn parts_are_assembled_across_requests() {
        let d = tempfile::tempdir().unwrap();
        let h = Handler::new(d.path().to_owned());
        let part = |i: u32, paras: Value| {
            h.handle(
                SID,
                req(json!({"command":"ingest","doc_key":"big","path":"big.txt","size":9,"mtime":1,"date":1,"scopes":["tm"],
                    "level_src":"py","level_rule":4,"paras":paras,"part":i,"parts":3})),
            )
        };
        assert_eq!(
            part(0, json!([{"p":0,"lv":8,"t":"예산 집"}])).unwrap()["received"],
            1
        );
        assert_eq!(
            query(&h, "예산", json!({}))["total_docs"],
            0,
            "not visible until the last part"
        );
        assert_eq!(
            part(
                1,
                json!([{"p":0,"lv":8,"t":"행 계획"}, {"p":1,"lv":8,"t":"둘째"}])
            )
            .unwrap()["indexed"],
            false
        );
        assert_eq!(h.handle(SID, Request::Status {}).unwrap()["uploads"], 1);
        let r = part(2, json!([{"p":2,"lv":8,"t":"셋째 문단"}])).unwrap();
        assert_eq!(
            (r["indexed"].as_bool(), r["sentences"].as_u64()),
            (Some(true), Some(3))
        );
        let q = query(&h, "예산집행", json!({}));
        assert_eq!(q["hits"][0]["sentences"][0]["start"], 0);
        assert_eq!(q["hits"][0]["sentences"][0]["len"], 8);
        assert!(part(2, json!([])).is_err(), "part without part 0");
    }
}
