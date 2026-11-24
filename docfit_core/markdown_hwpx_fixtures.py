"""KORDOCB2 계열 합성 Markdown 11종. 사용자 문서·외부 자원 없음."""
from __future__ import annotations

from .conversion_fixtures import DOC_SPECS, expected_from_blocks, render_markdown


def fixtures() -> dict[str, tuple[str, dict]]:
    cases = {
        name: (render_markdown(blocks), expected_from_blocks(blocks))
        for name, blocks in DOC_SPECS.items()
    }
    blocks = {
        "headings": [("h", 1, "공공 보고서"), ("h", 2, "추진 배경"), ("h", 3, "세부 계획"), ("p", "기본 내용")],
        "public_markers": [("ul", [(0, "추진 방향"), (1, "운영 방식"), (2, "세부 내용"), (0, "참고 사항")])],
        "bullets": [("ul", [(0, "일반 하이픈"), (0, "별표 항목"), (0, "추가 항목")])],
        "ordered": [("ol", [(0, "계획 수립"), (0, "시범 운영"), (0, "전면 시행")])],
        "nested": [("ul", [(0, "상위 가"), (1, "하위 가"), (2, "세부 가"), (1, "하위 나"), (0, "상위 나")])],
        "emphasis": [("h", 1, "강조 시험"), ("p", "일반 굵게 기울임 동시 강조 끝"),
                     ("table", [["항목", "설명"], ["굵은 칸", "기울인 칸"]])],
        "linebreaks": [("h", 1, "줄바꿈 시험"), ("p", "첫째 줄 둘째 줄 셋째 줄 넷째 줄"),
                       ("table", [["항목", "설명"], ["줄 하나 줄 둘", "A B"]])],
        "mixed": [("h", 1, "2026년 업무 보고"), ("p", "한국어 English 1,234 & <태그>"),
                  ("ul", [(0, "접수 120건"), (1, "처리 118건")]),
                  ("table", [["구분", "값"], ["합계", "1,234"], ["부호", "A | B"]]), ("p", "끝.")],
    }
    for name, spec in blocks.items():
        cases[name] = (render_markdown(spec), expected_from_blocks(spec))
    overrides = {
        "public_markers": "□\t  추진 방향\n  ㅇ   운영 방식\n    -\t세부 내용\n※  참고 사항\n",
        "bullets": "- 일반 하이픈\n* 별표 항목\n+ 추가 항목\n",
        "emphasis": "# 강조 시험\n\n일반 **굵게** *기울임* ***동시 강조*** 끝\n\n| 항목 | 설명 |\n| --- | --- |\n| **굵은 칸** | *기울인 칸* |\n",
        "linebreaks": "# 줄바꿈 시험\n\n첫째 줄  \n둘째 줄\\\n셋째 줄<br>넷째 줄\n\n| 항목 | 설명 |\n| --- | --- |\n| 줄 하나<br>줄 둘 | A<br/>B |\n",
        "mixed": "# 2026년 업무 보고\n\n한국어 English 1,234 & <태그>\n\n- 접수 120건\n  - 처리 118건\n\n| 구분 | 값 |\n| --- | --- |\n| 합계 | 1,234 |\n| 부호 | A \\| B |\n\n끝.\n",
    }
    for name, source in overrides.items():
        cases[name] = (source, cases[name][1])
    return cases
