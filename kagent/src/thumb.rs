//! 그림 항목 미리보기용 축소 썸네일(사용자 요청 2026-11-13).
//!
//! - 마우스·키보드로 그림 항목을 가리킬 때(앱의 `thumb` 요청 포함) 없으면 바로 만든다. 그림·화면캡처 항목을
//!   기록하면 `prewarm`이 쉬는 때 미리 만들어 둔다(KCAP2, 첫 미리보기 50ms 목표).
//! - 메모리 캐시(최근 것부터, 합계 `MEM_CAP` 바이트)와 디스크 캐시(`thumbs\`, DPAPI 암호화, 합계 `DISK_CAP`).
//!   디스크 캐시는 에이전트를 다시 켠 뒤 큰 그림을 다시 풀지 않게 하려는 것이며, 클립보드 내용 보호 원칙에
//!   맞춰 평문으로 두지 않는다. 모두 지우기 때 함께 지운다.
//! - 단계형(KCAP3, 측정 표는 docs/KAGENT_DESIGN.md): 출처별로 정해진 순서(`steps`)대로 해 보고 처음 된 것을 쓴다.
//!   | 출처 | 단계 |
//!   |---|---|
//!   | 저장 그림(PNG) | 메모리 → 디스크(QOI) → PNG 줄 단위 풀기+줄 단위 박스 축소 |
//!   | 클립보드 비트맵(CF_DIB) | 기록 때 메모리 비트맵에서 바로 축소해 캐시에 넣음(`prime`, 인코드·디코드 없음) |
//!   | PNG 파일 | 메모리 → 디스크 → PNG 줄 단위(새 캡처는 탐색기 캐시에 없어 캐시 조회는 손해) |
//!   | 그 밖 그림 파일(JPG 등) | 메모리 → 디스크 → 탐색기 캐시만 조회 → WIC 축소 해독 → 탐색기 썸네일 만들기 |
//!   어느 단계에서 만들었는지는 `status.json`의 `thumb_steps`(단계별 성공 수)로 본다.
//! - 줄 단위 축소(`boxscale`)는 예전 화소 평균과 같은 값이며 원본 전체 해상도 버퍼를 만들지 않는다.
//! 순수 계산(크기 맞춤·축소·캐시 한도·단계 순서)은 따로 시험한다.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::boxscale::{self, BoxScaler, Layout};

/// 썸네일 긴 변 최대(물리 픽셀).
pub const MAX_SIDE: u32 = 320;
/// 메모리 캐시 합계 상한(약 4MB — 16:9 썸네일 17장 정도).
pub const MEM_CAP: usize = 4 * 1024 * 1024;
/// 디스크 캐시 합계 상한.
pub const DISK_CAP: u64 = 16 * 1024 * 1024;

/// 위에서 아래로 쌓인 BGRA(알파는 흰 바탕에 합성해 255).
#[derive(Clone, Debug, PartialEq)]
pub struct Thumb {
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

/// 비율을 지키며 긴 변이 `max`를 넘지 않게(작은 그림은 키우지 않음).
pub fn fit(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (0, 0);
    }
    if w <= max && h <= max {
        return (w, h);
    }
    if w >= h {
        (max, ((h as u64 * max as u64 + w as u64 / 2) / w as u64).max(1) as u32)
    } else {
        (((w as u64 * max as u64 + h as u64 / 2) / h as u64).max(1) as u32, max)
    }
}

fn layout_of(channels: usize) -> Option<Layout> {
    Some(match channels {
        1 => Layout::Gray,
        2 => Layout::GrayA,
        3 => Layout::Rgb,
        4 => Layout::Rgba,
        _ => return None,
    })
}

/// 영역 평균 축소: 채널 1~4개(회색·회색+알파·RGB·RGBA) → 흰 바탕 합성 BGRA. `stride`는 원본 줄 바이트.
/// (KCAP3부터 `boxscale` 줄 단위 축소로 계산 — 값은 예전과 같다.)
pub fn downscale(src: &[u8], sw: u32, sh: u32, channels: usize, stride: usize, tw: u32, th: u32) -> Option<Vec<u8>> {
    if sw == 0 || sh == 0 || stride < sw as usize * channels {
        return None;
    }
    boxscale::scale(src, sw, sh, stride, layout_of(channels)?, false, tw, th)
}

/// PNG → 썸네일. 원본 크기 한도는 클립보드와 같다(가로·세로 2만, 1,600만 화소).
/// 한 줄씩 풀어 바로 줄이므로 원본 전체 해상도 버퍼를 만들지 않는다(엇갈림 PNG만 전체를 푼다).
pub fn from_png(data: &[u8], max: u32) -> Option<Thumb> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let info = decoder.read_header_info().ok()?;
    let (w, h) = (info.width, info.height);
    if w == 0 || h == 0 || w > crate::clip::MAX_DIB_DIMENSION || h > crate::clip::MAX_DIB_DIMENSION
        || (w as usize).checked_mul(h as usize)? > crate::clip::MAX_DIB_PIXELS
    {
        return None;
    }
    let mut reader = decoder.read_info().ok()?;
    let (tw, th) = fit(w, h, max);
    let (color, _) = reader.output_color_type();
    let layout = layout_of(color.samples())?;
    if reader.info().interlaced {
        let mut buf = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buf).ok()?;
        let bgra = downscale(&buf, frame.width, frame.height, frame.color_type.samples(), frame.line_size, tw, th)?;
        return Some(Thumb { w: tw, h: th, bgra });
    }
    let mut scaler = BoxScaler::new(w, h, tw, th, layout)?;
    while let Some(row) = reader.next_row().ok()? {
        if !scaler.push_row(row.data()) {
            return None;
        }
    }
    Some(Thumb { w: tw, h: th, bgra: scaler.finish()? })
}

/// 클립보드 CF_DIB(24·32비트) → 썸네일. 인코드·디코드 없이 메모리 비트맵에서 바로 줄인다.
/// 알파는 `clip::dib_to_png`와 같게 본다(32비트인데 알파가 모두 0이면 불투명) — 저장 PNG에서 만든 것과 같은 값.
pub fn from_dib(dib: &[u8], max: u32) -> Option<Thumb> {
    let u32_at = |o: usize| dib.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = u32_at(0)? as usize;
    let height = u32_at(8)? as i32;
    let bits = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]);
    let l = crate::clip::checked_dib_layout(u32_at(4)? as i32, height, bits, u32_at(16)?, header, dib.len())?;
    let pixels = dib.get(l.pixel_offset..l.pixel_offset.checked_add(l.pixel_bytes)?)?;
    let layout = if bits == 24 {
        Layout::Bgr
    } else if pixels.chunks_exact(l.stride).any(|line| line[..l.width * 4].chunks_exact(4).any(|p| p[3] != 0)) {
        Layout::Bgra
    } else {
        Layout::Bgrx
    };
    let (tw, th) = fit(l.width as u32, l.height as u32, max);
    let bgra = boxscale::scale(pixels, l.width as u32, l.height as u32, l.stride, layout, height > 0, tw, th)?;
    Some(Thumb { w: tw, h: th, bgra })
}

/// 디스크 캐시 형식(KCAP3): QOI(RGB). 같은 썸네일이 PNG보다 약 40% 작고 풀기가 빠르다.
pub fn to_disk(t: &Thumb) -> Option<Vec<u8>> {
    let rgb: Vec<u8> = t.bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
    qoi::encode_to_vec(&rgb, t.w, t.h).ok()
}

/// 디스크 캐시 풀기: QOI, 또는 KCAP3 전에 만든 PNG.
pub fn from_disk(data: &[u8]) -> Option<Thumb> {
    if !data.starts_with(b"qoif") {
        return from_png(data, MAX_SIDE);
    }
    let (head, px) = qoi::decode_to_vec(data).ok()?;
    let (w, h) = (head.width, head.height);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let ch = head.channels.as_u8() as usize;
    if px.len() != w as usize * h as usize * ch {
        return None;
    }
    let bgra = px.chunks_exact(ch).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
    Some(Thumb { w, h, bgra })
}

/// 썸네일 → PNG(RGB). 앱 전달용(파이프 `thumb`).
pub fn to_png(t: &Thumb) -> Option<Vec<u8>> {
    let mut rgb = Vec::with_capacity(t.w as usize * t.h as usize * 3);
    for px in t.bgra.chunks_exact(4) {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, t.w, t.h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgb).ok()?;
    }
    Some(out)
}

// ---- 메모리 캐시 ----

/// 최근 쓴 것이 뒤. 합계 바이트가 상한을 넘으면 앞에서부터 버린다.
#[derive(Default)]
pub struct MemCache {
    items: VecDeque<(String, Arc<Thumb>)>,
    bytes: usize,
    cap: usize,
}

impl MemCache {
    pub fn new(cap: usize) -> Self {
        MemCache { items: VecDeque::new(), bytes: 0, cap }
    }
    pub fn get(&mut self, key: &str) -> Option<Arc<Thumb>> {
        let pos = self.items.iter().position(|(k, _)| k == key)?;
        let item = self.items.remove(pos)?;
        let t = item.1.clone();
        self.items.push_back(item);
        Some(t)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.items.iter().any(|(k, _)| k == key)
    }
    pub fn put(&mut self, key: String, t: Arc<Thumb>) {
        if let Some(pos) = self.items.iter().position(|(k, _)| *k == key) {
            if let Some((_, old)) = self.items.remove(pos) {
                self.bytes -= old.bgra.len();
            }
        }
        if t.bgra.len() > self.cap {
            return;
        }
        self.bytes += t.bgra.len();
        self.items.push_back((key, t));
        while self.bytes > self.cap {
            let Some((_, old)) = self.items.pop_front() else { break };
            self.bytes -= old.bgra.len();
        }
    }
    pub fn clear(&mut self) {
        self.items.clear();
        self.bytes = 0;
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// 디스크 캐시 정리 계획: (이름, 크기, 수정 시각) 목록에서 합계가 상한 이하가 될 때까지 오래된 것부터 지울 이름.
pub fn disk_evictions(mut files: Vec<(String, u64, u64)>, cap: u64) -> Vec<String> {
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.2);
    let mut out = Vec::new();
    for (name, size, _) in files {
        if total <= cap {
            break;
        }
        total -= size;
        out.push(name);
    }
    out
}

/// 디스크 캐시 파일 이름(내용 지문, 경로·내용은 드러나지 않음).
pub fn disk_name(fingerprint: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in fingerprint.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}.thumb")
}

/// 미리보기를 만들 그림 파일 확장자(탐색기 썸네일로 만들 수 있는 흔한 형식).
pub fn is_image_path(path: &str) -> bool {
    matches!(crate::category::extension(path).as_deref(),
             Some("png" | "jpg" | "jpeg" | "jpe" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "heif" | "avif"))
}

/// 파일 항목에서 미리보기할 첫 그림 경로.
pub fn first_image_path(data: &[u8]) -> Option<String> {
    String::from_utf8_lossy(data)
        .split(['\n', '\0'])
        .map(str::trim)
        .find(|p| !p.is_empty() && is_image_path(p))
        .map(str::to_owned)
}

/// 메모리 캐시 키: 항목 번호와 크기(번호가 다시 쓰여도 다른 내용이면 다른 키).
pub fn mem_key(id: i64, size: i64) -> String {
    format!("{id}/{size}")
}

// ---- 시스템 연결(에이전트 실행 중에만) ----

static MEM: Mutex<Option<MemCache>> = Mutex::new(None);
/// 캐시 세대: 모두 지우기마다 하나 늘린다. 지우기 전에 시작한 생성 결과는 캐시에 넣지 않는다
/// (미리 만들기·즉시 생성이 지우기와 겹쳐도 지운 뒤에 썸네일이 되살아나지 않게).
static EPOCH: AtomicU64 = AtomicU64::new(0);

fn with_mem<R>(f: impl FnOnce(&mut MemCache) -> R) -> R {
    let mut guard = MEM.lock().unwrap();
    f(guard.get_or_insert_with(|| MemCache::new(MEM_CAP)))
}

pub fn cached(id: i64, size: i64) -> Option<Arc<Thumb>> {
    with_mem(|m| m.get(&mem_key(id, size)))
}

pub fn is_cached(id: i64, size: i64) -> bool {
    with_mem(|m| m.contains(&mem_key(id, size)))
}

fn disk_dir() -> String {
    format!("{}\\thumbs", crate::app().dir)
}

/// 메모리·디스크 캐시를 모두 지운다(모두 지우기·끄기 때).
pub fn clear_all() {
    EPOCH.fetch_add(1, Ordering::SeqCst);
    crate::prewarm::clear();
    with_mem(|m| m.clear());
    let _ = std::fs::remove_dir_all(disk_dir());
}

fn disk_get(name: &str) -> Option<Thumb> {
    let sealed = std::fs::read(format!("{}\\{name}", disk_dir())).ok()?;
    let png = crate::crypt::unprotect(&sealed)?;
    from_disk(&png)
}

fn disk_put(name: &str, t: &Thumb, epoch: u64) {
    let Some(sealed) = to_disk(t).and_then(|p| crate::crypt::protect(&p)) else { return };
    if EPOCH.load(Ordering::SeqCst) != epoch {
        return;
    }
    let dir = disk_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let tmp = format!("{dir}\\{name}.tmp");
    if std::fs::write(&tmp, &sealed).is_err() || std::fs::rename(&tmp, format!("{dir}\\{name}")).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    let files: Vec<(String, u64, u64)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
                    Some((e.file_name().to_string_lossy().into_owned(), m.len(), t))
                })
                .collect()
        })
        .unwrap_or_default();
    for old in disk_evictions(files, DISK_CAP) {
        let _ = std::fs::remove_file(format!("{dir}\\{old}"));
    }
}

/// 썸네일을 만드는 단계(KCAP3 단계형). 출처별 순서는 `steps`가 정하고, `get`은 그 순서대로 해 보다
/// 처음 된 단계에서 멈춘다(앞 단계가 실패·없음이면 다음 단계, 마지막까지 안 되면 None).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// 메모리 캐시(최근 것, 약 1ms).
    Memory,
    /// 디스크 캐시(DPAPI 봉인 QOI, 에이전트 재시작 뒤).
    Disk,
    /// 클립보드 비트맵에서 기록 때 바로 만든 것(`prime`). `get` 순서에는 없고 집계만 한다.
    Clipboard,
    /// PNG 줄 단위 풀기 + 줄 단위 박스 축소.
    Png,
    /// 탐색기 썸네일 캐시만 조회(만들지 않음).
    ShellCache,
    /// WIC 축소 해독(JPEG 1/2·1/4·1/8) 또는 줄 묶음 축소.
    Wic,
    /// 탐색기 썸네일 만들기(회전 정보·HEIC 등 마지막 수단).
    Shell,
}

pub const STEP_NAMES: [(Step, &str); 7] = [
    (Step::Memory, "memory"), (Step::Disk, "disk"), (Step::Clipboard, "clipboard"), (Step::Png, "png"),
    (Step::ShellCache, "shell_cache"), (Step::Wic, "wic"), (Step::Shell, "shell"),
];

/// 썸네일 출처 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// 저장 그림(클립보드 그림, 내용은 PNG).
    StoredImage,
    /// PNG 파일(화면캡처 폴더 등). 새 캡처는 탐색기 캐시에 없어 캐시 조회를 건너뛴다(측정: 없음 판정도 3ms+셸 적재).
    PngFile,
    /// 그 밖 그림 파일(JPG·BMP·GIF·WEBP·HEIC 등).
    OtherFile,
}

/// 출처별 단계 순서(순수 — 시험 대상).
pub fn steps(kind: SourceKind) -> &'static [Step] {
    match kind {
        SourceKind::StoredImage => &[Step::Memory, Step::Disk, Step::Png],
        // PNG 파일을 못 풀면(손상·50MB 넘음) 탐색기 썸네일로.
        SourceKind::PngFile => &[Step::Memory, Step::Disk, Step::Png, Step::Shell],
        SourceKind::OtherFile => &[Step::Memory, Step::Disk, Step::ShellCache, Step::Wic, Step::Shell],
    }
}

pub fn source_kind(path: &str) -> SourceKind {
    if path.to_ascii_lowercase().ends_with(".png") { SourceKind::PngFile } else { SourceKind::OtherFile }
}

/// 단계별 성공 수(`status.json`의 `thumb_steps`).
static STEP_HITS: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];

fn hit(step: Step) {
    if let Some(i) = STEP_NAMES.iter().position(|(s, _)| *s == step) {
        STEP_HITS[i].fetch_add(1, Ordering::Relaxed);
    }
}

pub fn steps_json() -> crate::json::Value {
    crate::json::Value::obj(STEP_NAMES.iter().enumerate()
        .map(|(i, (_, name))| (*name, (STEP_HITS[i].load(Ordering::Relaxed) as i64).into())).collect())
}

/// 항목 번호로 썸네일을 구한다(`steps` 순서). 그림 항목이 아니면 None.
/// 창 스레드에서 부르지 말 것(파일·복호화·풀기가 들어간다) — 미리보기 작업 스레드·파이프 스레드에서 부른다.
pub fn get(id: i64, size: i64) -> Option<Arc<Thumb>> {
    if let Some(t) = cached(id, size) {
        hit(Step::Memory);
        return Some(t);
    }
    let epoch = EPOCH.load(Ordering::SeqCst);
    // 저장 그림은 DB의 내용 지문으로 디스크 캐시를 먼저 본다(큰 그림을 복호화하지 않고).
    let (kind, hash) = crate::app().store.lock().unwrap().kind_hash(id).ok().flatten()?;
    if kind == "image" {
        if let Some(t) = disk_get(&disk_name(&hash)) {
            hit(Step::Disk);
            return Some(remember(id, size, t, epoch));
        }
    }
    let (kind, data, _) = crate::app().store.lock().unwrap().payload(id).ok().flatten()?;
    let (fingerprint, source, plan) = match kind.as_str() {
        "image" => (hash, Source::Png(data), steps(SourceKind::StoredImage)),
        "files" => {
            let path = first_image_path(&data)?;
            let meta = std::fs::metadata(&path).ok()?;
            let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
            let plan = steps(source_kind(&path));
            (format!("file:{path}:{}:{mtime}", meta.len()), Source::File(path), plan)
        }
        _ => return None,
    };
    let name = disk_name(&fingerprint);
    for &step in plan {
        let made = match (step, &source) {
            (Step::Memory, _) | (Step::Clipboard, _) => continue, // 위에서 봄 / 기록 때만
            (Step::Disk, _) => {
                if kind == "image" {
                    continue; // 지문으로 이미 봄
                }
                if let Some(t) = disk_get(&name) {
                    hit(Step::Disk);
                    return Some(remember(id, size, t, epoch));
                }
                continue;
            }
            (Step::Png, Source::Png(bytes)) => from_png(bytes, MAX_SIDE),
            (Step::Png, Source::File(path)) => png_file(path, MAX_SIDE),
            (Step::ShellCache, Source::File(path)) => shell_thumbnail(path, MAX_SIDE, true),
            (Step::Wic, Source::File(path)) => crate::wic::thumb_file(path, MAX_SIDE, crate::wic::Mode::Auto)
                .map(|(w, h, bgra)| Thumb { w, h, bgra }),
            (Step::Shell, Source::File(path)) => shell_thumbnail(path, MAX_SIDE, false),
            _ => None,
        };
        if let Some(t) = made {
            hit(step);
            disk_put(&name, &t, epoch);
            return Some(remember(id, size, t, epoch));
        }
    }
    None
}

/// 클립보드 비트맵에서 바로 만든 썸네일을 기록 직후 캐시에 넣는다(③ — 미리 만들기가 PNG를 다시 풀 필요 없음).
/// 넣었으면 true(이 항목은 미리 만들기 대기열에 넣지 않아도 된다).
pub fn prime(id: i64, t: Thumb) -> bool {
    let epoch = EPOCH.load(Ordering::SeqCst);
    let found = {
        let store = crate::app().store.lock().unwrap();
        store.size_of(id).ok().flatten().zip(store.kind_hash(id).ok().flatten())
    };
    let Some((size, (kind, hash))) = found else { return false };
    if kind != "image" {
        return false;
    }
    hit(Step::Clipboard);
    disk_put(&disk_name(&hash), &t, epoch);
    remember(id, size, t, epoch);
    true
}

fn remember(id: i64, size: i64, thumb: Thumb, epoch: u64) -> Arc<Thumb> {
    let t = Arc::new(thumb);
    with_mem(|m| {
        if EPOCH.load(Ordering::SeqCst) == epoch {
            m.put(mem_key(id, size), t.clone());
        }
    });
    t
}

enum Source {
    Png(Vec<u8>),
    File(String),
}

/// PNG 파일 → 썸네일(50MB 넘는 파일은 읽지 않음).
fn png_file(path: &str, max: u32) -> Option<Thumb> {
    if std::fs::metadata(path).ok()?.len() > 50 * 1024 * 1024 {
        return None;
    }
    from_png(&std::fs::read(path).ok()?, max)
}

/// 그림 파일 → 썸네일(캐시 없이 `steps` 순서의 만들기 단계만). 시험·도구용.
#[cfg_attr(not(test), allow(dead_code))]
pub fn from_file(path: &str, max: u32) -> Option<Thumb> {
    steps(source_kind(path)).iter().find_map(|step| match step {
        Step::Png => png_file(path, max),
        Step::ShellCache => shell_thumbnail(path, max, true),
        Step::Wic => crate::wic::thumb_file(path, max, crate::wic::Mode::Auto).map(|(w, h, bgra)| Thumb { w, h, bgra }),
        Step::Shell => shell_thumbnail(path, max, false),
        _ => None,
    })
}

/// Windows 탐색기 썸네일(IShellItemImageFactory::GetImage). `cache_only`면 시스템 썸네일 캐시에 있을 때만(만들지 않음).
/// COM은 스레드마다 한 번 켜 둔다(`wic::ensure_com`).
fn shell_thumbnail(path: &str, max: u32, cache_only: bool) -> Option<Thumb> {
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
        HBITMAP, ReleaseDC,
    };
    use windows_sys::Win32::UI::Shell::SHCreateItemFromParsingName;
    use windows_sys::core::{GUID, HRESULT};

    #[repr(C)]
    struct FactoryVtbl {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
        get_image: unsafe extern "system" fn(*mut std::ffi::c_void, SIZE, i32, *mut HBITMAP) -> HRESULT,
    }
    const IID_FACTORY: GUID = GUID::from_u128(0xbcc18b79_ba16_442f_80c4_8a59c30c463b);
    const SIIGBF_THUMBNAILONLY: i32 = 0x08;
    const SIIGBF_INCACHEONLY: i32 = 0x10;
    let flags = if cache_only { SIIGBF_THUMBNAILONLY | SIIGBF_INCACHEONLY } else { SIIGBF_THUMBNAILONLY };

    unsafe {
        crate::wic::ensure_com();
        let mut factory: *mut std::ffi::c_void = std::ptr::null_mut();
        let wide = crate::util::wide(path);
        let hr = SHCreateItemFromParsingName(wide.as_ptr(), std::ptr::null_mut(), &IID_FACTORY, &mut factory);
        let mut result = None;
        if hr >= 0 && !factory.is_null() {
            let vtbl = &**(factory as *mut *const FactoryVtbl);
            let mut hbmp: HBITMAP = std::ptr::null_mut();
            let size = SIZE { cx: max as i32, cy: max as i32 };
            if (vtbl.get_image)(factory, size, flags, &mut hbmp) >= 0 && !hbmp.is_null() {
                let mut bm: BITMAP = std::mem::zeroed();
                if GetObjectW(hbmp, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut _ as _) != 0
                    && bm.bmWidth > 0 && bm.bmHeight > 0
                {
                    let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
                    let mut bi: BITMAPINFO = std::mem::zeroed();
                    bi.bmiHeader = BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w as i32, biHeight: -(h as i32),
                        biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..std::mem::zeroed()
                    };
                    let mut px = vec![0u8; w as usize * h as usize * 4];
                    let dc = GetDC(std::ptr::null_mut());
                    let lines = GetDIBits(dc, hbmp, 0, h, px.as_mut_ptr() as _, &mut bi, DIB_RGB_COLORS);
                    ReleaseDC(std::ptr::null_mut(), dc);
                    if lines == h as i32 {
                        // 탐색기 썸네일의 알파는 0일 때가 많아 불투명(Bgrx)으로 본다.
                        let (tw, th) = fit(w, h, max);
                        result = boxscale::scale(&px, w, h, w as usize * 4, Layout::Bgrx, false, tw, th).map(|bgra| Thumb { w: tw, h: th, bgra });
                    }
                }
                DeleteObject(hbmp);
            }
            (vtbl.release)(factory);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_ratio_and_never_enlarges() {
        assert_eq!(fit(1920, 1080, 320), (320, 180));
        assert_eq!(fit(1080, 1920, 320), (180, 320));
        assert_eq!(fit(100, 50, 320), (100, 50));
        assert_eq!(fit(5000, 1, 320), (320, 1));
        assert_eq!(fit(0, 10, 320), (0, 0));
    }

    #[test]
    fn downscale_averages_and_composites_on_white() {
        // 2×1 RGB(검정·흰색) → 1×1 회색
        let out = downscale(&[0, 0, 0, 255, 255, 255], 2, 1, 3, 6, 1, 1).unwrap();
        assert_eq!(out, [127, 127, 127, 255]);
        // 완전 투명 빨강 → 흰색, 불투명 빨강 → BGRA 빨강
        assert_eq!(downscale(&[255, 0, 0, 0], 1, 1, 4, 4, 1, 1).unwrap(), [255, 255, 255, 255]);
        assert_eq!(downscale(&[255, 0, 0, 255], 1, 1, 4, 4, 1, 1).unwrap(), [0, 0, 255, 255]);
        // 줄 끝 여백(stride)이 있어도 맞게 읽는다.
        assert_eq!(downscale(&[9, 0, 7, 0], 1, 2, 1, 2, 1, 2).unwrap(), [9, 9, 9, 255, 7, 7, 7, 255]);
        assert!(downscale(&[0; 3], 2, 2, 3, 6, 1, 1).is_none(), "짧은 버퍼");
        assert!(downscale(&[0; 3], 1, 1, 5, 5, 1, 1).is_none(), "채널 수");
    }

    fn png_of(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(&vec![200u8; (w * h * 4) as usize]).unwrap();
        }
        out
    }

    #[test]
    fn png_thumbnail_roundtrip_and_speed() {
        let src = png_of(1920, 1080);
        let t0 = std::time::Instant::now();
        let t = from_png(&src, MAX_SIDE).unwrap();
        let ms = t0.elapsed().as_millis();
        assert_eq!((t.w, t.h), (320, 180));
        assert_eq!(t.bgra.len(), 320 * 180 * 4);
        let back = from_png(&to_png(&t).unwrap(), MAX_SIDE).unwrap();
        assert_eq!(back, t);
        // 디버그 빌드에서도 한도 안(목표 50ms는 릴리스 기준, 여기서는 큰 회귀만 잡는다).
        assert!(ms < 2000, "1920×1080 썸네일 {ms}ms");
        assert!(from_png(b"not png", MAX_SIDE).is_none());
    }

    #[test]
    fn file_thumbnails_png_direct_and_bmp_through_shell() {
        let dir = std::env::temp_dir().join(format!("kagent-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("캡처 1.png");
        std::fs::write(&png, png_of(640, 360)).unwrap();
        let t = from_file(png.to_str().unwrap(), MAX_SIDE).unwrap();
        assert_eq!((t.w, t.h), (320, 180));
        // 24비트 BMP 400×200(파랑) — PNG가 아니므로 탐색기 썸네일 경로를 탄다.
        let (w, h) = (400u32, 200u32);
        let stride = (w * 3).div_ceil(4) * 4;
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&(54 + stride * h).to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        for v in [40u32, w, h] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[0u8; 24]);
        for _ in 0..h {
            let mut line: Vec<u8> = (0..w).flat_map(|_| [255u8, 0, 0]).collect();
            line.resize(stride as usize, 0);
            bmp.extend_from_slice(&line);
        }
        let bmp_path = dir.join("캡처 2.bmp");
        std::fs::write(&bmp_path, &bmp).unwrap();
        let t = from_file(bmp_path.to_str().unwrap(), MAX_SIDE).expect("탐색기 썸네일");
        assert!(t.w <= MAX_SIDE && t.h <= MAX_SIDE && t.w > t.h, "{}×{}", t.w, t.h);
        let mid = ((t.h / 2 * t.w + t.w / 2) * 4) as usize;
        assert!(t.bgra[mid] > 200 && t.bgra[mid + 2] < 60, "가운데는 파랑: {:?}", &t.bgra[mid..mid + 4]);
        assert!(from_file(dir.join("없음.jpg").to_str().unwrap(), MAX_SIDE).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mem_cache_is_lru_and_bounded_by_bytes() {
        let t = |n: usize| Arc::new(Thumb { w: 1, h: 1, bgra: vec![0; n] });
        let mut c = MemCache::new(10);
        c.put("a".into(), t(4));
        c.put("b".into(), t(4));
        assert!(c.get("a").is_some()); // a가 최근
        c.put("c".into(), t(4)); // 12 > 10 → 가장 오래된 b를 버린다
        assert!(c.contains("a") && c.contains("c") && !c.contains("b"));
        assert_eq!(c.bytes(), 8);
        c.put("a".into(), t(2)); // 같은 키 교체
        assert_eq!((c.len(), c.bytes()), (2, 6));
        c.put("big".into(), t(11)); // 상한보다 큰 것은 넣지 않음
        assert!(!c.contains("big"));
        c.clear();
        assert_eq!((c.len(), c.bytes()), (0, 0));
    }

    #[test]
    fn steps_are_tiered_per_source() {
        use Step::*;
        assert_eq!(steps(SourceKind::StoredImage), [Memory, Disk, Png]);
        assert_eq!(steps(SourceKind::PngFile), [Memory, Disk, Png, Shell]);
        assert_eq!(steps(SourceKind::OtherFile), [Memory, Disk, ShellCache, Wic, Shell]);
        assert_eq!(source_kind("C:\\a\\캡처.PNG"), SourceKind::PngFile);
        assert_eq!(source_kind("C:\\a\\사진.jpg"), SourceKind::OtherFile);
        for kind in [SourceKind::StoredImage, SourceKind::PngFile, SourceKind::OtherFile] {
            let s = steps(kind);
            assert_eq!(&s[..2], [Memory, Disk], "캐시가 늘 먼저");
            assert!(!s.contains(&Clipboard), "클립보드 단계는 기록 때(prime)만");
            // 캐시 조회(ShellCache)는 만들기(Wic·Shell)보다 앞, 탐색기 만들기는 늘 마지막.
            if let Some(i) = s.iter().position(|x| *x == ShellCache) {
                assert!(s[i + 1..].contains(&Wic));
            }
            if let Some(i) = s.iter().position(|x| *x == Shell) {
                assert_eq!(i, s.len() - 1);
            }
        }
        // 집계 이름은 단계마다 하나씩.
        assert_eq!(STEP_NAMES.len(), 7);
        let names: std::collections::HashSet<_> = STEP_NAMES.iter().map(|(_, n)| *n).collect();
        assert_eq!(names.len(), 7);
    }

    fn noise_rgba(w: u32, h: u32, alpha: bool) -> Vec<u8> {
        let mut s = 12345u32;
        (0..w * h * 4).map(|i| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            if i % 4 == 3 && !alpha { 255 } else { (s >> 24) as u8 }
        }).collect()
    }

    fn rgba_png(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(px).unwrap();
        out
    }

    #[test]
    fn streaming_png_matches_old_full_decode() {
        for (w, h, alpha) in [(961, 541, false), (641, 359, true), (300, 200, true)] {
            let px = noise_rgba(w, h, alpha);
            let (tw, th) = fit(w, h, MAX_SIDE);
            let old = downscale(&px, w, h, 4, w as usize * 4, tw, th).unwrap();
            let t = from_png(&rgba_png(w, h, &px), MAX_SIDE).unwrap();
            assert_eq!((t.w, t.h, t.bgra), (tw, th, old), "{w}×{h}");
        }
    }

    #[test]
    fn dib_thumbnail_equals_thumbnail_of_stored_png() {
        // 32비트(알파 0 = 불투명으로 봄)·32비트 반투명·24비트, 아래에서 위로.
        let (w, h) = (643u32, 367u32);
        for (bits, alpha) in [(32u16, 0u8), (32, 128), (24, 0)] {
            let bpp = bits as usize / 8;
            let stride = (w as usize * bpp).div_ceil(4) * 4;
            let mut dib = Vec::new();
            for v in [40u32, w, h] {
                dib.extend_from_slice(&v.to_le_bytes());
            }
            dib.extend_from_slice(&1u16.to_le_bytes());
            dib.extend_from_slice(&bits.to_le_bytes());
            dib.extend_from_slice(&[0u8; 24]);
            let px = noise_rgba(w, h, false);
            for y in 0..h as usize {
                let mut line: Vec<u8> = px[y * w as usize * 4..(y + 1) * w as usize * 4].chunks_exact(4)
                    .flat_map(|p| if bpp == 4 { vec![p[0], p[1], p[2], alpha] } else { vec![p[0], p[1], p[2]] }).collect();
                line.resize(stride, 0);
                dib.extend_from_slice(&line);
            }
            let direct = from_dib(&dib, MAX_SIDE).unwrap();
            let via_png = from_png(&crate::clip::dib_to_png(&dib).unwrap(), MAX_SIDE).unwrap();
            assert_eq!(direct, via_png, "{bits}비트 알파 {alpha}");
        }
        assert!(from_dib(b"short", MAX_SIDE).is_none());
    }

    #[test]
    fn disk_format_is_qoi_and_reads_old_png() {
        let t = from_png(&rgba_png(400, 300, &noise_rgba(400, 300, true)), MAX_SIDE).unwrap();
        let q = to_disk(&t).unwrap();
        assert!(q.starts_with(b"qoif"));
        assert_eq!(from_disk(&q).unwrap(), t);
        assert_eq!(from_disk(&to_png(&t).unwrap()).unwrap(), t, "KCAP3 전 PNG 캐시");
        assert!(from_disk(b"qoif junk").is_none());
        // 썸네일보다 큰 QOI는 거절(디스크 캐시는 썸네일만).
        let big = qoi::encode_to_vec(vec![0u8; 400 * 1 * 3], 400, 1).unwrap();
        assert!(from_disk(&big).is_none());
    }

    #[test]
    fn wic_file_thumbnail_matches_png_decoder() {
        let dir = std::env::temp_dir().join(format!("kagent-wic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (w, h) = (900u32, 500u32);
        let path = dir.join("그림 1.png");
        std::fs::write(&path, rgba_png(w, h, &noise_rgba(w, h, true))).unwrap();
        let p = path.to_str().unwrap();
        let ours = from_png(&std::fs::read(&path).unwrap(), MAX_SIDE).unwrap();
        let (tw, th, bgra) = crate::wic::thumb_file(p, MAX_SIDE, crate::wic::Mode::Auto).expect("WIC");
        assert_eq!((tw, th), (ours.w, ours.h));
        // WIC PNG 해독도 같은 화소 → 같은 박스 축소(반투명 합성 포함).
        assert_eq!(bgra, ours.bgra);
        assert!(crate::wic::thumb_file(dir.join("없음.jpg").to_str().unwrap(), MAX_SIDE, crate::wic::Mode::Auto).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_evictions_remove_oldest_until_under_cap() {
        let files = vec![("new".into(), 5, 30), ("old".into(), 5, 10), ("mid".into(), 5, 20)];
        assert_eq!(disk_evictions(files.clone(), 10), ["old"]);
        assert_eq!(disk_evictions(files.clone(), 4), ["old", "mid", "new"]);
        assert!(disk_evictions(files, 15).is_empty());
    }

    #[test]
    fn image_paths_and_keys() {
        assert_eq!(first_image_path("C:\\a.txt\nC:\\b.JPG\nC:\\c.png".as_bytes()).as_deref(), Some("C:\\b.JPG"));
        assert_eq!(first_image_path(b"C:\\a.txt").as_deref(), None);
        assert_eq!(first_image_path("C:\\x\\y.png\0".as_bytes()).as_deref(), Some("C:\\x\\y.png"));
        assert_ne!(mem_key(1, 10), mem_key(1, 11));
        assert!(disk_name("image:1:abc").ends_with(".thumb"));
        assert!(!disk_name("C:\\secret\\name.png").contains("secret"));
    }
}
