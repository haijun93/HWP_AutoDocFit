//! 합성 사진 목록(캡처·시험용). 실제 파일을 읽지 않는다: 같은 입력이면 같은 목록이 나온다.
use crate::model::{App, Doc, ScanState, ScanView, SortKey};
use grid_core::layout::GridSpec;

const EXTS: [&str; 8] = ["jpg", "jpg", "jpg", "jpeg", "png", "heic", "webp", "tif"];
const FOLDERS: [&str; 5] = [r"D:\사진\2026 현장 점검", r"D:\사진\민원 현장", r"C:\Users\공무원\Pictures", r"E:\공유\총무과\행사", r"D:\사진\공사 준공"];
const PLACES: [&str; 8] = ["현장 점검", "준공 검사", "도로 보수", "행사 스케치", "민원 확인", "공원 정비", "안전 점검", "시설물 촬영"];

/// 결정적 의사 난수(LCG).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// `n` 개 합성 사진. 이름에 번호가 들어 있어 모두 다르다(카메라 이름·메신저·스크린샷·설명 이름이 섞인다).
pub fn synthetic_photos(n: usize, seed: u64) -> Vec<Doc> {
    let mut r = Lcg(seed ^ 0x9E37_79B9_7F4A_7C15);
    (0..n)
        .map(|i| {
            let ext = EXTS[(r.next() % EXTS.len() as u64) as usize];
            let folder = FOLDERS[(r.next() % FOLDERS.len() as u64) as usize];
            // 2023-01-01 ~ 2026-11-20 사이.
            let mtime = 1_672_531_200.0 + (r.next() % 111_000_000) as f64;
            let stamp = crate::model::format_date(mtime, 9 * 3600).replace('-', "");
            let name = match r.next() % 4 {
                0 => format!("IMG_{stamp}_{:04}.{ext}", i + 1),
                1 => format!("DSC{:05}.{ext}", i + 1),
                2 => format!("{} {:05}.{ext}", PLACES[(r.next() % PLACES.len() as u64) as usize], i + 1),
                _ => format!("KakaoTalk_{stamp}_{:05}.{ext}", i + 1),
            };
            let size = 120_000 + r.next() % (9 * 1024 * 1024);
            Doc { path: format!("{folder}\\{name}"), name, ext: ext.into(), size, mtime, cloud: r.next() % 40 == 0 }
        })
        .collect()
}

/// 합성 사진을 `klib scan` 의 JSON 줄 형식으로 직렬화(파서 시험과 합성 스캔 소스용).
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

/// 새 사진뷰 화면 상태(사진 격자 규칙·기본 160·최신 사진 먼저).
pub fn new_app(thumb: f64) -> App {
    let mut app = App::with_spec(GridSpec::PHOTO, crate::settings::DEFAULT_PHOTO_THUMB, thumb);
    app.coll.set_sort(SortKey::Modified, true);
    app
}

/// 장면 이름 → 화면 상태(합성 모델). 창 크기·dpi 는 스크롤·선택 위치 계산에 쓴다.
pub fn scene(name: &str, w: i32, h: i32, dpi: u32, thumb: f64) -> Result<App, String> {
    let mut app = new_app(thumb);
    app.engine = "합성 모델".into();
    app.tz_offset = 9 * 3600;
    app.excluded_count = 2;
    let running = |visited: usize, folder: &str, secs: u64| ScanView { state: ScanState::Running, visited, folder: folder.into(), elapsed_secs: secs };
    let complete = |n: usize, secs: u64| ScanView { state: ScanState::Complete, visited: n, folder: String::new(), elapsed_secs: secs };
    match name {
        "list" => {
            app.coll.replace(synthetic_photos(400, 1));
            app.scan = complete(400, 7);
            let m = app.metrics(w, h, dpi);
            app.select(&m, Some(9));
        }
        "search" => {
            app.coll.replace(synthetic_photos(400, 1));
            app.coll.set_query("점검");
            app.scan = complete(400, 7);
            let m = app.metrics(w, h, dpi);
            app.select(&m, Some(2));
        }
        "scanning" => {
            app.coll.replace(synthetic_photos(1240, 2));
            app.scan = running(1240, r"D:\사진\2026 현장 점검", 12);
            let m = app.metrics(w, h, dpi);
            app.select(&m, Some(3));
        }
        "scanning_empty" => app.scan = running(215, r"C:\Users\공무원\Pictures", 2),
        "idle" => {}
        "empty" => app.scan = complete(0, 31),
        "failed" => app.scan = ScanView { state: ScanState::Failed("klib 실행 파일을 찾지 못했습니다(DOCFIT_KLIB 또는 --klib 로 지정).".into()), ..ScanView::default() },
        "virtual" => {
            app.coll.replace(synthetic_photos(50_000, 3));
            app.scan = complete(50_000, 94);
            let m = app.metrics(w, h, dpi);
            app.select(&m, Some(31_337));
        }
        other => return Err(format!("알 수 없는 장면: {other} ({})", SCENES.join(", "))),
    }
    Ok(app)
}
