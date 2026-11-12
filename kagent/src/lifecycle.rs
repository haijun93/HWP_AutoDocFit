//! 에이전트 교체 순서. 실제 프로세스와 파일 IO는 Runtime으로 분리해 격리 시험한다.
//! kvf(가상 폴더 창)도 이 파일을 컴파일한다. `std`·`windows-sys`·`crate::util` 말고는 쓰지 말 것.
pub fn stale(file_differs: bool, running: bool, running_version: Option<&str>, bundled_version: &str) -> bool {
    file_differs || (running && running_version != Some(bundled_version))
}
pub trait Runtime {
    fn quit_and_wait(&mut self) -> Result<(), String>;
    fn install(&mut self) -> Result<(), String>;
    fn start_and_wait(&mut self) -> Result<(), String>;
    fn rollback(&mut self) -> Result<(), String>;
}

pub fn update(runtime: &mut impl Runtime, running: bool, stale: bool, enable: bool)
    -> Result<(bool, bool), String>
{
    if running && !stale { return Ok((false, false)); }
    if running { runtime.quit_and_wait()?; }
    if stale {
        if let Err(error) = runtime.install() {
            // 설치 실패 시 원자 교체 덕분에 이전 실행 파일은 그대로다.
            let recovery = if running { runtime.start_and_wait() } else { Ok(()) };
            return Err(format!("install: {error}; recovery: {recovery:?}"));
        }
    }
    let restart = running || enable;
    if restart {
        if let Err(error) = runtime.start_and_wait() {
            // 새 프로세스가 떠 있다면 정상 종료가 확인된 뒤에만 되돌린다.
            let recovery = (|| {
                runtime.quit_and_wait()?;
                if stale { runtime.rollback()?; }
                if running { runtime.start_and_wait()?; }
                Ok::<(), String>(())
            })();
            return Err(format!("start: {error}; recovery: {recovery:?}"));
        }
    }
    Ok((stale, running && stale))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fake {
        calls: Vec<&'static str>,
        stop_fails: bool,
        install_fails: bool,
        starts: usize,
        start_fails_once: bool,
    }
    impl Runtime for Fake {
        fn quit_and_wait(&mut self) -> Result<(), String> {
            self.calls.push("quit");
            if self.stop_fails { Err("still_running".into()) } else { Ok(()) }
        }
        fn install(&mut self) -> Result<(), String> {
            self.calls.push("install");
            if self.install_fails { Err("locked".into()) } else { Ok(()) }
        }
        fn start_and_wait(&mut self) -> Result<(), String> {
            self.calls.push("start"); self.starts += 1;
            if self.start_fails_once && self.starts == 1 { Err("spawn".into()) } else { Ok(()) }
        }
        fn rollback(&mut self) -> Result<(), String> { self.calls.push("rollback"); Ok(()) }
    }
    #[test]
    fn running_version_is_compared_even_if_installed_file_is_current() {
        assert!(!stale(false, true, Some("0.1.1"), "0.1.1"));
        assert!(stale(false, true, Some("0.1.0"), "0.1.1"));
        assert!(stale(false, true, None, "0.1.1"));
        assert!(stale(true, true, Some("0.1.1"), "0.1.1"));
        assert!(!stale(false, false, None, "0.1.1"));
    }
    #[test]
    fn current_running_agent_is_untouched_on_startup_and_enable() {
        for enable in [false, true] {
            let mut rt = Fake::default();
            assert_eq!(update(&mut rt, true, false, enable), Ok((false, false)));
            assert!(rt.calls.is_empty());
        }
    }
    #[test]
    fn stale_running_agent_quits_installs_and_restarts_in_order() {
        for enable in [false, true] {
            let mut rt = Fake::default();
            assert_eq!(update(&mut rt, true, true, enable), Ok((true, true)));
            assert_eq!(rt.calls, ["quit", "install", "start"]);
        }
    }
    #[test]
    fn failed_quit_never_installs_or_starts() {
        let mut rt = Fake { stop_fails: true, ..Default::default() };
        assert_eq!(update(&mut rt, true, true, true), Err("still_running".into()));
        assert_eq!(rt.calls, ["quit"]);
    }
    #[test]
    fn failed_install_restarts_previous_agent() {
        let mut rt = Fake { install_fails: true, ..Default::default() };
        assert!(update(&mut rt, true, true, false).unwrap_err().contains("install:"));
        assert_eq!(rt.calls, ["quit", "install", "start"]);
    }
    #[test]
    fn failed_restart_restores_and_starts_previous_agent() {
        let mut rt = Fake { start_fails_once: true, ..Default::default() };
        assert!(update(&mut rt, true, true, false).unwrap_err().contains("start:"));
        assert_eq!(rt.calls, ["quit", "install", "start", "quit", "rollback", "start"]);
    }
    #[test]
    fn disabled_agent_is_updated_without_starting() {
        let mut rt = Fake::default();
        assert_eq!(update(&mut rt, false, true, false), Ok((true, false)));
        assert_eq!(rt.calls, ["install"]);
    }
    #[test]
    fn enabling_stopped_current_agent_only_starts() {
        let mut rt = Fake::default();
        assert_eq!(update(&mut rt, false, false, true), Ok((false, false)));
        assert_eq!(rt.calls, ["start"]);
    }
}
