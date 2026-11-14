"""붙임 쪽 경계 시험용 익명 합성 HWPX 세 개를 tests/fixtures/에 만든다.

attp_text.hwpx: 문단 글 '[붙임 1]'로 시작하는 붙임 쪽
attp_table.hwpx: 쪽 나눔 문단의 '붙임 1 | | 제목' 3칸 머리 표
attp_shape.hwpx: 글자처럼 취급하는 사각형 안 '붙임 1'
모두 공문 붙임 목록('붙임  계획서 1부.  끝.') 같은 붙임 쪽이 아닌 사례도 함께 담는다.
실행: python scripts/make_attachment_page_fixtures.py
"""
from pathlib import Path
from xml.sax.saxutils import escape
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile

ROOT = Path(__file__).resolve().parents[1] / "tests" / "fixtures"
NS = ('xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" '
      'xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" '
      'xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" '
      'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"')
HEADER = f'<?xml version="1.0" encoding="UTF-8"?><hh:head {NS} version="1.4" secCnt="1"/>'


def run(text):
    return f'<hp:run charPrIDRef="0"><hp:t>{escape(text)}</hp:t></hp:run>'


def para(inner, page_break=False):
    return f'<hp:p paraPrIDRef="0" styleIDRef="0" pageBreak="{int(page_break)}" columnBreak="0" merged="0">{inner}</hp:p>'


def text(value, page_break=False):
    return para(run(value), page_break)


def cell(col, row, value):
    return (f'<hp:tc borderFillIDRef="1"><hp:subList>{text(value)}</hp:subList>'
            f'<hp:cellAddr colAddr="{col}" rowAddr="{row}"/><hp:cellSpan colSpan="1" rowSpan="1"/>'
            f'<hp:cellSz width="5000" height="1500"/></hp:tc>')


def table(rows):
    body = "".join("<hp:tr>" + "".join(cell(c, r, v) for c, v in enumerate(row)) + "</hp:tr>"
                   for r, row in enumerate(rows))
    return (f'<hp:run charPrIDRef="0"><hp:tbl rowCnt="{len(rows)}" colCnt="{len(rows[0])}">'
            f'{body}</hp:tbl></hp:run>')


def rect(value, as_char):
    return (f'<hp:run charPrIDRef="0"><hp:rect id="1" zOrder="0"><hp:drawText name=""><hp:subList>'
            f'{text(value)}</hp:subList></hp:drawText>'
            f'<hp:sz width="6000" height="2000"/><hp:pos treatAsChar="{int(as_char)}" vertRelTo="PARA" horzRelTo="PARA"/>'
            f'</hp:rect></hp:run>')


BODY = [
    text("시험 보고서"),
    text("□ 추진 배경"),
    text(" ㅇ 시험 문장 하나입니다."),
    text("붙임"),
    text("1. 계획서 1부.  끝."),
]

DOCS = {
    # 문단 3~5는 붙임 쪽이 아닌 사례: 공문 붙임 목록('붙임' + '1. … 끝.'), 문장 속 '붙임'
    "attp_text.hwpx": BODY + [
        text("관련 자료는 붙임 참고"),
        text("[붙임 1] 세부 일정", page_break=True),
        text(" ㅇ 일정 문장입니다."),
        text("붙임 2"),
        text(" ㅇ 둘째 붙임 문장입니다."),
    ],
    "attp_table.hwpx": BODY + [
        para(table([["붙임 1", "", "세부 추진 계획"]]), page_break=True),
        text(" ㅇ 계획 문장입니다."),
        para(table([["붙임", "구분"], ["가", "나"], ["다", "라"]])),
        text(" ㅇ 표 아래 문장입니다."),
    ],
    "attp_shape.hwpx": BODY + [
        para(rect("붙임 1", True) + run(" 세부 추진 계획")),
        text(" ㅇ 계획 문장입니다."),
        para(rect("붙임 2", False) + run("떠 있는 도형 문단")),
        text(" ㅇ 마지막 문장입니다."),
    ],
}


def build(name, paragraphs):
    section = f'<?xml version="1.0" encoding="UTF-8"?><hs:sec {NS}>' + "".join(paragraphs) + "</hs:sec>"
    with ZipFile(ROOT / name, "w") as archive:
        archive.writestr("mimetype", "application/hwp+zip", compress_type=ZIP_STORED)
        archive.writestr("Contents/header.xml", HEADER, compress_type=ZIP_DEFLATED)
        archive.writestr("Contents/section0.xml", section, compress_type=ZIP_DEFLATED)


if __name__ == "__main__":
    for file_name, paragraphs in DOCS.items():
        build(file_name, paragraphs)
        print(ROOT / file_name)
