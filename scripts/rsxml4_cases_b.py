"""RSXML4 합성 HWPX 생성기(2): 기본 표 서식(table_style)·정렬 재확인(table_style_align).

예시 표 프로필은 손으로 쓴 값이 아니라 파이썬 `table_style.learn_from_table`(합성 예시 문서)·`default_style()` 이 만든 값이다.
정렬 재확인(2차 호출)은 같은 함수를 표 칸 너비 처리 결과(파이썬이 만든 문서)에 다시 부른 것이다.
"""
from __future__ import annotations

import json
import random
import re
import sys
import tempfile
from pathlib import Path

import rsxml3_cases as m3
from rsxml3_cases import C, PP, R, P, T, mk_header, NSH
import rsxml4_cases as m4
from rsxml4_cases import cell, table, host, grid, sec, pack, pagepr, W, S, A, K, FUZZ

FACES_EX = ['함초롬바탕', '함초롬돋움', '휴먼명조', 'HY견고딕', '한컴돋움']
FACES_DOC = ['함초롬바탕', '함초롬돋움', '맑은 고딕']


def bf(i, l='SOLID', r='SOLID', t='SOLID', b='SOLID', fill=None, pic=False, w='0.12 mm'):
    fb = ''
    if fill:
        fb = f'<hc:fillBrush><hc:winBrush faceColor="{fill}" hatchColor="#FFFFFF" alpha="0"/></hc:fillBrush>'
    if pic:
        fb = '<hc:fillBrush><hc:imgBrush mode="TILE"><hc:img binaryItemIDRef="image1" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/></hc:imgBrush></hc:fillBrush>'
    return (f'<hh:borderFill id="{i}" threeD="0" shadow="0" centerLine="NONE" breakCellSeparateLine="0">'
            f'<hh:slash type="NONE" Crooked="0" isCounter="0"/><hh:backSlash type="NONE" Crooked="0" isCounter="0"/>'
            f'<hh:leftBorder type="{l}" width="{w}" color="#000000"/><hh:rightBorder type="{r}" width="{w}" color="#000000"/>'
            f'<hh:topBorder type="{t}" width="{w}" color="#000000"/><hh:bottomBorder type="{b}" width="{w}" color="#000000"/>'
            f'<hh:diagonal type="NONE" width="0.1 mm" color="#000000"/>{fb}</hh:borderFill>')


def style_header(fills, chars, paras, faces):
    h = mk_header(chars, paras=paras, faces=faces)
    block = f'<hh:borderFills itemCnt="{len(fills)}">{"".join(fills)}</hh:borderFills>'
    h2 = re.sub(r'<hh:borderFills .*?</hh:borderFills>', lambda m: block, h, count=1, flags=re.S)
    assert h2 != h
    return h2


EX_FILLS = [bf(1), bf(2, b='DOUBLE', fill='#D9D9D9', w='0.3 mm'), bf(3, l='NONE', r='NONE', fill='#F2F2F2'), bf(4, t='DASH', b='DOT'), bf(5, l='DOUBLE', w='0.5 mm')]
EX_CHARS = [C(1000, 0), C(1300, 4, bold=True, ratio=95, sp=-3), C(1200, 2), C(1100, 3, bold=True), C(1000, 1, ratio=90, sp=5)]
EX_PARAS = [PP(), PP(align='CENTER'), PP(align='LEFT'), PP(align='DISTRIBUTE'), PP(align='RIGHT')]
EX_HEADER = style_header(EX_FILLS, EX_CHARS, EX_PARAS, FACES_EX)

# 대상 문서 머리: 서로 다른 글꼴 수·테두리·글자·문단 모양(그림 바탕 4번 포함, 낱말 '배분' 정렬 문단 모양 포함)
DOC_FILLS = [bf(1), bf(2, fill='#FFFF00'), bf(3), bf(4, pic=True), bf(5, l='NONE', r='NONE')]
DOC_CHARS = [C(1000, 0), C(1000, 1, bold=True), C(1400, 2), C(900, 0, sp=3), C(1000, 1, ratio=80), C(1200, 2, bold=True, sp=-2)]
DOC_PARAS = [PP(), PP(align='CENTER'), PP(align='LEFT', left=500), PP(align='RIGHT', switch=True, prev=100), PP(align='DISTRIBUTE'), PP(align='JUSTIFY', intent=-1000)]
DOC_HEADER = style_header(DOC_FILLS, DOC_CHARS, DOC_PARAS, FACES_DOC)
DOC_HEADER_SAME_FACES = style_header(DOC_FILLS, DOC_CHARS, DOC_PARAS, FACES_EX)


def ex_table(variant=0):
    """예시 표(5행 3열, 머리글 1행). variant 1 이면 머리글 2행."""
    rows = []
    if variant == 1:
        rows.append([cell('구분', 0, 0, w=2000, rs=2, cid=1, pp=1, bf=2), cell('실적', 0, 1, w=4000, cs=2, cid=1, pp=1, bf=2)])
        rows.append([cell('목표', 1, 1, w=2000, cid=1, pp=1, bf=2), cell('달성', 1, 2, w=2000, cid=1, pp=1, bf=2)])
        start = 2
    else:
        rows.append([cell('구분', 0, 0, w=2000, cid=1, pp=1, bf=2), cell('내용', 0, 1, w=3000, cid=1, pp=1, bf=2), cell('비고', 0, 2, w=1000, cid=1, pp=1, bf=2)])
        start = 1
    for r in range(start, 5):
        rows.append([cell(f'항목{r}', r, 0, w=2000, cid=3, pp=3, bf=3), cell(f'내용{r}', r, 1, w=3000, cid=2, pp=2, bf=4 if r % 2 else 1),
                     cell(f'{r}00', r, 2, w=1000, cid=4, pp=4, bf=5)])
    return table(rows, width=6000, inm=(200, 300))


def learn(header_xml, table_xml, name='ex'):
    from defusedxml import ElementTree as ET
    from docfit_core.table_style import learn_from_table
    root = ET.fromstring(f'<r {NSH}>{table_xml}</r>'.encode('utf-8'))
    tbl = next(x for x in root.iter() if x.tag.endswith('}tbl'))
    return json.loads(json.dumps(learn_from_table(ET.fromstring(header_xml.encode('utf-8')), tbl, name)))


def _prefixed(style, mapping):
    """프로필의 XML 조각 접두사를 바꾼다(파이썬 직렬화기가 등록되지 않은 이름공간에 ns0 같은 접두사를 쓰는 경우를 흉내)."""
    s = json.loads(json.dumps(style))

    def fix(x):
        for a, b in mapping.items():
            x = x.replace(f'xmlns:{a}=', f'xmlns:{b}=').replace(f'<{a}:', f'<{b}:').replace(f'</{a}:', f'</{b}:')
            x = re.sub(rf'(\s){a}:', rf'\1{b}:', x)
        return x
    s['borders'] = {k: fix(v) for k, v in s['borders'].items()}
    for c in s['chars'].values():
        c['fonts'] = {k: fix(v) for k, v in c['fonts'].items()}
    if s.get('table_border'):
        s['table_border'] = fix(s['table_border'])
    return s


def make_configs():
    from docfit_core.table_style import default_style
    learned = learn(EX_HEADER, ex_table(0))
    learned2 = learn(EX_HEADER, ex_table(1), 'ex2')
    cfg = {
        'ts_default': {'table_style': json.loads(json.dumps(default_style()))},
        'ts_learned': {'table_style': learned},
        'ts_learned_2head': {'table_style': learned2},
        'ts_learned_ns0': {'table_style': _prefixed(learned, {'hh': 'ns0', 'hc': 'ns1'})},
        'ts_learned_nohc': {'table_style': _prefixed(learned, {'hh': 'q'})},
        'ts_invalid_version': {'table_style': {**learned, 'version': 2}},
        'ts_invalid_shape': {'table_style': {**learned, 'rows': 9}},
        'ts_empty': {'table_style': {}},
        'ts_none': {'table_style': None},
    }
    no_line = json.loads(json.dumps(learned))
    for c in no_line['cells']:
        c['line'] = None
    cfg['ts_no_line'] = {'table_style': no_line}
    va = json.loads(json.dumps(learned))
    for c in va['cells']:
        c['valign'] = 'TOP'
    va['in_margin'] = {'left': '700', 'right': '700', 'top': '50', 'bottom': '50', 'extra': '9'}
    cfg['ts_valign_inmargin'] = {'table_style': va}
    nb = json.loads(json.dumps(learned))
    nb['table_border'] = None
    cfg['ts_no_table_border'] = {'table_style': nb}
    ta = json.loads(json.dumps(learned))
    for i, c in enumerate(ta['cells']):
        c['text_align'] = ['LEFT', 'RIGHT', 'CENTER'][i % 3]
    cfg['ts_text_align_mix'] = {'table_style': ta}
    dist = json.loads(json.dumps(default_style()))
    for c in dist['cells'][:4]:
        c['align'] = 'DISTRIBUTE'
    cfg['ts_default_distribute'] = {'table_style': dist}
    return cfg


def _style_ok(cfgs):
    for k in ('ts_learned', 'ts_learned_2head', 'ts_default'):
        from docfit_core.table_style import valid_style
        assert valid_style(cfgs[k]['table_style']), k


# ---------------------------------------------------------------- 대상 표 만들기

def data_table(texts, widths=None, cids=None, pps=None, bfs=None, rs_head=None, **kw):
    nrows, ncols = len(texts), len(texts[0])
    widths = widths or [2500] * ncols
    rows = []
    for r in range(nrows):
        rows.append([cell(texts[r][c], r, c, w=widths[c], cid=(cids or {}).get((r, c), 0), pp=(pps or {}).get((r, c), 0), bf=(bfs or {}).get((r, c), 1)) for c in range(ncols)])
    return table(rows, **kw)


def build(h, py, m4mod):
    out = {}
    rnd = random.Random(20261124)
    cfgs = make_configs()
    _style_ok(cfgs)
    m4.CONFIGS.update(cfgs)
    body = 'Contents/section0.xml'

    def doc(name, secs, header=DOC_HEADER, cfg='ts_default', jobs=None, extra=None):
        out[name] = (pack(h, secs, header, extra), jobs or [(S, cfg)])

    pg = pagepr()
    base = [['구분', '내용', '비고'], ['가', '긴 문장이 들어 있는 칸입니다 이 글은 길다', '10'], ['나', '짧음', '2026. 10. 10.']]
    doc('s_basic_3x3', sec(pg, host(data_table(base))))
    doc('s_basic_2x2', sec(pg, host(data_table([['구분', '내용'], ['가', '나']]))))
    doc('s_one_col_not_grid', sec(pg, host(data_table([['구분'], ['가']]))))
    doc('s_one_row_not_grid', sec(pg, host(data_table([['구분', '내용']]))))
    rows = [[cell('구분', 0, 0, rs=2), cell('실적', 0, 1, cs=2)], [cell('목표', 1, 1), cell('달성', 1, 2)], [cell('가', 2, 0), cell('1', 2, 1), cell('2', 2, 2)], [cell('나', 3, 0), cell('3', 3, 1), cell('4', 3, 2)]]
    doc('s_header_two_rows', sec(pg, host(table(rows))))
    doc('s_header_two_rows_learned2', sec(pg, host(table(rows))), cfg='ts_learned_2head')
    rows = [[cell('분류', 0, 0, cs=2), cell('내용', 0, 2)], [cell('가', 1, 0), cell('나', 1, 1), cell('다', 1, 2)], [cell('라', 2, 0), cell('마', 2, 1), cell('바', 2, 2)]]
    doc('s_colspan_header', sec(pg, host(table(rows))))
    rows = [[cell('가', 0, 0), cell('나', 0, 1)], [cell('세로 병합', 1, 0, rs=2), cell('x', 1, 1)], [cell('y', 2, 1)]]
    doc('s_rowspan_body', sec(pg, host(table(rows))))
    rows = [[cell('가', 0, 0), cell('나', 0, 1)], [cell('전체 병합 글', 1, 0, cs=2)]]
    doc('s_body_single_cell_row_not_grid', sec(pg, host(table(rows))))
    doc('s_number_cells_align', sec(pg, host(data_table([['항목', '금액', '비율'], ['가', '1,000', '12.5%'], ['나', '2026. 1. 1.', '10:30'], ['다', '약 3명', '25 개']]))))
    doc('s_sentence_left', sec(pg, host(data_table([['구분', '설명'], ['가', '함대 이동에 약 450년 소요 예상'], ['나', '기타']]))))
    doc('s_long_text_multi_line', sec(pg, host(data_table([['구분', '설명'], ['가', '이 글은 아주 길어서 칸 한 줄에 들어가지 않는 글입니다 ' * 3], ['나', '기타']], widths=[1500, 3000]))))
    doc('s_spread_header_label', sec(pg, host(data_table([['구 분', '내 용', '시 설 명'], ['가', '나', '다']], pps={(0, 0): 4, (0, 1): 4, (0, 2): 4}))))
    doc('s_spread_digits_header', sec(pg, host(data_table([['2026', '332', '가나다라마바사'], ['가', '나', '다']], pps={(0, 0): 4, (0, 1): 4, (0, 2): 4}))))
    doc('s_default_distribute_profile', sec(pg, host(data_table([['구 분', '내 용', '비 고'], ['가', '나', '다']]))), cfg='ts_default_distribute')
    rows = [[cell('', 0, 0, raw=P([R(0, '가'), R(1, '나'), R(2, '다')], 0)), cell('', 0, 1, raw=P([R(1, '라')], 0))], [cell('x', 1, 0), cell('y', 1, 1)]]
    doc('s_mixed_runs_paragraph', sec(pg, host(table(rows))))
    rows = [[cell('가', 0, 0, bf=4), cell('나', 0, 1, bf=2)], [cell('x', 1, 0, bf=4), cell('y', 1, 1)]]
    doc('s_picture_fill_cell', sec(pg, host(table(rows))))
    rows = [[cell('가', 0, 0, cid=77), cell('나', 0, 1, cid=88)], [cell('x', 1, 0, bf=99), cell('y', 1, 1, pp=99)]]
    doc('s_missing_refs', sec(pg, host(table(rows))))
    rows = [[cell('', 0, 0, raw=P([R(None, '가')], 0)), cell('', 0, 1, raw=P([R(0, '나')], None))], [cell('x', 1, 0), cell('y', 1, 1)]]
    doc('s_run_para_attr_missing', sec(pg, host(table(rows))))
    rows = [[cell('', 0, 0, raw=P([R(None, '가')], 0)), cell('', 0, 1, raw=P([R(0, '나')], 0))], [cell('x', 1, 0), cell('y', 1, 1)]]
    doc('s_run_attr_missing_noid_char', sec(pg, host(table(rows))), header=DOC_HEADER.replace('<hh:charPr id="0"', '<hh:charPr', 1))
    rows = [[cell('가', 0, 0), cell('나', 0, 1)], ['<hp:tc borderFillIDRef="1"><hp:cellAddr colAddr="0" rowAddr="1"/><hp:cellSpan colSpan="1" rowSpan="1"/><hp:cellSz width="2500" height="2000"/></hp:tc>', cell('y', 1, 1)]]
    doc('s_cell_without_sublist', sec(pg, host(table(rows))))
    doc('s_empty_cells', sec(pg, host(data_table([['', ''], ['', '']]))))
    rows = [[cell('', 0, 0, raw=host(data_table([['가', '나'], ['다', '라']]))), cell('옆', 0, 1)], [cell('x', 1, 0), cell('y', 1, 1)]]
    doc('s_nested_table', sec(pg, host(table(rows))))
    doc('s_unit_caption_protected', sec(pg, host(data_table(base, caption='(단위: 개소)')), host(data_table(base))))
    doc('s_normal_caption_applied', sec(pg, host(data_table(base, caption='<표 1> 추진 현황'))))
    rows = [[cell('구분', 0, 0, rs=2), cell('', 0, 1)], [cell('', 1, 1)]]
    doc('s_design_bar_protected', sec(pg, host(table(rows, rowcnt=2, colcnt=2)), host(data_table(base))))
    rows = [[cell('구분' if (r, c) == (0, 0) else '', r, c) for c in range(4)] for r in range(3)]
    doc('s_diagram_protected', sec(pg, host(table(rows)), host(data_table(base))))
    doc('s_flow_protected', sec(pg, host(data_table([['접수', '→', '심사']])), host(data_table(base))))
    doc('s_title_table_excluded', sec(pg, host(table([[cell('보고서 제목', 0, 0, w=5000, cs=2)], [cell('2026. 10. 10.', 1, 0), cell('담당 과장 홍길동', 1, 1)]], rowcnt=2, colcnt=2)), host(data_table(base))))
    doc('s_title_blank_info_excluded', sec(pg, host(table([[cell('보고서 제목', 0, 0, w=5000, cs=2)], [cell('', 1, 0), cell('', 1, 1)]], rowcnt=2, colcnt=2)), host(data_table(base))))
    doc('s_title_type3_blank', sec(pg, host(table([[cell('보고서 제목', 0, 0)], [cell('', 1, 0)]], rowcnt=2, colcnt=1)), host(data_table(base))))
    doc('s_mid_attach_excluded', sec(pg, host(data_table([['Ⅰ.', '개요']])), host(data_table([['붙임 1', '서류']])), host(data_table(base))))
    # 문서 머리 표(본문 시작 전)·큰 글자 표 제외
    approve = data_table([['담당', '팀장', '과장'], ['', '', '']])
    doc('s_head_block_before_body', sec(pg, host(approve), T('□ 추진 배경', pp=0), host(data_table(base))))
    doc('s_head_block_after_body_applied', sec(pg, T('□ 추진 배경'), host(approve)))
    doc('s_head_block_body_sentence_starts', sec(pg, host(approve), T('이것은 열두 글자를 넘는 본문 문장입니다 가나다', pp=0), host(data_table(base))))
    doc('s_head_block_centered_sentence_not_body', sec(pg, host(approve), T('이것은 열두 글자를 넘는 본문 문장입니다 가나다', pp=1), host(data_table(base))))
    doc('s_head_block_big_char_title_sentence', sec(pg, host(approve), T('이것은 열두 글자를 넘는 본문 문장입니다 가나다', cid=2), host(data_table(base))))
    doc('s_head_block_attach_sentence_not_body', sec(pg, host(approve), T('붙임 1. 서류 가나다라마바사 1부.'), host(data_table(base))))
    doc('s_head_block_numbered_end_not_body', sec(pg, host(approve), T('2. 가나다라마바사아자차카 1부.'), host(data_table(base))))
    doc('s_head_block_aside_size', sec(pg, host(approve), P([R(2, '(괄호 부연만 큰 글씨입니다 가나다라마바사)'), R(0, '본문')], 0), host(data_table(base))))
    doc('s_head_block_parenthetical_size_mix', sec(pg, host(approve), P([R(0, '가나다 '), R(2, '(가나다라마바사아자차카타파하)'), R(0, ' 라마바사아자')], 0), host(data_table(base))))
    doc('s_head_block_no_body_all_found', sec(pg, host(approve), host(data_table(base))))
    doc('s_big_char_table_excluded', sec(pg, T('□ 추진 배경'), host(data_table([['구분', '내용'], ['가', '나']], cids={(0, 0): 2, (1, 1): 2})), host(data_table(base))))
    doc('s_big_char_nested_inside', sec(pg, T('□ 추진 배경'), host(table([[cell('', 0, 0, raw=host(data_table([['가', '나'], ['다', '라']], cids={(0, 0): 2}))), cell('x', 0, 1)], [cell('y', 1, 0), cell('z', 1, 1)]]))))
    out['s_two_sections_head'] = (h.hwpx_bytes({'Contents/section0.xml': sec(pg, host(approve)), 'Contents/section1.xml': sec(T('□ 추진 배경'), host(data_table(base)))}, header=DOC_HEADER), [(S, 'ts_default')])
    out['s_sections_order_10_2'] = (h.hwpx_bytes({'Contents/section10.xml': sec(T('□ 본문'), host(data_table(base))), 'Contents/section2.xml': sec(host(approve), host(data_table(base)))}, header=DOC_HEADER), [(S, 'ts_default')])
    out['s_sections_body_in_later'] = (h.hwpx_bytes({'Contents/section0.xml': sec(host(approve), host(data_table(base))), 'Contents/section1.xml': sec(T('□ 추진 배경'), host(data_table(base)))}, header=DOC_HEADER), [(S, 'ts_default')])
    # 설정(프로필) 변형
    for cname in ['ts_learned', 'ts_learned_2head', 'ts_learned_ns0', 'ts_learned_nohc', 'ts_invalid_version', 'ts_invalid_shape', 'ts_empty', 'ts_none', 'ts_no_line', 'ts_valign_inmargin', 'ts_no_table_border',
                  'ts_text_align_mix']:
        doc(f's_profile_{cname[3:]}', sec(pg, T('□ 추진 배경'), host(data_table(base)), host(data_table([['구 분', '내 용'], ['가', '나']], pps={(0, 0): 4, (0, 1): 4}))), cfg=cname)
    doc('s_learned_header_same_faces', sec(pg, T('□ 추진 배경'), host(data_table(base))), header=DOC_HEADER_SAME_FACES, cfg='ts_learned')
    doc('s_learned_font_reuse_existing', sec(pg, T('□ 추진 배경'), host(data_table(base))), header=DOC_HEADER.replace('맑은 고딕', '한컴돋움'), cfg='ts_learned')
    doc('s_header_missing_borderfills', sec(pg, host(data_table(base))), header=DOC_HEADER.replace('borderFills', 'borderFillsX'))
    doc('s_header_missing_charprops', sec(pg, host(data_table(base))), header=DOC_HEADER.replace('charProperties', 'charPropertiesX'))
    doc('s_header_missing_paraprops', sec(pg, host(data_table(base))), header=DOC_HEADER.replace('paraProperties', 'paraPropertiesX'))
    doc('s_header_missing_fontfaces', sec(pg, host(data_table(base))), header=re.sub(r'<hh:fontfaces.*?</hh:fontfaces>', '', DOC_HEADER, flags=re.S), cfg='ts_learned')
    doc('s_header_one_fontface_lang', sec(pg, host(data_table(base))), header=re.sub(r'<hh:fontface lang="LATIN".*?</hh:fontface>', '', DOC_HEADER, flags=re.S), cfg='ts_learned')
    doc('s_header_nonnumeric_ids', sec(pg, host(data_table(base))), header=DOC_HEADER.replace('<hh:borderFill id="5"', '<hh:borderFill id="x5"'))
    doc('s_header_pretty_printed', sec(pg, T('□ 추진 배경'), host(data_table(base))), header=DOC_HEADER.replace('><', '>\n  <'), cfg='ts_learned')
    doc('s_header_no_fontref', sec(pg, host(data_table(base))), header=DOC_HEADER.replace('<hh:fontRef', '<hh:fontRefX').replace('hangul="0" latin="0"', 'hangul="0" latin="0"'), cfg='ts_learned')
    doc('s_header_empty_char_group', sec(pg, host(data_table(base))), header=re.sub(r'<hh:charPr .*</hh:charPr>', '', DOC_HEADER, flags=re.S), cfg='ts_learned')
    doc('s_east_asian_ambiguous_text', sec(pg, host(data_table([['구분', '내용'], ['가', '“따옴표” ① ★ ‘작은’ ±'], ['나', 'ABC ＡＢＣ ｱｲｳ 한글']], widths=[1500, 2000]))))
    doc('s_cell_margin_fit', sec(pg, host(table([[cell('가', 0, 0), cell('나', 0, 1)], [cell('x' * 18, 1, 0, w=2500, hasm=True, mg=(1500, 1500)), cell('y', 1, 1)]], inm=(100, 100)))))
    doc('s_inmargin_missing', sec(pg, host(table([[cell('가', 0, 0), cell('나', 0, 1)], [cell('x', 1, 0), cell('y', 1, 1)]]).replace('<hp:inMargin left="510" right="510" top="141" bottom="141"/>', ''))), cfg='ts_valign_inmargin')
    doc('s_many_tables_dedupe', sec(pg, *[host(data_table(base)) for _ in range(6)]))
    doc('s_tables_in_one_paragraph', sec(pg, host([data_table(base), data_table([['가', '나'], ['다', '라']])])))
    doc('s_lineseg_removed', sec(pg, host(data_table(base))))
    doc('s_cell_two_paragraphs', sec(pg, host(table([[cell(['구분', '둘째'], 0, 0), cell('내용', 0, 1)], [cell(['가', '1,000'], 1, 0), cell('나', 1, 1)]]))))
    doc('s_no_tables', sec(pg, T('본문만')))
    doc('s_table_border_missing_id', sec(pg, host(data_table(base).replace(' borderFillIDRef="1"', '', 1))))
    doc('s_pic_in_cell', sec(pg, host(table([[cell('', 0, 0, raw='<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:pic id="1"/></hp:run></hp:p>'), cell('나', 0, 1)], [cell('x', 1, 0), cell('y', 1, 1)]]))))
    doc('s_pretty_section', sec('\n  ' + pg + '\n  ' + host(data_table(base)) + '\n'))
    doc('s_bad_rowcnt_error', sec(pg, host(data_table(base).replace('rowCnt="3"', 'rowCnt="x"', 1))))
    doc('s_bad_celladdr_error', sec(pg, host(data_table(base).replace('rowAddr="1"', 'rowAddr="q"', 1))))
    # 무작위
    words = ['가', '구분', '12,345', '2026. 10. 10.', '아주 긴 글이 들어가는 칸입니다 가나다라마바사', '', 'abc', '10', '계 약', '합 계', '約 3명', '가 나 다']
    for i in range(14 * FUZZ):
        ncols = rnd.randint(2, 5)
        nrows = rnd.randint(2, 6)
        texts = [[rnd.choice(words) for _ in range(ncols)] for _ in range(nrows)]
        cids = {(r, c): rnd.randrange(6) for r in range(nrows) for c in range(ncols) if rnd.random() < 0.4}
        pps = {(r, c): rnd.randrange(6) for r in range(nrows) for c in range(ncols) if rnd.random() < 0.5}
        bfs = {(r, c): rnd.randrange(1, 6) for r in range(nrows) for c in range(ncols) if rnd.random() < 0.3}
        widths = [rnd.choice([1200, 2500, 4000]) for _ in range(ncols)]
        doc(f's_random_{i:02d}', sec(pg, T('□ 추진 배경'), host(data_table(texts, widths=widths, cids=cids, pps=pps, bfs=bfs))), cfg=rnd.choice(['ts_default', 'ts_learned', 'ts_learned_2head', 'ts_no_line', 'ts_text_align_mix']))
    for i in range(6 * FUZZ):
        ncols = rnd.randint(3, 5)
        rows = [[cell(rnd.choice(['구분', '내용', '비고']), 0, c, w=2000, cid=rnd.randrange(6), pp=rnd.randrange(6)) for c in range(ncols)]]
        for r in range(1, rnd.randint(3, 5)):
            row, c = [], 0
            while c < ncols:
                cs = rnd.choice([1, 1, 2]) if c + 2 <= ncols else 1
                row.append(cell(rnd.choice(['가나다', '아주 긴 낱말이 들어갑니다 가나다라', '10', '']), r, c, w=2000 * cs, cs=cs, cid=rnd.randrange(6), pp=rnd.randrange(6)))
                c += cs
            rows.append(row)
        doc(f's_random_merge_{i:02d}', sec(pg, T('□ 추진 배경'), host(table(rows))), cfg=rnd.choice(['ts_default', 'ts_learned']))
    out.update(real_header_cases(h, rnd))
    # ---------------------------------------------------------------- 정렬 재확인(2차 호출): 파이썬 처리 결과에 같은 함수를 다시 부른다
    out.update(chain_cases(h, py, out))
    return out


def real_header_cases(h, rnd):
    """tests/fixtures 의 실제 HWPX 머리(글자 47·문단 47·테두리 20 / 글자 7·문단 20·테두리 2)에 합성 표·연결 부호 문단을 얹은 문서."""
    out = {}
    for stem in ('label228_circled_black', 'hwp_shapes_sample'):
        header = m3.real_header(stem)
        nc = len(re.findall(r'<hh:charPr ', header))
        npr = len(re.findall(r'<hh:paraPr ', header))
        nf = max(1, len(re.findall(r'<hh:borderFill ', header)))
        tag = stem.split('_')[0]
        for i in range(4):
            texts = [[rnd.choice(['구분', '내용', '비고', '', '가나다', '12,345', '2026. 10. 10.', '긴 문장이 들어 있는 칸입니다 이 글은 길다']) for _ in range(3)] for _ in range(rnd.randint(2, 4))]
            cids = {(r, c): rnd.randrange(nc) for r in range(len(texts)) for c in range(3) if rnd.random() < 0.7}
            pps = {(r, c): rnd.randrange(npr) for r in range(len(texts)) for c in range(3) if rnd.random() < 0.7}
            bfs = {(r, c): rnd.randrange(1, nf + 1) for r in range(len(texts)) for c in range(3) if rnd.random() < 0.5}
            body = sec(pagepr(), T('□ 추진 배경', cid=rnd.randrange(nc), pp=rnd.randrange(npr)),
                       host(data_table(texts, widths=[rnd.choice([2000, 3000, 4000]) for _ in range(3)], cids=cids, pps=pps, bfs=bfs), pp=rnd.randrange(npr), cid=rnd.randrange(nc)),
                       T('제1장 총칙 ........ 3', cid=rnd.randrange(nc), pp=rnd.randrange(npr)), T('제2장 목적 ........ 12', cid=rnd.randrange(nc), pp=rnd.randrange(npr)))
            data = pack(h, body, header)
            out[f'w_real_{tag}_{i}'] = (data, [(W, None)])
            out[f'k_real_{tag}_{i}'] = (data, [(K, None)])
            out[f's_real_{tag}_{i}'] = (data, [(S, ['ts_default', 'ts_learned', 'ts_learned_ns0', 'ts_no_line'][i])])
    return out


def chain_cases(h, py, base_cases):
    """기존 사례 문서에 파이썬 표 칸 너비(·기본 표 서식)를 먼저 적용한 결과를 2차 입력으로 삼는다."""
    out = {}
    width_fn = py['표너비_hwpx_처리']
    style_fn = py['기본표서식_hwpx_처리']
    tmp = Path(tempfile.mkdtemp(prefix='rsxml4_chain_'))
    n = 0
    for name, (data, jobs) in list(base_cases.items()):
        procs = {j[0] for j in jobs}
        if W in procs:
            src = tmp / f'{name}.src.hwpx'
            src.write_bytes(data)
            dst = tmp / f'{name}.w.hwpx'
            _, cnt, err = h.run_py(width_fn, src, dst, None)
            if err or not dst.exists():
                continue
            cfg = 'ts_default' if n % 3 else 'ts_learned'
            out[f'a_after_width_{name}'] = (dst.read_bytes(), [(A, cfg)])
            n += 1
        elif S in procs and name.startswith('s_') and not name.startswith('s_header_missing') and 'error' not in name:
            src = tmp / f'{name}.src.hwpx'
            src.write_bytes(data)
            cfgname = dict(jobs)[S]
            cfg = h.config_dict(cfgname)
            dst = tmp / f'{name}.w.hwpx'
            _, cnt, err = h.run_py(width_fn, src, dst, None)
            if err or not dst.exists():
                dst = src
            dst2 = tmp / f'{name}.s.hwpx'
            _, cnt2, err2 = h.run_py(style_fn, dst, dst2, cfg)
            if err2 or not dst2.exists():
                continue
            out[f'a_after_style_{name}'] = (dst2.read_bytes(), [(A, cfgname)])
    return out


def unsupported_cases(h):
    from docfit_core.table_style import default_style
    base = default_style()
    pg = pagepr()
    doc = pack(h, sec(pg, T('□ 추진 배경'), host(data_table([['구분', '내용'], ['가', '나']]))), DOC_HEADER)

    def bad(**kw):
        s = json.loads(json.dumps(base))
        s.update(kw)
        return {'table_style': s}
    out = [
        ('s_style_version_float', S, bad(version=1.0), doc),
        ('s_style_rows_str', S, bad(rows='5'), doc),
        ('s_style_grid_value_str', S, {'table_style': {**base, 'grid': [[str(x) for x in r] for r in base['grid']]}}, doc),
        ('s_style_missing_header_rows', S, {'table_style': {k: v for k, v in base.items() if k != 'header_rows'}}, doc),
        ('s_style_cell_no_border', S, {'table_style': {**base, 'cells': [{'char': 'c0'}] + base['cells'][1:]}}, doc),
        ('s_style_char_fonts_not_obj', S, {'table_style': {**base, 'chars': {k: {**v, 'fonts': 3} for k, v in base['chars'].items()}}}, doc),
        ('s_style_ratio_value_int', S, {'table_style': {**base, 'chars': {k: {**v, 'ratio': {'hangul': 100}} for k, v in base['chars'].items()}}}, doc),
        ('s_style_borders_value_not_str', S, {'table_style': {**base, 'borders': {k: 1 for k in base['borders']}}}, doc),
        ('s_style_table_border_not_str', S, bad(table_border=5), doc),
        ('sa_style_type', A, {'table_style': 'x'}, doc),
    ]
    # 처리 모드에서만 거부: 프로필 XML 조각의 이름공간이 문서 머리에 없으면 파이썬은 선언을 덧붙여 처리하지만 Rust 는 거부
    foreign = json.loads(json.dumps(base))
    k0 = next(iter(foreign['borders']))
    foreign['borders'] = {k: v.replace('http://www.hancom.co.kr/hwpml/2011/head', 'urn:other:head') for k, v in foreign['borders'].items()}
    out.append(('proc_s_style_foreign_namespace', S, {'table_style': foreign}, doc))
    return out
