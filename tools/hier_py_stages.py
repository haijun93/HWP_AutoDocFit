"""테스트 전용 analyze_hierarchy 단계 사본.

프로덕션 함수는 수정하지 않으며, 기준 함수의 블록을 순서대로 실행해
각 단계 경계의 중간값을 반환한다.
"""
from __future__ import annotations
from docfit_core.style_hierarchy import *
from docfit_core.style_hierarchy import _most_common
from docfit_core.marker_scheme import detect_scheme

STAGE_NAMES = (
    "01 문단 순회·marker/role 수집",
    "02 문서 단위 통계(Counter)",
    "03 원문자(circled) 역할 결정",
    "04 표 첫 칸 로마자 등 예외",
    "05 최종 레벨·분류 산출",
    "06 결과 dict 구성",
)

def stage_01(paragraphs):
    paragraphs = list(paragraphs)
    scheme = detect_scheme(item.get("text", "") for item in paragraphs)
    records = []
    for index, item in enumerate(paragraphs):
        marker, marker_role = leading_marker(item.get("text", ""), scheme)
        records.append({**item, "index": index, "marker": marker,
                        "marker_role": marker_role,
                        "indent_hwpunit": int(item.get("left", 0)) + int(item.get("indent", 0))})
    return {"records": records}


def stage_02(records):
    marked = [r for r in records if r["marker_role"]]
    role_indent = {
        role: _most_common(Counter(r["indent_hwpunit"] for r in marked
                                   if r["marker_role"] == role))
        for role in ROLE_ORDER
    }
    role_typography = {
        role: _most_common(Counter((r.get("font"), r.get("size_pt"))
                                   for r in marked if r["marker_role"] == role))
        for role in ROLE_ORDER
    }
    return {"role_indent": role_indent, "role_typography": role_typography,
            "indent_levels": sorted(set(r["indent_hwpunit"] for r in records))}


def _classify(records, role_indent, role_typography):
    for index, record in enumerate(records):
        role = record["marker_role"]
        if CIRCLED_MARKER.fullmatch(record["marker"]):
            candidates = []
            for name in CIRCLED_ROLES:
                value = role_indent[name]
                if value is None:
                    continue
                font, size = role_typography[name]
                distance = abs(record["indent_hwpunit"] - value)
                penalty = (0 if record.get("font") == font else 150)
                penalty += abs((record.get("size_pt") or 0) - (size or 0)) * 100
                candidates.append((distance, distance + penalty,
                                   CIRCLED_ROLES.index(name), name))
            if candidates:
                role = min(candidates)[3]
            else:
                previous = records[index - 1].get("role", "") if index else ""
                role = "본문" if previous == "소제목" else "내용" if previous in ("본문", "내용") else "부연설명" if previous == "부연설명" else "미분류"
        elif not role:
            nearest = []
            for name, value in role_indent.items():
                if value is None:
                    continue
                font, size = role_typography[name]
                distance = abs(record["indent_hwpunit"] - value)
                penalty = (0 if record.get("font") == font else 150)
                penalty += abs((record.get("size_pt") or 0) - (size or 0)) * 100
                nearest.append((distance, distance + penalty, ROLE_ORDER.index(name), name))
            if nearest and min(nearest)[0] <= 100:
                role = min(nearest)[3]
            elif record is records[0] and record.get("size_pt", 0) > 0:
                sizes = [r.get("size_pt", 0) for r in records[1:] if r.get("size_pt", 0) > 0]
                role = "제목" if sizes and record["size_pt"] > max(sizes) else "미분류"
            else:
                role = "미분류"
        record["role"] = role
    return records


def _summarize(records):
    styles = defaultdict(list)
    for record in records:
        styles[(record["role"], record["marker"] or "(없음)")].append(record)
    summary, variants = [], []
    for (role, marker), items in sorted(styles.items(), key=lambda kv:
                                       (ROLE_ORDER.index(kv[0][0]) if kv[0][0] in ROLE_ORDER else 9,
                                        min(item["index"] for item in kv[1]))):
        primary = _most_common(Counter((x.get("font"), x.get("size_pt"),
                                        x["indent_hwpunit"], x.get("left", 0),
                                        x.get("indent", 0)) for x in items))
        spaces = Counter(x.get("prev_spacing") for x in items
                         if x.get("prev_spacing") is not None)
        summary.append({"role": role, "marker": marker, "count": len(items),
                        "font": primary[0], "size_pt": primary[1],
                        "indent_hwpunit": primary[2], "left_hwpunit": primary[3],
                        "first_line_hwpunit": primary[4],
                        "prev_spacing_values": sorted(spaces),
                        "prev_spacing_typical": _most_common(spaces)})
        signatures = Counter((x.get("font"), x.get("size_pt"), x.get("left", 0),
                              x.get("indent", 0), x.get("prev_spacing")) for x in items)
        for (font, size, left, indent, prev), count in signatures.most_common():
            variants.append({"role": role, "marker": marker, "count": count,
                             "kind": "반복" if count > 1 else "비반복", "font": font,
                             "size_pt": size, "left_hwpunit": left,
                             "first_line_hwpunit": indent, "prev_spacing_hwpunit": prev})
    return {"styles": summary, "variants": variants}


def _warnings(records, role_indent):
    warnings = []
    for index, record in enumerate(records):
        role = record["role"]
        if CIRCLED_MARKER.fullmatch(record["marker"]) and all(
                role_indent[name] is None for name in CIRCLED_ROLES):
            warnings.append(f"{index + 1}번째 원문자의 역할은 비교할 서식이 없어 확인이 필요합니다.")
        later = [item["role"] for item in records[index + 1:]]
        if role == "소제목":
            following = next((name for name in later if name != "부연설명"), None)
            if following != "본문":
                warnings.append(f"{index + 1}번째 {display_role('소제목')} 아래에 {display_role('본문')}가 없습니다.")
        if role == "내용" and not any(item["role"] == "본문" for item in records[:index]):
            warnings.append(f"{index + 1}번째 {display_role('내용')}에 앞선 {display_role('본문')}가 없습니다.")
        if role == "부연설명" and (index == 0 or records[index - 1]["role"] not in
                                 ("소제목", "본문", "내용", "부연설명")):
            warnings.append(f"{index + 1}번째 부연설명에 짝이 되는 앞 항목이 없습니다.")
    return warnings


def _analyze_staged(paragraphs, capture=None):
    """Infer roles from marker + indent + font + size; spacing is diagnostic only.

    Each input has text, left/indent in HWP units, font, size_pt and
    optional prev_spacing.  No pagination is inferred from XML alone.
    """
    from docfit_core.marker_scheme import detect_scheme

    paragraphs = list(paragraphs)
    scheme = detect_scheme(item.get("text", "") for item in paragraphs)
    # stage 01: marker/role 수집
    records = []
    for idx, item in enumerate(paragraphs):
        marker, marker_role = leading_marker(item.get("text", ""), scheme)
        indent = int(item.get("left", 0)) + int(item.get("indent", 0))
        records.append({**item, "index": idx, "marker": marker, "marker_role": marker_role,
                        "indent_hwpunit": indent})
    stage_02_values = stage_02(records)
    role_indent = stage_02_values["role_indent"]
    role_typography = stage_02_values["role_typography"]
    indent_levels = stage_02_values["indent_levels"]
    if capture is not None:
        capture["stage_02"] = {"role_indent": role_indent,
                               "role_typography": role_typography,
                               "indent_levels": indent_levels}
    stage_03_input = [{**record, "role": record["marker_role"]} for record in records]
    records = _classify(records, role_indent, role_typography)
    if capture is not None:
        capture["stage_03"] = {"records": [{"index": r["index"], "role": r["role"]}
                                            for r in records
                                            if CIRCLED_MARKER.fullmatch(r["marker"])]}
        capture["stage_04"] = {"records": []}
        capture["stage_05"] = {"role_sequence": [r["role"] for r in records]}
    grouped = _summarize(records)
    summary, variants = grouped["styles"], grouped["variants"]
    warnings = _warnings(records, role_indent)
    if capture is not None:
        capture["stage_06"] = {"styles": summary, "variants": variants,
                               "warnings": warnings,
                               "document_type": document_type([r["role"] for r in records])}
    # stage 06: 최종 결과 dict
    result = {"version": 1, "priority": ["indentation", "font", "size", "marker"],
            "auxiliary": "prev_spacing", "indent_levels_hwpunit": indent_levels,
            "ambiguous_marker_roles": {"원문자": list(CIRCLED_ROLES)},
            "styles": summary,
            "variants": variants,
            "role_sequence": [r["role"] for r in records],
            "document_type": document_type([r["role"] for r in records]),
            "warnings": warnings,
            "page_policy": {"subheading_with_bodies": True,
                            "optional_explanations_after_subheading": True,
                            "body_with_contents": True,
                            "explanation_with_parent": True}}
    return result


def run(paragraphs):
    capture = {}
    result = _analyze_staged(paragraphs, capture)
    return {"stage_01": stage_01(paragraphs), **capture, "result": result}, result

