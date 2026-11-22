//! 실제 이름 변경 적용과 JSON Lines 저널 되돌리기. 디스크 접근은 `Fs` 로 주입해 시험한다.
//!
//! 안전 규칙: 사전 상태(경로·크기·수정시간) 재확인 → 깊은 폴더 먼저 → 순환·대소문자만 바뀌는 이름은
//! 임시 이름(`.rename-tmp-*`) 경유 → 시작·항목·종료를 저널에 즉시 기록(동기화) → 덮어쓰기 없음.
use crate::{Model, RowStatus};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const JOURNAL_FILE: &str = "rename_journal.jsonl";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    pub size: u64,
    pub mtime_ns: u128,
}

pub fn stat_file(p: &Path) -> Option<Fingerprint> {
    let m = std::fs::metadata(p).ok()?;
    if !m.is_file() {
        return None;
    }
    let t = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some(Fingerprint { size: m.len(), mtime_ns: t })
}

#[derive(Clone, Debug)]
pub struct PlanItem {
    pub dir: String,
    pub old: String,
    pub new: String,
    pub print: Fingerprint,
}
impl PlanItem {
    fn src(&self) -> PathBuf {
        PathBuf::from(&self.dir).join(&self.old)
    }
    fn dst(&self) -> PathBuf {
        PathBuf::from(&self.dir).join(&self.new)
    }
}

/// 적용 가능한 상태인지 판정하고 계획을 만든다(깊은 폴더 우선). 충돌·이름 오류가 하나라도 있으면 거부.
pub fn build_plan(m: &Model) -> Result<Vec<PlanItem>, String> {
    if m.files.is_empty() {
        return Err("목록이 비어 있습니다".into());
    }
    if m.count(RowStatus::Conflict) > 0 || m.count(RowStatus::Invalid) > 0 {
        return Err("충돌 또는 이름 오류가 있어 적용할 수 없습니다".into());
    }
    let mut plan = Vec::new();
    for (i, row) in m.rows.iter().enumerate() {
        if row.status != RowStatus::Changed {
            continue;
        }
        let f = &m.files[i];
        let print = m.prints.get(i).copied().flatten().ok_or("파일 정보를 읽지 못했습니다")?;
        plan.push(PlanItem { dir: f.dir.clone(), old: f.name.clone(), new: row.proposed.clone(), print });
    }
    if plan.is_empty() {
        return Err("변경할 항목이 없습니다".into());
    }
    plan.sort_by_key(|p| std::cmp::Reverse(p.dir.matches(['\\', '/']).count()));
    Ok(plan)
}

fn dir_names_lower(dir: &str) -> HashSet<String> {
    let d = if dir.is_empty() { "." } else { dir };
    std::fs::read_dir(d)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_lowercase()).collect())
        .unwrap_or_default()
}

/// 적용 직전 사전 상태 재확인. 하나라도 어긋나면 아무것도 바꾸지 않는다.
pub fn verify_plan(plan: &[PlanItem]) -> Result<(), String> {
    let mut listing: HashMap<String, HashSet<String>> = HashMap::new();
    let mut sources: HashMap<String, HashSet<String>> = HashMap::new();
    let mut dsts: HashSet<(String, String)> = HashSet::new();
    for p in plan {
        sources.entry(p.dir.to_lowercase()).or_default().insert(p.old.to_lowercase());
    }
    for p in plan {
        if stat_file(&p.src()) != Some(p.print) {
            return Err(format!("'{}' 의 경로·크기·수정시간이 달라졌습니다. 파일을 다시 불러오세요.", p.old));
        }
        let key = p.dir.to_lowercase();
        if !dsts.insert((key.clone(), p.new.to_lowercase())) {
            return Err(format!("'{}' 이름이 중복됩니다", p.new));
        }
        let names = listing.entry(key.clone()).or_insert_with(|| dir_names_lower(&p.dir));
        let nl = p.new.to_lowercase();
        if names.contains(&nl) && !sources[&key].contains(&nl) {
            return Err(format!("'{}' 이름이 이미 폴더에 있습니다. 파일을 다시 불러오세요.", p.new));
        }
    }
    Ok(())
}

/// 덮어쓰지 않는 이름 변경.
pub trait Fs {
    fn rename(&self, src: &Path, dst: &Path) -> io::Result<()>;
}
pub struct RealFs;
impl Fs for RealFs {
    #[cfg(windows)]
    fn rename(&self, src: &Path, dst: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        let w = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(Some(0)).collect() };
        let (s, d) = (w(src), w(dst));
        // 플래그 0 = 대상이 있으면 실패(덮어쓰기 금지)
        if unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileExW(s.as_ptr(), d.as_ptr(), 0) } != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(windows))]
    fn rename(&self, src: &Path, dst: &Path) -> io::Result<()> {
        if dst.exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "대상이 있음"));
        }
        std::fs::rename(src, dst)
    }
}

// ---- 최소 JSON(평면 객체) ----
fn esc(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
enum V<'a> {
    S(&'a str),
    N(u128),
    B(bool),
}
fn line(fields: &[(&str, V)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v)| {
            let val = match v {
                V::S(s) => esc(s),
                V::N(n) => format!("\"{n}\""),
                V::B(b) => b.to_string(),
            };
            format!("{}:{}", esc(k), val)
        })
        .collect();
    format!("{{{}}}", body.join(","))
}
fn ws(c: &[char], i: &mut usize) {
    while *i < c.len() && c[*i].is_whitespace() {
        *i += 1;
    }
}
fn json_string(c: &[char], i: &mut usize) -> Option<String> {
    if c.get(*i) != Some(&'"') {
        return None;
    }
    *i += 1;
    let mut o = String::new();
    loop {
        let ch = *c.get(*i)?;
        *i += 1;
        match ch {
            '"' => return Some(o),
            '\\' => {
                let e = *c.get(*i)?;
                *i += 1;
                match e {
                    'n' => o.push('\n'),
                    'r' => o.push('\r'),
                    't' => o.push('\t'),
                    'u' => {
                        let h: String = c.get(*i..*i + 4)?.iter().collect();
                        *i += 4;
                        o.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?).unwrap_or('\u{fffd}'));
                    }
                    other => o.push(other),
                }
            }
            ch => o.push(ch),
        }
    }
}
/// 평면 객체 한 줄을 해석한다. 값은 모두 문자열(불리언은 "true"/"false")로 돌려준다. 깨진 줄은 None.
pub fn parse_line(s: &str) -> Option<HashMap<String, String>> {
    let c: Vec<char> = s.trim().chars().collect();
    let mut i = 0;
    let mut out = HashMap::new();
    ws(&c, &mut i);
    if c.get(i) != Some(&'{') {
        return None;
    }
    i += 1;
    loop {
        ws(&c, &mut i);
        if c.get(i) == Some(&'}') {
            return if i + 1 == c.len() { Some(out) } else { None };
        }
        let k = json_string(&c, &mut i)?;
        ws(&c, &mut i);
        if c.get(i) != Some(&':') {
            return None;
        }
        i += 1;
        ws(&c, &mut i);
        let v = if c.get(i) == Some(&'"') {
            json_string(&c, &mut i)?
        } else {
            let st = i;
            while i < c.len() && !matches!(c[i], ',' | '}') {
                i += 1;
            }
            c[st..i].iter().collect::<String>().trim().to_string()
        };
        out.insert(k, v);
        ws(&c, &mut i);
        match c.get(i) {
            Some(',') => i += 1,
            Some('}') => {}
            _ => return None,
        }
    }
}

struct Journal {
    file: std::fs::File,
}
impl Journal {
    fn open(path: &Path) -> io::Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        Ok(Journal { file: OpenOptions::new().create(true).append(true).open(path)? })
    }
    fn write(&mut self, l: String) -> io::Result<()> {
        self.file.write_all(format!("{l}\n").as_bytes())?;
        self.file.sync_data()
    }
}

fn now_secs() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as u128).unwrap_or(0)
}
fn new_run_id() -> String {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{n:x}-{}", std::process::id())
}
fn p2s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

struct Step {
    src: PathBuf,
    dst: PathBuf,
    via: &'static str,
    print: Fingerprint,
}

/// 직접 이동할 수 있는 것은 먼저, 순환·대소문자 변경(대상이 같은 폴더의 원본 이름)은 임시 이름을 거친다.
fn schedule(plan: &[PlanItem], run: &str) -> Vec<Step> {
    let mut srcs: HashMap<String, HashSet<String>> = HashMap::new();
    for p in plan {
        srcs.entry(p.dir.to_lowercase()).or_default().insert(p.old.to_lowercase());
    }
    let (mut direct, mut p1, mut p2) = (Vec::new(), Vec::new(), Vec::new());
    for (i, p) in plan.iter().enumerate() {
        let blocked = srcs[&p.dir.to_lowercase()].contains(&p.new.to_lowercase());
        if blocked {
            let tmp = PathBuf::from(&p.dir).join(format!(".rename-tmp-{run}-{i}"));
            p1.push(Step { src: p.src(), dst: tmp.clone(), via: "tmp1", print: p.print });
            p2.push(Step { src: tmp, dst: p.dst(), via: "tmp2", print: p.print });
        } else {
            direct.push(Step { src: p.src(), dst: p.dst(), via: "direct", print: p.print });
        }
    }
    direct.into_iter().chain(p1).chain(p2).collect()
}

#[derive(Debug, Default)]
pub struct ApplyReport {
    pub run: String,
    /// 최종 이름까지 바뀐 파일 수.
    pub renamed: usize,
    /// 임시 이름으로 남아 복구하지 못한 파일(있으면 사용자에게 알려야 함).
    pub stranded: Vec<String>,
}

fn item_line(run: &str, seq: usize, s: &Step, ok: bool, err: &str) -> String {
    line(&[
        ("type", V::S("item")),
        ("run", V::S(run)),
        ("seq", V::N(seq as u128)),
        ("via", V::S(s.via)),
        ("src", V::S(&p2s(&s.src))),
        ("dst", V::S(&p2s(&s.dst))),
        ("size", V::N(s.print.size as u128)),
        ("mtime", V::N(s.print.mtime_ns)),
        ("ok", V::B(ok)),
        ("err", V::S(err)),
    ])
}

/// 계획을 적용한다. 사전 확인 실패 시 디스크·저널 모두 손대지 않는다.
/// 중간 실패 시 임시 이름으로 남은 파일은 원래 이름으로 복구를 시도하고, 이미 끝난 변경은 저널에 남겨 되돌릴 수 있다.
pub fn apply(plan: &[PlanItem], journal: &Path, fs: &dyn Fs) -> Result<ApplyReport, (String, ApplyReport)> {
    let mut rep = ApplyReport::default();
    verify_plan(plan).map_err(|e| (e, ApplyReport::default()))?;
    let run = new_run_id();
    rep.run = run.clone();
    let mut j = Journal::open(journal).map_err(|e| (format!("저널을 열 수 없습니다: {e}"), ApplyReport::default()))?;
    j.write(line(&[("type", V::S("start")), ("run", V::S(&run)), ("ts", V::N(now_secs())), ("count", V::N(plan.len() as u128))]))
        .map_err(|e| (format!("저널 기록 실패: {e}"), ApplyReport::default()))?;
    let steps = schedule(plan, &run);
    let mut done: Vec<&Step> = Vec::new();
    let mut seq = 0usize;
    let mut failure: Option<String> = None;
    for s in &steps {
        seq += 1;
        match fs.rename(&s.src, &s.dst) {
            Ok(()) => {
                done.push(s);
                if let Err(e) = j.write(item_line(&run, seq, s, true, "")) {
                    failure = Some(format!("저널 기록 실패: {e}"));
                    break;
                }
            }
            Err(e) => {
                let _ = j.write(item_line(&run, seq, s, false, &e.to_string()));
                let name = s.src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                failure = Some(format!("'{name}' 변경 실패: {e}"));
                break;
            }
        }
    }
    if let Some(msg) = failure {
        // 임시 이름으로 남은 것(tmp1 은 했으나 tmp2 는 못 한 것)을 원래 이름으로 복구
        let finished: HashSet<PathBuf> = done.iter().filter(|s| s.via == "tmp2").map(|s| s.src.clone()).collect();
        for s in done.iter().rev().filter(|s| s.via == "tmp1" && !finished.contains(&s.dst)) {
            seq += 1;
            let back = Step { src: s.dst.clone(), dst: s.src.clone(), via: "recover", print: s.print };
            match fs.rename(&back.src, &back.dst) {
                Ok(()) => {
                    let _ = j.write(item_line(&run, seq, &back, true, ""));
                }
                Err(e) => {
                    let _ = j.write(item_line(&run, seq, &back, false, &e.to_string()));
                    rep.stranded.push(p2s(&back.src));
                }
            }
        }
        rep.renamed = done.iter().filter(|s| s.via != "tmp1").count();
        let _ = j.write(line(&[
            ("type", V::S("end")),
            ("run", V::S(&run)),
            ("status", V::S("failed")),
            ("renamed", V::N(rep.renamed as u128)),
            ("stranded", V::N(rep.stranded.len() as u128)),
            ("ts", V::N(now_secs())),
        ]));
        return Err((msg, rep));
    }
    rep.renamed = plan.len();
    let _ = j.write(line(&[
        ("type", V::S("end")),
        ("run", V::S(&run)),
        ("status", V::S("done")),
        ("renamed", V::N(rep.renamed as u128)),
        ("stranded", V::N(0)),
        ("ts", V::N(now_secs())),
    ]));
    Ok(rep)
}

// ---- 되돌리기 ----
struct RunInfo {
    id: String,
    /// 성공한 이동(순서대로): (src, dst, 사전 상태)
    moves: Vec<(String, String, Fingerprint)>,
}

fn read_runs(journal: &Path) -> Vec<RunInfo> {
    let text = std::fs::read(journal).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut runs: Vec<RunInfo> = Vec::new();
    let mut undone: HashSet<String> = HashSet::new();
    for l in text.lines() {
        let Some(o) = parse_line(l) else { continue };
        let ty = o.get("type").map(String::as_str).unwrap_or("");
        let run = o.get("run").cloned().unwrap_or_default();
        match ty {
            "start" => runs.push(RunInfo { id: run, moves: Vec::new() }),
            "item" if o.get("ok").map(String::as_str) == Some("true") => {
                if let Some(r) = runs.iter_mut().rev().find(|r| r.id == run) {
                    let fp = Fingerprint {
                        size: o.get("size").and_then(|v| v.parse().ok()).unwrap_or(0),
                        mtime_ns: o.get("mtime").and_then(|v| v.parse().ok()).unwrap_or(0),
                    };
                    r.moves.push((o.get("src").cloned().unwrap_or_default(), o.get("dst").cloned().unwrap_or_default(), fp));
                }
            }
            "undo_end" => {
                undone.insert(o.get("target").cloned().unwrap_or_default());
            }
            _ => {}
        }
    }
    runs.retain(|r| !undone.contains(&r.id) && !r.moves.is_empty());
    runs
}

/// 되돌릴 수 있는 직전 작업의 이동 건수(없으면 None).
pub fn undoable_count(journal: &Path) -> Option<usize> {
    read_runs(journal).last().map(|r| r.moves.len())
}

#[derive(Debug, Default)]
pub struct UndoReport {
    pub restored: usize,
    pub skipped: Vec<String>,
}

/// 직전 작업을 역순으로 되돌린다. 대상이 없거나 바뀌었거나 원래 이름이 이미 쓰이면 그 항목만 건너뛰고 보고한다.
pub fn undo_last(journal: &Path, fs: &dyn Fs) -> Result<UndoReport, String> {
    let run = read_runs(journal).pop().ok_or("되돌릴 작업이 없습니다")?;
    let mut j = Journal::open(journal).map_err(|e| format!("저널을 열 수 없습니다: {e}"))?;
    j.write(line(&[("type", V::S("undo_start")), ("target", V::S(&run.id)), ("ts", V::N(now_secs()))]))
        .map_err(|e| format!("저널 기록 실패: {e}"))?;
    let mut rep = UndoReport::default();
    for (src, dst, fp) in run.moves.iter().rev() {
        let (s, d) = (Path::new(src), Path::new(dst));
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let now = stat_file(d);
        let reason = if now.is_none() {
            Some("현재 이름의 파일이 없음(이미 바뀜)")
        } else if now != Some(*fp) {
            Some("파일이 바뀌어 있음(크기·수정시간 불일치)")
        } else if s.exists() {
            Some("원래 이름이 이미 사용 중")
        } else {
            None
        };
        let outcome = match reason {
            Some(r) => Err(r.to_string()),
            None => fs.rename(d, s).map_err(|e| e.to_string()),
        };
        let ok = outcome.is_ok();
        let why = outcome.err().unwrap_or_default();
        let _ = j.write(line(&[
            ("type", V::S("undo_item")),
            ("target", V::S(&run.id)),
            ("src", V::S(src)),
            ("dst", V::S(dst)),
            ("ok", V::B(ok)),
            ("reason", V::S(&why)),
        ]));
        if ok {
            rep.restored += 1;
        } else {
            rep.skipped.push(format!("{} → {}: {}", name(d), name(s), why));
        }
    }
    let _ = j.write(line(&[
        ("type", V::S("undo_end")),
        ("target", V::S(&run.id)),
        ("restored", V::N(rep.restored as u128)),
        ("skipped", V::N(rep.skipped.len() as u128)),
        ("ts", V::N(now_secs())),
    ]));
    Ok(rep)
}

/// 화면 모델에서 곧바로 적용하는 편의 함수.
pub fn apply_model(m: &Model, journal: &Path, fs: &dyn Fs) -> Result<ApplyReport, (String, ApplyReport)> {
    let plan = build_plan(m).map_err(|e| (e, ApplyReport::default()))?;
    apply(&plan, journal, fs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("renamewin5-{tag}-{}", new_run_id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn mk(dir: &Path, files: &[(&str, &str)]) {
        for (n, c) in files {
            if let Some(p) = Path::new(n).parent() {
                std::fs::create_dir_all(dir.join(p)).unwrap();
            }
            std::fs::write(dir.join(n), c).unwrap();
        }
    }
    fn walk(d: &Path, base: &Path, v: &mut Vec<String>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, base, v);
            } else {
                v.push(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    fn names(dir: &Path) -> Vec<String> {
        let mut v = Vec::new();
        walk(dir, dir, &mut v);
        v.sort();
        v
    }
    fn model(dir: &Path, files: &[&str], rules: crate::Rules) -> Model {
        let mut m = Model { rules, ..Default::default() };
        let paths: Vec<String> = files.iter().map(|f| dir.join(f).to_string_lossy().into_owned()).collect();
        m.add_paths_with(
            &paths,
            |p| Path::new(p).is_file(),
            |d| {
                std::fs::read_dir(d)
                    .map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                    .unwrap_or_default()
            },
            |p| stat_file(Path::new(p)),
        );
        m
    }
    fn rules(prefix: &str) -> crate::Rules {
        crate::Rules { prefix: prefix.into(), digits: 0, ..Default::default() }
    }
    fn plan_of(dir: &Path, items: &[(&str, &str)]) -> Vec<PlanItem> {
        items
            .iter()
            .map(|(o, n)| PlanItem {
                dir: dir.to_string_lossy().into_owned(),
                old: o.to_string(),
                new: n.to_string(),
                print: stat_file(&dir.join(o)).unwrap(),
            })
            .collect()
    }
    fn read(p: PathBuf) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn normal_apply_then_undo_roundtrip() {
        let d = tmp("normal");
        mk(&d, &[("a.txt", "1"), ("b.txt", "22"), ("sub/c.txt", "333")]);
        let m = model(&d, &["a.txt", "b.txt", "sub/c.txt"], rules("새_"));
        let j = d.join("data").join(JOURNAL_FILE);
        let r = apply_model(&m, &j, &RealFs).unwrap();
        assert_eq!(r.renamed, 3);
        assert_eq!(names(&d), vec!["data/rename_journal.jsonl", "sub/새_c.txt", "새_a.txt", "새_b.txt"]);
        assert_eq!(undoable_count(&j), Some(3));
        let u = undo_last(&j, &RealFs).unwrap();
        assert_eq!((u.restored, u.skipped.len()), (3, 0));
        assert_eq!(names(&d), vec!["a.txt", "b.txt", "data/rename_journal.jsonl", "sub/c.txt"]);
        assert_eq!(read(d.join("b.txt")), "22");
        assert_eq!(undoable_count(&j), None);
        assert!(undo_last(&j, &RealFs).is_err());
        let text = read(j);
        assert!(text.lines().all(|l| parse_line(l).is_some()));
        assert!(text.contains("\"type\":\"start\"") && text.contains("\"status\":\"done\"") && text.contains("undo_end"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn deep_paths_are_renamed_first() {
        let d = tmp("deep");
        mk(&d, &[("a.txt", "1"), ("x/y/c.txt", "2")]);
        let m = model(&d, &["a.txt", "x/y/c.txt"], rules("p_"));
        let plan = build_plan(&m).unwrap();
        assert!(plan[0].old == "c.txt" && plan[1].old == "a.txt");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn conflict_or_invalid_refuses_and_touches_nothing() {
        let d = tmp("conflict");
        mk(&d, &[("a.txt", "1"), ("b.txt", "2"), ("p_a.txt", "x")]);
        let m = model(&d, &["a.txt", "b.txt"], rules("p_"));
        assert_eq!(m.count(RowStatus::Conflict), 1);
        assert!(build_plan(&m).is_err());
        let m2 = model(&d, &["a.txt"], rules("a?"));
        assert_eq!(m2.count(RowStatus::Invalid), 1);
        assert!(build_plan(&m2).is_err());
        assert!(!d.join("data").exists());
        assert_eq!(names(&d), vec!["a.txt", "b.txt", "p_a.txt"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn stale_state_aborts_before_any_change() {
        let d = tmp("stale");
        mk(&d, &[("a.txt", "1"), ("b.txt", "2")]);
        let m = model(&d, &["a.txt", "b.txt"], rules("n_"));
        let plan = build_plan(&m).unwrap();
        std::fs::write(d.join("b.txt"), "크기가 바뀜").unwrap();
        let j = d.join("j.jsonl");
        let e = apply(&plan, &j, &RealFs).unwrap_err();
        assert!(e.0.contains("달라졌"));
        assert_eq!(names(&d), vec!["a.txt", "b.txt"]);
        assert!(!j.exists());
        let m = model(&d, &["a.txt"], rules("n_"));
        let plan = build_plan(&m).unwrap();
        std::fs::write(d.join("n_a.txt"), "x").unwrap();
        assert!(apply(&plan, &j, &RealFs).unwrap_err().0.contains("이미 폴더에"));
        std::fs::remove_file(d.join("a.txt")).unwrap();
        assert!(apply(&plan, &j, &RealFs).is_err());
        assert!(!j.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn swap_cycle_goes_through_temp_names() {
        let d = tmp("cycle");
        mk(&d, &[("a.txt", "AAA"), ("b.txt", "BBBB")]);
        let plan = plan_of(&d, &[("a.txt", "b.txt"), ("b.txt", "a.txt")]);
        let j = d.join("j.jsonl");
        apply(&plan, &j, &RealFs).unwrap();
        assert_eq!(read(d.join("a.txt")), "BBBB");
        assert_eq!(read(d.join("b.txt")), "AAA");
        assert!(read(j.clone()).contains(".rename-tmp-"));
        assert!(names(&d).iter().all(|n| !n.contains(".rename-tmp-")));
        undo_last(&j, &RealFs).unwrap();
        assert_eq!(read(d.join("a.txt")), "AAA");
        assert_eq!(read(d.join("b.txt")), "BBBB");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn chain_a_to_b_to_c_without_overwrite() {
        let d = tmp("chain");
        mk(&d, &[("a", "1"), ("b", "2")]);
        let plan = plan_of(&d, &[("a", "b"), ("b", "c")]);
        apply(&plan, &d.join("j.jsonl"), &RealFs).unwrap();
        assert_eq!(read(d.join("b")), "1");
        assert_eq!(read(d.join("c")), "2");
        assert!(!d.join("a").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn case_only_rename_works_and_undoes() {
        let d = tmp("case");
        mk(&d, &[("report.txt", "r")]);
        let plan = plan_of(&d, &[("report.txt", "REPORT.txt")]);
        let j = d.join("j.jsonl");
        apply(&plan, &j, &RealFs).unwrap();
        assert!(names(&d).contains(&"REPORT.txt".to_string()), "{:?}", names(&d));
        undo_last(&j, &RealFs).unwrap();
        assert!(names(&d).contains(&"report.txt".to_string()), "{:?}", names(&d));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `only` 번째 호출만(또는 `from` 번째부터 전부) 실패한다.
    struct Fail {
        n: Cell<usize>,
        at: usize,
        onwards: bool,
    }
    impl Fs for Fail {
        fn rename(&self, s: &Path, d: &Path) -> io::Result<()> {
            self.n.set(self.n.get() + 1);
            let n = self.n.get();
            if n == self.at || (self.onwards && n > self.at) {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "주입된 실패"));
            }
            RealFs.rename(s, d)
        }
    }
    fn fail(at: usize, onwards: bool) -> Fail {
        Fail { n: Cell::new(0), at, onwards }
    }

    #[test]
    fn mid_failure_keeps_done_items_in_journal_and_can_undo() {
        let d = tmp("fail");
        mk(&d, &[("a.txt", "1"), ("b.txt", "2"), ("c.txt", "3")]);
        let m = model(&d, &["a.txt", "b.txt", "c.txt"], rules("n_"));
        let j = d.join("data").join(JOURNAL_FILE);
        let (msg, rep) = apply_model(&m, &j, &fail(3, true)).unwrap_err();
        assert!(msg.contains("변경 실패"));
        assert_eq!(rep.renamed, 2);
        assert!(rep.stranded.is_empty());
        assert_eq!(names(&d), vec!["c.txt", "data/rename_journal.jsonl", "n_a.txt", "n_b.txt"]);
        let text = read(j.clone());
        assert!(text.contains("\"status\":\"failed\"") && text.contains("\"ok\":false"));
        assert_eq!(undoable_count(&j), Some(2));
        let u = undo_last(&j, &RealFs).unwrap();
        assert_eq!((u.restored, u.skipped.len()), (2, 0));
        assert_eq!(names(&d), vec!["a.txt", "b.txt", "c.txt", "data/rename_journal.jsonl"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cycle_failure_recovers_original_names() {
        let d = tmp("failcycle");
        mk(&d, &[("a.txt", "AAA"), ("b.txt", "BBBB")]);
        let plan = plan_of(&d, &[("a.txt", "b.txt"), ("b.txt", "a.txt")]);
        let j = d.join("j.jsonl");
        // 2번째(b→임시)만 실패 → a 의 임시 파일을 원래 이름으로 복구
        let (_, rep) = apply(&plan, &j, &fail(2, false)).unwrap_err();
        assert!(rep.stranded.is_empty());
        assert_eq!(names(&d), vec!["a.txt", "b.txt", "j.jsonl"]);
        assert_eq!(read(d.join("a.txt")), "AAA");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cycle_failure_with_blocked_recovery_reports_stranded() {
        let d = tmp("stranded");
        mk(&d, &[("a.txt", "AAA"), ("b.txt", "BBBB")]);
        let plan = plan_of(&d, &[("a.txt", "b.txt"), ("b.txt", "a.txt")]);
        let j = d.join("j.jsonl");
        // 3번째(임시→b)부터 모든 이름 변경이 실패 → 복구도 막혀 임시 이름 파일이 남고 보고되어야 한다
        let (_, rep) = apply(&plan, &j, &fail(3, true)).unwrap_err();
        let left: Vec<String> = names(&d).into_iter().filter(|n| n.contains(".rename-tmp-")).collect();
        assert_eq!(rep.stranded.len(), 2);
        assert_eq!(left.len(), 2);
        assert!(read(j).contains("\"stranded\":\"2\""));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn undo_skips_items_changed_externally_and_reports() {
        let d = tmp("external");
        mk(&d, &[("a.txt", "1"), ("b.txt", "22"), ("c.txt", "333"), ("d.txt", "4444")]);
        let m = model(&d, &["a.txt", "b.txt", "c.txt", "d.txt"], rules("n_"));
        let j = d.join("data").join(JOURNAL_FILE);
        apply_model(&m, &j, &RealFs).unwrap();
        std::fs::rename(d.join("n_a.txt"), d.join("외부가_바꿈.txt")).unwrap(); // 대상이 이미 사라짐
        std::fs::write(d.join("b.txt"), "원래 이름 자리를 다른 파일이 차지").unwrap(); // 원래 이름 사용 중
        std::fs::write(d.join("n_c.txt"), "내용이 바뀌어 크기가 다름!!").unwrap(); // 내용 변경
        let u = undo_last(&j, &RealFs).unwrap();
        assert_eq!(u.restored, 1);
        assert_eq!(u.skipped.len(), 3, "{:?}", u.skipped);
        assert!(d.join("d.txt").exists());
        assert!(d.join("외부가_바꿈.txt").exists() && d.join("n_b.txt").exists() && d.join("n_c.txt").exists());
        assert_eq!(read(d.join("b.txt")), "원래 이름 자리를 다른 파일이 차지");
        let text = read(j.clone());
        assert!(text.contains("undo_end") && text.contains("\"skipped\":\"3\""));
        assert_eq!(undoable_count(&j), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn undo_only_targets_last_run_and_survives_torn_line() {
        let d = tmp("two");
        mk(&d, &[("a.txt", "1"), ("b.txt", "2")]);
        let j = d.join("data").join(JOURNAL_FILE);
        apply_model(&model(&d, &["a.txt"], rules("x_")), &j, &RealFs).unwrap();
        apply_model(&model(&d, &["b.txt"], rules("y_")), &j, &RealFs).unwrap();
        let mut f = OpenOptions::new().append(true).open(&j).unwrap();
        f.write_all(b"{\"type\":\"item\",\"ru").unwrap();
        drop(f);
        undo_last(&j, &RealFs).unwrap();
        assert!(d.join("b.txt").exists() && d.join("x_a.txt").exists());
        undo_last(&j, &RealFs).unwrap();
        assert!(d.join("a.txt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn json_roundtrip_escapes() {
        let l = line(&[("k", V::S("a\"b\\c\n한글\u{1}")), ("n", V::N(12)), ("b", V::B(true))]);
        let o = parse_line(&l).unwrap();
        assert_eq!(o["k"], "a\"b\\c\n한글\u{1}");
        assert_eq!((o["n"].as_str(), o["b"].as_str()), ("12", "true"));
        assert!(parse_line("{\"a\":").is_none());
    }
}
