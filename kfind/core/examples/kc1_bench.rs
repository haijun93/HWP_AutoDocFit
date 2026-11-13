//! KC1 성능 측정 도구(IS-S1과 같은 합성 말뭉치 형식: `문서번호\t위계순위\t문장`).
//!
//! kc1_bench build <corpus.tsv> <색인 폴더> <커밋 간격(문서 수, 0=한 번)>   색인 만들기 + 하나로 합치기
//! kc1_bench build-segments (같은 인자)                                      합치지 않고 조각 여러 개로 둠
//! kc1_bench query <색인 폴더> <검색어 파일(한 줄에 하나)> <반복 수>          방법(off·dict·bigram)별 지연
//! kc1_bench update <색인 폴더> <corpus.tsv> <문서 수>                        문서 N개 교체 → 커밋 → 합치기 지연
//! 결과는 JSON 한 덩어리로 표준 출력에 쓴다.
use kfind_core::{Index, SearchOptions, Sentence, SplitMode, Writer, WriterOptions};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

#[cfg(windows)]
fn memory_mb() -> (f64, f64) {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: 구조체는 0으로 채워도 유효하고, 크기를 cb에 적어 넘긴다.
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: 현재 프로세스 핸들과 방금 만든 구조체를 넘긴다.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok == 0 {
        return (0.0, 0.0);
    }
    (c.PeakWorkingSetSize as f64 / 1048576.0, c.PeakPagefileUsage as f64 / 1048576.0)
}

#[cfg(not(windows))]
fn memory_mb() -> (f64, f64) {
    (0.0, 0.0)
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
}

/// 말뭉치를 문서 단위로 읽어 `f(문서 번호, [(위계, 문장)])`를 부른다.
fn read_docs(path: &Path, mut f: impl FnMut(u64, &[(u8, String)])) {
    let reader = BufReader::with_capacity(1 << 20, std::fs::File::open(path).expect("corpus"));
    let mut cur: Option<u64> = None;
    let mut sents: Vec<(u8, String)> = Vec::new();
    for line in reader.lines() {
        let line = line.expect("line");
        let mut parts = line.splitn(3, '\t');
        let doc: u64 = parts.next().expect("doc").parse().expect("doc number");
        let level: u8 = parts.next().expect("level").parse().expect("level");
        let text = parts.next().unwrap_or("").to_string();
        if cur != Some(doc) {
            if let Some(d) = cur {
                f(d, &sents);
            }
            sents.clear();
            cur = Some(doc);
        }
        sents.push((level, text));
    }
    if let Some(d) = cur {
        f(d, &sents);
    }
}

fn add(w: &mut Writer, doc: u64, sents: &[(u8, String)]) {
    let v: Vec<Sentence> = sents.iter().enumerate().map(|(i, (level, text))| Sentence { para: i as u32, level: *level, text }).collect();
    w.add_document(doc, 0, &v).expect("add_document");
}

fn stats(samples: &[f64]) -> String {
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let p = |q: f64| s[((s.len() as f64 * q).ceil() as usize).max(1) - 1];
    format!("{{\"p50_ms\": {:.4}, \"p95_ms\": {:.4}, \"max_ms\": {:.4}}}", p(0.5), p(0.95), s[s.len() - 1])
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn build(corpus: &Path, dir: &Path, every: usize, merge: bool) {
    let _ = std::fs::remove_dir_all(dir);
    let t = Instant::now();
    let mut w = Writer::create(dir, WriterOptions::default()).expect("writer");
    let mut docs = 0usize;
    read_docs(corpus, |doc, sents| {
        add(&mut w, doc, sents);
        docs += 1;
        if every > 0 && docs % every == 0 {
            w.commit().expect("commit");
        }
    });
    w.commit().expect("commit");
    let build_s = t.elapsed().as_secs_f64();
    let segments = w.num_segments();
    let size_before = dir_size(dir);
    let t = Instant::now();
    let merged = merge && w.merge_all().expect("merge");
    let merge_s = t.elapsed().as_secs_f64();
    let index = Index::open(dir).expect("open");
    let (ws, private) = memory_mb();
    println!(
        "{{\"docs\": {docs}, \"sentences\": {}, \"build_s\": {build_s:.3}, \"segments_before_merge\": {segments}, \"bytes_before_merge\": {size_before}, \"merged\": {merged}, \"merge_s\": {merge_s:.3}, \"bytes\": {}, \"peak_working_set_mb\": {ws:.1}, \"peak_private_mb\": {private:.1}}}",
        index.num_sentences(),
        dir_size(dir)
    );
}

fn query(dir: &Path, queries: &Path, reps: usize) {
    let t = Instant::now();
    let index = Index::open(dir).expect("open");
    let open_ms = ms(t);
    let list: Vec<String> = std::fs::read_to_string(queries).expect("queries").lines().map(str::to_string).filter(|l| !l.is_empty()).collect();
    let mut rows: Vec<String> = Vec::new();
    for q in &list {
        for (name, mode) in [("off", SplitMode::Off), ("dict", SplitMode::DictLongest), ("bigram", SplitMode::Bigram)] {
            let top = SearchOptions { top: 20, split: mode, time_limit: None, cancel: None, refs_per_doc: 3, ..SearchOptions::default() };
            let full = SearchOptions { top: 0, refs_per_doc: 0, ..top.clone() };
            let t = Instant::now();
            let first = index.search(q, &top).expect("search");
            let first_ms = ms(t);
            let (mut top_s, mut full_s) = (Vec::with_capacity(reps), Vec::with_capacity(reps));
            for _ in 0..reps {
                let t = Instant::now();
                let r = index.search(q, &top).expect("search");
                top_s.push(ms(t));
                std::hint::black_box(r);
                let t = Instant::now();
                let r = index.search(q, &full).expect("search");
                full_s.push(ms(t));
                std::hint::black_box(r);
            }
            let words: Vec<String> = first.words.iter().map(|w| match &w.pieces {
                Some(p) => format!("{}={}", w.text, p.join("+")),
                None => w.text.clone(),
            }).collect();
            let top20: Vec<String> = first.hits.iter().map(|h| format!("[{}, {}, {}, {}]", h.doc_id, h.level, h.count, h.adjacent as u8)).collect();
            rows.push(format!(
                "{{\"q\": {}, \"mode\": \"{name}\", \"words\": {}, \"sentences\": {}, \"docs\": {}, \"adjacent_docs\": {}, \"first_ms\": {first_ms:.4}, \"top20\": {}, \"full\": {}, \"top20_hits\": [{}]}}",
                json_str(q),
                json_str(&words.join(" ")),
                first.total_sentences,
                first.total_docs,
                first.adjacent_docs,
                stats(&top_s),
                stats(&full_s),
                top20.join(", ")
            ));
        }
    }
    let (ws, private) = memory_mb();
    println!(
        "{{\"open_ms\": {open_ms:.3}, \"segments\": {}, \"sentences\": {}, \"peak_working_set_mb\": {ws:.1}, \"peak_private_mb\": {private:.1}, \"queries\": [\n{}\n]}}",
        index.num_segments(),
        index.num_sentences(),
        rows.join(",\n")
    );
}

fn update(dir: &Path, corpus: &Path, n: usize) {
    let mut w = Writer::open(dir, WriterOptions::default()).expect("writer");
    let mut picked: Vec<(u64, Vec<(u8, String)>)> = Vec::new();
    read_docs(corpus, |doc, sents| {
        if picked.len() < n {
            picked.push((doc, sents.to_vec()));
        }
    });
    let t = Instant::now();
    for (doc, sents) in &picked {
        add(&mut w, *doc, sents);
    }
    let add_ms = ms(t);
    let t = Instant::now();
    w.commit().expect("commit");
    let commit_ms = ms(t);
    let mut reader = Index::open(dir).expect("open");
    let segments = reader.num_segments();
    let t = Instant::now();
    let job = w.plan_merge_all().expect("merge job");
    let out = std::thread::spawn(move || job.run()).join().expect("thread").expect("merge");
    let merge_run_ms = ms(t);
    let t = Instant::now();
    w.apply_merge(out).expect("apply");
    let apply_ms = ms(t);
    let t = Instant::now();
    reader.reload().expect("reload");
    let reload_ms = ms(t);
    println!(
        "{{\"docs\": {n}, \"add_ms\": {add_ms:.3}, \"commit_ms\": {commit_ms:.3}, \"segments_after_commit\": {segments}, \"merge_run_ms\": {merge_run_ms:.3}, \"merge_apply_ms\": {apply_ms:.3}, \"reader_reload_ms\": {reload_ms:.3}, \"sentences\": {}}}",
        reader.num_sentences()
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("build") if a.len() == 5 => build(Path::new(&a[2]), Path::new(&a[3]), a[4].parse().expect("commit interval"), true),
        Some("build-segments") if a.len() == 5 => build(Path::new(&a[2]), Path::new(&a[3]), a[4].parse().expect("commit interval"), false),
        Some("query") if a.len() == 5 => query(Path::new(&a[2]), Path::new(&a[3]), a[4].parse().expect("reps")),
        Some("update") if a.len() == 5 => update(Path::new(&a[2]), Path::new(&a[3]), a[4].parse().expect("docs")),
        _ => {
            eprintln!("usage: kc1_bench build|build-segments <corpus.tsv> <index dir> <commit every N docs> | query <index dir> <queries.txt> <reps> | update <index dir> <corpus.tsv> <docs>");
            std::process::exit(2);
        }
    }
}
