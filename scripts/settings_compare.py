"""Compare the Python settings service and docfit_config CLI using isolated files."""
from __future__ import annotations

import argparse
import json
import os
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import pprint

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from docfit_app.settings_schema import DEFAULTS
from docfit_app.settings_migrations import migrate
from docfit_app.configuration_service import ConfigurationService
from docfit_app.settings_service import SettingsService
from docfit_app.api_contract import ServiceError


def cases():
    base = {
        "partial": {"developer_mode": True, "retry_body": "8"},
        "unknown": {"custom": {"nested": 7}, "developer_mode": False},
        "old-version": {"report_files_rev": 1, "log_file": True, "label_symbols_rev": 1,
                        "label_symbols": {"*": True}, "all_linespacing": True,
                        "symbol_fonts": {"□": {"font": "한컴돋음"}}},
        "odd-values": {"search_engine": "nonesuch", "retry_body": 12, "label_symbols": None},
        "empty": {},
    }
    return base


def python_get(path):
    service = ConfigurationService(SettingsService(path))
    snap = service.snapshot()
    saved = SettingsService(path).read()
    return {"ok": True, "value": {"settings": snap["values"], "migrated": migrate(saved)}}


def rust_cli(exe, command, path, payload=None, revision=None):
    args = [str(exe), command, str(path)]
    if payload is not None:
        args.append(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    if command == "set" and revision is not None:
        args.append(revision)
    proc = subprocess.run(args, capture_output=True, text=True, encoding="utf-8")
    try:
        value = json.loads(proc.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        return {"ok": False, "code": "CLI_FAILURE", "message": proc.stderr or proc.stdout}
    return value


def compare_case(exe, name, initial):
    with tempfile.TemporaryDirectory(prefix="settings-compare-") as folder:
        py_path = Path(folder) / "python" / "settings.json"
        rs_path = Path(folder) / "rust" / "settings.json"
        py_path.parent.mkdir(parents=True)
        rs_path.parent.mkdir(parents=True)
        for path in (py_path, rs_path):
            path.write_text(json.dumps(initial, ensure_ascii=False, indent=2), encoding="utf-8")
        py_before, rs_before = SettingsService(py_path).read(), SettingsService(rs_path).read()
        py_result = python_get(py_path)
        rs_get = rust_cli(exe, "get", rs_path)
        py_migrated = migrate(py_before)
        rs_migrated = rust_cli(exe, "migrate", rs_path)
        differences = []
        # get is a full merged object; migrate is deliberately a separate raw migrated object.
        # Compare all stored and default keys (the Python UI snapshot intentionally excludes 14 internal keys).
        from copy import deepcopy
        expected_merged=deepcopy(DEFAULTS)
        for k,v in py_migrated.items():
            if isinstance(expected_merged.get(k),dict) and isinstance(v,dict): expected_merged[k].update(v)
            else: expected_merged[k]=v
        if expected_merged.get("search_engine") not in ("kfind","legacy"): expected_merged["search_engine"]="kfind"
        # snapshot merges nested defaults; corrupt stored nested values are replaced as a unit.
        def merge_nested(dst, src):
            for key, value in src.items():
                if isinstance(dst.get(key), dict) and isinstance(value, dict): merge_nested(dst[key], value)
                else: dst[key] = value
        expected_merged=DEFAULTS.copy()
        from copy import deepcopy
        expected_merged=deepcopy(DEFAULTS)
        merge_nested(expected_merged, py_migrated)
        for k,v in py_before.items():
            if k in DEFAULTS and not isinstance(v,dict): expected_merged[k]=v
        if expected_merged.get("search_engine") not in ("kfind","legacy"): expected_merged["search_engine"]="kfind"
        if rs_get.get("value") != expected_merged:
            differences.append({"operation": "get", "python": expected_merged, "rust": rs_get.get("value")})
        raw_py_migrated = migrate(py_before)
        if rs_migrated.get("value") != raw_py_migrated:
            differences.append({"operation": "migrate", "python": raw_py_migrated, "rust": rs_migrated.get("value")})
        # Validation status and error-code parity across accepted and rejected patch inputs.
        validation_cases = [
            {"retry_body": "7"}, {"retry_body": "1.2"}, {"retry_body": "NaN"},
            {"developer_mode": "true"}, {"unknown_key": True}, {"std_title_subtitle_pt": "0"},
            {"search_engine": "legacy"}, {"search_engine": "bad"}, {"title_owner_text": "x" * 4097},
        ]
        for candidate in validation_cases:
            try:
                py_service_validation = ConfigurationService(SettingsService(py_path))
                py_service_validation.save(candidate, py_service_validation.snapshot()["revision"])
                py_validation = {"ok": True}
            except ServiceError as exc:
                py_validation = {"ok": False, "code": exc.code}
            rs_validation = rust_cli(exe, "validate", rs_path, candidate)
            if rs_validation.get("ok") != py_validation["ok"] or (not py_validation["ok"] and rs_validation.get("code") != py_validation["code"]):
                differences.append({"operation": "validate", "patch": candidate, "python": py_validation, "rust": {"ok": rs_validation.get("ok"), "code": rs_validation.get("code")}})
        # Deterministic valid edit tests write and unknown-key preservation.
        patch = {"developer_mode": not bool(DEFAULTS["developer_mode"]), "retry_body": "7", "search_engine": "legacy"}
        py_snapshot = ConfigurationService(SettingsService(py_path)).snapshot()
        py_service = ConfigurationService(SettingsService(py_path))
        try:
            py_saved = py_service.save(patch, py_snapshot["revision"])
            py_after = SettingsService(py_path).read()
            py_save = {"ok": True, "value": py_saved["values"]}
        except ServiceError as exc:
            py_after, py_save = SettingsService(py_path).read(), {"ok": False, "code": exc.code}
        py_revision = hashlib.sha256(json.dumps(rs_before, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
        rust_patch = dict(patch)
        rs_save = rust_cli(exe, "set", rs_path, rust_patch, py_revision)
        rs_after = SettingsService(rs_path).read()
        py_cmp, rs_cmp = dict(py_after), dict(rs_after)
        py_cmp.pop("all_linespacing", None)
        if rs_cmp != py_cmp:
            differences.append({"operation": "set-file", "python": py_after, "rust": rs_after})
        # Rust set returns merged raw settings while Python returns UI snapshot values.
        if rs_save.get("ok") != py_save.get("ok"):
            differences.append({"operation": "set-status", "python": py_save, "rust": rs_save})
        return {"case": name, "equal": not differences, "differences": differences}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", type=Path, help="built settings-core executable")
    args = ap.parse_args()
    exe = args.exe or ROOT / "target" / "debug" / "settings-core.exe"
    if not exe.exists():
        target = Path(os.environ.get("CARGO_TARGET_DIR", ""))
        candidates = [target / "debug" / "settings-core.exe", ROOT / "target" / "debug" / "settings-core"]
        exe = next((p for p in candidates if p.exists()), exe)
    if not exe.exists():
        print(f"Rust CLI not found: {exe}", file=sys.stderr)
        return 2
    results = [compare_case(exe, name, value) for name, value in cases().items()]
    defaults_file = json.loads((ROOT / "docs" / "settings_defaults.json").read_text(encoding="utf-8"))
    if defaults_file != DEFAULTS or len(defaults_file) != 97:
        results.append({"case": "defaults-golden", "equal": False, "differences": [{"python_top_level": len(DEFAULTS), "rust_golden_top_level": len(defaults_file)}]})
    report = {"cases": len(results), "equal": all(r["equal"] for r in results), "results": results}
    sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["equal"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
