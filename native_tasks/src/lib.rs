//! Rust 창의 작업 프로세스 연결. JSON은 상속된 stdio로만 전달한다.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
#[cfg(windows)]
pub mod window;

pub fn catalog() -> Value {
    serde_json::from_str(include_str!("rules.json")).expect("규칙 표")
}
pub fn data_dir() -> PathBuf {
    docfit_config::settings_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .into()
}

/// 작업 관측은 명시적으로 지정한 파일에만 쓴다(기본 실행에서는 기록 없음).
pub fn trace(event: &Value) {
    if let Some(path) = std::env::var_os("DOCFIT_NATIVE_TRACE") {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{event}");
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Requesting,
    Running,
    Stopping,
    Closing,
}
pub fn phase(pending: bool, running: bool, cancel: bool, closing: bool) -> Phase {
    if closing {
        Phase::Closing
    } else if cancel {
        Phase::Stopping
    } else if running {
        Phase::Running
    } else if pending {
        Phase::Requesting
    } else {
        Phase::Idle
    }
}

pub fn command_for(
    root: &Path,
    own: &Path,
    env: impl Fn(&str) -> Option<PathBuf>,
) -> Result<Command, String> {
    if let Some(python) = env("DOCFIT_PYTHON") {
        let source = env("DOCFIT_SOURCE_ROOT").unwrap_or_else(|| root.into());
        if !python.is_file() || !source.join("hwp-auto-docfit.py").is_file() {
            return Err("Python 작업 경로를 확인해 주세요.".into());
        }
        let mut cmd = Command::new(python);
        cmd.arg(source.join("hwp-auto-docfit.py"))
            .arg("--native-tasks");
        cmd.current_dir(source);
        return Ok(cmd);
    }
    if let Some(app) = env("DOCFIT_APP_EXE") {
        if !app.is_file() {
            return Err("작업 프로그램을 찾지 못했습니다.".into());
        }
        let mut cmd = Command::new(app);
        cmd.arg("--native-tasks");
        return Ok(cmd);
    }
    if root.join("hwp-auto-docfit.py").is_file() && root.join(".venv/Scripts/python.exe").is_file()
    {
        let mut cmd = Command::new(root.join(".venv/Scripts/python.exe"));
        cmd.arg(root.join("hwp-auto-docfit.py"))
            .arg("--native-tasks");
        cmd.current_dir(root);
        return Ok(cmd);
    }
    for dir in own.ancestors().skip(1).take(4) {
        let app = dir.join("HWP_AutoDocFit.exe");
        if app.is_file() {
            let mut cmd = Command::new(app);
            cmd.arg("--native-tasks");
            return Ok(cmd);
        }
    }
    Err("작업 프로그램이 없습니다. 앱과 함께 배치하거나 개발 실행 경로를 지정해 주세요.".into())
}

pub struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}
impl Client {
    pub fn start(data: &Path) -> Result<Self, String> {
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let own = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut command = command_for(&root, &own, |key| std::env::var_os(key).map(PathBuf::from))?;
        command.args(["--data-dir".as_ref(), data.as_os_str()]);
        let args: Vec<_> = std::env::args_os().collect();
        if let Some(pair) = args.windows(2).find(|p| p[0] == "--settings") {
            command.arg("--settings").arg(&pair[1]);
        }
        command
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYINSTALLER_SUPPRESS_SPLASH_SCREEN", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().ok_or("작업 출력 없음")?);
        Ok(Self {
            child,
            input,
            output,
        })
    }
    pub fn call(&mut self, request: &Value) -> Result<Value, String> {
        let input = self.input.as_mut().ok_or("작업 종료")?;
        serde_json::to_writer(&mut *input, request).map_err(|e| e.to_string())?;
        input
            .write_all(b"\n")
            .and_then(|_| input.flush())
            .map_err(|e| e.to_string())?;
        let mut line = String::new();
        std::io::Read::take(&mut self.output, 16 * 1024 * 1024)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if line.is_empty() {
            return Err("작업 프로세스가 응답 없이 종료되었습니다.".into());
        }
        serde_json::from_str(&line).map_err(|e| format!("작업 응답 오류: {e}"))
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.wait();
    }
}

pub fn request(action: &str, args: Value) -> Value {
    json!({"op":"rename","payload":{"action":action,"args":args}})
}

/// 입력 폼에서 선택지/숫자/체크를 다시 같은 저장 자료형으로 모은다.
pub fn field_value(default: &Value, text: &str, checked: bool) -> Result<Value, String> {
    if default.is_boolean() {
        Ok(Value::Bool(checked))
    } else if default.is_number() {
        text.trim()
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| "정수를 입력해 주세요.".into())
    } else {
        Ok(Value::String(text.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_app_rule_kinds_have_native_forms() {
        let c = catalog();
        let rules = c.as_array().unwrap();
        assert_eq!(rules.len(), 23);
        let mut kinds = std::collections::BTreeSet::new();
        for r in rules {
            assert!(kinds.insert(r["kind"].as_str().unwrap()));
            assert!(r["label"].as_str().is_some());
            assert!(r["defaults"].is_object());
        }
    }
    #[test]
    fn typed_fields_reject_bad_numbers_and_preserve_multiline_korean() {
        assert_eq!(field_value(&json!(1), "-7", false).unwrap(), json!(-7));
        assert!(field_value(&json!(1), "1.5", false).is_err());
        assert_eq!(field_value(&json!(true), "", false).unwrap(), json!(false));
        assert_eq!(
            field_value(&json!(""), "한글\n둘째", false).unwrap(),
            json!("한글\n둘째")
        );
    }
    #[test]
    fn explicit_missing_worker_never_falls_back() {
        assert!(
            command_for(Path::new("."), Path::new("native.exe"), |_| Some(
                "missing-file.exe".into()
            ))
            .is_err()
        );
    }
    #[test]
    fn closing_and_cancel_cannot_be_hidden_by_a_poll_request() {
        assert_eq!(phase(true, true, true, true), Phase::Closing);
        assert_eq!(phase(true, true, true, false), Phase::Stopping);
        assert_eq!(phase(true, true, false, false), Phase::Running);
        assert_eq!(phase(true, false, false, false), Phase::Requesting);
        assert_eq!(phase(false, false, false, false), Phase::Idle);
    }
    #[test]
    fn add_rule_only_offers_positions_the_engine_accepts() {
        #[cfg(windows)]
        {
            assert_eq!(window::choices("position", "add").len(), 2);
            assert_eq!(window::choices("position", "number").len(), 3);
        }
    }
}
