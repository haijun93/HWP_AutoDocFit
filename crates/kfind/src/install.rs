use crate::{DISPLAY_NAME, Result, SERVICE_NAME};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use windows_service::{service::*, service_manager::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Start,
    Stop,
    Remove,
}
pub trait Scm {
    fn apply(&self, action: Action) -> Result<()>;
}
pub struct WindowsScm {
    pub executable: PathBuf,
    pub index_root: PathBuf,
}
impl WindowsScm {
    pub fn info(&self) -> ServiceInfo {
        ServiceInfo {
            name: SERVICE_NAME.into(),
            display_name: DISPLAY_NAME.into(),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: self.executable.clone(),
            launch_arguments: vec![
                "--service".into(),
                "--index-root".into(),
                self.index_root.clone().into_os_string(),
            ],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        }
    }
}
pub fn execute(scm: &impl Scm, action: Action, root: &Path) -> Result<()> {
    if action == Action::Remove {
        remove(scm, root)
    } else {
        scm.apply(action)
    }
}
impl Scm for WindowsScm {
    fn apply(&self, action: Action) -> Result<()> {
        let access = if action == Action::Install {
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE
        } else {
            ServiceManagerAccess::CONNECT
        };
        let manager = ServiceManager::local_computer(None::<&str>, access)?;
        if action == Action::Install {
            let info = self.info();
            let service = manager
                .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::DELETE)?;
            if let Err(e) = service.set_delayed_auto_start(true) {
                service.delete()?;
                return Err(e.into());
            }
            return Ok(());
        }
        let service = match manager.open_service(
            SERVICE_NAME,
            ServiceAccess::QUERY_STATUS
                | ServiceAccess::START
                | ServiceAccess::STOP
                | ServiceAccess::DELETE,
        ) {
            Ok(s) => s,
            Err(windows_service::Error::Winapi(e))
                if action == Action::Remove && e.raw_os_error() == Some(1060) =>
            {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        control(&service, action, || {
            std::thread::sleep(Duration::from_millis(100))
        })
    }
}
trait ManagedService {
    fn state(&self) -> Result<ServiceState>;
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn delete(&self) -> Result<()>;
}
impl ManagedService for Service {
    fn state(&self) -> Result<ServiceState> {
        Ok(self.query_status()?.current_state)
    }
    fn start(&self) -> Result<()> {
        Service::start::<&str>(self, &[])?;
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        Service::stop(self)?;
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        Service::delete(self)?;
        Ok(())
    }
}
fn control(service: &impl ManagedService, action: Action, mut wait: impl FnMut()) -> Result<()> {
    match action {
        Action::Start => {
            if service.state()? != ServiceState::Running {
                service.start()?;
            }
        }
        Action::Stop | Action::Remove => {
            let state = service.state()?;
            if state != ServiceState::Stopped {
                if state != ServiceState::StopPending {
                    service.stop()?;
                }
                let mut stopped = false;
                for _ in 0..150 {
                    if service.state()? == ServiceState::Stopped {
                        stopped = true;
                        break;
                    }
                    wait();
                }
                if !stopped {
                    return Err("service stop timed out; index retained".into());
                }
            }
            if action == Action::Remove {
                service.delete()?;
            }
        }
        Action::Install => return Err("install must use the service manager".into()),
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    Task,
    User,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    pub fallback: Option<Fallback>,
}
pub fn fallback(path: &Path, ask: impl FnOnce() -> Result<Fallback>) -> Result<Fallback> {
    let mut settings: Settings = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => return Err(e.into()),
    };
    if let Some(mode) = settings.fallback {
        return Ok(mode);
    }
    let mode = ask()?;
    settings.fallback = Some(mode);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&settings)?)?;
    std::fs::rename(tmp, path)?;
    Ok(mode)
}
/// Cleanup only the fixed public index child; never follow junctions or symlinks.
pub fn remove(scm: &impl Scm, root: &Path) -> Result<()> {
    scm.apply(Action::Remove)?;
    let target = root.join("index");
    if !target.exists() {
        return Ok(());
    }
    reject_links(&target)?;
    std::fs::remove_dir_all(target)?;
    Ok(())
}
fn reject_links(path: &Path) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_attributes() & 0x400 != 0 {
        return Err("public index contains a reparse point; cleanup refused".into());
    }
    if meta.is_dir() {
        for entry in std::fs::read_dir(path)? {
            reject_links(&entry?.path())?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(bool);
    impl Scm for Fake {
        fn apply(&self, _: Action) -> Result<()> {
            if self.0 {
                Ok(())
            } else {
                Err("blocked".into())
            }
        }
    }
    struct ManagedFake {
        state: std::cell::Cell<ServiceState>,
        events: std::cell::RefCell<Vec<&'static str>>,
        stuck: bool,
    }
    impl ManagedFake {
        fn new(state: ServiceState, stuck: bool) -> Self {
            Self {
                state: std::cell::Cell::new(state),
                events: Default::default(),
                stuck,
            }
        }
    }
    impl ManagedService for ManagedFake {
        fn state(&self) -> Result<ServiceState> {
            Ok(self.state.get())
        }
        fn start(&self) -> Result<()> {
            self.events.borrow_mut().push("start");
            self.state.set(ServiceState::Running);
            Ok(())
        }
        fn stop(&self) -> Result<()> {
            self.events.borrow_mut().push("stop");
            if !self.stuck {
                self.state.set(ServiceState::Stopped);
            }
            Ok(())
        }
        fn delete(&self) -> Result<()> {
            assert_eq!(self.state.get(), ServiceState::Stopped);
            self.events.borrow_mut().push("delete");
            Ok(())
        }
    }
    #[test]
    fn lifecycle_is_idempotent_and_removes_after_stop() {
        let fake = ManagedFake::new(ServiceState::Stopped, false);
        control(&fake, Action::Start, || {}).unwrap();
        control(&fake, Action::Start, || {}).unwrap();
        control(&fake, Action::Remove, || {}).unwrap();
        assert_eq!(*fake.events.borrow(), ["start", "stop", "delete"]);
    }
    #[test]
    fn stop_timeout_never_deletes() {
        let fake = ManagedFake::new(ServiceState::Running, true);
        let waits = std::cell::Cell::new(0);
        assert!(control(&fake, Action::Remove, || waits.set(waits.get() + 1)).is_err());
        assert_eq!(waits.get(), 150);
        assert_eq!(*fake.events.borrow(), ["stop"]);
    }
    #[test]
    fn stopped_service_needs_no_stop() {
        let fake = ManagedFake::new(ServiceState::Stopped, false);
        control(&fake, Action::Stop, || panic!("wait")).unwrap();
        assert!(fake.events.borrow().is_empty());
    }
    #[test]
    fn fake_scm_routes_all_commands() {
        struct Recorder(std::cell::RefCell<Vec<Action>>);
        impl Scm for Recorder {
            fn apply(&self, a: Action) -> Result<()> {
                self.0.borrow_mut().push(a);
                Ok(())
            }
        }
        let d = tempfile::tempdir().unwrap();
        let fake = Recorder(Default::default());
        for a in [Action::Install, Action::Start, Action::Stop, Action::Remove] {
            execute(&fake, a, d.path()).unwrap();
        }
        assert_eq!(
            *fake.0.borrow(),
            [Action::Install, Action::Start, Action::Stop, Action::Remove]
        );
    }
    #[test]
    fn service_registration_contract() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("space directory/kfind.exe");
        let index = d.path().join("index");
        let info = WindowsScm {
            executable: exe.clone(),
            index_root: index.clone(),
        }
        .info();
        assert_eq!(info.name, SERVICE_NAME);
        assert_eq!(info.display_name, DISPLAY_NAME);
        assert_eq!(info.executable_path, exe);
        assert_eq!(
            info.launch_arguments,
            vec![
                std::ffi::OsString::from("--service"),
                "--index-root".into(),
                index.into_os_string()
            ]
        );
        assert!(info.account_name.is_none());
        assert_eq!(info.start_type, ServiceStartType::AutoStart);
    }
    #[test]
    fn remembers_once() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        assert_eq!(fallback(&p, || Ok(Fallback::User)).unwrap(), Fallback::User);
        assert_eq!(
            fallback(&p, || panic!("asked twice")).unwrap(),
            Fallback::User
        );
    }
    #[test]
    fn bad_settings_do_not_prompt() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(&p, b"broken").unwrap();
        assert!(fallback(&p, || panic!("must not discard saved preference")).is_err());
    }
    #[test]
    fn failed_remove_preserves_index() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("index")).unwrap();
        assert!(remove(&Fake(false), d.path()).is_err());
        assert!(d.path().join("index").exists());
    }
    #[test]
    fn removes_only_public_index() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("index")).unwrap();
        std::fs::write(d.path().join("keep"), b"private").unwrap();
        remove(&Fake(true), d.path()).unwrap();
        assert!(!d.path().join("index").exists());
        assert!(d.path().join("keep").exists());
    }
}
