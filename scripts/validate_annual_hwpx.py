"""Validate generated annual HWPX files with the repository's Python inspector.

Ensures:
- Handover report has 2 tables: [3 columns, 15 columns (timeline grid)]
- Performance report has 3 tables: [2 columns (monthly), 3 columns (top topics), 9 columns (weekday heatmap)]
- All tables have total width exactly equal to 48,189 HWPUNIT (170mm)
- Safe parsing via defusedxml
"""
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
    parser.add_argument("--charts", action="store_true", help="확장 bundle 표 검증")
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    results = []
    for path in args.files:
        validation = validate_hwpx(path, structure_only=True, parse_xml=True)
        inspection = inspect_hwpx(path)
        expected_tables = (2 if path.name == "인수인계서.hwpx" else 3) if args.charts else (1 if path.name == "인수인계서.hwpx" else 2)
        if inspection.table_count != expected_tables:
            raise SystemExit(f"{path}: 표 {expected_tables}개 예상, {inspection.table_count}개 확인")
        expected_title = "업무 인수인계서" if path.name == "인수인계서.hwpx" else "업무 실적보고서"
        if expected_title not in inspection.text:
            raise SystemExit(f"{path}: 제목 텍스트가 없습니다: {expected_title}")
        if inspection.image_count:
            raise SystemExit(f"{path}: 불필요한 이미지 {inspection.image_count}개가 있습니다")
        tables = [block.rows or [] for block in inspection.blocks if block.type == "table"]
        expected_columns = ([3, 15] if expected_tables == 2 else [2, 3, 9]) if args.charts else ([3] if expected_tables == 1 else [2, 3])
        actual_columns = [len(rows[0]) if rows else 0 for rows in tables]
        if actual_columns != expected_columns:
            raise SystemExit(f"{path}: 표 열 수 불일치: 예상 {expected_columns}, 실제 {actual_columns}")
        if any(len(rows) < 2 for rows in tables):
            raise SystemExit(f"{path}: 데이터 행이 없는 표가 있습니다")
        if path.name == "실적보고서.hwpx" and len(tables[0]) != 13:
            raise SystemExit(f"{path}: 월별 표는 머리글 포함 13행이어야 합니다")
        with ZipFile(path) as archive:
            root = ET.fromstring(archive.read("Contents/section0.xml"))
            header = ET.fromstring(archive.read("Contents/header.xml"))
        ns = {"hp": "http://www.hancom.co.kr/hwpml/2011/paragraph"}
        hh = {"hh": "http://www.hancom.co.kr/hwpml/2011/head", "hc": "http://www.hancom.co.kr/hwpml/2011/core"}
        fills = {node.get("id"): node.find("hc:fillBrush/hc:winBrush", hh) for node in header.findall(".//hh:borderFill", hh)}
        if args.charts:
            colors = {fills[str(i)].get("faceColor") for i in range(3, 8) if fills.get(str(i)) is not None}
            if len(colors) != 5:
                raise SystemExit(f"{path}: 5색 core 네임스페이스 배경색이 없습니다: {colors}")
            if {p.get("height") for p in header.findall(".//hh:charPr", hh)} != {"1100", "1600"}:
                raise SystemExit(f"{path}: 제목 16pt/본문 11pt 규격 불일치")
        width_sums = []
        for table in root.findall(".//hp:tbl", ns):
            rows = table.findall("hp:tr", ns)
            if len(rows) != int(table.get("rowCnt")):
                raise SystemExit(f"{path}: 표 행 개수 불일치")
            for ri, row in enumerate(rows):
                cells = row.findall("hp:tc", ns)
                if len(cells) != int(table.get("colCnt")):
                    raise SystemExit(f"{path}: 표 열 개수 불일치")
                widths = [int(cell.find("hp:cellSz", ns).attrib["width"]) for cell in cells]
                width_sums.append(sum(widths))
                for ci, cell in enumerate(cells):
                    addr = cell.find("hp:cellAddr", ns)
                    if addr.get("rowAddr") != str(ri) or addr.get("colAddr") != str(ci) or cell.get("borderFillIDRef") not in fills:
                        raise SystemExit(f"{path}: 셀 주소/배경 참조 불일치")
                if any(width <= 0 for width in widths):
                    raise SystemExit(f"{path}: 표 열 너비가 0 이하입니다: {widths}")
        if any(width != 48_189 for width in width_sums):
            raise SystemExit(f"{path}: 표 너비 합계가 본문 170mm와 다릅니다: {width_sums}")
        results.append({
            "file": path.name,
            "valid": True,
            "tables": inspection.table_count,
            "table_columns": actual_columns,
            "paragraphs": inspection.paragraph_count,
            "text_sha256": inspection.text_sha256,
            "warnings": inspection.warnings,
            "archive": validation.get("entry_count"),
        })
    print(json.dumps(results, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
