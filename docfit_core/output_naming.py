"""결과 파일 이름 충돌 방지: 같은 이름이 있으면 덮어쓰지 않고 ' (2)', ' (3)' … 을 붙인다."""
from __future__ import annotations

import os
import re
from pathlib import Path


def available_output_path(path: str | Path) -> str:
    """`path` 가 비어 있으면 그대로, 이미 있으면 번호를 붙인 첫 빈 경로를 돌려준다.

    이전 결과(사용자가 손봤을 수 있음)와, 같은 폴더의 a.hwp·a.hwpx 처럼 결과 이름이 같아지는
    다른 문서의 결과를 지우지 않기 위해서다.
    """
    target = Path(path)
    number = 2
    candidate = target
    while os.path.lexists(candidate):
        candidate = target.with_name(f"{target.stem} ({number}){target.suffix}")
        number += 1
    return str(candidate)


def latest_output_path(path: str | Path) -> str:
    """`path` 와 번호 붙은 결과 중 가장 최근에 쓴 파일. 하나도 없으면 `path`."""
    target = Path(path)
    pattern = re.compile(re.escape(target.stem) + r"( \(\d+\))?" + re.escape(target.suffix) + r"\Z", re.IGNORECASE)
    try:
        found = [item for item in target.parent.iterdir() if pattern.match(item.name) and item.is_file()]
    except OSError:
        found = []
    if not found:
        return str(target)
    return str(max(found, key=lambda item: item.stat().st_mtime_ns))
