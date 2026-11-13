use kfind::{
    Result,
    install::{self, Action, Fallback, WindowsScm},
};
use std::path::PathBuf;
fn main() {
    if let Err(e) = run() {
        eprintln!("kfind: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let command = args.first().and_then(|s| s.to_str()).unwrap_or("--help");
    if command == "--service" {
        return kfind::service::dispatch();
    }
    let base = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .ok_or("ProgramData unavailable")?
        .join("HwpAutoDocFit");
    let base = args
        .windows(2)
        .find(|w| w[0] == "--data-root")
        .map(|w| PathBuf::from(&w[1]))
        .unwrap_or(base);
    let base = std::path::absolute(base)?;
    let scm = WindowsScm {
        executable: std::env::current_exe()?,
        index_root: base.join("index"),
    };
    match command {
        "--install" => match install::execute(&scm, Action::Install, &base) {
            Ok(()) => Ok(()),
            Err(e) => {
                // Only policy/access denial triggers the alternative prompt; other failures remain errors.
                let blocked = matches!(e.downcast_ref::<windows_service::Error>(),Some(windows_service::Error::Winapi(io)) if matches!(io.raw_os_error(),Some(5|1314|1260)));
                if !blocked {
                    return Err(e);
                }
                eprintln!("서비스 설치가 차단되었습니다: {e}");
                let config = std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .ok_or("LOCALAPPDATA unavailable")?
                    .join("HwpAutoDocFit/kfind/settings.json");
                let mode = install::fallback(&config, || {
                    eprintln!("대체 방식 선택 (한 번만 질문): 1 예약 작업, 2 사용자 권한");
                    let mut answer = String::new();
                    std::io::stdin().read_line(&mut answer)?;
                    match answer.trim() {
                        "1" => Ok(Fallback::Task),
                        "2" => Ok(Fallback::User),
                        _ => Err("선택 취소 (저장 안 함)".into()),
                    }
                })?;
                println!("저장된 대체 방식: {mode:?}");
                match mode {
                    Fallback::User => user_mode(user_index()?, kfind::PIPE_NAME),
                    Fallback::Task => install_task(),
                }
            }
        },
        "--start" => install::execute(&scm, Action::Start, &base),
        "--stop" => install::execute(&scm, Action::Stop, &base),
        "--uninstall" => install::execute(&scm, Action::Remove, &base),
        "--user" => user_mode(
            args.windows(2)
                .find(|w| w[0] == "--index-root")
                .map(|w| PathBuf::from(&w[1]))
                .unwrap_or(
                    std::env::var_os("LOCALAPPDATA")
                        .map(PathBuf::from)
                        .ok_or("LOCALAPPDATA unavailable")?
                        .join("HwpAutoDocFit/index"),
                ),
            args.windows(2)
                .find(|w| w[0] == "--pipe-name")
                .and_then(|w| w[1].to_str())
                .unwrap_or(kfind::PIPE_NAME),
        ),
        _ => {
            println!(
                "kfind --install | --start | --stop | --uninstall | --service | --user [--index-root DIR]\nSCM operations: [--data-root DIR]. Pipe: length-prefixed UTF-8 JSON (u32 little endian), query/cancel/status."
            );
            Ok(())
        }
    }
}
fn user_index() -> Result<PathBuf> {
    Ok(std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("LOCALAPPDATA unavailable")?
        .join("HwpAutoDocFit/index"))
}
fn user_mode(root: PathBuf, name: &str) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let (_sender, receiver) = tokio::sync::watch::channel(false);
    runtime.block_on(kfind::pipe::run(name, root, receiver))
}
fn install_task() -> Result<()> {
    let exe = std::env::current_exe()?;
    let action = format!("\"{}\" --user", exe.display());
    let status = std::process::Command::new("schtasks.exe")
        .args([
            "/Create",
            "/TN",
            kfind::SERVICE_NAME,
            "/SC",
            "ONLOGON",
            "/TR",
            &action,
            "/RL",
            "LIMITED",
        ])
        .status()?;
    if !status.success() {
        return Err("scheduled task installation failed".into());
    }
    Ok(())
}
