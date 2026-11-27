"""PHOTOPATH1 전체 Rust/관련 Python 시험을 합성 TEMP 안에서 실행한다.

앱 개발 가상환경 Python으로 실행: python scripts/test_photopath1_synthetic.py --artifacts <시험 폴더>
작업 폴더 밖의 빈 시험 폴더를 지정한다. --junction 은 TEMP/TMP를 정션 경로로 지정한다.
실제 klib는 --klib 또는 KLIB_EXE로 지정한다. 설치 서비스는 호출하지 않는다.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
MODULES = (
    "test_photopath1", "test_photo_library", "test_photo_album_service", "test_photofix1",
    "test_photoview_library_schema", "test_photoview_window_service", "test_library_db",
    "test_fileops1",
)

# 기존 취소 시험은 첫 고정 드라이브를 인수로 준다. 그 인수만 큰 합성 트리로 바꿔
# 실제 klib를 실행한다. stdout/종료 코드 그대로 전달, 지연/결과 조작 없음.
# Job Object로 부모 취소 시 실제 klib도 종료한다. 사용자 루트는 klib에 전달되지 않는다.
SCAN_PROXY = r'''
use std::{env, process::{Command, exit}, os::windows::io::AsRawHandle};
type Handle = *mut std::ffi::c_void;
#[link(name="kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(a: Handle, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: u32, data: *const u64, size: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}
fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let sandbox = std::path::PathBuf::from(env::var_os("PHOTOPATH1_SANDBOX").unwrap()).canonicalize().unwrap();
    let mut roots = false;
    for arg in &mut args {
        if arg == "--roots" { roots = true; continue; }
        if arg.starts_with("--") { roots = false; }
        if !roots { continue; }
        if arg.len() == 3 && arg.as_bytes()[1] == b':' {
            *arg = env::var("PHOTOPATH1_CANCEL_TREE").unwrap();
        }
        if arg == r"Z:\photoview_win_no_such_root" {
            *arg = sandbox.join("nonexistent-scan-root").to_string_lossy().into_owned();
            assert!(!std::path::Path::new(arg).exists());
            continue;
        }
        let path = std::path::Path::new(arg).canonicalize().unwrap();
        assert!(path.starts_with(&sandbox), "scan outside synthetic sandbox blocked");
    }
    unsafe {
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        assert!(!job.is_null());
        // JOBOBJECT_EXTENDED_LIMIT_INFORMATION (x64), LimitFlags: KILL_ON_JOB_CLOSE.
        assert_eq!(std::mem::size_of::<usize>(), 8);
        let mut limits = [0u64; 18]; limits[2] = 0x2000;
        assert_ne!(SetInformationJobObject(job, 9, limits.as_ptr(), 144), 0);
        let mut child = Command::new(env::var_os("PHOTOPATH1_REAL_KLIB").unwrap()).args(args).spawn().unwrap();
        if AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
            let _ = child.kill(); panic!("cannot isolate scan child");
        }
        let code = child.wait().unwrap().code().unwrap_or(1);
        CloseHandle(job); exit(code);
    }
}
'''


def run(args, env, log):
    with log.open("w", encoding="utf-8") as stream:
        result = subprocess.run(args, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
    text = log.read_text(encoding="utf-8")
    print(text, flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)


def python_tests():
    """기존 사진대지 시험의 기본 HWPX 입력도 합성 템플릿으로 공급한다."""
    import tempfile
    import unittest
    from unittest.mock import patch

    sys.path.insert(0, str(ROOT))
    import tests  # 사용자 데이터 격리를 먼저 적용한다.
    from scripts.test_mergerust_synthetic import synthetic_template
    from docfit_app import photo_album_service

    suite = unittest.defaultTestLoader.loadTestsFromNames(["tests." + name for name in MODULES])
    with tempfile.TemporaryDirectory(prefix="photopath1-template-") as folder:
        template = Path(folder) / "synthetic.hwpx"
        synthetic_template(template)
        with patch.object(photo_album_service, "default_base_hwpx", return_value=template):
            result = unittest.TextTestRunner(verbosity=1).run(suite)
    return 0 if result.wasSuccessful() else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--klib", type=Path, default=os.environ.get("KLIB_EXE"))
    parser.add_argument("--junction", action="store_true")
    args = parser.parse_args()
    prefix = "junction-" if args.junction else "normal-"
    artifacts = args.artifacts.resolve()
    assert not artifacts.is_relative_to(ROOT), "시험 산출물은 작업 폴더 밖에 둔다"
    assert args.klib and args.klib.is_file(), "개발용 klib 실행 파일 지정 필요"
    artifacts.mkdir(parents=True, exist_ok=True)
    real_temp = artifacts / "tmp"
    real_temp.mkdir(exist_ok=True)
    temp = real_temp
    env = dict(os.environ)
    env.pop("DOCFIT_TEST_RECYCLE", None)
    for name, folder in (("RUSTUP_HOME", ".rustup"), ("CARGO_HOME", ".cargo")):
        env.setdefault(name, str(Path(env["USERPROFILE"]) / folder))
    if args.junction:
        temp = artifacts / "junction-temp"
        assert not temp.exists(), "기존 정션을 재사용하지 않는다"
        subprocess.run(["powershell", "-NoProfile", "-Command",
                        "New-Item -ItemType Junction -Path $env:PHOTOPATH1_LINK "
                        "-Target $env:PHOTOPATH1_REAL | Out-Null"], check=True,
                       env={**env, "PHOTOPATH1_LINK": str(temp), "PHOTOPATH1_REAL": str(real_temp)})
    try:
        if args.junction:
            assert temp.is_junction(), "TEMP 정션이 실제로 만들어져야 한다"
        print(f"TEMP_IS_JUNCTION={temp.is_junction()}", flush=True)
        env.update(TEMP=str(temp), TMP=str(temp), PYTHONIOENCODING="utf-8",
                   CARGO_TARGET_DIR=str(artifacts / "target"), DOCFIT_FORBID_REAL_DATA="1",
                   DOCFIT_REAL_LOCALAPPDATA=env.get("LOCALAPPDATA", ""))
        mingw = Path(env["LOCALAPPDATA"]) / "DocFitTools" / "mingw64" / "bin"
        if mingw.is_dir():
            env["PATH"] = str(mingw) + os.pathsep + env["PATH"]
        for name in ("LOCALAPPDATA", "APPDATA", "USERPROFILE", "HOME", "KAGENT_DATA"):
            folder = real_temp / ("isolated-" + name.lower())
            folder.mkdir(exist_ok=True)
            env[name] = str(folder)
        cancel_tree = real_temp / "cancel-tree"
        cancel_tree.mkdir(exist_ok=True)
        print("Preparing synthetic cancellation tree", flush=True)
        # 충분한 실제 합성 항목을 순회하므로 기존 300ms 이전 미완료 검사를 유지한다.
        for i in range(4096):
            folder = cancel_tree / f"folder-{i:05}"
            folder.mkdir(exist_ok=True)
            for j in range(16):
                path = folder / f"synthetic-{j:02}.jpg"
                if not path.exists():
                    path.write_bytes(b"synthetic")
        print("Synthetic cancellation tree: 4096 folders, 65536 files", flush=True)
        proxy_source = artifacts / "scan_proxy.rs"
        proxy_source.write_text(SCAN_PROXY, encoding="utf-8")
        proxy = artifacts / "scan_proxy.exe"
        run(["rustc", "--edition=2024", str(proxy_source), "-o", str(proxy)], env, artifacts / (prefix + "proxy-build.log"))
        env.update(PHOTOPATH1_SANDBOX=str(real_temp), PHOTOPATH1_CANCEL_TREE=str(cancel_tree),
                   PHOTOPATH1_REAL_KLIB=str(args.klib.resolve()), DOCFIT_KLIB=str(proxy), KLIB_EXE=str(proxy))
        cargo = ["cargo", "--locked", "--manifest-path", "photoview_win/Cargo.toml"]
        run([cargo[0], "build", *cargo[1:], "--example", "path_peer"], env, artifacts / (prefix + "peer-build.log"))
        env["PHOTOPATH1_RUST_PEER"] = str(artifacts / "target/debug/examples/path_peer.exe")
        run([cargo[0], "test", *cargo[1:]], env, artifacts / (prefix + "cargo-test.log"))
        run([sys.executable, str(Path(__file__).resolve()), "--python-tests"],
            env, artifacts / (prefix + "python-test.log"))
    finally:
        if args.junction:
            temp.rmdir()  # 정션 자체만 제거한다. 합성 시험 산출물은 기록으로 남긴다.


if __name__ == "__main__":
    if sys.argv[1:] == ["--python-tests"]:
        raise SystemExit(python_tests())
    main()
