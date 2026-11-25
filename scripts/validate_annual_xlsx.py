"""Validate generated annual XLSX files.

Ensures:
- Valid OpenXML ZIP container
- Contains required 6 sheets: 월별통계, 주차별통계, 요일열지도, 주제타임라인, 주기업무, 폴더이력
- No calculation formulas (<f>)
- Safe XML parsing via defusedxml
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from docfit_core.annual_xlsx import inspect_annual_xlsx  # noqa: E402


EXPECTED_SHEETS = [
    "월별통계",
    "주차별통계",
    "요일열지도",
    "주제타임라인",
    "주기업무",
    "폴더이력",
]


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate annual statistics XLSX files.")
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()

    results = []
    for path in args.files:
        info = inspect_annual_xlsx(path)
        if info["has_formulas"]:
            raise SystemExit(f"{path}: 계산 수식(<f>)이 포함되어 있습니다 (금지 조건 위반)")
        for expected in EXPECTED_SHEETS:
            if expected not in info["sheet_names"]:
                raise SystemExit(f"{path}: 필수 시트가 누락되었습니다: {expected}")
        if info["sheet_count"] != len(EXPECTED_SHEETS):
            raise SystemExit(f"{path}: 시트 수가 일치하지 않습니다 (예상 {len(EXPECTED_SHEETS)}, 실제 {info['sheet_count']})")
        results.append({
            "file": path.name,
            "valid": True,
            "sheet_count": info["sheet_count"],
            "sheet_names": info["sheet_names"],
            "has_formulas": info["has_formulas"],
        })

    print(json.dumps(results, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
