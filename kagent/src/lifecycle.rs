//! 에이전트 교체 순서. 실제 프로세스와 파일 IO는 Runtime으로 분리해 격리 시험한다.
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
