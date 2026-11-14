//! FN1 측정: 이 PC 고정 드라이브 전체 이름 색인 → 질의 지연(콜드·웜 p50/p95, 입력 연속) → Everything(IPC) 같은 질의 비교.
//! 파일 내용은 읽지 않는다(이름·속성만). 결과 이름은 출력하지 않고 개수만 적는다.
//!
//! cargo run --release --example fn1_bench -- --out <출력 폴더> [--runs 30] [--everything] [--kfind-exe PATH]
#![cfg(windows)]
use std::ffi::c_void;
use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use kfind_names::query::{hits, search};
use kfind_names::{Builder, NamesSeg, Options, Searcher, Sort, ntfs, parse, seg, walk};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetSystemTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_VM_READ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn mem_of(h: *mut c_void) -> (usize, usize) {
    let mut c: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    unsafe { K32GetProcessMemoryInfo(h, &mut c as *mut _ as *mut _, c.cb) };
    (c.PrivateUsage, c.WorkingSetSize)
}

fn my_mem() -> (usize, usize) {
    mem_of(unsafe { GetCurrentProcess() })
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn sys_times() -> (u64, u64) {
    let (mut i, mut k, mut u) =
        unsafe { (std::mem::zeroed(), std::mem::zeroed(), std::mem::zeroed()) };
    unsafe { GetSystemTimes(&mut i, &mut k, &mut u) };
    (ft(i), ft(k) + ft(u))
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() {
        return f64::NAN;
    }
    let k = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[k]
}

// ───────────── Everything IPC (SDK의 공개 프로토콜: WM_COPYDATA + EVERYTHING_IPC_QUERY2W) ─────────────
const EV_CLASS: &str = "EVERYTHING_TASKBAR_NOTIFICATION";
const EV_QUERY2W: usize = 18;
const EV_REPLY_ID: u32 = 0x464E_3142; // 임의 회신 번호
const EV_REQUEST_NAME: u32 = 0x1;
const EV_REQUEST_PATH: u32 = 0x2;
const EV_SORT_NAME_ASC: u32 = 1;
const EV_IS_DB_LOADED: usize = 401;

static REPLY: Mutex<Option<Vec<u8>>> = Mutex::new(None);

unsafe extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_COPYDATA {
        let cds = unsafe { &*(l as *const COPYDATASTRUCT) };
        if cds.dwData == EV_REPLY_ID as usize {
            let b =
                unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) }
                    .to_vec();
            *REPLY.lock().unwrap() = Some(b);
            return 1;
        }
    }
    unsafe { DefWindowProcW(h, m, w, l) }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

struct Everything {
    ev: HWND,
    me: HWND,
    pid: u32,
}

impl Everything {
    fn find() -> Option<Everything> {
        let cls = wide(EV_CLASS);
        let ev = unsafe { FindWindowW(cls.as_ptr(), std::ptr::null()) };
        if ev.is_null() {
            return None;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(ev, &mut pid) };
        let cname = wide("FN1BenchReply");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cname.as_ptr(),
        };
        unsafe { RegisterClassW(&wc) };
        let me = unsafe {
            CreateWindowExW(
                0,
                cname.as_ptr(),
                cname.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                wc.hInstance,
                std::ptr::null(),
            )
        };
        Some(Everything { ev, me, pid })
    }

    fn db_loaded(&self) -> bool {
        unsafe { SendMessageW(self.ev, WM_USER, EV_IS_DB_LOADED, 0) != 0 }
    }

    fn version(&self) -> String {
        let g = |k: usize| unsafe { SendMessageW(self.ev, WM_USER, k, 0) };
        format!("{}.{}.{}.{}", g(0), g(1), g(2), g(3))
    }

    /// (전체 개수, 받은 개수, 걸린 ms). 질의 보내기 → 회신 받기까지.
    fn query(&self, text: &str, max: u32) -> Option<(u32, u32, f64)> {
        let mut b: Vec<u8> = Vec::new();
        for v in [
            self.me as usize as u32,
            EV_REPLY_ID,
            0,
            0,
            max,
            EV_REQUEST_NAME | EV_REQUEST_PATH,
            EV_SORT_NAME_ASC,
        ] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for c in text.encode_utf16().chain([0]) {
            b.extend_from_slice(&c.to_le_bytes());
        }
        let cds = COPYDATASTRUCT {
            dwData: EV_QUERY2W,
            cbData: b.len() as u32,
            lpData: b.as_mut_ptr() as *mut c_void,
        };
        *REPLY.lock().unwrap() = None;
        let t = Instant::now();
        let ok = unsafe {
            SendMessageW(
                self.ev,
                WM_COPYDATA,
                self.me as usize,
                &cds as *const _ as isize,
            )
        };
        if ok == 0 {
            return None;
        }
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
                unsafe { DispatchMessageW(&msg) };
            }
            if let Some(r) = REPLY.lock().unwrap().take() {
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                if r.len() < 20 {
                    return None;
                }
                let tot = u32::from_le_bytes(r[0..4].try_into().unwrap());
                let num = u32::from_le_bytes(r[4..8].try_into().unwrap());
                return Some((tot, num, ms));
            }
            if t.elapsed() > Duration::from_secs(60) {
                return None;
            }
        }
    }

    fn mem(&self) -> Option<(usize, usize)> {
        let h = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                0,
                self.pid,
            )
        };
        if h.is_null() {
            return None;
        }
        let m = mem_of(h);
        unsafe { CloseHandle(h) };
        Some(m)
    }
}

/// 우리 문법 → Everything 문법(제외 `-x` → `!x`). 나머지(공백 AND·ext:·folder:·file:)는 같다.
fn to_everything(q: &str) -> String {
    q.split_whitespace()
        .map(|t| {
            if let Some(r) = t.strip_prefix('-') {
                format!("!{r}")
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ───────────── kfind 파이프(사용자 모드 실행 파일) ─────────────
struct Pipe(String);
impl Pipe {
    fn call(&self, json: &str) -> Option<(String, f64)> {
        let t = Instant::now();
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.0)
            .ok()?;
        let b = json.as_bytes();
        let mut out = (b.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(b);
        f.write_all(&out).ok()?;
        let mut l = [0u8; 4];
        f.read_exact(&mut l).ok()?;
        let mut r = vec![0u8; u32::from_le_bytes(l) as usize];
        f.read_exact(&mut r).ok()?;
        Some((
            String::from_utf8_lossy(&r).to_string(),
            t.elapsed().as_secs_f64() * 1000.0,
        ))
    }
}

fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn jnum(s: &str, key: &str) -> Option<f64> {
    let k = format!("\"{key}\":");
    let p = s.find(&k)? + k.len();
    let e = s[p..].find([',', '}']).map(|x| p + x)?;
    s[p..e].trim().parse().ok()
}

/// es.exe(Everything 명령줄) 한 번: 프로세스 시작 → 상위 100건 이름 순 출력 → 끝날 때까지 ms. 출력은 버린다.
fn es_once(es: &str, q: &str) -> Option<f64> {
    let t = Instant::now();
    let mut c = std::process::Command::new(es);
    c.args(["-n", "100", "-sort", "name"]);
    c.args(q.split_whitespace());
    let st = c
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    st.success().then(|| t.elapsed().as_secs_f64() * 1000.0)
}

/// es.exe -get-result-count 로 Everything 개수.
fn es_count(es: &str, q: &str) -> Option<u64> {
    let o = std::process::Command::new(es)
        .arg("-get-result-count")
        .args(q.split_whitespace())
        .output()
        .ok()?;
    String::from_utf8_lossy(&o.stdout).trim().parse().ok()
}

fn arg(args: &[String], k: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == k).map(|w| w[1].clone())
}

const QUERIES: &[(&str, &str)] = &[
    ("1글자", "a"),
    ("1글자", "e"),
    ("1글자", "s"),
    ("1글자", "보"),
    ("2글자", "do"),
    ("2글자", "in"),
    ("2글자", "보고"),
    ("2글자", "문서"),
    ("3글자", "doc"),
    ("3글자", "win"),
    ("3글자", "보고서"),
    ("긴 낱말", "microsoft"),
    ("없는 말", "zqxjv"),
    ("여러 단어", "win sys"),
    ("여러 단어", "program files"),
    ("여러 단어", "microsoft office 16"),
    ("여러 단어", "보고 2026"),
    ("확장자", "ext:pdf"),
    ("확장자", "ext:hwp;hwpx"),
    ("확장자+낱말", "보고 ext:hwp"),
    ("확장자+낱말", "setup ext:exe"),
    ("제외", "report -pdf"),
    ("폴더만", "folder: data"),
];

const TYPING: &[&str] = &["보고서", "program", "setup.exe"];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = arg(&args, "--out").unwrap_or_else(|| "fn1-out".into());
    let runs: usize = arg(&args, "--runs")
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let use_ev = args.iter().any(|a| a == "--everything");
    let kfind_exe = arg(&args, "--kfind-exe");
    let es_exe = arg(&args, "--es");
    std::fs::create_dir_all(&out).unwrap();
    let mut rep = String::new();
    macro_rules! say { ($($t:tt)*) => {{ let s = format!($($t)*); println!("{s}"); rep.push_str(&s); rep.push('\n'); }} }

    let (i0, b0) = sys_times();
    let threads = rayon::current_num_threads();
    say!("# FN1 측정 원자료\n\n- 스레드: {threads}, 반복: {runs}");
    // ── 1. 색인 ──
    let m0 = my_mem();
    let t_all = Instant::now();
    let mut b = Builder::new();
    let mut vol_rows = Vec::new();
    let mut roots = Vec::new();
    for (letter, fs) in ntfs::fixed_drives() {
        let t = Instant::now();
        let before = b.len();
        let mut src = "walk";
        let mut note = String::new();
        if fs == "NTFS" {
            match ntfs::index_volume(&mut b, letter) {
                Ok(_) => src = "mft",
                Err(e) => note = format!("MFT 불가({e})"),
            }
        }
        let mut errs = 0;
        if src == "walk" {
            errs = walk::index_tree(&mut b, &format!("{letter}:")).errors;
        }
        roots.push(format!("{letter}:\\"));
        vol_rows.push(format!(
            "| {letter}: | {fs} | {src} | {} | {:.0} | {errs} | {note} |",
            b.len() - before,
            t.elapsed().as_secs_f64() * 1000.0
        ));
    }
    let t_collect = t_all.elapsed().as_secs_f64();
    let m_builder = my_mem();
    let t = Instant::now();
    let names = b.finish();
    let t_finish = t.elapsed().as_secs_f64();
    let m1 = my_mem();
    let v = names.view();
    let n = names.len();
    let name_bytes = names.view().name.len();
    let fold_bytes = names.view().fold.len();
    say!(
        "\n## 색인\n\n| 볼륨 | 파일 시스템 | 원천 | 항목 | ms | 못 연 폴더 | 비고 |\n|---|---|---|---|---|---|---|"
    );
    for r in &vol_rows {
        say!("{r}");
    }
    say!(
        "\n- 전체 항목 {n}(폴더 {}), 모으기 {:.2}s + DFS 정렬·배열 {:.2}s = {:.2}s",
        v.flags
            .iter()
            .filter(|&&f| f & kfind_names::flag::DIR != 0)
            .count(),
        t_collect,
        t_finish,
        t_collect + t_finish
    );
    say!(
        "- 색인 배열 {:.1}MB(항목당 {:.1}B: 원래 이름 {:.1}B·비교형 {:.1}B·고정 열 27B·레코드 표), 프로세스 개인 메모리 {:.1}MB(만들기 직후 {:.1}MB, 시작 {:.1}MB)",
        names.heap_bytes() as f64 / 1e6,
        names.heap_bytes() as f64 / n as f64,
        name_bytes as f64 / n as f64,
        fold_bytes as f64 / n as f64,
        m1.0 as f64 / 1e6,
        m_builder.0 as f64 / 1e6,
        m0.0 as f64 / 1e6
    );
    // ── 2. names.seg ──
    let segp = std::path::Path::new(&out).join("names.seg");
    let t = Instant::now();
    let seg_size = seg::write(&names, &segp).unwrap();
    let t_write = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let sg = NamesSeg::open(&segp, false).unwrap();
    let t_open = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let _sg2 = NamesSeg::open(&segp, true).unwrap();
    let t_open_crc = t.elapsed().as_secs_f64() * 1000.0;
    drop(_sg2);
    let sv = sg.view();
    let t = Instant::now();
    let r = search(
        &sv,
        &parse("보고서"),
        &Options {
            sort: Sort::Name,
            ..Default::default()
        },
    )
    .unwrap();
    let _ = hits(&sv, &r.top);
    let t_seg_first = t.elapsed().as_secs_f64() * 1000.0;
    say!(
        "\n## names.seg\n\n- 크기 {:.1}MB, 쓰기 {t_write:.0}ms, 매핑 열기(구조 검사) {t_open:.1}ms, CRC 포함 {t_open_crc:.1}ms, 연 뒤 첫 질의(보고서) {t_seg_first:.2}ms",
        seg_size as f64 / 1e6
    );

    // ── 3. Everything ──
    let ev = if use_ev { Everything::find() } else { None };
    if let Some(e) = &ev {
        let t = Instant::now();
        while !e.db_loaded() && t.elapsed() < Duration::from_secs(600) {
            std::thread::sleep(Duration::from_millis(200));
        }
        let ev_total = e.query("", 0).map(|x| x.0);
        let mem = e.mem();
        say!(
            "\n## Everything\n\n- 판 {}, DB 적재 {}, 전체 개체 {:?}, 클라이언트 개인 메모리 {:?}MB / 작업 집합 {:?}MB",
            e.version(),
            e.db_loaded(),
            ev_total,
            mem.map(|m| (m.0 as f64 / 1e5).round() / 10.0),
            mem.map(|m| (m.1 as f64 / 1e5).round() / 10.0)
        );
    } else {
        say!("\n## Everything\n\n- 미측정(클라이언트 창 없음 또는 --everything 없음)");
    }

    // ── 4. kfind 파이프(선택) ──
    let mut pipe: Option<(Pipe, std::process::Child)> = None;
    if let Some(exe) = &kfind_exe {
        let idx = std::path::Path::new(&out).join("kfind-user-index");
        std::fs::create_dir_all(&idx).unwrap();
        let pname = format!("\\\\.\\pipe\\FN1Bench.{}", std::process::id());
        let child = std::process::Command::new(exe)
            .args(["--user", "--index-root"])
            .arg(&idx)
            .args(["--pipe-name", &pname])
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let p = Pipe(pname);
        let rj = roots.iter().map(|r| jstr(r)).collect::<Vec<_>>().join(",");
        let t = Instant::now();
        let r = p.call(&format!("{{\"command\":\"names_build\",\"roots\":[{rj}]}}"));
        println!("names_build: {r:?}");
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let s = p
                .call("{\"command\":\"names_status\"}")
                .map(|x| x.0)
                .unwrap_or_default();
            if !s.contains("\"state\":\"building\"") {
                say!(
                    "\n## kfind 파이프(사용자 모드, 순회)\n\n- 만들기 {:.1}s, 항목 {:?}",
                    t.elapsed().as_secs_f64(),
                    jnum(&s, "entries")
                );
                break;
            }
        }
        pipe = Some((p, child));
    }

    // ── 5. 질의 ──
    say!(
        "\n## 질의 지연(ms, 상위 100건 + 경로 조립, 이름 순)\n\n| 종류 | 질의 | 우리 개수 | 우리 콜드 | 우리 p50 | 우리 p95 | 관련도 순 p50 | 파이프 p50 | 파이프 p95 | Everything 개수 | Ev 콜드 | Ev p50 | Ev p95 | 배수(Ev p50 / 우리 p50) |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    let name_opts = Options {
        sort: Sort::Name,
        ..Default::default()
    };
    let rel_opts = Options {
        sort: Sort::Relevance,
        ..Default::default()
    };
    let mut wins = 0;
    let mut cmp_n = 0;
    let mut es_rows: Vec<String> = Vec::new();
    for (k, (kind, qs)) in QUERIES.iter().enumerate() {
        let q = parse(qs);
        let t = Instant::now();
        let r = search(&v, &q, &name_opts).unwrap();
        let _h = hits(&v, &r.top);
        let cold = t.elapsed().as_secs_f64() * 1000.0;
        let mut w = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            let r = search(&v, &q, &name_opts).unwrap();
            let _h = hits(&v, &r.top);
            w.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let mut wr = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            let r = search(&v, &q, &rel_opts).unwrap();
            let _h = hits(&v, &r.top);
            wr.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let (p50, p95, r50) = (
            pct(&mut w.clone(), 0.5),
            pct(&mut w, 0.95),
            pct(&mut wr, 0.5),
        );
        let (mut pp50, mut pp95) = (f64::NAN, f64::NAN);
        if let Some((p, _)) = &pipe {
            let mut pw = Vec::new();
            for i in 0..runs {
                // 매번 다른 id. 좁히기를 피하려고 앞 질의와 다른 질의를 사이에 끼운다.
                let _ = p.call(&format!("{{\"command\":\"names_query\",\"id\":{},\"text\":\"zz{i}q\",\"limit\":1,\"sort\":\"name\"}}", 1_000_000 + k * 1000 + i));
                if let Some((_, ms)) = p.call(&format!("{{\"command\":\"names_query\",\"id\":{},\"text\":{},\"limit\":100,\"sort\":\"name\"}}", k * 1000 + i, jstr(qs))) {
                    pw.push(ms);
                }
            }
            pp50 = pct(&mut pw.clone(), 0.5);
            pp95 = pct(&mut pw, 0.95);
        }
        let mut evc = String::from("미측정 | - | - | -");
        let mut ratio = String::from("-");
        if let Some(e) = &ev {
            let eq = to_everything(qs);
            let first = e.query(&eq, 100);
            let mut ew = Vec::new();
            let mut etot = 0;
            for _ in 0..runs {
                if let Some((tot, _, ms)) = e.query(&eq, 100) {
                    ew.push(ms);
                    etot = tot;
                }
            }
            let (e50, e95) = (pct(&mut ew.clone(), 0.5), pct(&mut ew, 0.95));
            evc = format!(
                "{etot} | {:.2} | {e50:.2} | {e95:.2}",
                first.map(|f| f.2).unwrap_or(f64::NAN)
            );
            // 같은 조건 비교: 파이프가 있으면 파이프 왕복(프로세스 사이), 없으면 프로세스 안 지연.
            let ours = if pp50.is_finite() { pp50 } else { p50 };
            ratio = format!("{:.1}", e50 / ours);
            cmp_n += 1;
            if ours < e50 {
                wins += 1;
            }
        }
        say!(
            "| {kind} | `{qs}` | {} | {cold:.2} | {p50:.2} | {p95:.2} | {r50:.2} | {pp50:.2} | {pp95:.2} | {evc} | {ratio} |",
            r.total
        );
        if let Some(es) = &es_exe {
            let eq = to_everything(qs);
            let mut ew: Vec<f64> = (0..runs.min(15)).filter_map(|_| es_once(es, &eq)).collect();
            es_rows.push(format!(
                "| {kind} | `{qs}` | {} | {} | {:.1} | {:.1} | {:.2} |",
                r.total,
                es_count(es, &eq)
                    .map(|c| c.to_string())
                    .unwrap_or("-".into()),
                pct(&mut ew.clone(), 0.5),
                pct(&mut ew, 0.95),
                if pp50.is_finite() { pp50 } else { p50 }
            ));
        }
    }
    if !es_rows.is_empty() {
        say!(
            "\n## es.exe(Everything 명령줄 {}) — 프로세스 시작 포함 한 번 질의 ms(상위 100, 이름 순)\n\n| 종류 | 질의 | 우리 개수 | Ev 개수 | es p50 | es p95 | 우리(파이프 왕복 또는 프로세스 안) p50 |\n|---|---|---|---|---|---|---|",
            es_exe.as_deref().unwrap_or("")
        );
        for r in &es_rows {
            say!("{r}");
        }
    }
    if cmp_n > 0 {
        say!(
            "\n- Everything보다 빠른 질의: {wins}/{cmp_n} (기준: 파이프 왕복 p50이 있으면 그것, 없으면 프로세스 안 p50 vs Everything IPC p50)"
        );
    }
    // ── 6. 입력 연속(한 글자씩) ──
    say!(
        "\n## 입력 연속(한 글자씩 칠 때 매 입력 지연 ms, 우리 = Searcher 좁히기, p50/최대 over {runs}회)\n\n| 입력 | 우리 p50 | 우리 최대 | 좁힘 | Everything p50 | Ev 최대 |\n|---|---|---|---|---|---|"
    );
    for word in TYPING {
        let chars: Vec<char> = word.chars().collect();
        for k in 1..=chars.len() {
            let pre: String = chars[..k].iter().collect();
            let mut w = Vec::new();
            let mut nar = false;
            for _ in 0..runs {
                let mut s = Searcher::new();
                if k > 1 {
                    let prev: String = chars[..k - 1].iter().collect();
                    s.search(&v, names.generation(), &parse(&prev), &name_opts)
                        .unwrap();
                }
                let t = Instant::now();
                let r = s
                    .search(&v, names.generation(), &parse(&pre), &name_opts)
                    .unwrap();
                let _h = hits(&v, &r.top);
                w.push(t.elapsed().as_secs_f64() * 1000.0);
                nar = r.narrowed;
            }
            let mx = w.iter().cloned().fold(0.0, f64::max);
            let p50 = pct(&mut w, 0.5);
            let evs = if let Some(e) = &ev {
                let mut ew: Vec<f64> = (0..runs)
                    .filter_map(|_| e.query(&pre, 100).map(|x| x.2))
                    .collect();
                let emx = ew.iter().cloned().fold(0.0, f64::max);
                format!("{:.2} | {emx:.2}", pct(&mut ew, 0.5))
            } else {
                "미측정 | -".into()
            };
            say!("| `{pre}` | {p50:.2} | {mx:.2} | {nar} | {evs} |");
        }
    }
    let (i1, b1) = sys_times();
    let busy = 1.0 - (i1 - i0) as f64 / (b1 - b0).max(1) as f64;
    say!(
        "\n- 측정 동안 시스템 CPU 사용률(GetSystemTimes) {:.1}%",
        busy * 100.0
    );
    if let Some((_, mut c)) = pipe {
        let _ = c.kill();
    }
    std::fs::write(std::path::Path::new(&out).join("FN1_BENCH_RAW.md"), rep).unwrap();
    let _ = std::fs::remove_file(&segp);
}
