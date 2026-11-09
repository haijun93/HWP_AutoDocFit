"""Measure cold extraction and warm cache-hit startup via the app self-test."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]


def compare_launchers(args: argparse.Namespace) -> int:
    import shutil
    from scripts.build_smart_launcher import build, compile_launcher, find_csc, make_payload

    child_source = ROOT / "tests" / "fixtures" / "rust_launcher_args_child.cs"
    with tempfile.TemporaryDirectory(prefix="docfit-rust-launcher-compare-") as temporary:
        root = Path(temporary)
        bundle = root / "bundle"
        bundle.mkdir()
        app = bundle / "HWP_AutoDocFit.exe"
        subprocess.run([str(find_csc()), "/nologo", "/target:exe", f"/out:{app}", str(child_source)], check=True)
        (bundle / "asset.txt").write_text("fixture", encoding="utf-8")
        results = {}
        trace_logs = {}
        for kind in ("csharp", "rust"):
            out = root / kind
            out.mkdir()
            if kind == "csharp":
                payload = out / "smart-launcher-build" / "payload.zip"
                _, manifest = make_payload(bundle, payload, "9.97.0-fixture")
                compile_launcher(payload, manifest, out / "launcher.exe", "9.97.0-fixture", ROOT / "resources" / "app_icon.ico", ROOT / "resources" / "splash.png")
            else:
                build(bundle, out / "launcher.exe", "9.97.0-fixture", "rust")
                manifest = out / "launcher.manifest.txt"
            env = os.environ.copy()
            env["LOCALAPPDATA"] = str(out / "local")
            env["DOCFIT_LAUNCHER_TEST_ROOT"] = str(out / "cache")
            env["DOCFIT_LAUNCHER_MANIFEST"] = str(manifest)
            env["DOCFIT_LAUNCHER_PAYLOAD"] = str(out / "launcher.payload.zip")
            launcher, report = out / "launcher.exe", out / "report.json"
            results[kind] = {}
            for label in ("self_test", "cached"):
                start = time.perf_counter()
                try:
                    traced_env = env.copy()
                    traced_env["HWP_LAUNCHER_TRACE"] = "1"
                    completed = subprocess.run([str(launcher), "--self-test", "--self-test-report", str(report), "argument with spaces"], env=traced_env, timeout=args.timeout, check=True, capture_output=True, text=True)
                    trace_logs.setdefault(kind, {})[label] = completed.stderr.splitlines()
                except subprocess.CalledProcessError as error:
                    raise RuntimeError(f"{kind} {label} failed; launcher={launcher}, manifest={manifest}, payload={env['DOCFIT_LAUNCHER_PAYLOAD']}") from error
                results[kind][label + "_ms"] = round((time.perf_counter() - start) * 1000, 2)
            shutil.rmtree(out / "cache", ignore_errors=True)
            start = time.perf_counter()
            try:
                subprocess.run([str(launcher), "--self-test", "--self-test-report", str(report), "argument with spaces"], env=env, timeout=args.timeout, check=True)
            except subprocess.CalledProcessError as error:
                raise RuntimeError(f"{kind} cold failed; launcher={launcher}, manifest={manifest}, payload={env['DOCFIT_LAUNCHER_PAYLOAD']}") from error
            results[kind]["cold_ms"] = round((time.perf_counter() - start) * 1000, 2)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps({"comparison": results}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        args.report.with_suffix(".trace.json").write_text(json.dumps(trace_logs, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(results, ensure_ascii=False))
    return 0


def _manifest_identity(manifest: Path) -> tuple[str, str]:
    fields = {}
    for line in manifest.read_text(encoding="utf-8").splitlines():
        parts = line.split("\t", 2)
        if len(parts) >= 2 and parts[0] in {"V", "P"}:
            fields[parts[0]] = parts[1]
    if not fields.get("V") or not fields.get("P"):
        raise ValueError("launcher manifest does not contain version and payload hash")
    return fields["V"], fields["P"]


def run_once(exe: Path, report: Path, timeout: int) -> float:
    started = time.perf_counter()
    completed = subprocess.run(
        [str(exe), "--self-test", "--self-test-report", str(report)],
        cwd=exe.parent,
        timeout=timeout,
        check=False,
    )
    elapsed_ms = (time.perf_counter() - started) * 1000
    if completed.returncode != 0:
        raise RuntimeError(f"launcher self-test exit code {completed.returncode}")
    if not report.is_file():
        raise RuntimeError("launcher did not forward --self-test-report")
    result = json.loads(report.read_text(encoding="utf-8"))
    if not result.get("ok"):
        raise RuntimeError("child app self-test reported failure")
    return elapsed_ms


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, default=ROOT / "dist" / "HWP_AutoDocFit.exe")
    parser.add_argument("--manifest", type=Path, default=ROOT / "dist" / "smart-launcher-build" / "payload.manifest.txt")
    parser.add_argument("--report", type=Path, default=ROOT / "dist" / "smart-launcher-startup.json")
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--target-cache-ms", type=float, default=1500)
    parser.add_argument("--target-splash-ms", type=float, default=100)
    parser.add_argument("--launcher", choices=("csharp", "rust"), default="csharp")
    parser.add_argument("--compare", action="store_true", help="measure C# and Rust self-test/cached/cold once using an isolated fixture bundle")
    parser.add_argument("--require-target", action="store_true",
                        help="return failure unless both warm UI launches meet the target")
    args = parser.parse_args()
    if args.compare:
        return compare_launchers(args)
    exe = args.exe.resolve()
    if args.launcher == "rust":
        rust_exe = exe.parent / (exe.stem + ".rust.exe")
        rust_manifest = rust_exe.with_suffix(".manifest.txt")
        rust_payload = rust_exe.with_suffix(".payload.zip")
        if not rust_exe.exists() or not rust_manifest.exists() or not rust_payload.exists():
            raise FileNotFoundError("Rust launcher artifacts are missing; build with scripts/build_smart_launcher.py --launcher rust")
        exe = rust_exe
        args.manifest = rust_manifest
    version, payload_hash = _manifest_identity(args.manifest.resolve())
    cache_name = "app_v" + "".join(c if c not in '<>:"/\\|?*' else "_" for c in version).replace(" ", "_") + "_" + payload_hash[:12]
    cache = Path(os.environ["LOCALAPPDATA"]) / "HWP_AutoDocFit" / cache_name
    with tempfile.TemporaryDirectory(prefix="DocFit Smart Launcher ") as temporary:
        reports = Path(temporary)
        marker = cache / ".docfit-complete"
        cold_ui = None
        if not marker.is_file() or marker.read_text(encoding="utf-8").strip() != payload_hash:
            cold_report = reports / "cold-ui.json"
            subprocess.run(
                [sys.executable, str(ROOT / "scripts" / "smoke_windows_exe.py"),
                 "--exe", str(exe), "--report", str(cold_report), "--smart-launcher", "--require-launcher-splash"],
                cwd=ROOT,
                timeout=args.timeout,
                check=True,
            )
            cold_ui = json.loads(cold_report.read_text(encoding="utf-8"))
        cold_ms = run_once(exe, reports / "cold self-test.json", args.timeout)
        if not marker.is_file() or marker.read_text(encoding="utf-8").strip() != payload_hash:
            raise RuntimeError("cold run did not create a valid cache completion marker")
        marker_mtime = marker.stat().st_mtime_ns
        warm_ms = run_once(exe, reports / "warm self-test.json", args.timeout)
        if marker.stat().st_mtime_ns != marker_mtime:
            raise RuntimeError("second run rewrote the cache marker; expected a cache hit")
        ui_reports = []
        for label in ("warm-ui-first", "warm-ui-second"):
            report = reports / (label + ".json")
            subprocess.run(
                [sys.executable, str(ROOT / "scripts" / "smoke_windows_exe.py"),
                 "--exe", str(exe), "--report", str(report), "--smart-launcher"],
                cwd=ROOT,
                timeout=args.timeout,
                check=True,
            )
            ui_reports.append(json.loads(report.read_text(encoding="utf-8")))
    result = {
        "version": version,
        "payload_sha256": payload_hash,
        "self_test_start_ms": round(cold_ms, 2),
        "cached_start_ms": round(warm_ms, 2),
        "cached_start_target_ms": args.target_cache_ms,
        "cached_start_within_target": warm_ms <= args.target_cache_ms,
        "cold_ui_window_ready_ms": cold_ui.get("window_ready_ms") if cold_ui else None,
        "cold_ui_splash_visible_ms": cold_ui.get("first_run_splash_ms") if cold_ui else None,
        "cold_ui_splash_within_target": bool(
            cold_ui and cold_ui.get("first_run_splash_ms") is not None
            and cold_ui["first_run_splash_ms"] <= args.target_splash_ms
        ),
        "warm_ui_first_ms": ui_reports[0].get("window_ready_ms"),
        "warm_ui_second_ms": ui_reports[1].get("window_ready_ms"),
        "warm_ui_first_within_target": (ui_reports[0].get("window_ready_ms") or float("inf")) <= args.target_cache_ms,
        "warm_ui_second_within_target": (ui_reports[1].get("window_ready_ms") or float("inf")) <= args.target_cache_ms,
        "cache_hit_marker_unchanged": True,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False))
    if args.require_target and not all((
        result["cold_ui_splash_within_target"],
        result["warm_ui_first_within_target"],
        result["warm_ui_second_within_target"],
    )):
        print(
            "Smart launcher startup target not met: "
            f"splash target={args.target_splash_ms:.0f} ms, observed={result['cold_ui_splash_visible_ms']}; "
            f"warm UI target={args.target_cache_ms:.0f} ms, "
            f"first={result['warm_ui_first_ms']}, second={result['warm_ui_second_ms']}",
            file=sys.stderr,
        )
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
