//! 스캔 상태 표시값과 키보드 이동(문서뷰·사진뷰 공용).

/// 스캔 상태(화면 표시용).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanState {
    Idle,
    Running,
    Cancelling,
    Complete,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct ScanView {
    pub state: ScanState,
    pub visited: usize,
    pub folder: String,
    pub elapsed_secs: u64,
}

impl Default for ScanView {
    fn default() -> Self {
        ScanView { state: ScanState::Idle, visited: 0, folder: String::new(), elapsed_secs: 0 }
    }
}

impl ScanView {
    pub fn busy(&self) -> bool {
        matches!(self.state, ScanState::Running | ScanState::Cancelling)
    }
}

/// 키보드 이동.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}
