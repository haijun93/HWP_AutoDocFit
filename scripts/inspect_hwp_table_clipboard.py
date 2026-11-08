"""한/글에서 표를 복사했을 때 클립보드에 들어오는 형식을 조사하는 스크립트.

사용자 문서는 건드리지 않으며, 한/글 COM 잠금(C:\\HWP_AUTODOCFIT\\.com-test.lock)을 획득한 뒤
독립된 격리 HWP 프로세스(DispatchEx)에서 임시 표 문서를 열어 표를 복사하고,
클립보드에 등록되는 모든 포맷(HTML Format, RTF, 한/글 고유 형식 등)의 메타데이터와 내용을 수집한다.
조사 완료 후 격리 프로세스 및 임시 파일, 잠금을 안전하게 정리한다.
"""
from __future__ import annotations

import contextlib
from datetime import datetime
import json
import os
from pathlib import Path
import sys
import tempfile
import time
from zipfile import ZIP_DEFLATED, ZipFile

LOCK_PATH = Path(r"C:\HWP_AUTODOCFIT\.com-test.lock")


@contextlib.contextmanager
def com_lock(timeout_sec=60.0, poll_sec=0.2, stale_sec=600.0):
    deadline = time.monotonic() + timeout_sec
    acquired = None
    LOCK_PATH.parent.mkdir(parents=True, exist_ok=True)
    while True:
        # stale lock 확인 (600초 이상 지난 락 파일/디렉토리)
        try:
            if LOCK_PATH.exists():
                mtime = LOCK_PATH.stat().st_mtime
                if time.time() - mtime > stale_sec:
                    print(f"  [경고] 오래된(stale) COM 락({time.time() - mtime:.1f}초 경과) 발견. 강제 해제합니다: {LOCK_PATH}")
                    if LOCK_PATH.is_dir():
                        os.rmdir(str(LOCK_PATH))
                    else:
                        LOCK_PATH.unlink()
        except Exception as e:
            print(f"  [경고] stale lock 검사 중 예외: {e}")

        # mkdir 방식 시도
        try:
            os.mkdir(str(LOCK_PATH))
            acquired = "dir"
            break
        except FileExistsError:
            pass
        except OSError:
            pass
        # 파일 방식도 시도
        try:
            fd = os.open(str(LOCK_PATH), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            try:
                os.write(fd, f"inspect_hwp_table_clipboard {datetime.now().isoformat()}\n".encode("utf-8"))
            finally:
                os.close(fd)
            acquired = "file"
            break
        except FileExistsError:
            pass
        except OSError:
            pass
        if time.monotonic() >= deadline:
            raise TimeoutError(f"한/글 COM 잠금({LOCK_PATH}) 대기 시간({timeout_sec}s)을 초과했습니다.")
        time.sleep(poll_sec)
    try:
        yield LOCK_PATH
    finally:
        if acquired == "dir":
            try:
                os.rmdir(str(LOCK_PATH))
            except Exception:
                pass
        elif acquired == "file":
            try:
                LOCK_PATH.unlink()
            except Exception:
                pass


def create_sample_table_hwpx(path: Path) -> Path:
    """table_sketch.to_hwpx를 이용해 한/글 2020이 완벽히 인식하는 2x2 표 HWPX를 생성한다."""
    from docfit_core import table_sketch

    sketch = {
        "grid": {"cols": 20, "rows": 15, "cell_mm": 5.0},
        "lines": [
            # 외곽 사각형 (0,0) ~ (8,4)
            {"x1": 0, "y1": 0, "x2": 8, "y2": 0, "width": "thick", "style": "solid", "color": "#000000"},
            {"x1": 0, "y1": 4, "x2": 8, "y2": 4, "width": "thick", "style": "solid", "color": "#000000"},
            {"x1": 0, "y1": 0, "x2": 0, "y2": 4, "width": "thin", "style": "solid", "color": "#000000"},
            {"x1": 8, "y1": 0, "x2": 8, "y2": 4, "width": "thin", "style": "solid", "color": "#000000"},
            # 내부 가로선 y=2, 세로선 x=4
            {"x1": 0, "y1": 2, "x2": 8, "y2": 2, "width": "double", "style": "solid", "color": "#000000"},
            {"x1": 4, "y1": 0, "x2": 4, "y2": 4, "width": "thin", "style": "solid", "color": "#000000"},
        ],
        "texts": [
            {"x": 0, "y": 0, "text": "항목구분", "bold": True, "align": "center"},
            {"x": 4, "y": 0, "text": "비고(원)", "bold": True, "align": "center"},
            {"x": 0, "y": 2, "text": "데이터A", "bold": False, "align": "left"},
            {"x": 4, "y": 2, "text": "12,500", "bold": False, "align": "right"},
        ],
        "fills": [
            {"x1": 0, "y1": 0, "x2": 8, "y2": 2, "color": "#E2EFDA"},
        ],
        "images": [],
    }
    return table_sketch.to_hwpx(sketch, path)


def inspect_clipboard():
    import pythoncom
    import win32clipboard

    pythoncom.CoInitialize()
    try:
        for _ in range(10):
            try:
                win32clipboard.OpenClipboard()
                break
            except Exception:
                time.sleep(0.05)
        else:
            raise RuntimeError("클립보드를 열 수 없습니다.")

        formats = []
        fmt = 0
        while True:
            fmt = win32clipboard.EnumClipboardFormats(fmt)
            if fmt == 0:
                break
            try:
                name = win32clipboard.GetClipboardFormatName(fmt)
            except Exception:
                name = None
            formats.append((fmt, name))

        standard_names = {
            1: "CF_TEXT",
            2: "CF_BITMAP",
            3: "CF_METAFILEPICT",
            7: "CF_OEMTEXT",
            8: "CF_DIB",
            13: "CF_UNICODETEXT",
            14: "CF_ENHMETAFILE",
            15: "CF_HDROP",
            16: "CF_LOCALE",
            17: "CF_DIBV5",
        }

        details = []
        for fmt_id, custom_name in formats:
            display_name = custom_name or standard_names.get(fmt_id, f"Format_{fmt_id}")
            size = 0
            sample = ""
            try:
                data = win32clipboard.GetClipboardData(fmt_id)
                if isinstance(data, bytes):
                    size = len(data)
                    # 텍스트 계열이면 디코딩 시도
                    try:
                        sample = data[:500].decode("utf-8", errors="replace")
                    except Exception:
                        sample = repr(data[:100])
                elif isinstance(data, str):
                    size = len(data)
                    sample = data[:500]
                else:
                    sample = repr(data)[:100]
            except Exception as e:
                sample = f"<읽기 오류: {e}>"

            details.append({
                "id": fmt_id,
                "name": display_name,
                "custom_name": custom_name,
                "size": size,
                "sample": sample,
            })
        return details
    finally:
        win32clipboard.CloseClipboard()
        pythoncom.CoUninitialize()


def get_hwp_pids() -> set[int]:
    import psutil
    pids = set()
    for p in psutil.process_iter(['pid', 'name']):
        try:
            if p.info['name'] and p.info['name'].lower() == 'hwp.exe':
                pids.add(p.info['pid'])
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            pass
    return pids


def run():
    import pythoncom
    import win32com.client
    from docfit_core.format_samples import APP_HOME, LEGACY_REGISTRY_VALUE, REGISTRY_VALUE

    print("[1] COM 잠금 획득 중...")
    with com_lock():
        print("  - COM 잠금 획득 완료.")
        before_pids = get_hwp_pids()
        app = None
        created_pids = set()
        with tempfile.TemporaryDirectory(prefix="hwp_clip_test_") as tmpdir:
            sample_hwpx = Path(tmpdir) / "sample_table.hwpx"
            create_sample_table_hwpx(sample_hwpx)
            print(f"[2] 임시 HWPX 생성 완료: {sample_hwpx}")

            pythoncom.CoInitialize()
            try:
                print("[3] 격리된 한/글 COM 인스턴스(DispatchEx) 생성...")
                app = win32com.client.DispatchEx("HwpFrame.HwpObject")
                after_pids = get_hwp_pids()
                created_pids = after_pids - before_pids
                print(f"  - 생성된 Hwp PID: {created_pids}")

                try:
                    app.XHwpWindows.Item(0).Visible = False
                except Exception:
                    pass

                module_name = REGISTRY_VALUE if (APP_HOME / "dll" / "HwpAutoDocFitSecurity.dll").is_file() else LEGACY_REGISTRY_VALUE
                reg = app.RegisterModule("FilePathCheckDLL", module_name)
                print(f"  - 보안 모듈 등록 결과: {reg}")

                print(f"[4] 표 문서 열기: {sample_hwpx}")
                opened = app.Open(str(sample_hwpx), "HWPX", "")
                if opened is False:
                    raise RuntimeError("한/글에서 표 문서를 열지 못했습니다.")

                print("[5] 표 전체 선택 및 복사(Copy) 실행...")
                app.HAction.Run("SelectAll")
                app.HAction.Run("Copy")
                try:
                    pythoncom.OleFlushClipboard()
                except Exception:
                    pass
                time.sleep(0.5)

                print("[6] 클립보드 형식 조사...")
                details = inspect_clipboard()
                print(f"  - 감지된 클립보드 형식 수: {len(details)}")

                output_json = Path(tmpdir) / "clipboard_formats.json"
                output_json.write_text(json.dumps(details, ensure_ascii=False, indent=2), encoding="utf-8")

                print("\n=== 클립보드 등록 형식 목록 ===")
                for item in details:
                    print(f"• ID: {item['id']:<6} | Name: {item['name']:<30} | Size: {item['size']} bytes")

                # 주요 포맷 상세 출력
                for target in ["HTML Format", "Rich Text Format", "Hwp Native", "HWP Table", "CF_UNICODETEXT"]:
                    matched = [d for d in details if target.lower() in d["name"].lower()]
                    if matched:
                        m = matched[0]
                        print(f"\n--- [{m['name']}] 샘플 ---")
                        print(m["sample"][:300])

                return details

            finally:
                if app is not None:
                    try:
                        app.Clear(1)
                        app.Quit()
                    except Exception:
                        pass
                pythoncom.CoUninitialize()

                # 우리 스크립트가 직접 띄운 PID 정리
                if created_pids:
                    print(f"[7] 테스트 인스턴스(PID: {created_pids}) 종료 확인 및 정리...")
                    time.sleep(0.3)
                    remaining = get_hwp_pids() & created_pids
                    for pid in remaining:
                        try:
                            import subprocess
                            subprocess.run(["taskkill", "/F", "/PID", str(pid)], capture_output=True)
                        except Exception:
                            pass
        print("[8] COM 잠금 해제 완료.")


if __name__ == "__main__":
    details = run()
