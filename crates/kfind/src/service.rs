use crate::{Result, SERVICE_NAME};
use std::{ffi::OsString, path::PathBuf, time::Duration};
use windows_service::{
    define_windows_service,
    service::*,
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};
define_windows_service!(ffi_main, service_main);
pub fn dispatch() -> Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_main)?;
    Ok(())
}
fn service_main(_: Vec<OsString>) {
    let dir = crate::errlog::log_dir_for(&index_root().unwrap_or_else(fallback_root));
    match std::panic::catch_unwind(run) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => crate::errlog::report(&dir, &format!("service error: {e}")),
        Err(_) => crate::errlog::report(&dir, "service panic"),
    }
}
fn index_root() -> Option<PathBuf> {
    let args: Vec<_> = std::env::args_os().collect();
    args.windows(2)
        .find(|w| w[0] == "--index-root")
        .map(|w| PathBuf::from(&w[1]))
}
// --index-root 가 없을 때의 로그 위치: %ProgramData%/HwpAutoDocFit/index 와 같은 형제 폴더.
fn fallback_root() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("HwpAutoDocFit")
        .join("index")
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    let root = args
        .windows(2)
        .find(|w| w[0] == "--index-root")
        .map(|w| PathBuf::from(&w[1]))
        .ok_or("missing index root")?;
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let handle = service_control_handler::register(SERVICE_NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = stop.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    let status = |state, code| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: ServiceExitCode::Win32(code),
        checkpoint: 0,
        wait_hint: Duration::ZERO,
        process_id: None,
    };
    handle.set_service_status(status(ServiceState::StartPending, 0))?;
    let runtime = tokio::runtime::Runtime::new()?;
    // Bind before reporting Running so clients can connect as soon as SCM reports readiness.
    let result = runtime.block_on(async {
        let initial = crate::pipe::create(crate::PIPE_NAME, true)?;
        handle.set_service_status(status(ServiceState::Running, 0))?;
        crate::pipe::run_bound_mode(initial, crate::PIPE_NAME, root, receiver, "service").await
    });
    handle.set_service_status(status(
        ServiceState::Stopped,
        if result.is_ok() { 0 } else { 1 },
    ))?;
    result
}
