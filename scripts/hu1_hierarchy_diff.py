#!/usr/bin/env python3
"""HU1: 같은 문서에 세 위계 판정을 돌려 문단별 결과를 비교한다(읽기 전용).

판정
  A  서식 분석용 Python  docfit_core.style_hierarchy.analyze_hierarchy (+ marker_scheme)
  B  Rust 위계 엔진       hierarchy.dll C ABI(compat::analyze_hierarchy) — --dll 로 지정
  C  검색 색인            docfit_core.timemachine.content_search.classify_paragraph_levels
                          (설정 hierarchy_engine 값과 무관하게 Python 경로로 고정)
  C' 검색 색인 + 엔진 켬  C 경로에서 analyze_levels가 B 결과를 돌려줄 때(연결 상태 점검용)

사용 예
  python -X utf8 scripts/hu1_hierarchy_diff.py --dll <hierarchy.dll> --out <폴더> 문서1 문서2 ...

문단 단위는 검색 색인 추출(extract_paragraphs) 결과를 기준으로 하며, 표 칸 문단은
A·B 입력에서 뺀다(서식 분석은 본문 문단만 위계를 매김). HWPX는 서식 분석 추출
(extract_review_paragraphs)의 글자 모양·여백을 본문 문단 순서·글자 일치로 붙인다.
문서 원본은 읽기만 한다.
"""
from __future__ import annotations

import argparse
import csv
import json
import sys
from collections import Counter
from pathlib import Path
from unittest.mock import patch

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))

from docfit_app import hierarchy_bridge  # noqa: E402
from docfit_core.style_hierarchy import analyze_hierarchy  # noqa: E402
from docfit_core.timemachine import content_search  # noqa: E402

# 공통 비교 눈금: 공문서 6단계(1 제목·2 장·3 중제목·4 □·5 ㅇ·6 -) + 부연설명·본문.
A_TO_COMMON = {"제목": "T", "장": "CH", "중제목": "SEC", "소제목": "L4", "본문": "L5",
               "내용": "L6", "부연설명": "NOTE", "미분류": "BODY"}
C_TO_COMMON = {"document_title": "T", "chapter": "CH", "section": "SEC", "topic": "TOPIC",
               "square": "L4", "circle": "L5", "dash": "L6", "asterisk": "NOTE",
               "body": "BODY", "table_cell": "CELL"}


def search_records(path: Path):
    with patch.object(hierarchy_bridge, "analyze_levels", return_value=None):
        return content_search.extract_paragraphs(path)


def review_formats(path: Path):
    if path.suffix.lower() != ".hwpx":
        return []
    from docfit_core.document_review import extract_review_paragraphs
    with patch("docfit_core.document_review.analyze_engine_levels", return_value=None):
        body, _cells = extract_review_paragraphs(path)
    return body


def analysis_input(records, formats):
    """검색 문단(표 칸 제외)에 서식 분석 추출의 모양 값을 글자 일치 순서로 붙인다."""
    rows, cursor = [], 0
    for record in records:
        if record.is_table_cell:
            continue
        item = {"text": record.text, "left": 0, "indent": 0, "font": None, "size_pt": 0}
        for offset in range(cursor, min(cursor + 5, len(formats))):
            if formats[offset]["text"].strip() == record.text.strip():
                found = formats[offset]
                item.update(left=found["left"] or 0, indent=found["indent"] or 0,
                            font=found["font"], size_pt=found["size_pt"] or 0)
                cursor = offset + 1
                break
        rows.append(item)
    return rows


def compare(path: Path, dll: Path | None):
    records = search_records(path)
    rows = analysis_input(records, review_formats(path))
    a_roles = analyze_hierarchy(rows)["role_sequence"] if rows else []
    b_result = hierarchy_bridge.run_library(dll, rows) if (dll and rows) else None
    b_roles = b_result["role_sequence"] if b_result else [None] * len(rows)
    engine_on = None
    if b_result is not None:
        with patch.object(hierarchy_bridge, "analyze_levels", return_value=b_result):
            engine_on = content_search.classify_paragraph_levels(path, records)
    out, body_index = [], 0
    for position, record in enumerate(records):
        a = b = None
        if not record.is_table_cell:
            a, b = a_roles[body_index], b_roles[body_index]
            body_index += 1
        c_prime = engine_on[position].level if engine_on and position < len(engine_on) else "(없음)"
        out.append({"doc": path.name, "no": position + 1, "cell": int(record.is_table_cell),
                    "text": record.text.strip()[:60],
                    "A": a or "", "B": b or "", "C": record.level, "C_engine_on": c_prime,
                    "A_common": A_TO_COMMON.get(a, "") if a else "",
                    "C_common": C_TO_COMMON.get(record.level, record.level)})
    return out, len(records), (len(engine_on) if engine_on is not None else None)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("paths", nargs="+", type=Path)
    parser.add_argument("--dll", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    rows, summary = [], []
    for path in args.paths:
        found, count, engine_count = compare(path, args.dll)
        rows.extend(found)
        body = [r for r in found if not r["cell"]]
        ab = sum(r["A"] != r["B"] for r in body) if args.dll else None
        ac = sum(r["A_common"] != r["C_common"] for r in body)
        pairs = Counter((r["A_common"], r["C_common"]) for r in body if r["A_common"] != r["C_common"])
        summary.append({"doc": path.name, "paragraphs": count, "body": len(body),
                        "A_vs_B": ab, "A_vs_C": ac, "C_engine_on_records": engine_count,
                        "A_dist": dict(Counter(r["A_common"] for r in body)),
                        "C_dist": dict(Counter(r["C_common"] for r in found)),
                        "A_C_pairs": {f"{a}->{c}": n for (a, c), n in pairs.most_common()}})
    with (args.out / "hu1_paragraphs.csv").open("w", encoding="utf-8-sig", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    (args.out / "hu1_summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=1),
                                               encoding="utf-8")
    for item in summary:
        print(json.dumps(item, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
