"""문서뷰·사진뷰 공용: 목록 대상 파일을 시스템 연결 프로그램으로 연다(DBLOPEN1).

화면이 보낸 경로를 그대로 믿지 않고, 목록에 나올 수 있는 확장자의 실제 파일만 연다
(실행 파일·바로가기 실행 방지).
"""
from __future__ import annotations

import logging
import os
import subprocess
import sys
from collections.abc import Callable
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


def default_open(path: str) -> None:
    if hasattr(os, "startfile"):
        os.startfile(path)
    elif sys.platform == "darwin":
        subprocess.Popen(["open", path])
    else:
        subprocess.Popen(["xdg-open", path])


def open_listed_file(
    path: str | Path,
    extensions: frozenset[str],
    opener: Callable[[str], Any] | None,
    noun: str,
) -> dict[str, Any]:
    """`extensions`(소문자, 점 포함)에 드는 파일만 연다. `noun`은 안내 문구의 대상(문서·사진)."""
    target = Path(path).resolve()
    if not target.is_file():
        return {
            "ok": False,
            "code": "FILE_NOT_FOUND",
            "message": f"파일을 찾을 수 없습니다: {path}",
        }
    # 이름의 ':'(NTFS 대체 데이터 스트림)은 확장자 검사를 속일 수 있어 거절한다.
    if target.suffix.lower() not in extensions or (os.name == "nt" and ":" in target.name):
        return {
            "ok": False,
            "code": "UNSUPPORTED_TYPE",
            "message": f"{noun} 파일이 아니어서 열지 않았습니다: {target.name}",
        }
    try:
        (opener or default_open)(str(target))
        return {"ok": True, "path": str(target)}
    except Exception as exc:
        logger.exception("파일 열기 실패: %s", target)
        return {
            "ok": False,
            "code": "OPEN_FAILED",
            "message": f"{noun} 파일을 열지 못했습니다: {exc}",
        }
