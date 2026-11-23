import itertools
import unittest

from docfit_core.stage_registry import STAGE_REGISTRY, StagePlanDiff, compare_stage_names, plan_stages
from docfit_core.stage_selection import _MODE_STAGES


class StageRegistryTest(unittest.TestCase):
    def test_stage_name_comparison(self):
        self.assertEqual(compare_stage_names(('a', 'b'), ('a', 'b')).differences, ())
        self.assertTrue(compare_stage_names(('a', 'b'), ('b', 'a')).differences[0].startswith('순서:'))
        self.assertTrue(compare_stage_names(('a', 'b'), ('a',)).differences[0].startswith('누락:'))
        self.assertTrue(compare_stage_names(('a',), ('a', 'b')).differences[0].startswith('추가:'))

    def test_page_range_plan_skips_document_global_xml_stages(self):
        plan = plan_stages('format', {'standard_format': True, 'table_profile': True}, True,
                           {key: True for key in _MODE_STAGES['format']})
        ids = {stage.id for stage in plan}
        self.assertNotIn('xml_table_date', ids)
        self.assertNotIn('xml_table_two_chars', ids)
        self.assertNotIn('xml_table_style', ids)

    def test_registry_ids_dependencies_and_mode_keys(self):
        ids = [item.id for item in STAGE_REGISTRY]
        self.assertEqual(len(ids), len(set(ids)))
        by_id = {item.id: item for item in STAGE_REGISTRY}
        for item in STAGE_REGISTRY:
            self.assertTrue(item.supported_modes)
            self.assertTrue(item.supported_modes <= set(_MODE_STAGES))
            self.assertTrue(set(item.prerequisites) <= set(by_id))
        for mode, keys in _MODE_STAGES.items():
            self.assertTrue(set(keys) <= set(by_id), mode)
        # prerequisite graph is acyclic
        visiting, visited = set(), set()
        def visit(key):
            self.assertNotIn(key, visiting, f"순환 의존: {key}")
            if key in visited:
                return
            visiting.add(key)
            for dep in by_id[key].prerequisites:
                visit(dep)
            visiting.remove(key)
            visited.add(key)
        for key in ids:
            visit(key)

    def test_stage_plan_diff_matches_pipeline_preformat_if_chain(self):
        # The 8 independent pipeline flags produce 256 option combinations.
        flags = ("xml_spacing_reset", "report_header_profile", "report_header_enabled",
                 "titles_enabled", "midtitle_enabled", "attachments_enabled",
                 "box_profile", "page_number")
        differences = []
        checked = 0
        for mode, bits, page_range in itertools.product(_MODE_STAGES, itertools.product((False, True), repeat=8), (False, True)):
            options = dict(zip(flags, bits))
            options.update(connector_tab=bits[0], colon_label=bits[1], table_profile=bits[2],
                           blank_lines=bits[3], emphasis=bits[4],
                           standard_format=bits[5] if mode in ("format", "all") else False)
            selected = {key: True for key in _MODE_STAGES[mode]}
            # Include selected detail-job on/off states in the same exhaustive matrix.
            for key, bit in (("reset_spacing", 0), ("asterisk_superscript", 1),
                             ("attachment_font", 2), ("pre_format", 3),
                             ("standard_format", 4), ("table_style", 5), ("table_width", 6)):
                if key in selected:
                    selected[key] = bits[bit]
            expected = _pipeline_reference(mode, options, page_range, selected)
            actual = tuple(item.name for item in plan_stages(mode, options, page_range, selected)
                           if item.id.startswith("xml_"))
            diff = StagePlanDiff(expected, actual)
            checked += 1
            if diff.differences:
                differences.append((mode, options, page_range, diff.differences))
        self.assertEqual(checked, 4 * 256 * 2)
        self.assertEqual(differences, [], f"검증 {checked}개 중 차이: {differences[:10]!r}")


def _pipeline_reference(mode, options, page_range, selected):
    """Test-only transcription of pipeline.py's 선행목록 append conditions."""
    names = []
    standard = bool(options.get("standard_format", mode in ("format", "all"))) and mode in ("format", "all")
    fmt = standard and not page_range
    chosen = lambda key: selected.get(key, True)
    if mode == "all" and standard and not page_range and options.get("xml_spacing_reset") and chosen("reset_spacing"):
        names.append("자간 초기화")
    if fmt:
        if chosen("asterisk_superscript"):
            names.append("별표 위첨자")
        if chosen("attachment_font"):
            names.append("붙임 글꼴")
        if chosen("pre_format"):
            if options.get("report_header_profile") and options.get("report_header_enabled"):
                names.append("보고서 머리 서식 복사")
            if options.get("titles_enabled", True):
                names.extend(("제목·개요", "개요 표 첫 빈칸"))
            if options.get("midtitle_enabled", True):
                names.append("중제목")
            if options.get("attachments_enabled", True):
                names.append("붙임")
            if options.get("box_profile"):
                names.append("한 칸 상자")
    if fmt and options.get("page_number") and chosen("standard_format"):
        names.append("쪽 번호 모양")
    table_allowed = (fmt or mode == "unify") and chosen("table_style") and options.get("table_profile", True)
    if table_allowed:
        names.append("기본 표 서식")
    if fmt and chosen("table_width"):
        names.append("표 칸 너비")
        if table_allowed:
            names.append("기본 표 서식 정렬 재확인")
    if fmt and options.get("connector_tab", True):
        names.append("연결 부호 탭 정렬")
    if fmt and options.get("colon_label", True):
        names.append("콜론 라벨 정렬")
    if (standard or mode == "unify") and not page_range:
        names.extend(("표 날짜 칸 한 줄", "표 두 글자 칸"))
    if fmt and options.get("blank_lines") and chosen("standard_format"):
        names.append("빈 줄 서식")
    if fmt and options.get("emphasis") and chosen("standard_format"):
        names.append("강조 낱말")
    return tuple(names)


if __name__ == "__main__":
    unittest.main()
