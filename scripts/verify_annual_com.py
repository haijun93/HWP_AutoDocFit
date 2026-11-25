"""Open, save and reopen synthetic annual reports with an isolated Hancom COM instance.

Uses the repository's common COM lock; preserves inputs and writes evidence outside the repo.
Missing Windows/COM or a busy lock is a failure, never a successful verification.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from docfit_core.save_guard import com_test_lock
from docfit_core.format_samples import REGISTRY_VALUE, LEGACY_REGISTRY_VALUE
from docfit_core.hwpx import inspect_hwpx
from docfit_core import hwp_process


def verify_with_hwp_com(files: list[Path], output: Path) -> list[dict]:
    if sys.platform != "win32":
        raise RuntimeError("Windows COM verification is unavailable")
    import pythoncom
    import win32com.client

    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    rows = []
    pythoncom.CoInitialize()
    try:
        with com_test_lock("ANNUALB1", "합성 HWPX 열기·저장·재열기", timeout_sec=5):
            before = hwp_process.hwp_pids()
            app = None
            try:
                app = win32com.client.DispatchEx("HwpFrame.HwpObject")
                hwp_process.register_owned(hwp_process.hwp_pids() - before)
                app.XHwpWindows.Item(0).Visible = False
                if not any(app.RegisterModule("FilePathCheckDLL", value) for value in (REGISTRY_VALUE, LEGACY_REGISTRY_VALUE)):
                    raise RuntimeError("한/글 보안 모듈 등록 실패")
                app.SetMessageBoxMode(0x00020000)
                version = str(app.Version)
                for source in files:
                    source = source.resolve(strict=True)
                    digest = hashlib.sha256(source.read_bytes()).hexdigest()
                    copy = output / ("input_" + source.name)
                    saved = output / source.name
                    if source in (copy, saved):
                        raise ValueError("입력과 검증 사본 경로가 같습니다")
                    shutil.copyfile(source, copy)
                    original = inspect_hwpx(copy)
                    if not app.Open(str(copy), "HWPX", "forceopen:true"):
                        raise RuntimeError(f"Open 실패: {source.name}")
                    pages = int(app.PageCount)
                    text = app.GetTextFile("UNICODE", "") or ""
                    if pages < 1 or not text.strip():
                        raise RuntimeError("쪽 또는 본문이 비어 있습니다")
                    if not app.SaveAs(str(saved), "HWPX", "") or not saved.is_file():
                        raise RuntimeError("SaveAs 실패")
                    app.Clear(1)
                    after = inspect_hwpx(saved)
                    if after.table_count != original.table_count:
                        raise RuntimeError("저장 후 네이티브 표 개수 불일치")
                    if not app.Open(str(saved), "HWPX", "forceopen:true"):
                        raise RuntimeError("저장본 재열기 실패")
                    reopened_pages = int(app.PageCount)
                    reopened_text = app.GetTextFile("UNICODE", "") or ""
                    if reopened_pages != pages or reopened_text != text:
                        raise RuntimeError("재열기 후 쪽 수 또는 본문 불일치")
                    app.Clear(1)
                    if hashlib.sha256(source.read_bytes()).hexdigest() != digest:
                        raise RuntimeError("입력 해시 변경")
                    row = {"file": source.name, "hwp_version": version, "pages": pages, "reopened_pages": reopened_pages,
                           "tables": after.table_count, "saved_bytes": saved.stat().st_size, "source_unchanged": True}
                    rows.append(row)
                    print(json.dumps(row, ensure_ascii=False), flush=True)
            finally:
                if app is not None:
                    try:
                        app.Clear(1)
                        app.Quit()
                    finally:
                        hwp_process.kill_owned()
    finally:
        pythoncom.CoUninitialize()
    (output / "com_result.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8")
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="+", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    verify_with_hwp_com(args.files, args.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
