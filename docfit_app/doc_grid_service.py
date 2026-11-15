"""문서뷰폴더(DocGridService): 폴더 내 문서 목록과 썸네일/글자 카드 제공 서비스 (DVF2).

문서(HWP·HWPX·XLS·XLSX·PPT·PPTX·PDF)를 스캔하여 목록을 반환하고,
기존 타임머신 썸네일 엔진(docfit_core/timemachine/thumbs.py) 및 디스크 캐시를 재사용한다.
HWP는 klib preview 하위 명령으로 미리보기를 추출하며,
미리보기 이미지가 없으면 PrvText 또는 첫 문단으로 160px '글자 카드'를 생성한다.
동시 생성 제한(기본 2개)을 지키며, 클라우드 전용 파일(OneDrive 등)은 내용을 절대 읽지 않는다.
"""
from __future__ import annotations

import base64
import io
import json
import logging
import os
import stat
import subprocess
import sys
import tempfile
import threading
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw, ImageFont

from docfit_core.timemachine import thumbs
from docfit_core.timemachine.rules import cloud_only, excluded_name
from docfit_core.timemachine.store import path_key

logger = logging.getLogger(__name__)

DOC_EXTENSIONS: frozenset[str] = frozenset(
    (".hwp", ".hwpx", ".xls", ".xlsx", ".ppt", ".pptx", ".pdf")
)

MAX_SLOTS: int = 2
SLOT_TIMEOUT: float = 5.0
MAX_PREVIEW_TEXT_LENGTH: int = 300


def find_klib_executable() -> Path | None:
    """klib(Rust) 실행 바이너리를 검색한다."""
    base = Path(getattr(sys, "_MEIPASS", ""))
    if base.is_dir():
        for candidate_name in ("klib.exe", "klib"):
            target = base / "resources" / candidate_name
            if target.is_file():
                return target

    repo = Path(__file__).resolve().parents[1]
    name = "klib.exe" if os.name == "nt" else "klib"
    for candidate in (
        repo / "resources" / name,
        repo / "klib" / "target" / "release" / name,
        repo / "klib" / "target" / "debug" / name,
    ):
        if candidate.is_file():
            return candidate
    return None


def render_text_card(
    text: str,
    *,
    ext: str = "",
    title: str = "",
    size: int = thumbs.SIZE,
) -> bytes:
    """썸네일 이미지가 없는 문서를 위한 160px JPEG 글자 카드(Text Card)를 렌더링한다."""
    image = Image.new("RGB", (size, size), (248, 249, 250))
    draw = ImageDraw.Draw(image)

    # 1. 외곽 테두리 (은은한 회색)
    draw.rectangle([(0, 0), (size - 1, size - 1)], outline=(226, 232, 240), width=1)

    # 2. 상단 배지 색상 결정
    ext_norm = ext.lower().lstrip(".")
    if ext_norm in ("hwp", "hwpx"):
        badge_bg = (37, 99, 235)      # 파랑
    elif ext_norm in ("xls", "xlsx"):
        badge_bg = (22, 163, 74)      # 초록
    elif ext_norm in ("ppt", "pptx"):
        badge_bg = (220, 38, 38)     # 빨강
    elif ext_norm == "pdf":
        badge_bg = (234, 88, 12)     # 주황
    else:
        badge_bg = (100, 116, 139)   # 슬레이트 기본값

    # 배지 박스
    draw.rectangle([(8, 8), (56, 24)], fill=badge_bg)
    font = ImageFont.load_default()

    badge_label = ext_norm.upper()[:6] if ext_norm else "DOC"
    draw.text((12, 10), badge_label, fill=(255, 255, 255), font=font)

    # 파일 제목
    if title:
        short_title = title if len(title) <= 12 else title[:11] + "…"
        draw.text((62, 11), short_title, fill=(71, 85, 105), font=font)

    # 상단 구분선
    draw.line([(8, 28), (size - 8, 28)], fill=(226, 232, 240), width=1)

    # 본문 텍스트 채우기
    clean_text = " ".join(text.split()).strip() if text else "(미리보기 글 없음)"
    if len(clean_text) > MAX_PREVIEW_TEXT_LENGTH:
        clean_text = clean_text[:MAX_PREVIEW_TEXT_LENGTH] + "…"

    margin_x = 10
    start_y = 34
    line_height = 14
    max_lines = (size - start_y - 8) // line_height

    chars_per_line = 18
    lines: list[str] = []
    remaining = clean_text
    while remaining and len(lines) < max_lines:
        if len(remaining) <= chars_per_line:
            lines.append(remaining)
            break
        lines.append(remaining[:chars_per_line])
        remaining = remaining[chars_per_line:]

    for i, line in enumerate(lines):
        y = start_y + i * line_height
        draw.text((margin_x, y), line, fill=(51, 65, 85), font=font)

    output = io.BytesIO()
    image.save(output, format="JPEG", quality=thumbs.QUALITY)
    return output.getvalue()


def extract_first_text(path: Path | str, ext: str) -> str:
    """HWPX, XLSX, PPTX, XLS, PDF 문서에서 첫 문단 또는 대표 텍스트를 추출한다."""
    path_obj = Path(path)
    ext_norm = ext.lower().lstrip(".")

    try:
        if ext_norm == "hwpx":
            from docfit_core.timemachine.extractors_hwpx import extract_hwpx
            records = extract_hwpx(path_obj)
            for r in records:
                t = (r.text or "").strip()
                if t:
                    return t
        elif ext_norm in ("xlsx", "pptx", "xls"):
            from docfit_core.timemachine import extractors_ooxml
            if ext_norm == "xlsx":
                records = extractors_ooxml.extract_xlsx(path_obj)
            elif ext_norm == "pptx":
                records = extractors_ooxml.extract_pptx(path_obj)
            else:
                records = extractors_ooxml.extract_xls(path_obj)
            for r in records:
                t = (r.text or "").strip()
                if t:
                    return t
        elif ext_norm == "pdf":
            from docfit_core.timemachine.extractors_pdf import extract_pdf
            records = extract_pdf(path_obj)
            for r in records:
                t = (r.text or "").strip()
                if t:
                    return t
    except Exception as exc:
        logger.debug("첫 문단 텍스트 추출 실패 (%s): %s", path_obj.name, exc)

    return ""


class DocGridService:
    """문서 목록 스캔 및 썸네일/글자 카드를 제공하는 서비스."""

    def __init__(
        self,
        thumb_dir: Path | str | None = None,
        klib_path: Path | str | None = None,
        max_slots: int = MAX_SLOTS,
    ):
        if thumb_dir is None:
            try:
                from .kagent_client import data_dir
                thumb_dir = data_dir() / "thumbs"
            except Exception:
                thumb_dir = Path.home() / ".hwp_autodocfit" / "thumbs"
        self.cache = thumbs.ThumbnailCache(thumb_dir)
        self._klib_executable_path = Path(klib_path) if klib_path else None
        self._slots = threading.BoundedSemaphore(max_slots)
        self.max_slots = max_slots

    def _get_klib_exe(self) -> Path | None:
        if self._klib_executable_path and self._klib_executable_path.is_file():
            return self._klib_executable_path
        return find_klib_executable()

    def list_documents(
        self,
        folder: str | Path,
        *,
        recursive: bool = False,
    ) -> list[dict[str, Any]]:
        """지정 폴더의 지원 문서(HWP·HWPX·XLS·XLSX·PPT·PPTX·PDF) 목록을 반환한다."""
        folder_path = Path(folder)
        if not folder_path.is_dir():
            raise FileNotFoundError(f"폴더를 찾을 수 없거나 디렉터리가 아닙니다: {folder}")

        results: list[dict[str, Any]] = []

        def scan_dir(target_dir: Path) -> None:
            try:
                with os.scandir(target_dir) as iterator:
                    for entry in iterator:
                        try:
                            # 숨김 파일 및 임시 파일 제외
                            name = entry.name
                            if name.startswith(".") or excluded_name(name):
                                continue

                            if entry.is_dir(follow_symlinks=False):
                                if recursive:
                                    scan_dir(Path(entry.path))
                                continue

                            if not entry.is_file(follow_symlinks=False):
                                continue

                            ext = os.path.splitext(name)[1].lower()
                            if ext not in DOC_EXTENSIONS:
                                continue

                            try:
                                stat_info = entry.stat(follow_symlinks=False)
                            except OSError:
                                continue

                            attrs = getattr(stat_info, "st_file_attributes", 0)
                            is_cloud = cloud_only(attrs)

                            mtime = stat_info.st_mtime
                            mtime_ns = stat_info.st_mtime_ns
                            size = stat_info.st_size

                            key = self.cache.key(entry.path, mtime_ns, size)
                            has_thumb = bool(not is_cloud and self.cache.get(key) is not None)

                            results.append({
                                "path": os.path.abspath(entry.path),
                                "name": name,
                                "stem": Path(name).stem,
                                "ext": ext.lstrip(".").lower(),
                                "size": size,
                                "mtime": mtime,
                                "cloud": is_cloud,
                                "thumb_key": key,
                                "has_thumb": has_thumb,
                            })
                        except OSError:
                            continue
            except OSError:
                pass

        scan_dir(folder_path)
        results.sort(key=lambda item: item["name"].casefold())
        return results

    def _run_klib_preview(self, hwp_path: Path, out_png: Path, timeout: float = 10.0) -> dict[str, Any]:
        """klib preview <hwp> <out.png> 실행 및 JSON 파싱."""
        klib_exe = self._get_klib_exe()
        if not klib_exe or not klib_exe.is_file():
            logger.debug("klib 실행 파일을 찾을 수 없어 klib preview를 건너뜁니다.")
            return {"has_image": False, "format": None, "text": ""}

        try:
            done = subprocess.run(
                [str(klib_exe), "preview", str(hwp_path), str(out_png)],
                capture_output=True,
                timeout=timeout,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
                check=False,
            )
            if done.returncode != 0:
                logger.debug("klib preview 실패 (code %d): %s", done.returncode, done.stderr.decode("utf-8", "ignore"))
                return {"has_image": False, "format": None, "text": ""}
            info = json.loads(done.stdout.decode("utf-8", "ignore"))
            return {
                "has_image": bool(info.get("has_image")),
                "format": info.get("format"),
                "text": str(info.get("text") or ""),
            }
        except Exception as exc:
            logger.debug("klib preview 실행 중 예외: %s", exc)
            return {"has_image": False, "format": None, "text": ""}

    def clear_cache(self) -> int:
        """디스크 썸네일 캐시를 비우고 삭제된 파일 수를 반환한다."""
        return self.cache.clear()

    def get_thumbnail(
        self,
        path: str | Path,
        *,
        timeout: float = SLOT_TIMEOUT,
        as_base64: bool = False,
    ) -> dict[str, Any]:
        """문서 썸네일 또는 글자 카드를 조회하거나 생성하여 반환한다."""
        def format_result(res: dict[str, Any]) -> dict[str, Any]:
            if as_base64 and res.get("data") is not None and isinstance(res["data"], bytes):
                res["data"] = base64.b64encode(res["data"]).decode("ascii")
            return res

        path_obj = Path(path)
        if not path_obj.is_file():
            return format_result({
                "ok": False,
                "type": "error",
                "data": None,
                "key": None,
                "text": None,
                "retry": False,
                "message": "파일을 찾을 수 없습니다.",
            })

        try:
            stat_info = os.lstat(path_obj)
        except OSError as exc:
            return format_result({
                "ok": False,
                "type": "error",
                "data": None,
                "key": None,
                "text": None,
                "retry": False,
                "message": str(exc),
            })

        # 클라우드 전용 파일은 내용을 절대 읽지 않는다
        attrs = getattr(stat_info, "st_file_attributes", 0)
        if cloud_only(attrs):
            return format_result({
                "ok": True,
                "type": "cloud",
                "data": None,
                "key": None,
                "text": None,
                "retry": False,
                "message": "클라우드 전용 파일입니다.",
            })

        ext = path_obj.suffix.lower()
        if ext not in DOC_EXTENSIONS:
            return format_result({
                "ok": False,
                "type": "error",
                "data": None,
                "key": None,
                "text": None,
                "retry": False,
                "message": "지원하지 않는 문서 형식입니다.",
            })

        key = self.cache.key(path_obj, stat_info.st_mtime_ns, stat_info.st_size)

        # 1. 이미 캐시에 존재하는지 확인
        cached_data = self.cache.get(key)
        if cached_data is not None:
            return format_result({
                "ok": True,
                "type": "cached",
                "data": cached_data,
                "key": key,
                "text": None,
                "retry": False,
            })

        # 2. 동시 생성 제한(슬롯 획득)
        acquired = self._slots.acquire(timeout=timeout)
        if not acquired:
            return format_result({
                "ok": False,
                "type": "busy",
                "data": None,
                "key": key,
                "text": None,
                "retry": True,
                "message": "동시 생성 슬롯이 부족합니다. 나중에 다시 시도하세요.",
            })

        try:
            # 3. 썸네일 생성 로직
            # HWP 파일인 경우 klib preview 우선 사용
            if ext == ".hwp":
                with tempfile.NamedTemporaryFile(prefix="hwp-prev-", suffix=".png", delete=False) as tmp_file:
                    tmp_out = Path(tmp_file.name)
                try:
                    res = self._run_klib_preview(path_obj, tmp_out)
                    if res["has_image"] and tmp_out.is_file() and tmp_out.stat().st_size > 0:
                        try:
                            encoded = thumbs._from_image(str(tmp_out))
                            if encoded:
                                self.cache.put(key, encoded)
                                return format_result({
                                    "ok": True,
                                    "type": "image",
                                    "data": encoded,
                                    "key": key,
                                    "text": None,
                                    "retry": False,
                                })
                        except Exception as exc:
                            logger.debug("HWP klib preview 이미지 인코딩 실패: %s", exc)

                    # 이미지가 없거나 실패했으면 PrvText 글자 카드 생성
                    prv_text = res.get("text", "")
                    card_data = render_text_card(
                        prv_text,
                        ext="HWP",
                        title=path_obj.stem,
                    )
                    self.cache.put(key, card_data)
                    return format_result({
                        "ok": True,
                        "type": "text_card",
                        "data": card_data,
                        "key": key,
                        "text": prv_text,
                        "retry": False,
                    })
                finally:
                    if tmp_out.exists():
                        try:
                            tmp_out.unlink()
                        except OSError:
                            pass

            # HWPX, Office, PDF 등 타임머신 썸네일 엔진 재사용 시도
            rendered_jpeg = None
            try:
                rendered_jpeg = thumbs.render(str(path_obj))
            except thumbs.ThumbnailBusy:
                return format_result({
                    "ok": False,
                    "type": "busy",
                    "data": None,
                    "key": key,
                    "text": None,
                    "retry": True,
                    "message": "PDF 렌더러가 사용 중입니다.",
                })
            except Exception as exc:
                logger.debug("thumbs.render 실패 (%s): %s", path_obj.name, exc)

            if rendered_jpeg is not None:
                self.cache.put(key, rendered_jpeg)
                return format_result({
                    "ok": True,
                    "type": "image",
                    "data": rendered_jpeg,
                    "key": key,
                    "text": None,
                    "retry": False,
                })

            # 썸네일 이미지가 없는 경우 첫 문단으로 글자 카드 생성
            first_text = extract_first_text(path_obj, ext)
            card_data = render_text_card(
                first_text,
                ext=ext.lstrip(".").upper(),
                title=path_obj.stem,
            )
            self.cache.put(key, card_data)
            return format_result({
                "ok": True,
                "type": "text_card",
                "data": card_data,
                "key": key,
                "text": first_text,
                "retry": False,
            })
        finally:
            self._slots.release()
