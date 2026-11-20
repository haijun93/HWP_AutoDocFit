"""문서뷰·사진뷰 썸네일 크기 슬라이더 값 저장(화면별)."""
from __future__ import annotations

from typing import Any

SETTING_KEY = "ui_thumb_sizes"
MIN_SIZE = 100.0
MAX_SIZE = 240.0
DEFAULT_SIZES = {"doc_grid": 190.0, "photo_album": 160.0}


def _clamp(value: Any, default: float) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError):
        return default
    if number != number:  # NaN
        return default
    return min(MAX_SIZE, max(MIN_SIZE, number))


def read_thumb_size(settings: Any | None, view: str) -> float:
    default = DEFAULT_SIZES[view]
    if settings is None:
        return default
    raw = settings.read().get(SETTING_KEY, {})
    if not isinstance(raw, dict):
        return default
    return _clamp(raw.get(view, default), default)


def write_thumb_size(settings: Any | None, view: str, size: Any) -> float:
    if settings is None:
        raise ValueError("settings 저장소가 연결되지 않았습니다.")
    value = _clamp(size, DEFAULT_SIZES[view])

    def change(current: dict[str, Any]) -> None:
        sizes = current.get(SETTING_KEY)
        if not isinstance(sizes, dict):
            sizes = {}
        sizes[view] = value
        current[SETTING_KEY] = sizes

    settings.update(change)
    return value


def handle_thumb_size(settings: Any | None, view: str, action: str, payload: dict[str, Any]) -> dict[str, Any]:
    if action == "thumb_size_get":
        return {"ok": True, "size": read_thumb_size(settings, view)}
    size = payload.get("size")
    if isinstance(size, bool) or not isinstance(size, (int, float)):
        return {"ok": False, "code": "INVALID_SIZE", "message": "크기 값을 확인해 주세요."}
    return {"ok": True, "size": write_thumb_size(settings, view, size)}
