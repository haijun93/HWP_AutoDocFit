from __future__ import annotations

"""테스트 전용 단계식 분석. 프로덕션 analyze_hierarchy는 수정하지 않는다."""
from collections import Counter
from docfit_core.marker_scheme import detect_scheme
from docfit_core.style_hierarchy import (
    CIRCLED_MARKER, CIRCLED_ROLES, ROLE_ORDER, analyze_hierarchy,
    document_type, leading_marker,
)

STAGE_NAMES = (
    "01 문단 순회·marker/role 수집 및 문서 체계 감지",
    "02 문서 단위 Counter 통계·대표 서식",
    "03 원문자(circled) 역할 결정",
    "04 표 첫 칸 로마자 등 입력 외 예외 경로",
    "05 최종 레벨·분류 산출",
    "06 결과 dict 구성",
)


def stage_01(paragraphs):
    records = []
    for index, item in enumerate(paragraphs):
        marker, role = leading_marker(item.get("text", ""))
        records.append({**item, "index": index, "marker": marker,
                        "marker_role": role,
                        "indent_hwpunit": int(item.get("left", 0)) + int(item.get("indent", 0))})
    return {"records": records}


def stage_02(state):
    records = state["records"]
    marked = [r for r in records if r["marker_role"]]
    scheme = detect_scheme(r.get("text", "") for r in records)
    return {"scheme": {"decimal": scheme.decimal, "symbols": scheme.symbols,
                       "excluded": sorted(scheme.excluded), "paragraph_count": scheme.paragraph_count,
                       "family_levels": scheme.family_levels},
            "role_indent": {role: Counter(r["indent_hwpunit"] for r in marked
                                           if r["marker_role"] == role).most_common(1)[0][0]
                            if any(r["marker_role"] == role for r in marked) else None
                            for role in ROLE_ORDER},
            "role_typography": {role: Counter((r.get("font"), r.get("size_pt")) for r in marked
                                                if r["marker_role"] == role).most_common(1)[0][0]
                                 if any(r["marker_role"] == role for r in marked) else (None, None)
                                 for role in ROLE_ORDER},
            "indent_levels": sorted({r["indent_hwpunit"] for r in records})}


def stage_03(state):
    records = state["records"]
    ambiguous = []
    for index, record in enumerate(records):
        if CIRCLED_MARKER.fullmatch(record["marker"]):
            ambiguous.append({"index": index, "marker": record["marker"],
                              "candidates": list(CIRCLED_ROLES)})
    return {"statistics": state["statistics"], "circled": ambiguous}


def stage_04(state):
    # analyze_hierarchy는 paragraph dict만 받으므로 표 셀의 첫 칸은 이 API 입력에 없다.
    return {"circled": state["circled"], "table_first_cell_roman": "not represented by paragraph input"}


def stage_05(paragraphs):
    result = analyze_hierarchy(paragraphs)
    return {"role_sequence": result["role_sequence"],
            "document_type": result["document_type"],
            "warnings": result["warnings"]}


def stage_06(paragraphs, result):
    return analyze_hierarchy(paragraphs)


def run(paragraphs):
    first = stage_01(paragraphs)
    second = {"statistics": stage_02(first)}
    third = stage_03({"records": first["records"], "statistics": second["statistics"]})
    fourth = stage_04(third)
    fifth = stage_05(paragraphs)
    output = stage_06(paragraphs, fifth)
    return {"stage_01": first, "stage_02": second["statistics"],
            "stage_03": third, "stage_04": fourth,
            "stage_05": fifth, "stage_06": output}, output
