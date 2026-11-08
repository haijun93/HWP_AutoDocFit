"""표 칸 안의 단독 그림을 셀 배경 이미지로 변환하는 서비스.

사용자가 고른 HWPX 문서를 사본으로 처리하여 변환하고(원본 불변),
결과 파일명은 기존 규칙('(…)' 접미사)에 따라 '(표배경변환)'을 붙여 저장한다.
"""
from __future__ import annotations

import os
from pathlib import Path

from docfit_app.api_contract import ServiceError
from docfit_core.cell_image_bg import convert_cell_images_to_bg_hwpx

CELL_BG_SUFFIX = "표배경변환"


def make_cell_bg_output_path(source_path: Path | str, suffix_tag: str = CELL_BG_SUFFIX) -> Path:
    """원본 파일 경로 옆에 기존 규칙 '(…)' 접미사를 붙인 사본 파일 경로를 생성한다.

    이미 동일한 이름의 파일이 존재할 경우 '(표배경변환) (2)' 형태로 번호를 부여하여 충돌을 방지한다.
    """
    source = Path(source_path)
    folder = source.parent
    stem = source.stem
    ext = source.suffix

    candidate = folder / f"{stem} ({suffix_tag}){ext}"
    index = 2
    while candidate.exists():
        candidate = folder / f"{stem} ({suffix_tag}) ({index}){ext}"
        index += 1
    return candidate


def convert_cell_bg_service(
    source_path: str | Path,
    output_path: str | Path | None = None,
) -> dict:
    """HWPX 문서 내 표 셀 안 그림을 셀 배경 이미지로 변환하여 사본으로 저장한다.

    - 원본 파일은 결코 수정하지 않는다 (원본 불변).
    - output_path가 생략되면 make_cell_bg_output_path로 자동 결정한다.
    - 변환된 셀 개수(converted_count)와 저장 경로 등을 반환한다.
    """
    if not source_path:
        raise ServiceError("FILE_SELECTION_REQUIRED", "변환할 HWPX 문서를 선택해 주세요.")

    source = Path(source_path).resolve()
    if not source.exists() or not source.is_file():
        raise ServiceError("FILE_NOT_FOUND", f"파일을 찾을 수 없습니다: {source}")

    if source.suffix.lower() != ".hwpx":
        raise ServiceError("INVALID_FORMAT", "표 칸 그림 배경 변환은 .hwpx 파일만 지원합니다.")

    if output_path is not None:
        target = Path(output_path).resolve()
        if target == source:
            raise ServiceError(
                "ORIGINAL_MODIFICATION_PROHIBITED",
                "원본 문서는 변경할 수 없습니다. 사본 경로를 지정해 주세요.",
            )
    else:
        target = make_cell_bg_output_path(source)

    try:
        # docfit_core의 cell_image_bg 변환 함수 호출
        converted_count = convert_cell_images_to_bg_hwpx(source, target)
    except Exception as exc:
        raise ServiceError("CONVERSION_FAILED", f"표 칸 그림 변환 중 오류가 발생했습니다: {exc}") from exc

    return {
        "ok": True,
        "source": str(source),
        "output": str(target),
        "output_name": target.name,
        "converted_count": converted_count,
        "message": (
            f"표 칸 그림 {converted_count}개를 셀 배경으로 변환하여 저장했습니다."
            if converted_count > 0
            else "변환 대상인 표 칸 그림이 없습니다 (단독 그림 칸만 변환 가능)."
        ),
    }
