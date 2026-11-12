//! 프로세스 수명: 상태 변경은 step만 담당하며 Win32 효과는 main에서 실행한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command { Show, Hidden, Reload, Quit, Version, Status, Install, Enable, Upgrade, Disable }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Args { pub command: Command, pub delay: u64, pub autostart: bool }
pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut result = Args { command: Command::Show, delay: 0, autostart: false };
    let mut chosen = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--delay" => { result.delay = it.next().ok_or("--delay 값 없음")?.parse::<u64>().map_err(|_| "잘못된 delay")?.min(300); }
            "--autostart" => result.autostart = true,
            other => {
                let command = match other {
                    "--hidden" => Command::Hidden, "--reload" => Command::Reload,
                    "--quit" => Command::Quit, "--version" => Command::Version,
                    "--status" => Command::Status, "--install" => Command::Install,
                    "--enable" => Command::Enable, "--upgrade-if-stale" => Command::Upgrade,
                    "--disable" => Command::Disable, _ => return Err(format!("알 수 없는 인자: {other}")),
                };
                if chosen { return Err("명령은 하나만 지정하세요".into()); }
                result.command = command; chosen = true;
            }
        }
    }
    if result.autostart && result.command != Command::Enable { return Err("--autostart는 --enable 전용".into()); }
    if result.delay != 0 && !matches!(result.command, Command::Show | Command::Hidden) { return Err("상주 명령만 지연 가능".into()); }
    Ok(result)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum State { NotRunning, Starting, Resident, Exiting }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Event { Start, Existing, Ready, Failed, Show, Reload, Hide, Quit, EndSession, Cleaned }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Effect { Acquire, Notify(Command), Create, SetView(&'static str), Reload, Cleanup, Exit(i32) }
pub fn step(state: State, event: Event, command: Command) -> (State, Vec<Effect>) {
    use Effect::*;
    match (state,event) {
        (State::NotRunning, Event::Start) => (State::Starting, vec![Acquire]),
        (State::Starting, Event::Existing) => (State::NotRunning, vec![Notify(command), Exit(0)]),
        (State::Starting, Event::Ready) if matches!(command, Command::Quit | Command::Reload) => (State::NotRunning, vec![Exit(0)]),
        (State::Starting, Event::Ready) => (State::Resident, vec![Create, SetView(if command == Command::Hidden { "hidden" } else { "loading" })]),
        (State::Starting, Event::Failed) => (State::Exiting, vec![Cleanup, Exit(1)]),
        (State::Resident, Event::Show) => (state, vec![SetView("loading")]),
        (State::Resident, Event::Reload) => (state, vec![Reload]),
        (State::Resident, Event::Hide) => (state, vec![SetView("hidden")]),
        (State::Resident, Event::Quit | Event::EndSession) => (State::Exiting, vec![Cleanup]),
        (State::Exiting, Event::Cleaned) => (State::NotRunning, vec![Exit(0)]),
        _ => (state, vec![]),
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn args(a: &[&str]) -> Result<Args,String> { parse(&a.iter().map(|s|s.to_string()).collect::<Vec<_>>()) }
    #[test] fn default_show() { assert_eq!(args(&[]).unwrap().command,Command::Show); }
    #[test] fn hidden() { assert_eq!(args(&["--hidden"]).unwrap().command,Command::Hidden); }
    #[test] fn delay() { assert_eq!(args(&["--hidden","--delay","999"]).unwrap().delay,300); }
    #[test] fn invalid_delay() { for a in [vec!["--delay"],vec!["--delay","-1"],vec!["--delay","x"]] { assert!(args(&a).is_err()); } }
    #[test] fn commands() { for a in ["--quit","--reload","--version","--status","--install","--enable","--upgrade-if-stale","--disable"] { assert!(args(&[a]).is_ok()); } }
    #[test] fn reject_conflicts() { assert!(args(&["--hidden","--quit"]).is_err()); assert!(args(&["--wat"]).is_err()); assert!(args(&["--autostart"]).is_err()); }
    #[test] fn enable_autostart() { assert!(args(&["--enable","--autostart"]).unwrap().autostart); }
    #[test] fn start() { assert_eq!(step(State::NotRunning,Event::Start,Command::Show),(State::Starting,vec![Effect::Acquire])); }
    #[test] fn existing() { for c in [Command::Show,Command::Hidden,Command::Quit,Command::Reload] { assert_eq!(step(State::Starting,Event::Existing,c),(State::NotRunning,vec![Effect::Notify(c),Effect::Exit(0)])); } }
    #[test] fn no_resident_for_controls() { for c in [Command::Quit,Command::Reload] { assert_eq!(step(State::Starting,Event::Ready,c).0,State::NotRunning); } }
    #[test] fn resident_initial_view() { for (c,v) in [(Command::Show,"loading"),(Command::Hidden,"hidden")] { assert_eq!(step(State::Starting,Event::Ready,c),(State::Resident,vec![Effect::Create,Effect::SetView(v)])); } }
    #[test] fn startup_failure() { assert_eq!(step(State::Starting,Event::Failed,Command::Show).0,State::Exiting); }
    #[test] fn resident_show_hide_reload() { for (e,f) in [(Event::Show,Effect::SetView("loading")),(Event::Hide,Effect::SetView("hidden")),(Event::Reload,Effect::Reload)] { assert_eq!(step(State::Resident,e,Command::Show),(State::Resident,vec![f])); } }
    #[test] fn exit_sources() { for e in [Event::Quit,Event::EndSession] { assert_eq!(step(State::Resident,e,Command::Show),(State::Exiting,vec![Effect::Cleanup])); } }
    #[test] fn cleaned() { assert_eq!(step(State::Exiting,Event::Cleaned,Command::Show),(State::NotRunning,vec![Effect::Exit(0)])); }
    #[test] fn impossible_calls() { for s in [State::Starting,State::Exiting,State::NotRunning] { for e in [Event::Show,Event::Hide,Event::Reload] { assert_eq!(step(s,e,Command::Show),(s,vec![])); } } }
}
