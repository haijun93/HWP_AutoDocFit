//! FN1: 드라이브 전체 파일명 색인(서비스 쪽). `kfind-names` 엔진을 들고 파이프 명령
//! `names_build`·`names_query`·`names_status`에 답한다.
//!
//! 상태 기계(한곳에서 읽힘: `names_status.state`):
//! - `empty` → (`names_build` 또는 서비스 시작) → `building` → `ready` | `failed`
//! - `ready` → (`names_build`) → `building`(그동안 옛 색인으로 계속 답함) → `ready` | `failed`(옛 색인 유지)
//! - `ready`에서 USN 따라가기가 저널 잘림을 만나면 `last_error`에 적고 그 볼륨 따라가기만 멈춘다(다시 `names_build`).
//! - 불가능한 전이: `building` 중 `names_build`(거절), `empty`에서 `names_query`(오류 `names index unavailable`).
//!
//! 서비스 모드(SYSTEM): 고정 드라이브마다 NTFS면 MFT 열거 + USN 따라가기, 아니면(또는 볼륨을 못 열면) 디렉터리 순회.
//! 결과에서 다른 사용자 프로필·`$Recycle.Bin`·`System Volume Information`을 뺀다(TM1 §1-5, IS1 Q9).
//! 사용자 모드: `roots`(조사 폴더)만 디렉터리 순회, 제외 없음(그 사용자가 열 수 있는 것만 보임).
use crate::Result;
use kfind_names::query::hits;
use kfind_names::{Builder, Names, Options, Searcher, Sort, parse};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

pub const MAX_LIMIT: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Empty,
    Building,
    Ready,
    Failed(String),
}

impl State {
    fn name(&self) -> &'static str {
        match self {
            State::Empty => "empty",
            State::Building => "building",
            State::Ready => "ready",
            State::Failed(_) => "failed",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct VolInfo {
    name: String,
    source: &'static str,
    entries: usize,
    ms: f64,
    note: Option<String>,
}

struct Info {
    state: State,
    build_ms: f64,
    vols: Vec<VolInfo>,
    last_error: Option<String>,
    followers: usize,
}

pub struct NamesService {
    mode: &'static str,
    names: RwLock<Option<Names>>,
    info: Mutex<Info>,
    searchers: Mutex<HashMap<String, Searcher>>,
    deny: Mutex<HashMap<String, (u64, Vec<u32>)>>,
    /// USN 반영·기본판 다시 만들기를 한 번에 하나만.
    writer: Mutex<()>,
    stop: AtomicBool,
    #[cfg(windows)]
    followers: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

fn sort_of(s: Option<&str>) -> Result<Sort> {
    Ok(match s.unwrap_or("relevance") {
        "relevance" => Sort::Relevance,
        "name" => Sort::Name,
        "path" => Sort::Path,
        other => return Err(format!("unknown sort {other}").into()),
    })
}

impl NamesService {
    pub fn new(mode: &'static str) -> Arc<Self> {
        Arc::new(NamesService {
            mode,
            names: RwLock::new(None),
            info: Mutex::new(Info { state: State::Empty, build_ms: 0.0, vols: Vec::new(), last_error: None, followers: 0 }),
            searchers: Mutex::new(HashMap::new()),
            deny: Mutex::new(HashMap::new()),
            writer: Mutex::new(()),
            stop: AtomicBool::new(false),
            #[cfg(windows)]
            followers: Mutex::new(Vec::new()),
        })
    }

    pub fn state(&self) -> State {
        self.info.lock().map(|i| i.state.clone()).unwrap_or(State::Failed("poisoned".into()))
    }

    /// 색인을 (다시) 만든다. 배경 스레드로 돌고 곧바로 돌아온다.
    pub fn build(self: &Arc<Self>, roots: Option<Vec<String>>) -> Result<Value> {
        if self.mode != "service" && roots.as_ref().is_none_or(|r| r.is_empty()) {
            return Err("user mode needs roots".into());
        }
        if self.mode == "service" && roots.is_some() {
            return Err("service mode indexes all fixed drives (no roots)".into());
        }
        if let Some(r) = &roots {
            if r.len() > 64 || r.iter().any(|p| p.len() > 1024 || !std::path::Path::new(p).is_absolute()) {
                return Err("invalid roots".into());
            }
        }
        {
            let mut i = self.info.lock().map_err(|_| "names info poisoned")?;
            if i.state == State::Building {
                return Err("names build already running".into());
            }
            i.state = State::Building;
        }
        let me = self.clone();
        std::thread::Builder::new().name("names-build".into()).spawn(move || me.build_now(roots))?;
        Ok(json!({"state":"building"}))
    }

    /// 지금 스레드에서 만든다(시험·서비스 시작용).
    pub fn build_now(self: &Arc<Self>, roots: Option<Vec<String>>) {
        let started = Instant::now();
        // 잠금 순서: names → info 를 동시에 쥐지 않는다(질의·상태·USN 반영과 교착 없음).
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.collect(roots)));
        match r {
            Ok(Ok((names, vols, journals))) => {
                self.stop_followers();
                if let Ok(mut g) = self.names.write() {
                    *g = Some(names);
                }
                if let Ok(mut d) = self.deny.lock() {
                    d.clear();
                }
                {
                    let mut info = self.info.lock().unwrap_or_else(|e| e.into_inner());
                    info.build_ms = started.elapsed().as_secs_f64() * 1000.0;
                    info.vols = vols;
                    info.state = State::Ready;
                }
                self.start_followers(journals);
            }
            other => {
                let msg = match other {
                    Ok(Err(e)) => e.to_string(),
                    _ => "names build panic isolated".to_string(),
                };
                let have = self.names.read().map(|g| g.is_some()).unwrap_or(false);
                let mut info = self.info.lock().unwrap_or_else(|e| e.into_inner());
                info.last_error = Some(msg.clone());
                info.state = if have { State::Ready } else { State::Failed(msg) };
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn collect(&self, roots: Option<Vec<String>>) -> Result<(Names, Vec<VolInfo>, Vec<(u16, char, kfind_names::ntfs::Journal)>)> {
        let mut b = Builder::new();
        let mut vols = Vec::new();
        let mut journals = Vec::new();
        match roots {
            Some(roots) => {
                for r in roots {
                    let t = Instant::now();
                    let before = b.len();
                    let st = kfind_names::walk::index_tree(&mut b, &r);
                    vols.push(VolInfo {
                        name: r,
                        source: "walk",
                        entries: b.len() - before,
                        ms: t.elapsed().as_secs_f64() * 1000.0,
                        note: (st.errors > 0).then(|| format!("{} folders unreadable", st.errors)),
                    });
                }
            }
            None => {
                for (letter, fs) in kfind_names::ntfs::fixed_drives() {
                    let t = Instant::now();
                    let before = b.len();
                    let mut note = None;
                    let mut source = "walk";
                    if fs == "NTFS" {
                        match kfind_names::ntfs::index_volume(&mut b, letter) {
                            Ok((vol, j, _)) => {
                                source = "mft";
                                if let Some(j) = j {
                                    journals.push((vol, letter, j));
                                } else {
                                    note = Some("no USN journal".into());
                                }
                            }
                            Err(e) => note = Some(format!("mft: {e}")),
                        }
                    }
                    if source == "walk" {
                        let st = kfind_names::walk::index_tree(&mut b, &format!("{letter}:"));
                        if st.errors > 0 {
                            note = Some(format!("{}; {} folders unreadable", note.unwrap_or_default(), st.errors));
                        }
                    }
                    vols.push(VolInfo { name: format!("{letter}:"), source, entries: b.len() - before, ms: t.elapsed().as_secs_f64() * 1000.0, note });
                }
            }
        }
        let mut names = b.finish();
        for (vol, _, j) in &journals {
            names.set_journal(*vol, j.id, j.next_usn);
        }
        Ok((names, vols, journals))
    }

    #[cfg(windows)]
    fn start_followers(self: &Arc<Self>, journals: Vec<(u16, char, kfind_names::ntfs::Journal)>) {
        self.stop.store(false, Ordering::SeqCst);
        let mut hs = self.followers.lock().unwrap_or_else(|e| e.into_inner());
        for (vol, letter, j) in journals {
            let me = self.clone();
            if let Ok(h) = std::thread::Builder::new().name(format!("usn-{letter}")).spawn(move || me.follow(vol, letter, j)) {
                hs.push(h);
            }
        }
        if let Ok(mut i) = self.info.lock() {
            i.followers = hs.len();
        }
    }

    /// 볼륨 하나의 USN 기록을 막힘 읽기로 따라가며 반영한다(새 기록이 없으면 깨지 않음).
    #[cfg(windows)]
    fn follow(&self, vol: u16, letter: char, j: kfind_names::ntfs::Journal) {
        let v = match kfind_names::ntfs::Volume::open(letter) {
            Ok(v) => v,
            Err(e) => return self.note(format!("usn {letter}: {e}")),
        };
        let mut next = j.next_usn;
        let mut buf = Vec::new();
        while !self.stop.load(Ordering::SeqCst) {
            match v.read_usn(vol, &j, next, Some(0), &mut buf) {
                Ok((n, changes)) => {
                    next = n;
                    if self.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if !changes.is_empty() {
                        self.apply(vol, &changes, j.id, next);
                    }
                }
                Err(e) => {
                    if !self.stop.load(Ordering::SeqCst) {
                        self.note(format!("usn {letter}: {e}"));
                    }
                    break;
                }
            }
        }
    }

    /// 변경 묶음 반영. 덧붙임이 기본판의 5%나 6만 항목을 넘으면 기본판을 다시 만든다(질의는 계속 옛 것으로).
    pub fn apply(&self, vol: u16, changes: &[kfind_names::Change], journal_id: u64, next_usn: i64) {
        let _w = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let need_compact = {
            let Ok(mut g) = self.names.write() else { return };
            let Some(n) = g.as_mut() else { return };
            for c in changes {
                n.apply(c);
            }
            n.set_journal(vol, journal_id, next_usn);
            n.delta_count() > 60_000 || n.delta_count() * 20 > n.len()
        };
        if need_compact {
            let fresh = match self.names.read() {
                Ok(g) => g.as_ref().map(|n| n.compact()),
                Err(_) => None,
            };
            if let (Some(f), Ok(mut g)) = (fresh, self.names.write()) {
                *g = Some(f);
            }
        }
    }

    fn note(&self, e: String) {
        if let Ok(mut i) = self.info.lock() {
            i.last_error = Some(e);
        }
    }

    #[cfg(windows)]
    pub fn stop_followers(&self) {
        use std::os::windows::io::AsRawHandle;
        self.stop.store(true, Ordering::SeqCst);
        let hs: Vec<_> = std::mem::take(&mut *self.followers.lock().unwrap_or_else(|e| e.into_inner()));
        for h in &hs {
            // 막혀 있는 DeviceIoControl을 깨운다.
            unsafe { windows_sys::Win32::System::IO::CancelSynchronousIo(h.as_raw_handle() as _) };
        }
        for h in hs {
            let _ = h.join();
        }
        if let Ok(mut i) = self.info.lock() {
            i.followers = 0;
        }
    }

    /// 그 SID가 보면 안 되는 폴더(다른 사용자 프로필·휴지통·시스템 볼륨 정보). 사용자 모드는 없음.
    fn deny_for(&self, sid: &str, names: &Names) -> Vec<u32> {
        if self.mode != "service" {
            return Vec::new();
        }
        let generation = names.generation();
        if let Ok(c) = self.deny.lock() {
            if let Some((g, d)) = c.get(sid) {
                if *g == generation {
                    return d.clone();
                }
            }
        }
        let v = names.view();
        let mut deny = Vec::new();
        for vol in names.volumes() {
            for d in ["$Recycle.Bin", "System Volume Information"] {
                if let Some(id) = v.find_dir(&format!("{}\\{d}", vol.name)) {
                    deny.push(id);
                }
            }
        }
        let own = profile_path(sid);
        // 프로필이 들어 있는 폴더(보통 `C:\Users`)의 다른 하위 폴더를 뺀다. 프로필을 모르면 모두 뺀다.
        let users = own
            .as_deref()
            .and_then(|p| p.rsplit_once('\\').map(|(a, _)| a.to_string()))
            .unwrap_or_else(|| "C:\\Users".into());
        let own_name = own.as_deref().and_then(|p| p.rsplit('\\').next()).map(|s| s.to_lowercase());
        if let Some(u) = v.find_dir(&users) {
            for c in v.children(u) {
                let n = v.name_of(c).to_lowercase();
                let shared = matches!(n.as_str(), "public" | "default" | "default user" | "all users");
                if !shared && Some(&n) != own_name.as_ref() && v.flags[c as usize] & kfind_names::flag::DIR != 0 {
                    deny.push(c);
                }
            }
        }
        if let Ok(mut c) = self.deny.lock() {
            if c.len() > 64 {
                c.clear();
            }
            c.insert(sid.to_string(), (generation, deny.clone()));
        }
        deny
    }

    pub fn query(&self, sid: &str, text: &str, limit: usize, sort: Option<&str>, hidden: bool, cancel: Arc<AtomicBool>) -> Result<Value> {
        let started = Instant::now();
        if text.len() > 4096 {
            return Err("query too long".into());
        }
        let sort = sort_of(sort)?;
        let g = self.names.read().map_err(|_| "names poisoned")?;
        let Some(names) = g.as_ref() else {
            drop(g);
            return Err(format!("names index unavailable ({})", self.state().name()).into());
        };
        let q = parse(text);
        let deny = self.deny_for(sid, names);
        let opts = Options { limit: limit.min(MAX_LIMIT), sort, deny, hidden, cancel: Some(cancel), ..Default::default() };
        let v = names.view();
        let r = {
            let mut ss = self.searchers.lock().map_err(|_| "searchers poisoned")?;
            if ss.len() > 64 && !ss.contains_key(sid) {
                ss.clear();
            }
            let s = ss.entry(sid.to_string()).or_insert_with(Searcher::new);
            s.search(&v, names.generation(), &q, &opts).map_err(|e| e.to_string())?
        };
        let search_ms = started.elapsed().as_secs_f64() * 1000.0;
        let items: Vec<Value> = hits(&v, &r.top).into_iter().map(|h| json!({"name":h.name,"dir":h.dir,"is_dir":h.is_dir})).collect();
        Ok(json!({
            "generation": names.generation(), "total": r.total, "items": items, "narrowed": r.narrowed,
            "search_ms": search_ms, "ms": started.elapsed().as_secs_f64() * 1000.0
        }))
    }

    pub fn status(&self) -> Result<Value> {
        let (entries, generation, heap, delta) = {
            let g = self.names.read().map_err(|_| "names poisoned")?;
            g.as_ref().map(|n| (n.len(), n.generation(), n.heap_bytes(), n.delta_count())).unwrap_or((0, 0, 0, 0))
        };
        let i = self.info.lock().map_err(|_| "names info poisoned")?;
        let vols: Vec<Value> = i
            .vols
            .iter()
            .map(|v| json!({"name":v.name,"source":v.source,"entries":v.entries,"ms":v.ms,"note":v.note}))
            .collect();
        Ok(json!({
            "state": i.state.name(), "mode": self.mode, "entries": entries, "generation": generation,
            "heap_bytes": heap, "delta": delta, "build_ms": i.build_ms, "volumes": vols,
            "usn_followers": i.followers, "last_error": i.last_error,
            "failed": match &i.state { State::Failed(e) => Some(e.clone()), _ => None },
        }))
    }
}

/// SID의 프로필 경로(`ProfileList\<SID>\ProfileImagePath`).
#[cfg(windows)]
fn profile_path(sid: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW};
    let key: Vec<u16> = format!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList\\{sid}").encode_utf16().chain([0]).collect();
    let val: Vec<u16> = "ProfileImagePath".encode_utf16().chain([0]).collect();
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            val.as_ptr(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
        )
    };
    if rc != 0 {
        return None;
    }
    let l = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..l]);
    if s.starts_with('%') { None } else { Some(s.trim_end_matches('\\').to_string()) }
}
