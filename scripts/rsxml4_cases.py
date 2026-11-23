"""RSXML4 합성 HWPX 생성기: 표 칸 너비(table_width)·연결 부호 탭 정렬(connector_tab).
(기본 표 서식·정렬 재확인 사례는 rsxml4_cases_b.py)

`build(h, py)` 의 h 는 scripts/rsxml_parity.py 모듈, py 는 파이썬 처리기 이름공간.
반환: {이름: (HWPX 바이트, [(처리기, 설정 이름)])}. 사용자 문서는 쓰지 않는다.
"""
from __future__ import annotations

import html
import json
import os
import random
import sys
import zipfile
from pathlib import Path

import rsxml3_cases as m3
from rsxml3_cases import C, PP, R, P, T, SEG, mk_header, NSH, ROOT

FUZZ = max(1, int(os.environ.get('RSXML4_FUZZ', '1')))  # 무작위 사례 배수(기본 1)
W, S, A, K = 'table_width', 'table_style', 'table_style_align', 'connector_tab'

CONFIGS = {
    'conn_off': {'connector_tab_enabled': False},
}

STD4 = mk_header([C(1000, 0), C(1000, 1, bold=True), C(1600, 3), C(2000, 2), C(1000, 0, sp=10, ratio=110), C(1000, 4, bold=True, sp=-5)],
                 paras=[PP(), PP(left=1000), PP(intent=-1500, left=2000), PP(switch=True, left=700), PP(align='CENTER')])


def pagepr(width=59528, height=84188, left=8504, right=8504, landscape='WIDELY', margin=True):
    mg = f'<hp:margin header="4252" footer="4252" gutter="0" left="{left}" right="{right}" top="5668" bottom="4252"/>' if margin else ''
    return (f'<hp:p paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:secPr><hp:pagePr landscape="{landscape}" width="{width}" '
            f'height="{height}" gutterType="LEFT_ONLY">{mg}</hp:pagePr></hp:secPr></hp:run></hp:p>')


def cell(text, r, c, w=5000, rs=1, cs=1, cid=0, pp=0, hasm=False, mg=None, raw=None, h=2000, sub_extra='', noaddr=False, nosz=False, bf=1):
    paras = raw if raw is not None else (text if isinstance(text, list) else [text])
    if raw is None:
        body = ''.join(P([R(cid, x)], pp, seg=True) for x in paras)
    else:
        body = raw
    hm = ' hasMargin="1"' if hasm else ''
    cm = f'<hp:cellMargin left="{mg[0]}" right="{mg[1]}" top="141" bottom="141"/>' if mg else ''
    addr = '' if noaddr else f'<hp:cellAddr colAddr="{c}" rowAddr="{r}"/>'
    sz = '' if nosz else f'<hp:cellSz width="{w}" height="{h}"/>'
    return (f'<hp:tc borderFillIDRef="{bf}"{hm}><hp:subList vertAlign="CENTER"{sub_extra}>{body}</hp:subList>{addr}'
            f'<hp:cellSpan colSpan="{cs}" rowSpan="{rs}"/>{sz}{cm}</hp:tc>')


def table(rows, width=None, rowcnt=None, colcnt=None, treat=True, out=(0, 0), inm=(510, 510), caption=None, rel='ABSOLUTE', extra='', nosz=False):
    rc = len(rows) if rowcnt is None else rowcnt
    cc = max(len(r) for r in rows) if colcnt is None else colcnt
    if width is None:
        try:
            width = sum(int(x.split('cellSz width="')[1].split('"')[0]) for x in rows[0] if 'cellSz' in x) if rows and rows[0] else 0
        except ValueError:
            width = 0
    cap = f'<hp:caption side="TOP" fullSz="0" width="8504" gap="850" lastWidth="{width}">{P([R(0, caption)])}</hp:caption>' if caption is not None else ''
    szx = '' if nosz else f'<hp:sz width="{width}" widthRelTo="{rel}" height="3000" heightRelTo="ABSOLUTE" protect="0"/>'
    trs = ''.join('<hp:tr>' + ''.join(r) + '</hp:tr>' for r in rows)
    return (f'<hp:tbl rowCnt="{rc}" colCnt="{cc}" borderFillIDRef="1"{extra}>{szx}<hp:pos treatAsChar="{1 if treat else 0}"/>'
            f'<hp:outMargin left="{out[0]}" right="{out[1]}" top="0" bottom="0"/><hp:inMargin left="{inm[0]}" right="{inm[1]}" top="141" bottom="141"/>'
            f'{cap}{trs}</hp:tbl>')


def host(tables, pp=0, lead=None, cid=0, tail=None):
    """표를 글자처럼 담은 문단(lead 가 있으면 표 앞에 글)."""
    t = tables if isinstance(tables, str) else ''.join(tables)
    pre = f'<hp:t>{html.escape(lead, quote=False)}</hp:t>' if lead else ''
    post = f'<hp:t>{html.escape(tail, quote=False)}</hp:t>' if tail else ''
    return f'<hp:p paraPrIDRef="{pp}" styleIDRef="0"><hp:run charPrIDRef="{cid}">{pre}{t}{post}</hp:run></hp:p>'


def grid(texts, widths, **kw):
    rows = [[cell(x, r, c, w=widths[c]) for c, x in enumerate(row)] for r, row in enumerate(texts)]
    return table(rows, **kw)


def sec(*body, decl='<?xml version="1.0" encoding="UTF-8"?>'):
    return f'{decl}<hs:sec {NSH}>{"".join(body)}</hs:sec>'


def pack(h, body, header=STD4, extra=None):
    if isinstance(body, str):
        body = {'Contents/section0.xml': body}
    return h.hwpx_bytes(body, extra_files=extra, header=header)


# ---------------------------------------------------------------- 표 칸 너비

def width_cases(h, rnd):
    out = {}

    def doc(name, body, header=STD4, cfg=None, extra=None, jobs=None):
        out[name] = (pack(h, body, header, extra), jobs or [(W, cfg)])

    pg = pagepr()
    # 기본: 글자 수 비례
    doc('w_two_col', sec(pg, host(grid([['도시', '마을 여행 이야기 길게 써 봅니다'], ['서울', '남산 둘레길 걷기']], [5000, 5000]))))
    doc('w_three_col', sec(pg, host(grid([['구분', '내용', '비고'], ['가', '긴 내용이 들어가는 칸입니다', '비'], ['나', '짧음', '비고란 긴 글자']], [3000, 3000, 3000]))))
    doc('w_single_col_untouched', sec(pg, host(grid([['하나'], ['둘']], [9000]))))
    doc('w_header_body_avg', sec(pg, host(grid([['항목', '설명'], ['a', 'bbbbbbbbbb'], ['cccccc', 'd'], ['ee', 'ffffffffffffffff']], [4000, 4000]))))
    doc('w_empty_cells_rows', sec(pg, host(grid([['항목', '설명', '비고'], ['가', '내용', ''], ['나', '내용2', '']], [3000, 3000, 3000]))))
    doc('w_all_empty_cells', sec(pg, host(grid([['', ''], ['', '']], [4000, 4000]))))
    doc('w_wider_than_body', sec(pg, host(grid([['가나', '다라마바사아자차카'], ['x', 'y']], [30000, 30000]))))
    doc('w_narrower_than_body', sec(pg, host(grid([['가나', '다라마바사아자차카'], ['x', 'y']], [1000, 1000]))))
    doc('w_long_word_floor', sec(pg, host(grid([['구분', '아주아주아주긴한낱말이들어가는칸입니다만'], ['a', 'b']], [2000, 2000]))))
    doc('w_two_long_words', sec(pg, host(grid([['가나다라마바사아자차카타파하', '가나다라마바사아자차카타파하가나다'], ['x', 'y']], [2000, 2000]))))
    doc('w_halfwidth_digits', sec(pg, host(grid([['2026. 10. 10.', '1234567890123456'], ['a', 'b']], [3000, 3000]))))
    doc('w_floor_cap_tiny_body', sec(pagepr(width=12000, left=1000, right=1000), host(grid([['가나다라마', '바사아자차카'], ['x', 'y']], [2000, 2000]))))
    doc('w_total_too_small', sec(pagepr(width=3000, left=1000, right=1000), host(grid([['가', '나', '다'], ['x', 'y', 'z']], [500, 500, 500]))))
    # 쪽 정보
    doc('w_no_pagepr', sec(host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))))
    doc('w_pagepr_width0', sec(pagepr(width=16000, left=8000, right=8000), host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))))
    doc('w_pagepr_no_margin', sec(pagepr(margin=False), host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))))
    doc('w_landscape', sec(pagepr(landscape='NARROWLY'), host(grid([['가나', '다라마바사'], ['x', 'y']], [3000, 3000]))))
    doc('w_pagepr_garbage_width', sec('<hp:p><hp:run><hp:secPr><hp:pagePr width="abc" height="84188"><hp:margin left="1" right="1"/></hp:pagePr></hp:secPr></hp:run></hp:p>',
                                       host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))))
    doc('w_pagepr_second_ignored', sec(pagepr(), pagepr(width=20000), host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))))
    # 병합
    rows = [[cell('분류', 0, 0, w=3000, cs=2), cell('내용', 0, 2, w=3000)], [cell('가', 1, 0, w=1500), cell('나나', 1, 1, w=1500), cell('다다다다다다', 1, 2, w=3000)]]
    doc('w_colspan_header', sec(pg, host(table(rows))))
    rows = [[cell('구분', 0, 0, w=2000, rs=2), cell('내용', 0, 1, w=4000)], [cell('상세한 내용이 길게 들어간다', 1, 1, w=4000)], [cell('다', 2, 0, w=2000), cell('라', 2, 1, w=4000)]]
    doc('w_rowspan_header', sec(pg, host(table(rows))))
    rows = [[cell('연도', 0, 0, w=2000, rs=2), cell('실적', 0, 1, w=4000, cs=2)], [cell('목표', 1, 1, w=2000), cell('달성률이 높음', 1, 2, w=2000)],
            [cell('2025', 2, 0, w=2000), cell('10', 2, 1, w=2000), cell('9', 2, 2, w=2000)]]
    doc('w_grouped_header_2level', sec(pg, host(table(rows))))
    rows = [[cell('가', 0, 0, w=2000), cell('나', 0, 1, w=2000), cell('다', 0, 2, w=2000)], [cell('합계 항목이 매우 긴 글', 1, 0, w=4000, cs=2), cell('x', 1, 2, w=2000)]]
    doc('w_colspan_body', sec(pg, host(table(rows))))
    rows = [[cell('가', 0, 0, w=2000), cell('나', 0, 1, w=2000)], [cell('세로 병합 칸입니다', 1, 0, w=2000, rs=2), cell('x', 1, 1, w=2000)], [cell('y', 2, 1, w=2000)]]
    doc('w_rowspan_body', sec(pg, host(table(rows))))
    # 장식 열(빈 열 보존)
    rows = [[cell('2026 추진실적', 0, 0, w=3000), cell('', 0, 1, w=2000), cell('(8. 31. 현재)', 0, 2, w=3000)],
            [cell('가나다', 1, 0, w=3000), cell('', 1, 1, w=2000), cell('라마바', 1, 2, w=3000)]]
    doc('w_empty_decor_column', sec(pg, host(table(rows))))
    rows = [[cell('가', 0, 0, w=2000), cell('', 0, 1, w=1000, cs=1), cell('나나나나', 0, 2, w=2000)], [cell('다', 1, 0, w=2000), cell('', 1, 1, w=1000), cell('라', 1, 2, w=2000)],
            [cell('병합 칸 전체', 2, 0, w=5000, cs=3)]]
    doc('w_empty_col_with_fullspan_row', sec(pg, host(table(rows))))
    # 개체·형식 표
    doc('w_nested_table_cell', sec(pg, host(table([[cell('', 0, 0, w=4000, raw=host(grid([['가', '나나']], [2000, 2000]))), cell('옆 칸', 0, 1, w=4000)]]))))
    doc('w_pic_cell', sec(pg, host(table([[cell('', 0, 0, w=4000, raw='<hp:p><hp:run><hp:pic id="1"/></hp:run></hp:p>'), cell('글', 0, 1, w=4000)]]))))
    doc('w_rel_to_para', sec(pg, host(table([[cell('가', 0, 0), cell('나', 0, 1)]], rel='PAPER'))))
    doc('w_missing_sz', sec(pg, host(table([[cell('가', 0, 0), cell('나', 0, 1)]], nosz=True))))
    doc('w_missing_cellsz', sec(pg, host(table([[cell('가', 0, 0, nosz=True), cell('나', 0, 1)]]))))
    doc('w_missing_celladdr', sec(pg, host(table([[cell('가', 0, 0, noaddr=True), cell('나', 0, 1)]]))))
    doc('w_title_table_skipped', sec(pg, host(table([[cell('보고서 제목', 0, 0, w=8000, cs=2)], [cell('2026. 10. 10.', 1, 0, w=4000), cell('담당 과장 홍길동', 1, 1, w=4000)]], rowcnt=2, colcnt=2))))
    doc('w_midtitle_skipped', sec(pg, host(table([[cell('Ⅰ.', 0, 0, w=1000), cell('개요 글', 0, 1, w=7000)]]))))
    doc('w_attach_skipped', sec(pg, host(table([[cell('붙임 1', 0, 0, w=2000), cell('서류', 0, 1, w=6000)]]))))
    doc('w_overview_box_skipped', sec(pg, host(table([[cell('이것은 열다섯 글자가 넘는 개요 상자 글입니다', 0, 0, w=8000)]]))))
    rows = [[cell('접수', 0, 0, w=3000), cell('→', 0, 1, w=1000), cell('심사', 0, 2, w=3000)]]
    doc('w_flow_table_skipped', sec(pg, host(table(rows))))
    rows = [[cell('구분', 0, 0, w=3000, rs=2), cell('', 0, 1, w=3000)], [cell('', 1, 1, w=3000)]]
    doc('w_design_bar_skipped', sec(pg, host(table(rows, rowcnt=2, colcnt=2))))
    # 캡션: 단위 → 비례 축소
    doc('w_unit_caption_scale_down', sec(pg, host(grid([['가', '나'], ['x', 'y']], [30000, 30000], caption='(단위: 개소)'))))
    cap_rows = [[cell('가', 0, 0, w=30000), cell('나', 0, 1, w=30000)], [cell('x', 1, 0, w=30000), cell('y', 1, 1, w=30000)]]
    doc('w_unit_caption_wide', sec(pg, host(table(cap_rows, caption='(단위: 개소)'))))
    cap_rows = [[cell('가', 0, 0, w=20000, cs=2), cell('나', 0, 2, w=20000)], [cell('x', 1, 0, w=10000), cell('y', 1, 1, w=10000), cell('z', 1, 2, w=20000)]]
    doc('w_unit_caption_merged_edges', sec(pg, host(table(cap_rows, caption='(단위: 원)'))))
    cap_rows = [[cell('가', 0, 0, w=2000), cell('나', 0, 1, w=2000)]]
    doc('w_unit_caption_narrow_untouched', sec(pg, host(table(cap_rows, caption='(단위: 원)'))))
    cap_rows = [[cell('가', 0, 0, w=30000, cs=2), cell('나', 0, 2, w=30000)], [cell('x', 1, 0, w=20000), cell('y', 1, 1, w=30000), cell('z', 1, 2, w=10000)]]
    doc('w_unit_caption_inconsistent_edges', sec(pg, host(table(cap_rows, caption='(단위: 원)'))))
    doc('w_normal_caption', sec(pg, host(table([[cell('가', 0, 0, w=3000), cell('나나나나나', 0, 1, w=3000)]], caption='<표 1> 추진 현황'))))
    # 문단 앞자리
    doc('w_host_lead_text', sec(pg, host(grid([['가나', '다라마바사'], ['x', 'y']], [3000, 3000]), lead='   ')))
    doc('w_host_lead_text_big_char', sec(pg, host(grid([['가나', '다라마바사'], ['x', 'y']], [3000, 3000]), lead='앞 글자들', cid=3)))
    doc('w_host_para_margins', sec(pg, host(grid([['가나', '다라마바사'], ['x', 'y']], [3000, 3000]), pp=2)))
    doc('w_host_para_switch', sec(pg, host(grid([['가나', '다라마바사'], ['x', 'y']], [3000, 3000]), pp=3)))
    doc('w_host_missing_para', sec(pg, host(grid([['가나', '다라마바사'], ['x', 'y']], [3000, 3000]), pp=99)))
    doc('w_host_missing_char', sec(pg, host(grid([['가나', '다라마바사'], ['x', 'y']], [3000, 3000]), cid=99, lead='글')))
    doc('w_not_treat_as_char', sec(pg, host(table([[cell('가나', 0, 0, w=3000), cell('다라마바사', 0, 1, w=3000)]], treat=False), lead='  ')))
    doc('w_outmargin', sec(pg, host(table([[cell('가나', 0, 0, w=3000), cell('다라마바사', 0, 1, w=3000)]], out=(500, 700)))))
    doc('w_host_tail_text_after_table', sec(pg, host(grid([['가나', '다라마바사'], ['x', 'y']], [3000, 3000]), lead='앞', tail='뒤')))
    # 글자 크기·굵기·자간
    for i, cid in enumerate([1, 2, 3, 4, 5]):
        rows = [[cell('가나다', 0, 0, w=3000, cid=cid), cell('아주긴낱말이들어감', 0, 1, w=3000, cid=cid)], [cell('x', 1, 0, w=3000, cid=cid), cell('y', 1, 1, w=3000, cid=cid)]]
        doc(f'w_char_variant_{i}', sec(pg, host(table(rows))))
    rows = [[cell('가나다', 0, 0, w=3000, hasm=True, mg=(1000, 1000)), cell('아주긴낱말이들어감', 0, 1, w=3000)], [cell('x', 1, 0, w=3000), cell('y', 1, 1, w=3000)]]
    doc('w_cell_margin', sec(pg, host(table(rows, inm=(100, 300)))))
    rows = [[cell('가나다', 0, 0, w=3000, hasm=True), cell('아주긴낱말이들어감', 0, 1, w=3000)], [cell('x', 1, 0, w=3000), cell('y', 1, 1, w=3000)]]
    doc('w_hasmargin_without_cellmargin', sec(pg, host(table(rows))))
    rows = [[cell(['가나다', '아주긴낱말'], 0, 0, w=3000), cell(['x', 'yyyyyyyyyyyyyyyyyyyy'], 0, 1, w=3000)]]
    doc('w_multi_paragraph_cells', sec(pg, host(table(rows))))
    rows = [[cell('', 0, 0, w=3000, raw=P([R(0, '가나') + '<hp:t>다<hp:fwSpace/>라</hp:t>', ])), cell('b', 0, 1, w=3000)]]
    doc('w_inline_space_in_t', sec(pg, host(table(rows))))
    rows = [[cell('', 0, 0, w=3000, raw=P([R(0, '가') + '<hp:run charPrIDRef="0"><hp:t>나<hp:tab/>다<hp:lineBreak/>라</hp:t></hp:run>'])), cell('b', 0, 1, w=3000)]]
    doc('w_tab_linebreak_in_t', sec(pg, host(table(rows))))
    rows = [[cell('', 0, 0, w=3000, raw='<hp:p><hp:run charPrIDRef="0"><hp:t>가나</hp:t></hp:run><hp:linesegarray/></hp:p>'), cell('다라마', 0, 1, w=3000)]]
    doc('w_lineseg_removed', sec(pg, host(table(rows))))
    doc('w_nbsp_ideographic_strip', sec(pg, host(grid([[' 가나　', '다라'], ['x', 'y']], [3000, 3000]))))
    doc('w_text_with_newline', sec(pg, host(grid([['가\n나', '다라마바'], ['x', 'y']], [3000, 3000]))))
    # 다중 표·구역
    doc('w_two_tables', sec(pg, host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000])), host(grid([['aaaaaaaa', 'b'], ['x', 'y']], [3000, 3000]))))
    doc('w_tables_in_one_paragraph', sec(pg, host([grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]), grid([['aaaaaaaa', 'b'], ['x', 'y']], [3000, 3000])])))
    out['w_two_sections'] = (h.hwpx_bytes({'Contents/section0.xml': sec(pagepr(), host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))),
                                           'Contents/section1.xml': sec(pagepr(width=42000, left=5000, right=5000), host(grid([['aa', 'bbbbbbb'], ['x', 'y']], [3000, 3000])))},
                                          header=STD4), [(W, None)])
    out['w_sections_order_10_2'] = (h.hwpx_bytes({'Contents/section10.xml': sec(pagepr(), host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))),
                                                  'Contents/section2.xml': sec(pagepr(), host(grid([['aa', 'bbbbbbb'], ['x', 'y']], [3000, 3000])))}, header=STD4), [(W, None)])
    out['w_second_section_no_page'] = (h.hwpx_bytes({'Contents/section0.xml': sec(pagepr(), host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))),
                                                     'Contents/section1.xml': sec(host(grid([['aa', 'bbbbbbb'], ['x', 'y']], [3000, 3000])))}, header=STD4), [(W, None)])
    doc('w_no_header_char_group', sec(pg, host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))), header=mk_header([C()], with_char_group=False))
    doc('w_no_tables', sec(pg, T('본문만 있다')))
    doc('w_colcnt_mismatch', sec(pg, host(table([[cell('가', 0, 0, w=3000), cell('나나', 0, 1, w=3000)]], colcnt=3))))
    doc('w_rowcnt_small', sec(pg, host(table([[cell('가', 0, 0, w=3000), cell('나나', 0, 1, w=3000)], [cell('다', 1, 0, w=3000), cell('라', 1, 1, w=3000)]], rowcnt=1))))
    doc('w_chained_after_style', sec(pg, host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000]))), jobs=[(W, None), (K, None)])
    # 반올림 규칙: 열 너비가 정확히 x.5 가 되는 사례(파이썬 round 는 짝수 쪽, 반올림이 '위로'가 아니다)
    for k, (bw, ncol, texts) in enumerate([(59529, 2, ['가나다라', '마바사아']), (59531, 2, ['가나다라', '마바사아']), (59528 + 1, 2, ['abcd', 'efgh']),
                                          (59530, 6, ['가나다라'] * 6), (59527, 4, ['가나다라'] * 4), (59533, 2, ['가나다라마', '바사아자차']),
                                          (59535, 6, ['가나다라'] * 6), (60001, 2, ['가나다라', '마바사아'])]):
        row = [cell(texts[c], 0, c, w=3000) for c in range(ncol)]
        row2 = [cell(texts[c], 1, c, w=3000) for c in range(ncol)]
        doc(f'w_tie_round_{k}', sec(pagepr(width=bw), host(table([row, row2]))))
    doc('w_tie_round_outmargin_odd', sec(pagepr(width=59528), host(table([[cell('가나다라', 0, 0, w=3000), cell('마바사아', 0, 1, w=3000)]], out=(1, 0)))))
    doc('w_tie_round_reserve', sec(pagepr(width=59528), host(table([[cell('가나다라', 0, 0, w=3000), cell('마바사아', 0, 1, w=3000)]]), lead='a')))
    # 무작위
    for i in range(14 * FUZZ):
        cols = rnd.randint(2, 5)
        nrows = rnd.randint(1, 5)
        words = ['가', '가나', '가나다', '구분 항목', 'abc', 'abcdef ghi', '2026. 10. 10.', '아주긴낱말이들어가는칸', '', '사업 추진 현황 및 계획', '10', '100,000', '가 나 다 라 마']
        rows = []
        for r in range(nrows):
            row = []
            for c in range(cols):
                row.append(cell(rnd.choice(words), r, c, w=rnd.choice([1500, 2500, 4000, 7000]), cid=rnd.randrange(6)))
            rows.append(row)
        doc(f'w_random_{i:02d}', sec(pagepr(width=rnd.choice([59528, 42000, 30000]), left=rnd.choice([8504, 5000, 3000]), right=rnd.choice([8504, 5000])),
                                     host(table(rows, inm=(rnd.choice([0, 141, 510]), rnd.choice([0, 141, 510]))), pp=rnd.randrange(5), lead=rnd.choice([None, ' ', '앞글']))))
    # 병합 무작위
    for i in range(8 * FUZZ):
        cols = rnd.randint(3, 5)
        rows = [[cell(rnd.choice(['구분', '내용', 'abc']), 0, c, w=2000) for c in range(cols)]]
        for r in range(1, rnd.randint(2, 4)):
            row, c = [], 0
            while c < cols:
                cs = rnd.choice([1, 1, 2]) if c + 2 <= cols else 1
                row.append(cell(rnd.choice(['가나다', '아주긴 낱말이 들어갑니다', 'x', '']), r, c, w=2000 * cs, cs=cs))
                c += cs
            rows.append(row)
        doc(f'w_random_merge_{i:02d}', sec(pagepr(), host(table(rows))))
    return out


# ---------------------------------------------------------------- 연결 부호 탭

def conn_cases(h, rnd):
    out = {}

    def doc(name, body, header=STD4, cfg=None, jobs=None, extra=None):
        out[name] = (pack(h, body, header, extra), jobs or [(K, cfg)])

    pg = pagepr()
    toc = [T('제1장 총칙 ........ 3'), T('제2장 목적과 범위 ........ 12'), T('제3장 부칙 ........ 45')]
    doc('k_toc_dots', sec(pg, *toc))
    doc('k_toc_middle_dots', sec(pg, T('가 ······ 1'), T('나나 ······ 22')))
    doc('k_toc_ellipsis', sec(pg, T('가 …… 1'), T('나나나 …… 22')))
    doc('k_toc_two_dot_leaders', sec(pg, T('가 ‥‥‥ 1'), T('나나나 ‥‥‥ 22')))
    doc('k_dash_connectors', sec(pg, T('일정 ----- 가'), T('장소 ----- 나나')))
    doc('k_emdash_connectors', sec(pg, T('일정 ——— 가'), T('장소 ——— 나나')))
    doc('k_space_connectors', sec(pg, T('가나다    라마바'), T('아자      차카타')))
    doc('k_space_connector_label_colon', sec(pg, T('일    시: 2026. 10. 7.'), T('장    소: 구청')))
    doc('k_space_connector_dept_tag', sec(pg, T('제목이 길다      (총무과)'), T('다른 제목      (인사과)')))
    doc('k_space_connector_left_colon', sec(pg, T('주요행사: 가나다      시상'), T('주요일정: 라마바      개막')))
    doc('k_space_connector_double_gap', sec(pg, T('일    시      2026. 10. 7.'), T('장    소      구청 대강당')))
    doc('k_ideographic_space_connector', sec(pg, T('가나다　　라마'), T('바사　　　아자')))
    doc('k_ideographic_single_not', sec(pg, T('가나다　라마'), T('바사　아자')))
    doc('k_two_spaces_not_connector', sec(pg, T('가나다  라마'), T('바사  아자')))
    doc('k_single_paragraph_no_group', sec(pg, T('제1장 총칙 ........ 3')))
    doc('k_group_broken_by_plain', sec(pg, T('가 ..... 1'), T('그냥 글'), T('나 ..... 2'), T('다 ..... 3')))
    doc('k_kind_changes_split', sec(pg, T('가 ..... 1'), T('나 ..... 2'), T('다 ----- 3'), T('라 ----- 4'), T('마    5바')))
    doc('k_kind_alternating_no_group', sec(pg, T('가 ..... 1'), T('나 ----- 2'), T('다 ..... 3')))
    doc('k_two_connectors_in_text', sec(pg, T('가 ..... 1 ..... 2'), T('나 ..... 3 ..... 4')))
    doc('k_empty_side_not', sec(pg, T('..... 1'), T('가 ..... ')))
    doc('k_leading_connector_only', sec(pg, T('...... 가'), T('...... 나')))
    doc('k_dots_two_not_enough', sec(pg, T('가 .. 1'), T('나 .. 2')))
    doc('k_mixed_connector_in_one_para_ws', sec(pg, T('가 ... 나   다'), T('라 ... 마   바')))
    doc('k_para_with_two_runs_not', sec(pg, P([R(0, '가 ..... '), R(0, '1')]), P([R(0, '나 ..... '), R(0, '2')])))
    doc('k_t_with_child_not', sec(pg, P(['<hp:run charPrIDRef="0"><hp:t>가 ..... <hp:fwSpace/>1</hp:t></hp:run>']), T('나 ..... 2')))
    doc('k_empty_t_siblings', sec(pg, P(['<hp:run charPrIDRef="0"><hp:t/><hp:t>가 ..... 1</hp:t></hp:run>']), T('나 ..... 2')))
    doc('k_para_with_table_breaks', sec(pg, T('가 ..... 1'), '<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>나 ..... 2</hp:t><hp:tbl rowCnt="1" colCnt="1"/></hp:run></hp:p>', T('다 ..... 3'), T('라 ..... 4')))
    doc('k_char_sizes_mix', sec(pg, T('가 ..... 1', cid=3), T('나나 ..... 2', cid=0), T('다 ..... 3', cid=2)))
    doc('k_char_missing', sec(pg, T('가 ..... 1', cid=99), T('나 ..... 2', cid=98)))
    doc('k_char_attr_missing', sec(pg, P([R(None, '가 ..... 1')]), P([R(None, '나 ..... 2')])))
    doc('k_paraPr_left_margin', sec(pg, T('가나다라마바사아자차 ..... 1', pp=1), T('나 ..... 2', pp=1)))
    doc('k_paraPr_intent', sec(pg, T('가나다 ..... 1', pp=2), T('나 ..... 2', pp=2)))
    doc('k_paraPr_switch', sec(pg, T('가나다 ..... 1', pp=3), T('나 ..... 2', pp=3)))
    doc('k_paraPr_missing_id_error', sec(pg, T('가나다 ..... 1', pp=99), T('나 ..... 2', pp=99)))
    doc('k_paraPr_attr_missing_error', sec(pg, P([R(0, '가 ..... 1')], pp=None), P([R(0, '나 ..... 2')], pp=None)))
    doc('k_pull_forward_narrow', sec(pagepr(width=15000, left=3000, right=3000), T('가나다라마바사아자차 ..... 12345678901234567890'), T('나 ..... 가나다라마바사아자차카타파하')))
    doc('k_pull_forward_cell', sec(pg, host(table([[cell('', 0, 0, w=6000, raw=T('가나다 ..... 긴 오른쪽 글자들이 많이 이어집니다') + T('나 ..... 오른쪽'))]]))))
    doc('k_cell_with_margin', sec(pg, host(table([[cell('', 0, 0, w=9000, mg=(700, 900), raw=T('가나다 ..... 오른쪽') + T('나 ..... 오른쪽 글'))]]))))
    doc('k_cell_no_margin_elem', sec(pg, host(table([[cell('', 0, 0, w=9000, raw=T('가나다 ..... 오른쪽') + T('나 ..... 오른쪽 글'))]]))))
    doc('k_cell_missing_cellsz', sec(pg, host(table([[cell('', 0, 0, nosz=True, raw=T('가나다 ..... 오른쪽') + T('나 ..... 오른쪽 글'))]]))))
    doc('k_cell_zero_width', sec(pg, host(table([[cell('', 0, 0, w=0, raw=T('가나다 ..... 오른쪽') + T('나 ..... 오른쪽 글'))]]))))
    doc('k_cell_width_garbage', sec(pg, host(table([[cell('', 0, 0, w='abc', raw=T('가나다 ..... 오른쪽') + T('나 ..... 오른쪽 글'))]]))))
    doc('k_sublist_not_in_cell', sec(pg, '<hp:p><hp:run><hp:rect><hp:drawText><hp:subList>' + T('가 ..... 1') + T('나 ..... 2') + '</hp:subList></hp:drawText></hp:rect></hp:run></hp:p>'))
    doc('k_nested_cells', sec(pg, host(table([[cell('', 0, 0, w=12000, raw=T('가 ..... 1') + T('나 ..... 2') + host(table([[cell('', 0, 0, w=5000, raw=T('다다 ..... 3') + T('라 ..... 4'))]])))]]))))
    doc('k_groups_outer_and_inner', sec(pg, T('가 ..... 1'), T('나 ..... 2'), host(table([[cell('', 0, 0, w=7000, raw=T('다 ----- 3') + T('라 ----- 4'))]]))))
    out['k_two_sections'] = (h.hwpx_bytes({'Contents/section0.xml': sec(pagepr(), T('가 ..... 1'), T('나 ..... 2')),
                                           'Contents/section1.xml': sec(pagepr(width=30000, left=2000, right=2000), T('다다다다다다다 ..... 3'), T('라 ..... 4'))}, header=STD4), [(K, None)])
    out['k_sections_order_10_2'] = (h.hwpx_bytes({'Contents/section10.xml': sec(pagepr(), T('가 ..... 1'), T('나 ..... 2')),
                                                  'Contents/section2.xml': sec(pagepr(), T('다다다 ..... 3'), T('라 ..... 4'))}, header=STD4), [(K, None)])
    doc('k_no_pagepr_default_width', sec(T('가 ..... 1'), T('나나나나나나나나나나나나나나나나나나나나나 ..... 가나다라마바사아자차카타파하가나다라마바')))
    doc('k_pagepr_zero_width', sec(pagepr(width=16000, left=8000, right=8000), T('가 ..... 1'), T('나 ..... 2')))
    doc('k_pagepr_negative_width', sec(pagepr(width=10000, left=8000, right=8000), T('가 ..... 1'), T('나 ..... 2')))
    doc('k_pagepr_no_margin', sec(pagepr(margin=False), T('가 ..... 1'), T('나 ..... 2')))
    doc('k_landscape', sec(pagepr(landscape='NARROWLY'), T('가 ..... 1'), T('나 ..... 2')))
    doc('k_disabled_flag', sec(pg, *toc), cfg='conn_off')
    doc('k_header_no_tabprops', sec(pg, *toc), header=STD4.replace('<hh:tabProperties', '<hh:tabPropertiesX').replace('</hh:tabProperties>', '</hh:tabPropertiesX>'))
    doc('k_header_no_paraprops', sec(pg, *toc), header=STD4.replace('paraProperties', 'paraPropertiesX'))
    doc('k_header_tabprops_ids_nondigit', sec(pg, *toc), header=STD4.replace('<hh:tabPr id="1"', '<hh:tabPr id="x"'))
    doc('k_header_tabprops_empty', sec(pg, *toc), header=STD4.replace('<hh:tabPr id="0" autoTabLeft="0" autoTabRight="0"/><hh:tabPr id="1" autoTabLeft="1" autoTabRight="0"/>', ''))
    doc('k_header_pretty_printed', sec(pg, *toc), header=STD4.replace('><', '>\n  <'))
    doc('k_same_pos_shares_tab', sec(pg, T('가 ..... 1'), T('가 ..... 2'), T('---'), T('나 ----- 1'), T('나 ----- 2')))
    doc('k_para_tabs_reuse_para', sec(pg, T('가나 ..... 1', pp=1), T('가나 ..... 2', pp=1), T('가나 ..... 3', pp=0)))
    doc('k_astral_text', sec(pg, T('𠀀𠀁 ..... 1'), T('나 ..... 𠀂')))
    doc('k_halfwidth_text', sec(pg, T('abc def ..... 1.5'), T('xx ..... 22.25')))
    doc('k_punct_widths', sec(pg, T('(가).,:; ..... 1'), T('나 ..... 2')))
    doc('k_lineseg_removed', sec(pg, T('가 ..... 1', seg=True), T('나 ..... 2', seg=False)))
    doc('k_entities_in_text', sec(pg, T('가 &amp; 나 ..... &lt;1&gt;'), T('다 ..... 2')))
    doc('k_t_tail_and_text_ok', sec(pg, '<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>가 ..... 1</hp:t></hp:run><hp:run charPrIDRef="0"/></hp:p>', T('나 ..... 2')))
    doc('k_non_p_between_paragraphs', sec(pg, T('가 ..... 1'), '<hp:ctrl/>', T('나 ..... 2')))
    doc('k_chained_after_width', sec(pg, host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000])), *toc))
    for i in range(14 * FUZZ):
        n = rnd.randint(2, 7)
        paras = []
        for _ in range(n):
            left = rnd.choice(['가', '가나다', '제1장 총칙', 'abc def', '사업 개요 및 추진 배경', '(주)가나', '일    시'])
            conn = rnd.choice([' ..... ', ' ······ ', ' ----- ', '     ', ' …… ', '　　'])
            right = rnd.choice(['1', '22', '2026. 10. 7.', '가나다라마바사아자차카타파하', 'abc', '구청 대강당 3층'])
            paras.append(T(left + conn + right, cid=rnd.randrange(6), pp=rnd.randrange(4)))
            if rnd.random() < 0.15:
                paras.append(T('중간 글'))
        doc(f'k_random_{i:02d}', sec(pagepr(width=rnd.choice([59528, 30000, 20000]), left=rnd.choice([8504, 3000, 1000]), right=rnd.choice([8504, 3000])), *paras))
    for i in range(6 * FUZZ):
        paras = [T(rnd.choice(['가나', '가나다라', '제2장']) + ' ..... ' + rnd.choice(['1', '가나다라마바사아자차카타파하가나다라마바사']), cid=rnd.randrange(6)) for _ in range(rnd.randint(2, 4))]
        cw = rnd.choice([3000, 6000, 12000])
        doc(f'k_random_cell_{i:02d}', sec(pagepr(), host(table([[cell('', 0, 0, w=cw, mg=rnd.choice([None, (141, 141), (700, 900)]), raw=''.join(paras))]]))))
    return out


def build(h, py):
    rnd = random.Random(20261123)
    out = {}
    out.update(width_cases(h, rnd))
    out.update(conn_cases(h, rnd))
    import rsxml4_cases_b
    out.update(rsxml4_cases_b.build(h, py, sys.modules[__name__]))
    out.update(rsxml4_cases_b.chain_cases(h, py, out))
    return out


def unsupported_cases(h):
    """(이름, 처리기, 설정, 문서 바이트). 'proc_' 로 시작하면 처리 모드에서만 거부(조사는 성공)."""
    pg = pagepr()
    doc = pack(h, sec(pg, host(grid([['가나', '다라마바'], ['x', 'y']], [3000, 3000])), T('가 ..... 1'), T('나 ..... 2')))
    out = [
        ('w_unknown_key', W, {'새_설정': 1}, doc),
        ('k_unknown_key', K, {'x': True}, doc),
        ('s_unknown_key', S, {'x': True}, doc),
        ('sa_unknown_key', A, {'x': True}, doc),
        ('w_owner_text', W, {'title_owner_text': '담당관'}, doc),
        ('s_owner_text', S, {'title_owner_text': '담당관', 'table_style': None}, doc),
        ('k_enabled_wrong_type', K, {'connector_tab_enabled': 1}, doc),
        ('k_margin_planned', K, {'standard_margin_planned': True}, doc),
        ('k_margin_planned_wrong_type', K, {'standard_margin_planned': 'yes'}, doc),
        ('s_style_wrong_type', S, {'table_style': [1]}, doc),
        ('config_not_object4', K, [1, 2], doc),
        # 해석이 파이썬과 갈릴 수 있는 속성 값(전각 숫자·밑줄)은 거부
        ('w_fullwidth_digit_attr', W, None, pack(h, sec(pg, host(table([[cell('가', 0, 0, w='５０００'), cell('나나', 0, 1)]]))))),
        ('w_underscore_int_attr', W, None, pack(h, sec(pg, host(table([[cell('가', 0, 0, w='5_000'), cell('나나', 0, 1)]]))))),
        ('w_exp_float_height', W, None, pack(h, sec(pg, host(grid([['가', '나'], ['x', 'y']], [3000, 3000]))), header=STD4.replace('height="1000"', 'height="1e3"', 1))),
        # 처리 모드에서만 거부: 머리글 뿌리에 문단 이름공간(hp) 선언이 없어 탭 요소의 접두사를 정할 수 없다.
        ('proc_k_header_no_hp_ns', K, None, pack(h, sec(pg, T('가 ..... 1'), T('나 ..... 2')), header=STD4.replace(' xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"', '').replace('<hp:switch>', '<hh:switch>').replace('</hp:switch>', '</hh:switch>').replace('<hp:case hp:required-namespace', '<hh:case hh:required-namespace').replace('</hp:case>', '</hh:case>').replace('<hp:default>', '<hh:default>').replace('</hp:default>', '</hh:default>'))),
    ]
    import rsxml4_cases_b
    return out + rsxml4_cases_b.unsupported_cases(h)
