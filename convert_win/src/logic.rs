//! 창 없이 시험하는 부분: 레이아웃·클릭 판정·파일 목록 모델·드롭 경로 정리·변환 명령 인자.
//! Win32 호출은 `win.rs` 에만 있고, 여기에는 순수 함수만 둔다.
use std::path::{Path, PathBuf};

/// (화면에 보이는 이름, 기존 변환 엔진의 `--to` 값). 순서가 곧 단추 순서다.
pub const TARGETS: [(&str, &str); 7] = [
    ("HWPX", "hwpx"),
    ("PDF", "pdf"),
    ("PDF 하나로 합치기", "pdfm"),
    ("PNG", "png"),
    ("JPG", "jpg"),
    ("TXT", "txt"),
    ("MD", "md"),
];
pub const MARGIN: i32 = 28;
pub const TARGET_W: i32 = 130;
pub const TARGET_STEP: i32 = 142;
pub const TARGET_TOP: i32 = 128;
pub const TARGET_BOTTOM: i32 = 164;
pub const ROW_TOP: i32 = 244;
pub const ROW_STEP: i32 = 28;

/// 파일 한 건의 명시적 변환 상태. 전이는 `transition`만으로 적용한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileStatus { Waiting, Running, Succeeded, Failed(String), Cancelled }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileResult { pub source: PathBuf, pub status: FileStatus }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event { Start(usize), Success(usize), Failure(usize), Cancel(usize) }
pub fn transition(items: &mut [FileResult], event: Event) {
    match event {
        Event::Start(i) => if let Some(x)=items.get_mut(i) { if x.status==FileStatus::Waiting { x.status=FileStatus::Running; } },
        Event::Success(i) => if let Some(x)=items.get_mut(i) { if x.status==FileStatus::Running { x.status=FileStatus::Succeeded; } },
        Event::Failure(i) => if let Some(x)=items.get_mut(i) { if x.status==FileStatus::Running { x.status=FileStatus::Failed("변환 실패".into()); } },
        Event::Cancel(from) => for x in items.iter_mut().skip(from) { if matches!(x.status, FileStatus::Waiting|FileStatus::Running) { x.status=FileStatus::Cancelled; } },
    }
}
pub fn results(files: &[PathBuf]) -> Vec<FileResult> { files.iter().cloned().map(|source| FileResult { source, status: FileStatus::Waiting }).collect() }

#[cfg(test)] mod progress_tests {
 use super::*;
 #[test] fn success_failure_cancel_and_empty_transitions() {
   assert!(results(&[]).is_empty());
   let mut v=results(&["a.hwp".into(),"b.hwp".into(),"c.hwp".into()]);
   transition(&mut v,Event::Start(0)); transition(&mut v,Event::Success(0));
   transition(&mut v,Event::Start(1)); transition(&mut v,Event::Failure(1));
   transition(&mut v,Event::Cancel(2));
   assert_eq!(v[0].status,FileStatus::Succeeded);
   assert!(matches!(v[1].status,FileStatus::Failed(_)));
   assert_eq!(v[2].status,FileStatus::Cancelled);
 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Target(usize),
    Start,
    Cancel,
    None,
}

/// 창 크기(클라이언트 영역)에서 정해지는 칸 위치.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub w: i32,
    pub h: i32,
}
impl Layout {
    pub fn new(w: i32, h: i32) -> Self {
        Self { w, h }
    }
    pub fn target_rect(&self, i: usize) -> Rect {
        let x = MARGIN + i as i32 * TARGET_STEP;
        Rect { left: x, top: TARGET_TOP, right: x + TARGET_W, bottom: TARGET_BOTTOM }
    }
    pub fn list_rect(&self) -> Rect {
        Rect { left: MARGIN, top: 222, right: self.w - MARGIN, bottom: self.h - 104 }
    }
    pub fn start_rect(&self) -> Rect {
        Rect { left: self.w - 220, top: self.h - 70, right: self.w - MARGIN, bottom: self.h - 24 }
    }
    pub fn cancel_rect(&self) -> Rect { Rect { left: self.w - 340, top: self.h - 70, right: self.w - 232, bottom: self.h - 24 } }
    /// 목록 칸에 한 번에 그릴 수 있는 줄 수(최소 1).
    pub fn visible_rows(&self) -> usize {
        let room = self.list_rect().bottom - ROW_TOP;
        (room / ROW_STEP).max(1) as usize
    }
    pub fn hit(&self, x: i32, y: i32) -> Hit {
        for i in 0..TARGETS.len() {
            if self.target_rect(i).contains(x, y) {
                return Hit::Target(i);
            }
        }
        if self.start_rect().contains(x, y) {
            return Hit::Start;
        }
        if self.cancel_rect().contains(x,y) { return Hit::Cancel; }
        Hit::None
    }
}

/// 변환할 파일 목록과 선택된 변환 대상.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model {
    pub files: Vec<PathBuf>,
    pub target: usize,
}
impl Model {
    /// 드롭된 경로를 목록에 더한다. 파일이 아닌 것(폴더·없는 경로)과 이미 있는 경로는 건너뛴다.
    /// 더해진 개수를 돌려준다.
    pub fn add_dropped<I, F>(&mut self, paths: I, is_file: F) -> usize
    where
        I: IntoIterator<Item = PathBuf>,
        F: Fn(&Path) -> bool,
    {
        let mut added = 0;
        for p in paths {
            if is_file(&p) && !self.files.contains(&p) {
                self.files.push(p);
                added += 1;
            }
        }
        added
    }
    pub fn select(&mut self, i: usize) {
        self.target = i.min(TARGETS.len() - 1);
    }
    pub fn format(&self) -> &'static str {
        TARGETS[self.target.min(TARGETS.len() - 1)].1
    }
    pub fn header(&self) -> String {
        format!("변환할 파일  ·  {}개", self.files.len())
    }
}
pub fn engine_file_args(file:&Path, format:&str)->Vec<std::ffi::OsString> { vec!["convert".into(),file.as_os_str().to_owned(),"--to".into(),format.into(),"--notify".into()] }

/// 기존 앱 실행 파일 찾기: 환경변수 지정 → 이 창 옆의 HWP_AutoDocFit.exe. 실제 파일일 때만 돌려준다.
pub fn resolve_engine(
    env: Option<PathBuf>,
    current_exe: Option<PathBuf>,
    is_file: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    env.or_else(|| current_exe.and_then(|p| p.parent().map(|d| d.join("HWP_AutoDocFit.exe"))))
        .filter(|p| is_file(p))
}

/// 캡처용 예시 목록.
pub fn sample_model() -> Model {
    Model {
        files: vec!["보고서.hwp".into(), "회의자료.hwpx".into(), "스캔본.pdf".into()],
        target: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn targets_map_to_the_existing_engine_formats() {
        let formats: Vec<_> = TARGETS.iter().map(|t| t.1).collect();
        assert_eq!(formats, ["hwpx", "pdf", "pdfm", "png", "jpg", "txt", "md"]);
    }

    #[test]
    fn layout_places_target_buttons_in_a_row() {
        let l = Layout::new(1050, 650);
        assert_eq!(l.target_rect(0), Rect { left: 28, top: 128, right: 158, bottom: 164 });
        assert_eq!(l.target_rect(6).left, 28 + 6 * 142);
        assert_eq!(l.start_rect(), Rect { left: 830, top: 580, right: 1022, bottom: 626 });
        assert_eq!(l.list_rect(), Rect { left: 28, top: 222, right: 1022, bottom: 546 });
    }

    #[test]
    fn hit_test_finds_targets_start_and_nothing() {
        let l = Layout::new(1050, 650);
        assert_eq!(l.hit(30, 130), Hit::Target(0));
        assert_eq!(l.hit(28 + 2 * 142 + 5, 150), Hit::Target(2));
        assert_eq!(l.hit(28 + 6 * 142 + 129, 163), Hit::Target(6));
        // 단추 사이 틈과 단추 줄 밖은 아무것도 아니다.
        assert_eq!(l.hit(28 + 130 + 5, 150), Hit::None);
        assert_eq!(l.hit(30, 127), Hit::None);
        assert_eq!(l.hit(30, 164), Hit::None);
        assert_eq!(l.hit(900, 600), Hit::Start);
        // 예전에는 y>500 이면 어디든 변환 시작이었다. 변환 시작 단추 밖 클릭은 이제 무시한다.
        assert_eq!(l.hit(300, 520), Hit::None);
        assert_eq!(l.hit(829, 600), Hit::None);
    }

    #[test]
    fn hit_test_follows_window_size() {
        let l = Layout::new(800, 500);
        assert_eq!(l.hit(700, 450), Hit::Start);
        assert_eq!(l.hit(900, 600), Hit::None);
    }

    #[test]
    fn visible_rows_grow_with_height_and_never_hit_zero() {
        assert_eq!(Layout::new(1050, 650).visible_rows(), 10); // (546-244)/28
        assert!(Layout::new(1050, 900).visible_rows() > 10);
        assert_eq!(Layout::new(1050, 100).visible_rows(), 1);
    }

    #[test]
    fn drop_adds_only_new_files() {
        let mut m = Model::default();
        let exists = |s: &Path| s.extension().is_some();
        let n = m.add_dropped([p("a.hwp"), p("folder"), p("b.pdf"), p("a.hwp")], exists);
        assert_eq!(n, 2);
        assert_eq!(m.files, [p("a.hwp"), p("b.pdf")]);
        assert_eq!(m.add_dropped([p("b.pdf")], exists), 0);
        assert_eq!(m.add_dropped(Vec::new(), exists), 0);
        assert_eq!(m.files.len(), 2);
    }

    #[test]
    fn selecting_a_target_is_clamped() {
        let mut m = Model::default();
        assert_eq!(m.format(), "hwpx");
        m.select(2);
        assert_eq!(m.format(), "pdfm");
        m.select(99);
        assert_eq!(m.target, 6);
        assert_eq!(m.format(), "md");
    }

    #[test]
    fn engine_args_follow_the_existing_convert_command() {
        let args:Vec<String>=engine_file_args(Path::new("b b.pdf"),"pdf").iter().map(|a|a.to_string_lossy().into_owned()).collect();
        assert_eq!(args,["convert","b b.pdf","--to","pdf","--notify"]);
    }

    #[test]
    fn engine_is_found_by_env_then_next_to_the_window() {
        let any = |_: &Path| true;
        assert_eq!(resolve_engine(Some(p("X/app.exe")), Some(p("Y/convert_win.exe")), any), Some(p("X/app.exe")));
        assert_eq!(resolve_engine(None, Some(p("Y/convert_win.exe")), any), Some(p("Y").join("HWP_AutoDocFit.exe")));
        assert_eq!(resolve_engine(None, Some(p("Y/convert_win.exe")), |_| false), None);
        assert_eq!(resolve_engine(None, None, any), None);
    }
}
