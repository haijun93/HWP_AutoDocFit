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
use kfind_names::query::parse_groups;
use kfind_names::{Builder, Names, Options, Searcher, Sort};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

pub const MAX_LIMIT: usize = 1000;
/// 한 질의의 `;` 묶음 최대 개수.
pub const MAX_GROUPS: usize = 8;

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
    let s = s.unwrap_or("relevance");
    // relevance·name·path, Q-F2 size(큰 것 먼저)·date(최근 먼저).
    Sort::parse(s).ok_or_else(|| format!("unknown sort {s}").into())
}

/// 서비스 없이 사용자 모드로 돌 때 파일명 색인을 자동으로 만들 기본 폴더(실제로 있는 절대 경로만, 중복 없이).
/// 사용자 폴더의 바탕 화면·문서·다운로드(한글 이름 포함)와 OneDrive로 옮겨진 같은 폴더들.
pub fn default_user_roots(
    home: Option<&std::path::Path>,
    onedrive: &[std::path::PathBuf],
) -> Vec<String> {
    let mut bases: Vec<std::path::PathBuf> = Vec::new();
    if let Some(h) = home {
        bases.push(h.to_path_buf());
    }
    bases.extend(onedrive.iter().cloned());
    let mut out: Vec<String> = Vec::new();
    for base in bases {
        for name in ["Desktop", "Documents", "Downloads", "바탕 화면", "문서", "다운로드"] {
            let path = base.join(name);
            if !path.is_absolute() || !path.is_dir() {
                continue;
            }
            let text = path.to_string_lossy().into_owned();
            if !out.iter().any(|p| p.eq_ignore_ascii_case(&text)) {
                out.push(text);
            }
        }
    }
    out.truncate(64);
    out
}

/// 실행 환경(USERPROFILE·OneDrive)에서 기본 폴더 목록을 읽는다.
pub fn default_user_roots_from_env() -> Vec<String> {
    let home = std::env::var_os("USERPROFILE").map(std::path::PathBuf::from);
    let onedrive: Vec<std::path::PathBuf> = ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"]
        .iter()
        .filter_map(|k| std::env::var_os(k).map(std::path::PathBuf::from))
        .collect();
    default_user_roots(home.as_deref(), &onedrive)
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
        // `;` 로 나눈 묶음은 교집합(묶음 하나면 예전 단일 질의와 같다).
        let q = parse_groups(text);
        if q.len() > MAX_GROUPS {
            return Err("too many groups".into());
        }
        let deny = self.deny_for(sid, names);
        let opts = Options { limit: limit.min(MAX_LIMIT), sort, deny, hidden, cancel: Some(cancel), ..Default::default() };
        let v = names.view();
        let r = {
            let mut ss = self.searchers.lock().map_err(|_| "searchers poisoned")?;
            if ss.len() > 64 && !ss.contains_key(sid) {
                ss.clear();
            }
            let s = ss.entry(sid.to_string()).or_insert_with(Searcher::new);
            s.search_groups(&v, names.generation(), &q, &opts).map_err(|e| e.to_string())?
        };
        let search_ms = started.elapsed().as_secs_f64() * 1000.0;
        let items: Vec<Value> = hits(&v, &r.top).into_iter().map(|h| json!({"name":h.name,"dir":h.dir,"is_dir":h.is_dir,"size":h.size,"mtime":h.mtime})).collect();
        Ok(json!({
            "generation": names.generation(), "total": r.total, "items": items, "narrowed": r.narrowed,
            "search_ms": search_ms, "ms": started.elapsed().as_secs_f64() * 1000.0
        }))
    }

    /// Separate optional command; the existing names_query contract is unchanged.
    pub fn query_related(&self, sid: &str, text: &str, limit: usize, cancel: Arc<AtomicBool>) -> Result<Value> {
        let groups=kfind_core::related_query::parse_related_query(text).map_err(|e|e.to_string())?;
        let g=self.names.read().map_err(|_|"names poisoned")?;
        let names=g.as_ref().ok_or("names index unavailable")?;
        let opts=Options { limit:limit.min(MAX_LIMIT),deny:self.deny_for(sid,names),cancel:Some(cancel),..Default::default() };
        let v=names.view();
        let r=kfind_names::query::search_related(&v,text,&opts).map_err(|e|e.to_string())?;
        let items: Vec<Value>=hits(&v,&r.top).into_iter().map(|h|json!({
            "name":h.name,"dir":h.dir,"is_dir":h.is_dir,"size":h.size,"mtime":h.mtime,
            "group_matches":groups.iter().enumerate().map(|(i,g)|json!({"group_index":i,"query":g.query,"match_count":1,"filename":true})).collect::<Vec<_>>()
        })).collect();
        Ok(json!({"generation":names.generation(),"total":r.total,"items":items,"narrowed":false,"related_groups":true}))
    }

    /// Cursor-paged file-list query (offset/next are raw index positions) used by the document and photo views.
    pub fn list(&self, extensions: &[String], roots: &[String], exclude: &[String], offset: usize, limit: usize) -> Result<Value> {
        use kfind_names::flag::{DELETED, DIR, CLOUD_ONLY};
        if self.state().name() != "ready" {
            return Err(format!("names index unavailable ({})", self.state().name()).into());
        }
        if extensions.is_empty() || extensions.len() > 64 || exclude.len() > 4096 || roots.len() > 128 || limit == 0 || limit > 10_000 {
            return Err("invalid names_list filters".into());
        }
        let extensions: std::collections::HashSet<String> = extensions.iter()
            .map(|value| value.trim_start_matches('.').to_lowercase())
            .filter(|value| !value.is_empty() && value.len() <= 16 && value.bytes().all(|b| b.is_ascii_alphanumeric()))
            .collect();
        if extensions.is_empty() { return Err("invalid names_list extensions".into()); }
        let excluded: Vec<String> = exclude.iter().map(|p| normalized_path(p)).collect();
        let included: Vec<String> = roots.iter().map(|p| normalized_path(p)).collect();
        let g = self.names.read().map_err(|_| "names poisoned")?;
        let Some(names) = g.as_ref() else {
            return Err(format!("names index unavailable ({})", self.state().name()).into());
        };
        let view = names.view();

        // 빠른 확장자 필터: view.exts 에서 등록된 확장자 ID 중 일치하는 항목을 미리 마킹
        let mut ext_matched = vec![false; view.exts.len()];
        for ext in &extensions {
            let fold_b = kfind_core::fold(ext).into_bytes();
            if let Some(id) = view.exts.find(&fold_b) {
                if (id as usize) < ext_matched.len() {
                    ext_matched[id as usize] = true;
                }
            }
        }

        let reserved = ["windows", "program files", "program files (x86)", "programdata", "$recycle.bin",
            "system volume information", "appdata", "node_modules", ".git", ".svn", ".hg", "venv", ".venv"];
        let mut items = Vec::new();
        let mut cursor = offset.min(names.len());
        let mut done = true;
        for raw_id in offset.min(names.len())..names.len() {
            cursor = raw_id + 1;
            let id = raw_id as u32;
            if view.flags[id as usize] & (DIR | DELETED) != 0 { continue; }

            // 대상 확장자 후보만 문자열을 검사한다. offset/next로 앞 페이지를 다시 훑지 않는다.
            let ext_id = view.ext[id as usize];
            if ext_id != u16::MAX {
                if !ext_matched.get(ext_id as usize).copied().unwrap_or(false) {
                    continue;
                }
            }

            // 2단계: 매칭된 확장자 후보만 파일명 및 세부 조건 검사
            let name = view.name_of(id);
            if name.starts_with('.') || name.to_lowercase().contains("#ieeee") { continue; }
            let Some((_, ext_raw)) = name.rsplit_once('.') else { continue };
            let ext_lower = ext_raw.to_lowercase();
            if !extensions.contains(&ext_lower) {
                continue;
            }

            let path = view.full_path(id);
            let canonical = normalized_path(&path);
            if !included.is_empty() && !included.iter().any(|root| canonical == *root || canonical.starts_with(&format!("{root}\\"))) { continue; }
            if excluded.iter().any(|root| canonical == *root || canonical.starts_with(&format!("{root}\\"))) { continue; }
            if canonical.split('\\').any(|part| reserved.contains(&part)) { continue; }

            // 3단계: 색인에 이미 있는 size, mtime, cloud 메타데이터 함께 반환 (하위 호환)
            let raw_size = view.size[id as usize];
            let size = if raw_size == u64::MAX { 0 } else { raw_size };
            let raw_mtime = view.mtime[id as usize];
            let (mtime_sec, mtime_ns) = if raw_mtime == i64::MIN {
                (0.0, 0i128)
            } else {
                let ns = (raw_mtime as i128 - 116_444_736_000_000_000i128) * 100;
                (ns as f64 / 1e9, ns)
            };
            let cloud = (view.flags[id as usize] & CLOUD_ONLY) != 0;

            items.push(json!({
                "path": path,
                "name": name,
                "ext": ext_lower,
                "size": size,
                "mtime": mtime_sec,
                "mtime_ns": i64::try_from(mtime_ns).unwrap_or(0),
                "cloud": cloud,
            }));
            if items.len() == limit { done = cursor >= names.len(); break; }
        }
        Ok(json!({"state":"ready","next":cursor,"done":done,"items":items}))
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
            "version": 2,
            "features": ["names_list", "metadata_v2"],
            "commands": ["names_build", "names_query", "names_related", "names_list", "names_status"],
        }))
    }
}

fn normalized_path(value: &str) -> String {
    value.replace('/', "\\").trim_end_matches('\\').to_lowercase()
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

#[cfg(test)]
mod tests {
    use super::*;
    use kfind_names::Builder;
    use std::sync::Arc;
    use std::time::Instant;

    fn make_test_service(entries_count: usize) -> Arc<NamesService> {
        let mut b = Builder::new();
        let vol = b.add_volume("C:", 5);

        // 기본 폴더 트리: C:\Users\user\Documents 및 C:\Windows 등
        b.push(vol, 10, 5, "Users", 0x10);
        b.push(vol, 11, 10, "user", 0x10);
        b.push(vol, 12, 11, "Documents", 0x10);
        b.push(vol, 13, 11, "Pictures", 0x10);
        b.push(vol, 14, 5, "Windows", 0x10); // 예약 폴더

        // 1. 특정 테스트 문서 (메타데이터 포함)
        // 합성 기준 시각: Unix 1732600000.5초.
        let test_mtime_ticks: i64 = 133_770_736_005_000_000;
        b.push_meta(vol, 100, 12, "보고서.hwpx", 0x20, 10240, test_mtime_ticks);
        b.push_meta(vol, 101, 12, "예산.xlsx", 0x20, 20480, test_mtime_ticks);
        b.push_meta(vol, 102, 13, "사진1.jpg", 0x20, 307200, test_mtime_ticks);
        // 클라우드/리파스 파일 (0x400)
        b.push_meta(vol, 103, 13, "클라우드사진.png", 0x20 | 0x400 | 0x1000, 409600, test_mtime_ticks);
        b.push_meta(vol, 107, 13, "local-reparse.png", 0x20 | 0x400, 123, test_mtime_ticks);
        // 예약 폴더 파일 (제외 대상)
        b.push_meta(vol, 104, 14, "system.pdf", 0x20, 512, test_mtime_ticks);
        b.push(vol, 105, 12, "unknown.hwpx", 0x20);
        b.push_meta(vol, 106, 12, "folded.h-wpx", 0x20, 1, test_mtime_ticks);

        // 2. 대량 더미 파일 생성 (확장자 다양성: txt, dll, log, tmp 등과 간헐적 docx/hwp)
        let exts = ["txt", "dll", "log", "tmp", "dat", "ini", "sys", "bin", "hwpx", "jpg"];
        for i in 0..entries_count {
            let ext = exts[i % exts.len()];
            let name = format!("file_{i}.{ext}");
            let is_cloud = (i % 20) == 0;
            let attrs = 0x20 | (if is_cloud { 0x400 | 0x1000 } else { 0 });
            let size = ((i * 137) % 1_000_000) as u64;
            let mtime = test_mtime_ticks + (i as i64 * 10_000_000);
            b.push_meta(vol, 1000 + i as u64, 12, &name, attrs, size, mtime);
        }

        let names = b.finish();
        let svc = NamesService::new("user");
        {
            let mut g = svc.names.write().unwrap();
            *g = Some(names);
        }
        {
            let mut info = svc.info.lock().unwrap();
            info.state = State::Ready;
        }
        svc
    }

    #[test]
    fn test_names_list_metadata_and_filtering() {
        let svc = make_test_service(50);
        let exts = vec!["hwpx".to_string(), "jpg".to_string(), "png".to_string()];
        let roots = vec!["C:\\Users\\user".to_string()];
        let exclude = vec![];

        let result = svc.list(&exts, &roots, &exclude, 0, 100).expect("list");
        assert_eq!(result["state"], "ready");
        let items = result["items"].as_array().expect("items array");

        // "보고서.hwpx", "사진1.jpg", "클라우드사진.png" 및 더미 중 hwpx, jpg 포함
        assert!(!items.is_empty());

        let report = items.iter().find(|it| it["name"] == "보고서.hwpx").expect("보고서.hwpx");
        assert_eq!(report["size"], 10240);
        assert_eq!(report["ext"], "hwpx");
        assert_eq!(report["cloud"], false);
        let mtime_sec = report["mtime"].as_f64().expect("mtime f64");
        assert_eq!(mtime_sec, 1732600000.5);
        assert_eq!(report["mtime_ns"], 1732600000500000000i64);

        let cloud_pic = items.iter().find(|it| it["name"] == "클라우드사진.png").expect("클라우드사진.png");
        assert_eq!(cloud_pic["cloud"], true);
        assert_eq!(cloud_pic["size"], 409600);
        let local = items.iter().find(|it| it["name"] == "local-reparse.png").unwrap();
        assert_eq!(local["cloud"], false);

        // Windows 폴더의 system.pdf 는 roots 및 예약 폴더에 의해 걸러져야 함
        assert!(items.iter().all(|it| it["name"] != "system.pdf" && it["name"] != "folded.h-wpx"));
        let unknown = items.iter().find(|it| it["name"] == "unknown.hwpx").unwrap();
        assert_eq!(unknown["size"], 0);
        assert_eq!(unknown["mtime_ns"], 0);
    }

    #[test]
    fn test_names_list_cursor_pagination() {
        let svc = make_test_service(100);
        let exts = vec!["hwpx".to_string(), "jpg".to_string()];
        let roots = vec!["C:\\Users".to_string()];
        let exclude = vec![];

        // limit = 5 로 페이지 순회
        let mut offset = 0;
        let mut total_collected = 0;
        let mut collected = Vec::new();
        let mut pages = 0;

        loop {
            let res = svc.list(&exts, &roots, &exclude, offset, 5).expect("list page");
            let items = res["items"].as_array().expect("items");
            let next = res["next"].as_u64().expect("next") as usize;
            let done = res["done"].as_bool().expect("done");

            total_collected += items.len();
            collected.extend(items.iter().cloned());
            pages += 1;
            assert!(next >= offset);

            if done {
                break;
            }
            offset = next;
        }

        assert!(pages > 1, "pagination happened across multiple pages");
        let full = svc.list(&exts, &roots, &exclude, 0, 1000).unwrap();
        assert_eq!(total_collected, full["items"].as_array().unwrap().len());
        assert_eq!(collected, *full["items"].as_array().unwrap());
    }

    fn legacy_list(svc: &NamesService, extensions: &[String], roots: &[String], exclude: &[String], offset: usize, limit: usize) -> Result<Value> {
        use kfind_names::flag::{DELETED, DIR};
        if svc.state().name() != "ready" {
            return Err(format!("names index unavailable ({})", svc.state().name()).into());
        }
        if extensions.is_empty() || extensions.len() > 64 || exclude.len() > 4096 || roots.len() > 128 || limit == 0 || limit > 10_000 {
            return Err("invalid names_list filters".into());
        }
        let extensions: std::collections::HashSet<String> = extensions.iter()
            .map(|value| value.trim_start_matches('.').to_lowercase())
            .filter(|value| !value.is_empty() && value.len() <= 16 && value.bytes().all(|b| b.is_ascii_alphanumeric()))
            .collect();
        if extensions.is_empty() { return Err("invalid names_list extensions".into()); }
        let excluded: Vec<String> = exclude.iter().map(|p| normalized_path(p)).collect();
        let included: Vec<String> = roots.iter().map(|p| normalized_path(p)).collect();
        let g = svc.names.read().map_err(|_| "names poisoned")?;
        let Some(names) = g.as_ref() else {
            return Err(format!("names index unavailable ({})", svc.state().name()).into());
        };
        let view = names.view();
        let mut items = Vec::new();
        let mut cursor = offset.min(names.len());
        let mut done = true;
        for raw_id in offset.min(names.len())..names.len() {
            cursor = raw_id + 1;
            let id = raw_id as u32;
            if view.flags[id as usize] & (DIR | DELETED) != 0 { continue; }
            let name = view.name_of(id);
            if name.starts_with('.') || name.to_lowercase().contains("#ieeee") { continue; }
            let Some((_, ext)) = name.rsplit_once('.') else { continue };
            if !extensions.contains(&ext.to_lowercase()) { continue; }
            let path = view.full_path(id);
            let canonical = normalized_path(&path);
            if !included.is_empty() && !included.iter().any(|root| canonical == *root || canonical.starts_with(&format!("{root}\\"))) { continue; }
            if excluded.iter().any(|root| canonical == *root || canonical.starts_with(&format!("{root}\\"))) { continue; }
            let reserved = ["windows", "program files", "program files (x86)", "programdata", "$recycle.bin",
                "system volume information", "appdata", "node_modules", ".git", ".svn", ".hg", "venv", ".venv"];
            if canonical.split('\\').any(|part| reserved.contains(&part)) { continue; }
            items.push(json!({"path":path,"name":name}));
            if items.len() == limit { done = cursor >= names.len(); break; }
        }
        Ok(json!({"state":"ready","next":cursor,"done":done,"items":items}))
    }

    #[test]
    #[ignore = "합성 전후 측정은 잠금 해제 후 명시적으로 실행"]
    fn benchmark_names_list_before_after() {
        let mut b = Builder::new();
        let vol = b.add_volume("C:", 5);
        b.push(vol, 10, 5, "Synthetic", 0x10);
        for i in 0..400_000u64 {
            let ext = match i % 100 { 0 => "hwpx", 1 => "jpg", _ => "log" };
            b.push_meta(vol, 100+i, 10, &format!("item_{i:08}.{ext}"), 0x20, i, 134_086_608_000_000_000);
        }
        let svc = NamesService::new("user");
        *svc.names.write().unwrap() = Some(b.finish());
        svc.info.lock().unwrap().state = State::Ready;
        let exts = vec!["hwpx".into(), "jpg".into()];
        let roots = vec!["C:\\Synthetic".into()];
        let collect = |legacy: bool| {
            let start = Instant::now();
            let mut cursor = 0;
            let mut pages = 0;
            let mut rows = Vec::new();
            loop {
                let r = if legacy { legacy_list(&svc, &exts, &roots, &[], cursor, 5000) }
                        else { svc.list(&exts, &roots, &[], cursor, 5000) }.unwrap();
                rows.extend(r["items"].as_array().unwrap().iter().cloned());
                pages += 1;
                cursor = r["next"].as_u64().unwrap() as usize;
                if r["done"] == true { break; }
            }
            (rows, pages, start.elapsed().as_secs_f64() * 1000.0)
        };
        let (before, bp, bm) = collect(true);
        let (after, ap, am) = collect(false);
        assert_eq!(before.len(), 8000);
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            assert_eq!(b["path"], a["path"]);
            assert_eq!(b["name"], a["name"]);
        }
        eprintln!("SVCCHECK1 synthetic=400000 matches={} before_pages={bp} after_pages={ap} before_ms={bm:.3} after_ms={am:.3} identical_paths=true", after.len());
    }

    #[test]
    fn test_names_status_reports_version_and_features() {
        let svc = make_test_service(1);
        let status = svc.status().expect("status");
        assert_eq!(status["version"], 2);
        let features = status["features"].as_array().expect("features");
        assert!(features.iter().any(|f| f == "names_list"));
        let commands = status["commands"].as_array().expect("commands");
        assert!(commands.iter().any(|c| c == "names_list"));
    }

    #[test]
    fn default_user_roots_keep_only_existing_unique_known_folders() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("Desktop")).unwrap();
        std::fs::create_dir_all(home.path().join("문서")).unwrap();
        std::fs::create_dir_all(home.path().join("Pictures")).unwrap();
        let onedrive = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(onedrive.path().join("Documents")).unwrap();
        std::fs::create_dir_all(onedrive.path().join("Desktop")).unwrap();
        let roots = default_user_roots(Some(home.path()), &[onedrive.path().to_path_buf(), home.path().to_path_buf()]);
        let names: Vec<String> = roots
            .iter()
            .map(|r| std::path::Path::new(r).file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["Desktop", "문서", "Desktop", "Documents"]);
        // 같은 경로를 두 번 넘겨도 한 번만, 없는 폴더(Downloads·Pictures)는 빠진다.
        assert_eq!(roots.len(), 4);
        assert!(default_user_roots(None, &[]).is_empty());
        assert!(default_user_roots(Some(std::path::Path::new("relative-home")), &[]).is_empty());
    }
}
