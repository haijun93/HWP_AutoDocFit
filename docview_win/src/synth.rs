//! 합성 데이터 모델(캡처·시험용). 실제 파일을 읽지 않는다: 같은 입력이면 같은 목록이 나온다.
use crate::model::Doc;

const TOPICS: [&str; 12] = [
    "업무계획(안)", "예산 편성 검토", "주민 설명회 자료", "회의록", "민원 처리 현황", "보도자료", "공사 계약 변경",
    "안전 점검 결과", "정책 제안서", "월간 실적 보고", "사업 추진 계획", "감사 대응 자료",
];
const EXTS: [&str; 7] = ["hwp", "hwpx", "xlsx", "pptx", "pdf", "xls", "ppt"];
const FOLDERS: [&str; 5] = [r"D:\업무\기획", r"D:\업무\예산", r"C:\Users\공무원\Documents", r"E:\공유\총무과", r"D:\업무\민원"];

/// 결정적 의사 난수(LCG).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// `n` 개 합성 문서. 이름에 번호가 들어 있어 모두 다르다.
pub fn synthetic_docs(n: usize, seed: u64) -> Vec<Doc> {
    let mut r = Lcg(seed ^ 0x9E37_79B9_7F4A_7C15);
    (0..n)
        .map(|i| {
            let topic = TOPICS[(r.next() % TOPICS.len() as u64) as usize];
            let ext = EXTS[(r.next() % EXTS.len() as u64) as usize];
            let folder = FOLDERS[(r.next() % FOLDERS.len() as u64) as usize];
            let year = 2022 + r.next() % 5;
            let name = format!("{year}년 {topic} {:05}.{ext}", i + 1);
            let size = 4096 + r.next() % (40 * 1024 * 1024);
            // 2022-01-01 ~ 2026-11-20 사이.
            let mtime = 1_640_995_200.0 + (r.next() % 148_000_000) as f64;
            Doc { path: format!("{folder}\\{name}"), name, ext: ext.into(), size, mtime, cloud: r.next() % 40 == 0 }
        })
        .collect()
}

/// 합성 문서를 `klib scan` 의 JSON 줄 형식으로 직렬화(파서 시험과 합성 스캔 소스용).
pub fn to_json_line(d: &Doc) -> String {
    fn esc(s: &str) -> String {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if c < ' ' => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }
    format!(
        "{{\"path\":{},\"name\":{},\"ext\":{},\"size\":{},\"mtime\":{},\"mtime_ns\":{},\"cloud\":{}}}",
        esc(&d.path),
        esc(&d.name),
        esc(&d.ext),
        d.size,
        d.mtime,
        (d.mtime * 1e9) as i128,
        d.cloud
    )
}

/// 캡처 장면 이름.
pub const SCENES: [&str; 8] = ["list", "search", "scanning", "scanning_empty", "idle", "empty", "failed", "virtual"];

/// 장면 이름 → 화면 상태(합성 모델). 창 크기·dpi 는 스크롤·선택 위치 계산에 쓴다.
pub fn scene(name: &str, w: i32, h: i32, dpi: u32, thumb: f64) -> Result<crate::model::App, String> {
    use crate::model::{App, ScanState, ScanView, SortKey};
    let mut app = App::new(thumb);
    app.engine = "합성 모델".into();
    app.tz_offset = 9 * 3600;
    app.excluded_count = 2;
    let running = |visited: usize, folder: &str, secs: u64| ScanView { state: ScanState::Running, visited, folder: folder.into(), elapsed_secs: secs };
    let complete = |n: usize, secs: u64| ScanView { state: ScanState::Complete, visited: n, folder: String::new(), elapsed_secs: secs };
    match name {
        "list" => {
            app.coll.replace(synthetic_docs(400, 1));
            app.scan = complete(400, 7);
            let m = app.metrics(w, h, dpi);
            app.select(&m, Some(9));
        }
        "search" => {
            app.coll.replace(synthetic_docs(400, 1));
            app.coll.set_query("예산");
            app.coll.set_sort(SortKey::Modified, true);
            app.scan = complete(400, 7);
            let m = app.metrics(w, h, dpi);
            app.select(&m, Some(2));
        }
        "scanning" => {
            app.coll.replace(synthetic_docs(1240, 2));
            app.scan = running(1240, r"D:\업무\예산\2026년 본예산", 12);
            let m = app.metrics(w, h, dpi);
            app.select(&m, Some(3));
        }
        "scanning_empty" => app.scan = running(215, r"C:\Users\공무원\Documents", 2),
        "idle" => {}
        "empty" => app.scan = complete(0, 31),
        "failed" => app.scan = ScanView { state: ScanState::Failed("klib 실행 파일을 찾지 못했습니다(DOCFIT_KLIB 또는 --klib 로 지정).".into()), ..ScanView::default() },
        "virtual" => {
            app.coll.replace(synthetic_docs(50_000, 3));
            app.scan = complete(50_000, 94);
            let m = app.metrics(w, h, dpi);
            app.select(&m, Some(31_337));
        }
        other => return Err(format!("알 수 없는 장면: {other} ({})", SCENES.join(", "))),
    }
    Ok(app)
}
