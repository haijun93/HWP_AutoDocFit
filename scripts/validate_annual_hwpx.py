"""Validate generated annual HWPX files with the repository's Python inspector."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from docfit_core.hwpx import inspect_hwpx, validate_hwpx  # noqa: E402
from defusedxml import ElementTree as ET  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    results = []
    for path in args.files:
        validation = validate_hwpx(path, structure_only=True, parse_xml=True)
        inspection = inspect_hwpx(path)
        expected_tables = 1 if path.name == "인수인계서.hwpx" else 2
        if inspection.table_count != expected_tables:
            raise SystemExit(f"{path}: 표 {expected_tables}개 예상, {inspection.table_count}개 확인")
        expected_title = "업무 인수인계서" if path.name == "인수인계서.hwpx" else "업무 실적보고서"
        if expected_title not in inspection.text:
            raise SystemExit(f"{path}: 제목 텍스트가 없습니다: {expected_title}")
        if inspection.image_count:
            raise SystemExit(f"{path}: 불필요한 이미지 {inspection.image_count}개가 있습니다")
        tables = [block.rows or [] for block in inspection.blocks if block.type == "table"]
        expected_columns = [3] if expected_tables == 1 else [2, 3]
        if [len(rows[0]) if rows else 0 for rows in tables] != expected_columns:
            raise SystemExit(f"{path}: 표 열 수 불일치: {[len(rows[0]) if rows else 0 for rows in tables]}")
        if any(len(rows) < 2 for rows in tables):
            raise SystemExit(f"{path}: 데이터 행이 없는 표가 있습니다")
        if expected_tables == 2 and len(tables[0]) != 13:
            raise SystemExit(f"{path}: 월별 표는 머리글 포함 13행이어야 합니다")
        with ZipFile(path) as archive:
            root = ET.fromstring(archive.read("Contents/section0.xml"))
        ns = {"hp": "http://www.hancom.co.kr/hwpml/2011/paragraph"}
        width_sums = []
        for table in root.findall(".//hp:tbl", ns):
            first_row = table.find("hp:tr", ns)
            widths = [int(cell.find("hp:cellSz", ns).attrib["width"])
                      for cell in first_row.findall("hp:tc", ns)]
            width_sums.append(sum(widths))
            if any(width <= 0 for width in widths):
                raise SystemExit(f"{path}: 표 열 너비가 0 이하입니다: {widths}")
        if any(width != 48_189 for width in width_sums):
            raise SystemExit(f"{path}: 표 너비 합계가 본문 170mm와 다릅니다: {width_sums}")
        results.append({"file": path.name, "valid": True, "tables": inspection.table_count,
                        "paragraphs": inspection.paragraph_count,
                        "text_sha256": inspection.text_sha256,
                        "warnings": inspection.warnings,
                        "archive": validation.get("entry_count")})
    print(json.dumps(results, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
