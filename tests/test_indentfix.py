"""INDENTFIX 단위 시험: 붙임·항목기호 내어쓰기 번호+빈칸 뒤 본문 첫 글자 기준 통일.

보고서 서식 적용 시 괄호형(-4200)과 콜론형(-15076 등) 사이의 내어쓰기 차이를 해소하고,
모든 번호 및 항목기호 문단이 번호+빈칸 뒤 본문 첫 글자 기준으로 일관되게 내어쓰기되도록 검증한다.
"""

from pathlib import Path
import tempfile
import unittest

from defusedxml import ElementTree as ET

from docfit_core.xml_hanging_indent import (
    apply_xml_hanging_indent,
    hanging_indent_offset,
    hanging_indent_pair,
    prefix_width_hwpunit,
)
from tests.test_batch_format import fixture, roots, shapes


HEADER = (
    '<?xml version="1.0" encoding="utf-8"?>'
    '<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"'
    ' xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"'
    ' xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">'
    '<hh:fontfaces itemCnt="1"><hh:fontface lang="hangul" fontCnt="1">'
    '<hh:font id="0" face="휴먼명조" type="TTF" isEmbedded="0"/>'
    '</hh:fontface></hh:fontfaces>'
    '<hh:borderFills itemCnt="1"><hh:borderFill id="1"/></hh:borderFills>'
    '<hh:charProperties itemCnt="1">'
    '<hh:charPr id="0" height="1400" textColor="#000000">'
    '<hh:fontRef hangul="0" latin="0" hanja="0" japanese="0" other="0" symbol="0" user="0"/>'
    '<hh:ratio hangul="100" latin="100" hanja="100" japanese="100" other="100" symbol="100" user="100"/>'
    '<hh:spacing hangul="0" latin="0" hanja="0" japanese="0" other="0" symbol="0" user="0"/>'
    '<hh:relSz hangul="100" latin="100" hanja="100" japanese="100" other="100" symbol="100" user="100"/>'
    '<hh:offset hangul="0" latin="0" hanja="0" japanese="0" other="0" symbol="0" user="0"/>'
    '</hh:charPr>'
    '</hh:charProperties>'
    '<hh:tabProperties itemCnt="1"><hh:tabPr id="0"/></hh:tabProperties>'
    '<hh:numberings itemCnt="1"><hh:numbering id="1" start="1"/></hh:numberings>'
    '<hh:paraProperties itemCnt="1">'
    '<hh:paraPr id="0" tabPrIDRef="0">'
    '<hp:switch><hp:case><hh:margin><hc:intent value="0"/><hc:left value="0"/></hh:margin></hp:case></hp:switch>'
    '</hh:paraPr>'
    '</hh:paraProperties>'
    '<hh:styles itemCnt="1"><hh:style id="0" paraPrIDRef="0" charPrIDRef="0"/></hh:styles>'
    '</hh:head>'
).encode("utf-8")


class IndentFixUnitTest(unittest.TestCase):
    def test_paren_and_colon_labels_share_identical_hanging_indent(self):
        """괄호형(-4200)과 콜론형(-15076) 차이 해소: 둘 다 번호+빈칸 뒤 본문 첫 글자(-4200)로 일치."""
        paren_text = "   ❶ (이동도서관) 차량 세부 안내 및 운영 계획입니다."
        colon_text = "   ❶ 모든 주민 대상: 안내 내용 세부 계획입니다."

        off_paren = hanging_indent_offset(paren_text)
        off_colon = hanging_indent_offset(colon_text)

        # 번호 '❶' + 빈칸 뒤 첫 글자 오프셋: 공백 3칸 + '❶' 1자 + 빈칸 1칸 = 5
        self.assertEqual(off_paren, 5)
        self.assertEqual(off_colon, 5)

        pair_paren = hanging_indent_pair(paren_text, size_pt=14.0)
        pair_colon = hanging_indent_pair(colon_text, size_pt=14.0)

        # 14pt 기준: 5글자(반각 글리프 5개 * 1400 * 0.5 = 3500? 아니면 전각 1 + 반각 4 = 1400 + 2800 = 4200)
        self.assertIsNotNone(pair_paren)
        self.assertIsNotNone(pair_colon)
        self.assertEqual(pair_paren, (4200, -4200))
        self.assertEqual(pair_colon, (4200, -4200))
        self.assertEqual(pair_paren, pair_colon)

    def test_various_marker_families_unify_on_body_start_behind_marker_and_space(self):
        """다양한 번호/기호 계층에서 콜론 라벨이 있어도 기호+빈칸 뒤 본문 첫 글자 기준 통일."""
        cases = [
            # (괄호형, 콜론형, 기대 오프셋)
            ("   - (운영방식) 비정기 운행 계획", "   - 운영방식: 비정기 운행 계획", 5),
            ("   ① (사업목적) 자동화 추진", "   ① 사업목적: 자동화 추진", 5),
            ("   1. (추진배경) 설명 내용", "   1. 추진배경: 설명 내용", 6),
            ("   가. (대상기관) 서울시청", "   가. 대상기관: 서울시청", 6),
            ("   * (주요내용) 세부 내역", "   * 주요내용: 세부 내역", 5),
        ]
        for paren, colon, expected_offset in cases:
            with self.subTest(paren=paren, colon=colon):
                off_p = hanging_indent_offset(paren)
                off_c = hanging_indent_offset(colon)
                self.assertEqual(off_p, expected_offset, f"Paren offset mismatch for {paren}")
                self.assertEqual(off_c, expected_offset, f"Colon offset mismatch for {colon}")
                self.assertEqual(
                    hanging_indent_pair(paren, size_pt=14.0),
                    hanging_indent_pair(colon, size_pt=14.0),
                )

    def test_corner_quotes_still_skipped_behind_marker(self):
        """기호 뒤 본문 첫 글자가 낫표(「, 『)인 경우 안쪽 첫 글자로 건너뜀 유지."""
        text = "   - 「공공기록물 관리에 관한 법률」 시행령"
        off = hanging_indent_offset(text)
        self.assertEqual(off, 6)  # '   - ' (5) + '「' (1) = 6 ('공')
        self.assertEqual(text[off], "공")

    def test_colon_immediately_attached_to_marker(self):
        """기호 바로 뒤에 콜론이 붙은 '①: 본문' 형태는 콜론 뒤 본문 첫 글자 기준."""
        text = "①: 인공지능 기반 공공문서"
        off = hanging_indent_offset(text)
        self.assertEqual(off, 3)  # '①: ' 뒤 '인'
        self.assertEqual(text[off], "인")

    def test_inline_item_keeps_inline_marker_origin(self):
        """항목기호 뒤 '라벨: ① 본문' 형태의 인라인 복합 문장은 ① 시작 위치 기준 유지."""
        text = "   • 위치: ① 와우산로 123"
        off = hanging_indent_offset(text)
        self.assertEqual(off, text.index("①"))

    def test_xml_batch_format_generates_identical_margins_for_paren_and_colon(self):
        """XML 일괄 적용 시 괄호형과 콜론형 문단이 동일한 paraPr intent(-4200)를 부여받음."""
        body = (
            '<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0">'
            '<hp:t>   ❶ (이동도서관) 차량 세부 안내 및 운영 계획입니다.</hp:t>'
            '</hp:run></hp:p>'
            '<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0">'
            '<hp:t>   ❶ 모든 주민 대상: 안내 내용 세부 계획입니다.</hp:t>'
            '</hp:run></hp:p>'
        )
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "source.hwpx"
            target = Path(folder) / "target.hwpx"
            fixture(source, body, header=HEADER)

            result = apply_xml_hanging_indent(source, target)
            self.assertGreater(result.applied, 0)

            new_header, root = roots(target)
            para_styles = shapes(new_header, "paraPr")

            paragraphs = list(root)
            self.assertEqual(len(paragraphs), 2)

            style_paren = para_styles[paragraphs[0].get("paraPrIDRef")]
            style_colon = para_styles[paragraphs[1].get("paraPrIDRef")]

            # 두 문단의 마진 값 추출
            def get_margin(para_pr):
                for margin in para_pr.iter("{http://www.hancom.co.kr/hwpml/2011/head}margin"):
                    intent_el = margin.find("{http://www.hancom.co.kr/hwpml/2011/core}intent")
                    left_el = margin.find("{http://www.hancom.co.kr/hwpml/2011/core}left")
                    if intent_el is not None and left_el is not None:
                        return int(left_el.get("value")), int(intent_el.get("value"))
                return None, None

            margin_paren = get_margin(style_paren)
            margin_colon = get_margin(style_colon)

            self.assertEqual(margin_paren, (4200, -4200))
            self.assertEqual(margin_colon, (4200, -4200))
            self.assertEqual(margin_paren, margin_colon)


if __name__ == "__main__":
    unittest.main()
