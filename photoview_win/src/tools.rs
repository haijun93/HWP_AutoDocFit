//! 배경 작업: 크게 보기 해독, 중복 찾기, 사본 저장·편집. 창은 알림을 받으면 `drain` 한다.
use crate::extras::EditSpec;
use crate::imaging::{self, Format};
use crate::model::Doc;
use crate::pixels::{self, Hashed, Image};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// 크게 보기에 쓰는 해독 크기 상한(긴 변).
pub const VIEW_EDGE: u32 = 2560;
/// 공문 첨부용 사본의 긴 변·JPEG 품질(앱 화면 기본값).
pub const EXPORT_EDGE: u32 = 1280;
pub const EXPORT_QUALITY: f32 = 0.85;
const EDIT_QUALITY: f32 = 0.95;

/// 크게 보기 해독 스레드. 밀린 요청은 버리고 마지막 것만 읽는다.
pub struct Loader {
    requests: Sender<PathBuf>,
    results: Receiver<(PathBuf, Result<Image, String>)>,
}

impl Loader {
    pub fn new(notify: Notify) -> Loader {
        let (requests, incoming) = mpsc::channel::<PathBuf>();
        let (done, results) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(mut path) = incoming.recv() {
                while let Ok(newer) = incoming.try_recv() {
                    path = newer;
                }
                let image = imaging::load(&path, Some(VIEW_EDGE));
                if done.send((path, image)).is_err() {
                    break;
                }
                notify();
            }
        });
        Loader { requests, results }
    }
    pub fn request(&self, path: PathBuf) {
        let _ = self.requests.send(path);
    }
    pub fn drain(&self) -> Vec<(PathBuf, Result<Image, String>)> {
        self.results.try_iter().collect()
    }
}

#[derive(Debug)]
pub enum Event {
    Progress(String),
    /// 중복 찾기 끝: 묶음마다의 경로.
    Duplicates { groups: Vec<Vec<String>>, read: usize, skipped: usize, cancelled: bool },
    /// 사본 저장·편집 끝.
    Saved { outputs: Vec<PathBuf>, failures: Vec<(String, String)>, edited: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Duplicates,
    Export,
    Edit,
}

/// 한 번에 하나만 도는 작업.
pub struct ToolJob {
    pub kind: Kind,
    events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

impl ToolJob {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn drain(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    fn spawn(kind: Kind, notify: Notify, work: impl FnOnce(&AtomicBool, &dyn Fn(Event)) -> Event + Send + 'static) -> ToolJob {
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            let send = |event: Event| {
                let _ = tx.send(event);
                notify();
            };
            let done = work(&flag, &send);
            send(done);
        });
        ToolJob { kind, events, cancel }
    }

    /// 클라우드에만 있는 사진은 읽지 않는다.
    pub fn duplicates(docs: Vec<Doc>, notify: Notify) -> ToolJob {
        ToolJob::spawn(Kind::Duplicates, notify, move |cancel, send| {
            let (mut items, mut skipped) = (Vec::new(), 0);
            let mut last = std::time::Instant::now();
            for (n, doc) in docs.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                match (!doc.cloud).then(|| hash(Path::new(&doc.path))) {
                    Some(Ok(hash)) => items.push(Hashed { path: doc.path.clone(), name: doc.name.clone(), size: doc.size, hash }),
                    _ => skipped += 1,
                }
                if last.elapsed().as_millis() >= 200 {
                    last = std::time::Instant::now();
                    send(Event::Progress(format!("중복 찾는 중 {}/{}", n + 1, docs.len())));
                }
            }
            let cancelled = cancel.load(Ordering::Relaxed);
            let groups = if cancelled {
                Vec::new()
            } else {
                pixels::duplicate_groups(&items, pixels::THRESHOLD)
                    .into_iter()
                    .map(|g| g.into_iter().map(|i| items[i].path.clone()).collect())
                    .collect()
            };
            Event::Duplicates { groups, read: items.len(), skipped, cancelled }
        })
    }

    pub fn export(paths: Vec<PathBuf>, folder: PathBuf, notify: Notify) -> ToolJob {
        ToolJob::spawn(Kind::Export, notify, move |cancel, send| {
            let (mut outputs, mut failures) = (Vec::new(), Vec::new());
            for (n, path) in paths.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                send(Event::Progress(format!("첨부용 사본 저장 중 {}/{}", n + 1, paths.len())));
                match export_one(path, &folder) {
                    Ok(out) => outputs.push(out),
                    Err(e) => failures.push((path.to_string_lossy().into_owned(), e)),
                }
            }
            Event::Saved { outputs, failures, edited: false }
        })
    }

    pub fn edit(path: PathBuf, spec: EditSpec, notify: Notify) -> ToolJob {
        ToolJob::spawn(Kind::Edit, notify, move |_, send| {
            send(Event::Progress("사본 편집 중".into()));
            match edit_one(&path, spec) {
                Ok(out) => Event::Saved { outputs: vec![out], failures: Vec::new(), edited: true },
                Err(e) => Event::Saved { outputs: Vec::new(), failures: vec![(path.to_string_lossy().into_owned(), e)], edited: true },
            }
        })
    }
}

fn hash(path: &Path) -> Result<u64, String> {
    // 9x8 로 줄이기 전에 256 으로 한 번 줄여 읽는다(큰 사진 전체를 메모리에 펴지 않는다).
    let image = imaging::load(path, Some(256))?;
    let small = pixels::shrink_9x8(&image.gray(), image.width as usize, image.height as usize);
    Ok(pixels::dhash(&small))
}

fn stem(path: &Path) -> Result<String, String> {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).ok_or_else(|| "파일 이름이 없습니다".into())
}

/// 긴 변 1280 JPEG 사본 `<이름>_docfit.jpg`(있으면 번호). 원본·기존 파일은 덮어쓰지 않는다.
pub fn export_one(path: &Path, folder: &Path) -> Result<PathBuf, String> {
    let image = imaging::load(path, Some(EXPORT_EDGE))?;
    let out = imaging::reserve(folder, &stem(path)?, "_docfit", "jpg")?;
    imaging::save(&out, &image, Format::Jpeg, EXPORT_QUALITY)?;
    Ok(out)
}

/// 원본 폴더에 `<이름>_edited.<확장자>` 사본. 순서는 앱과 같이 회전→자르기→밝기→대비.
pub fn edit_one(path: &Path, spec: EditSpec) -> Result<PathBuf, String> {
    let mut image = imaging::load(path, None)?.rotated(spec.quarter_turns);
    if let Some(crop) = spec.crop {
        image = image.cropped(crop).ok_or("유효하지 않은 자르기 영역입니다")?;
    }
    if (spec.brightness - 1.0).abs() > 1e-4 {
        image.brighten(spec.brightness);
    }
    if (spec.contrast - 1.0).abs() > 1e-4 {
        image.contrast(spec.contrast);
    }
    let extension = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let (format, extension) = Format::for_copy(&extension);
    let folder = path.parent().ok_or("원본 폴더를 알 수 없습니다")?;
    let out = imaging::reserve(folder, &stem(path)?, "_edited", extension)?;
    imaging::save(&out, &image, format, EDIT_QUALITY)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(folder: &Path, name: &str, w: u32, h: u32, shade: impl Fn(u32, u32) -> u8) -> PathBuf {
        let bgra = (0..w * h).flat_map(|i| { let v = shade(i % w, i / w); [v, v, v, 255] }).collect();
        let path = folder.join(name);
        std::fs::write(&path, b"").unwrap();
        imaging::save(&path, &Image::new(w, h, bgra).unwrap(), Format::Png, 1.0).unwrap();
        path
    }

    fn wait(job: &ToolJob) -> Event {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            for event in job.drain() {
                if !matches!(event, Event::Progress(_)) {
                    return event;
                }
            }
            assert!(std::time::Instant::now() < until, "작업이 끝나지 않음");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn export_and_edit_make_new_files_and_leave_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let source = write(dir.path(), "현장.png", 2000, 1000, |x, _| (x / 8) as u8);
        let before = std::fs::read(&source).unwrap();
        let out = export_one(&source, dir.path()).unwrap();
        assert_eq!(out.file_name().unwrap(), "현장_docfit.jpg");
        let small = imaging::load(&out, None).unwrap();
        assert_eq!((small.width, small.height), (1280, 640));
        assert_eq!(export_one(&source, dir.path()).unwrap().file_name().unwrap(), "현장_docfit_1.jpg");

        let spec = EditSpec { quarter_turns: 1, brightness: 1.2, contrast: 1.0, crop: Some([0, 0, 500, 800]) };
        let edited = edit_one(&source, spec).unwrap();
        assert_eq!(edited.file_name().unwrap(), "현장_edited.png");
        let image = imaging::load(&edited, None).unwrap();
        assert_eq!((image.width, image.height), (500, 800));
        assert!(edit_one(&source, EditSpec { crop: Some([5000, 0, 6000, 10]), ..spec }).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), before, "원본은 그대로");
    }

    #[test]
    fn duplicates_group_resized_copies_and_skip_cloud_and_broken_files() {
        let dir = tempfile::tempdir().unwrap();
        // 같은 무늬를 두 크기로, 반대 무늬 하나.
        let wave = |w: u32| move |x: u32, y: u32| (((x * 360 / w) as f64).to_radians().sin() * 90.0 + 128.0 + (y % 3) as f64) as u8;
        let a = write(dir.path(), "a.png", 400, 300, wave(400));
        let b = write(dir.path(), "b.png", 200, 150, wave(200));
        let c = write(dir.path(), "c.png", 400, 300, |x, _| 255 - (((x * 360 / 400) as f64).to_radians().sin() * 90.0 + 128.0) as u8);
        let broken = dir.path().join("broken.jpg");
        std::fs::write(&broken, b"x").unwrap();
        let doc = |p: &Path, size: u64, cloud: bool| Doc { path: p.to_string_lossy().into_owned(), name: p.file_name().unwrap().to_string_lossy().into_owned(), ext: "png".into(), size, mtime: 0.0, cloud };
        let docs = vec![doc(&a, 100, false), doc(&b, 50, false), doc(&c, 100, false), doc(&broken, 1, false), doc(&a, 9, true)];
        let job = ToolJob::duplicates(docs, Arc::new(|| {}));
        let Event::Duplicates { groups, read, skipped, cancelled } = wait(&job) else { panic!("다른 이벤트") };
        assert_eq!((read, skipped, cancelled), (3, 2, false));
        assert_eq!(groups, vec![vec![a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()]]);
    }

    #[test]
    fn loader_returns_the_latest_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "v.png", 3000, 100, |_, _| 9);
        let loader = Loader::new(Arc::new(|| {}));
        loader.request(path.clone());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let (got, image) = loop {
            if let Some(result) = loader.drain().pop() {
                break result;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(got, path);
        assert_eq!(image.unwrap().width, VIEW_EDGE);
    }
}

#[cfg(test)]
mod measure {
    use super::*;

    /// 합성 12MP JPEG 로 크게 보기·첨부용 사본·중복 해시 시간을 잰다. `cargo test --release -- --ignored --nocapture measure`
    #[test]
    #[ignore = "시간 측정용"]
    fn decode_export_and_hash_times() {
        let dir = tempfile::tempdir().unwrap();
        let (w, h) = (4000u32, 3000u32);
        let bgra = (0..w * h).flat_map(|i| [(i % w / 16) as u8, (i / w / 12) as u8, (i % 251) as u8, 255]).collect();
        let source = imaging::reserve(dir.path(), "synthetic", "", "jpg").unwrap();
        imaging::save(&source, &Image::new(w, h, bgra).unwrap(), Format::Jpeg, 0.9).unwrap();
        let time = |label: &str, work: &dyn Fn()| {
            let mut ms: Vec<u128> = (0..5).map(|_| { let t = std::time::Instant::now(); work(); t.elapsed().as_millis() }).collect();
            ms.sort_unstable();
            println!("{label}: median {} ms (min {} / max {})", ms[2], ms[0], ms[4]);
        };
        time("view decode 4000x3000 -> 2560", &|| { imaging::load(&source, Some(VIEW_EDGE)).unwrap(); });
        time("export 1280 jpeg", &|| { export_one(&source, dir.path()).unwrap(); });
        time("dhash", &|| { hash(&source).unwrap(); });
    }
}
