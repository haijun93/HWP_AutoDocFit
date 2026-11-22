"""RSXML3 합성 HWPX 생성기: 별표 위첨자(asterisk_superscript)·붙임 글꼴(attachment_font)·콜론 라벨(colon_label)·
빈 줄 서식(blank_lines)·강조 낱말(emphasis).

`build(h, py)` 의 h 는 scripts/rsxml_parity.py 모듈, py 는 파이썬 처리기 이름공간(예: 서식표_종류판별 을 쓰는 학습용).
반환: {이름: (HWPX 바이트, [(처리기, 설정 이름)])}. 설정 이름은 CONFIGS 의 키(None 이면 설정 없음).
사용자 문서는 쓰지 않는다. 머리(header.xml)는 합성 문자열이거나 tests/fixtures 의 실제 HWPX 머리를 그대로 가져온 것이다.
"""
from __future__ import annotations

import html
import random
import re
import zipfile
from pathlib import Path

NSH = ('xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" '
       'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section"')
FACES = ['함초롬바탕', '함초롬돋움', '휴먼명조', 'HY견고딕', '한컴돋움']
LANGS = ['HANGUL', 'LATIN', 'HANJA', 'JAPANESE', 'OTHER', 'SYMBOL', 'USER']
ATTRS = ('hangul', 'latin', 'hanja', 'japanese', 'other', 'symbol', 'user')
ROOT = Path(__file__).resolve().parents[1]

CONFIGS = {
    'sym_rule': {'symbol_rules': [['□', 0, '휴먼명조', 15], ['ㅇ', 0, 'HY견고딕', 13.5]]},
    'sym_rule_str_size': None,  # 자리표시(아래에서 덮어씀)
    'sym_off': {'std_symbols': False, 'symbol_rules': [['ㅇ', 0, 'HY견고딕', 13.5]]},
    'sym_rule_newface': {'symbol_rules': [['ㅇ', 0, '새글꼴체', 12]]},
    'sym_rule_none_for_o': {'symbol_rules': [['□', 0, '휴먼명조', 15]]},
    'colon_off': {'colon_label_enabled': False},
    'ratio_on': {'ratio_reduce': True},
    'scheme_on': {'marker_scheme_active': True},
}
del CONFIGS['sym_rule_str_size']


# ---------------------------------------------------------------- 머리 만들기

def C(h=1000, face=0, bold=False, sup=False, sub=False, sp=0, ratio=100, color='#000000', id=True, extra='', bf=1):
    return dict(h=h, face=face, bold=bold, sup=sup, sub=sub, sp=sp, ratio=ratio, color=color, id=id, extra=extra, bf=bf)


def PP(align='JUSTIFY', prev=0, switch=False, tab='0', border=1, intent=0, left=0):
    return dict(align=align, prev=prev, switch=switch, tab=tab, border=border, intent=intent, left=left)


def mk_header(chars, paras=None, faces=FACES, with_char_group=True, with_para_group=True, with_fonts=True, extra_ref=''):
    paras = paras if paras is not None else [PP()]
    fonts = ''
    if with_fonts:
        for lang in LANGS:
            fonts += (f'<hh:fontface lang="{lang}" fontCnt="{len(faces)}">' +
                      ''.join(f'<hh:font id="{i}" face="{f}" type="TTF" isEmbedded="0"/>' for i, f in enumerate(faces)) +
                      '</hh:fontface>')
        fonts = f'<hh:fontfaces itemCnt="{len(LANGS)}">{fonts}</hh:fontfaces>'
    cs = []
    for i, c in enumerate(chars):
        ida = f' id="{i}"' if c['id'] else ''
        f = c['face']
        ref = ' '.join(f'{a}="{f}"' for a in ATTRS)
        rat = ' '.join(f'{a}="{c["ratio"]}"' for a in ATTRS)
        spc = ' '.join(f'{a}="{c["sp"]}"' for a in ATTRS)
        body = (f'<hh:fontRef {ref}/><hh:ratio {rat}/><hh:spacing {spc}/>'
                f'<hh:relSz {" ".join(a + "=" + chr(34) + "100" + chr(34) for a in ATTRS)}/>'
                f'<hh:offset {" ".join(a + "=" + chr(34) + "0" + chr(34) for a in ATTRS)}/>')
        if c['bold']:
            body += '<hh:bold/>'
        body += '<hh:underline type="NONE" shape="SOLID" color="#000000"/><hh:strikeout shape="NONE" color="#000000"/>'
        if c['sup']:
            body += '<hh:supscript/>'
        if c['sub']:
            body += '<hh:subscript/>'
        body += c['extra']
        cs.append(f'<hh:charPr{ida} height="{c["h"]}" textColor="{c["color"]}" shadeColor="none" useFontSpace="0" '
                  f'useKerning="0" symMark="NONE" borderFillIDRef="{c["bf"]}">{body}</hh:charPr>')
    chars_xml = ''.join(cs)
    if with_char_group:
        chars_xml = f'<hh:charProperties itemCnt="{len(cs)}">{chars_xml}</hh:charProperties>'
    ps = []
    for i, p in enumerate(paras):
        margin = (f'<hh:margin><hc:intent value="{p["intent"]}" unit="HWPUNIT"/><hc:left value="{p["left"]}" unit="HWPUNIT"/>'
                  f'<hc:right value="0" unit="HWPUNIT"/><hc:prev value="{p["prev"]}" unit="HWPUNIT"/><hc:next value="0" unit="HWPUNIT"/></hh:margin>'
                  f'<hh:lineSpacing type="PERCENT" value="160" unit="HWPUNIT"/>')
        if p['switch']:
            m2 = margin.replace(f'value="{p["prev"]}"', f'value="{p["prev"] * 2}"', 1) if p['prev'] else margin
            margin = (f'<hp:switch><hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar">{margin}</hp:case>'
                      f'<hp:default>{m2}</hp:default></hp:switch>')
        ps.append(f'<hh:paraPr id="{i}" tabPrIDRef="{p["tab"]}" condense="0"><hh:align horizontal="{p["align"]}" vertical="BASELINE"/>'
                  f'<hh:heading type="NONE" idRef="0" level="0"/>{margin}<hh:border borderFillIDRef="{p["border"]}" offsetLeft="0"/></hh:paraPr>')
    paras_xml = ''.join(ps)
    if with_para_group:
        paras_xml = f'<hh:paraProperties itemCnt="{len(ps)}">{paras_xml}</hh:paraProperties>'
    bf = ('<hh:borderFills itemCnt="2"><hh:borderFill id="1" threeD="0" shadow="0"><hh:leftBorder type="NONE"/></hh:borderFill>'
          '<hh:borderFill id="2" threeD="0" shadow="0"><hh:leftBorder type="SOLID"/></hh:borderFill></hh:borderFills>')
    tabs = '<hh:tabProperties itemCnt="2"><hh:tabPr id="0" autoTabLeft="0" autoTabRight="0"/><hh:tabPr id="1" autoTabLeft="1" autoTabRight="0"/></hh:tabProperties>'
    return (f'<?xml version="1.0" encoding="UTF-8"?><hh:head {NSH}><hh:refList>{fonts}{bf}{tabs}{chars_xml}{paras_xml}{extra_ref}'
            f'</hh:refList></hh:head>')


# ---------------------------------------------------------------- 문단·구역 만들기

def R(cid, text, extra=''):
    """run 하나(글 요소 하나). cid None 이면 charPrIDRef 속성 없음. text None 이면 빈 t."""
    a = '' if cid is None else f' charPrIDRef="{cid}"'
    t = '<hp:t/>' if text is None else f'<hp:t>{html.escape(text, quote=False)}</hp:t>'
    return f'<hp:run{a}>{extra}{t}</hp:run>'


SEG = '<hp:linesegarray><hp:lineseg textpos="0" vertpos="0" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="42520" flags="393216"/></hp:linesegarray>'


def P(runs, pp=0, seg=True, tail=''):
    """문단. runs 는 run XML 문자열 목록(또는 문자열 하나)."""
    body = runs if isinstance(runs, str) else ''.join(runs)
    pa = '' if pp is None else f' paraPrIDRef="{pp}"'
    return f'<hp:p{pa} styleIDRef="0">{body}{SEG if seg else ""}</hp:p>{tail}'


def T(text, cid=0, pp=0, seg=True):
    return P([R(cid, text)], pp, seg)


def blank(cid=0, pp=0, seg=True, text=''):
    return P([R(cid, text)], pp, seg)


def sec(*paras, decl='<?xml version="1.0" encoding="UTF-8"?>'):
    return f'{decl}<hs:sec {NSH}>{"".join(paras)}</hs:sec>'


def tc(paras_xml, r=0, c=0):
    return (f'<hp:tc borderFillIDRef="1"><hp:subList>{paras_xml}</hp:subList><hp:cellAddr colAddr="{c}" rowAddr="{r}"/>'
            f'<hp:cellSpan colSpan="1" rowSpan="1"/><hp:cellSz width="5669" height="2835"/></hp:tc>')


def tbl_in_p(rows, pp=0):
    """rows: 칸 문단 XML 의 2차원 목록 → 표를 담은 글 없는 문단."""
    trs = ''.join('<hp:tr>' + ''.join(tc(x, r, c) for c, x in enumerate(row)) + '</hp:tr>' for r, row in enumerate(rows))
    t = f'<hp:tbl rowCnt="{len(rows)}" colCnt="{max(len(r) for r in rows)}">{trs}</hp:tbl>'
    return f'<hp:p paraPrIDRef="{pp}"><hp:run charPrIDRef="0">{t}</hp:run></hp:p>'


def pack(h, secs, header, extra=None):
    """secs: {이름: 구역 XML} 또는 XML 하나."""
    if isinstance(secs, str):
        secs = {'Contents/section0.xml': secs}
    return h.hwpx_bytes(secs, extra_files=extra, header=header)


def real_header(stem='label228_circled_black'):
    with zipfile.ZipFile(ROOT / 'tests' / 'fixtures' / f'{stem}.hwpx') as z:
        return z.read('Contents/header.xml').decode('utf-8')


def real_char_count(header):
    return len(re.findall(r'<hh:charPr ', header))


# ---------------------------------------------------------------- 별표 위첨자

STD = mk_header([C(1000, 0), C(1000, 1, bold=True), C(1000, 0, sup=True), C(1500, 3), C(1000, 2, sub=True), C(1200, 4, sp=-5, ratio=95)])


def ast_cases(h):
    out = {}
    A = ['asterisk_superscript']
    lines = ['ㅇ 단어* 설명', 'ㅇ 단어** 설명', '□ 용어* 설명입니다', '- 항목 끝*', '* 주석 본문', '** 둘째 주석', '※ 참고* 사항',
             'ㅇ 앞에 빈칸 *', 'ㅇ 세 개***', 'ㅇ a*b 곱', 'ㅇ 3*4=12', 'ㅇ 단어*a', 'ㅇ 단어*(주)', 'ㅇ 단어**.', '1. 번호* 항목', '가. 한글* 항목',
             '① 원문자* 항목', '(1) 괄호* 항목', '일반 문장* 은 대상 아님', '', 'ㅇ', 'ㅇ *', 'ㅇ 별표**와 하나* 둘', 'ㅇ 마침표.* 뒤', 'ㅇ 𠀀* 서로게이트',
             'ㅇ 단어 *', 'ㅇ 단어＊', 'ㅇ 별*두*번*', '- 가*나**다*', '  ㅇ 들여쓴* 문장', '\tㅇ 탭* 문장', 'ㅇ 끝 **', 'ㅇ 연속 ** ** 끝']
    out['ast_lines_all'] = (pack(h, sec(*[T(x, i % 4) for i, x in enumerate(lines)]), STD), [(A[0], None)])
    for k in range(0, len(lines), 8):
        out[f'ast_lines_{k // 8}'] = (pack(h, sec(*[T(x, 0) for x in lines[k:k + 8]]), STD), [(A[0], None)])
    out['ast_split_runs'] = (pack(h, sec(P([R(0, 'ㅇ 단어'), R(0, '*'), R(0, '*'), R(1, ' 설명*')])), STD), [(A[0], None)])
    out['ast_run_boundary'] = (pack(h, sec(P([R(0, 'ㅇ 가*'), R(1, '* 나* 다**'), R(2, '*')])), STD), [(A[0], None)])
    out['ast_already_super'] = (pack(h, sec(P([R(0, 'ㅇ 단어'), R(2, '*'), R(0, ' 설명*')])), STD), [(A[0], None)])
    out['ast_all_already_super'] = (pack(h, sec(P([R(0, 'ㅇ 단어'), R(2, '*')])), STD), [(A[0], None)])
    out['ast_run_two_t'] = (pack(h, sec(P(['<hp:run charPrIDRef="0"><hp:t>ㅇ 단어*</hp:t><hp:t> 둘째 글*</hp:t></hp:run>'])), STD), [(A[0], None)])
    out['ast_t_with_tab'] = (pack(h, sec(P(['<hp:run charPrIDRef="0"><hp:t>ㅇ 단어*<hp:tab/>설명</hp:t></hp:run>'])), STD), [(A[0], None)])
    out['ast_secpr_ctrl'] = (pack(h, sec(P(['<hp:run charPrIDRef="0"><hp:secPr/><hp:ctrl><hp:colPr/></hp:ctrl><hp:t>ㅇ 단어* 설명</hp:t></hp:run>'])), STD), [(A[0], None)])
    out['ast_ctrl_unknown'] = (pack(h, sec(P(['<hp:run charPrIDRef="0"><hp:ctrl><hp:fieldBegin/></hp:ctrl><hp:t>ㅇ 단어* 설명</hp:t></hp:run>'])), STD), [(A[0], None)])
    out['ast_tbl_para'] = (pack(h, sec(tbl_in_p([[T('ㅇ 칸* 안', 0)]]), T('ㅇ 밖* 문장')), STD), [(A[0], None)])
    out['ast_non_run_child'] = (pack(h, sec('<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>ㅇ 단어*</hp:t></hp:run><hp:extra/></hp:p>'), STD), [(A[0], None)])
    out['ast_no_char_group'] = (pack(h, sec(T('ㅇ 단어* 설명')), mk_header([C()], with_char_group=False)), [(A[0], None)])
    out['ast_no_char_group_noop'] = (pack(h, sec(T('ㅇ 단어 설명')), mk_header([C()], with_char_group=False)), [(A[0], None)])
    out['ast_charpr_noid'] = (pack(h, sec(P([R(None, 'ㅇ 단어* 설명')])), STD), [(A[0], None)])
    out['ast_charpr_noid_match'] = (pack(h, sec(P([R(None, 'ㅇ 단어* 설명')])), mk_header([C(), C(id=False)])), [(A[0], None)])
    out['ast_missing_charpr'] = (pack(h, sec(P([R(99, 'ㅇ 단어* 설명')])), STD), [(A[0], None)])
    out['ast_sub_removed'] = (pack(h, sec(P([R(4, 'ㅇ 단어* 설명')])), STD), [(A[0], None)])
    out['ast_super_no_id'] = (pack(h, sec(P([R(0, 'ㅇ 단어* 설명')])), mk_header([C(), C(sup=True, id=False)])), [(A[0], None)])
    out['ast_id_gap'] = (pack(h, sec(P([R(0, 'ㅇ 단어* 설명'), R(7, 'ㅇ 둘*')])), STD.replace('<hh:charPr id="5"', '<hh:charPr id="7"').replace('itemCnt="6"', 'itemCnt="9"')), [(A[0], None)])
    out['ast_two_sections'] = (pack(h, {'Contents/section0.xml': sec(T('ㅇ 하나* 둘')), 'Contents/section1.xml': sec(T('ㅇ 셋** 넷'), T('ㅇ 다섯*'))}, STD), [(A[0], None)])
    out['ast_section_order'] = (pack(h, {'Contents/section10.xml': sec(T('ㅇ 열* 번째')), 'Contents/section2.xml': sec(T('ㅇ 두* 번째'))}, STD), [(A[0], None)])
    out['ast_empty_t'] = (pack(h, sec(P([R(0, None), R(0, 'ㅇ 단어* 설명')])), STD), [(A[0], None)])
    out['ast_nested_para_untouched'] = (pack(h, sec(tbl_in_p([[T('ㅇ 칸*', 0)]]), T('ㅇ 단어*')), STD), [(A[0], None)])
    out['ast_no_stars'] = (pack(h, sec(T('ㅇ 별표 없음'), T('□ 없음')), STD), [(A[0], None)])
    out['ast_ns_prefix_other'] = (pack(h, sec(T('ㅇ 단어* 설명')).replace('hs:', 'sec:').replace('xmlns:hs=', 'xmlns:sec='), STD), [(A[0], None)])
    out['ast_no_lineseg'] = (pack(h, sec(T('ㅇ 단어* 설명', seg=False)), STD), [(A[0], None)])
    out['ast_scheme_on'] = (pack(h, sec(T('□ 하나* 둘'), T('ㅇ 셋* 넷'), T('1. 다* 라')), STD), [(A[0], 'scheme_on')])
    out['ast_decl_variants'] = (pack(h, sec(T('ㅇ 단어* 설명'), decl="<?xml version='1.0' encoding='UTF-8' standalone='yes'?>"), STD), [(A[0], None)])
    rh = real_header()
    n = real_char_count(rh)
    rnd = random.Random(20261123)
    pool = ['ㅇ ', '□ ', '- ', '* ', '** ', '※ ', '1. ', '가. ', '① ', '', '(1) ', '  ㅇ ', '1) ']
    words = ['단어', '용어', 'abc', '3', '가나다', '', ' ']
    stars = ['*', '**', '***', ' *', '', '*a', '*.', '**)']
    for i in range(14):
        paras = []
        for _ in range(rnd.randint(2, 7)):
            text = rnd.choice(pool) + ''.join(rnd.choice(words) + rnd.choice(stars) for _ in range(rnd.randint(1, 4)))
            if rnd.random() < 0.35:
                cut = rnd.randint(0, len(text))
                paras.append(P([R(rnd.randrange(n), text[:cut]), R(rnd.randrange(n), text[cut:])]))
            else:
                paras.append(T(text, rnd.randrange(n)))
        hdr = rh if i % 2 else STD
        k = n if i % 2 else 6
        paras = [re.sub(r'charPrIDRef="(\d+)"', lambda m: f'charPrIDRef="{int(m.group(1)) % k}"', x) for x in paras]
        out[f'ast_random_{i:02d}'] = (pack(h, sec(*paras), hdr), [(A[0], None)])
    return out


# ---------------------------------------------------------------- 붙임 글꼴

ATT = mk_header([C(1000, 0), C(1400, 1), C(1300, 3, bold=True), C(1400, 3), C(1500, 2), C(1000, 3), C(1400, 3, sp=-3, ratio=97)],
                [PP('JUSTIFY'), PP('CENTER'), PP('RIGHT'), PP('LEFT'), PP('DISTRIBUTE')])


def att_cases(h):
    out = {}
    A = 'attachment_font'
    body = [T('ㅇ 본문 문장입니다', 3, 0)]  # 'ㅇ' 문장의 대표 글꼴: 1400 / HY견고딕(3)

    def doc(name, paras, header=ATT, cfg=None, secs=None):
        out[name] = (pack(h, secs if secs is not None else sec(*paras), header), [(A, cfg)])
    doc('att_basic', body + [T('붙임 1. 계획서 1부.', 0), T('2. 참고자료 1부.', 1), T('끝.', 0, 2)])
    doc('att_one_line', body + [T('붙임 사업계획서 1부.  끝.', 0)])
    doc('att_colon_header', body + [T('붙임 : 1. 계획서 1부', 0), T('2. 참고자료 1부.  끝.', 0)])
    doc('att_numbered_no_period', body + [T('붙임 1. 계획서 1부', 0), T('2. 참고 자료 1부', 1), T('끝.', 0)])
    doc('att_trailing_spaces', body + [T('붙임 1. 계획서 1부.   ', 0), T('끝.\t  ', 0)])
    doc('att_two_blocks', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0), T('본문 사이 글', 1), T('붙임 2. 나 1부.', 0), T('끝.', 0)])
    doc('att_already_ok', body + [T('붙임 1. 계획서 1부.', 3, 3), T('끝.', 3, 3)])
    doc('att_goal_none', [T('붙임 1. 계획서 1부.', 0), T('끝.', 0)])
    doc('att_no_end', body + [T('붙임 1. 계획서 1부.', 0), T('2. 참고', 0)])
    doc('att_too_long', body + [T('붙임 1. 하나.', 0)] + [T(f'{i}. 항목 {i}.', 0) for i in range(2, 40)] + [T('끝.', 0)])
    doc('att_table_breaks', body + [T('붙임 1. 계획서 1부.', 0), tbl_in_p([[T('칸', 0)]]), T('끝.', 0)])
    doc('att_tab_child', body + [P(['<hp:run charPrIDRef="0"><hp:t>붙임 1. 계획서<hp:tab/>1부.</hp:t></hp:run>']), T('끝.', 0)])
    doc('att_tab_tail_spaces', body + [P(['<hp:run charPrIDRef="0"><hp:t>붙임 1. 계획서 1부.<hp:tab/>  </hp:t></hp:run>']), T('끝.', 0)])
    doc('att_tab_only_tail', body + [P(['<hp:run charPrIDRef="0"><hp:t>붙임 1. 계획서 1부.  <hp:tab/><hp:tab/></hp:t></hp:run>']), T('끝.', 0)])
    doc('att_t_other_child', body + [P(['<hp:run charPrIDRef="0"><hp:t>붙임 1. 계획서<hp:fwSpace/>1부.</hp:t></hp:run>']), T('끝.', 0)])
    doc('att_multi_run', body + [P([R(0, '붙임 1. 계획서 '), R(1, '1부.  ')]), P([R(2, '끝.'), R(0, '  ')])])
    doc('att_align_center_shared', body + [T('붙임 1. 가 1부.', 0, 1), T('2. 나 1부.', 1, 1), T('끝.', 0, 2)])
    doc('att_align_two_kinds', body + [T('붙임 1. 가 1부.', 0, 1), T('2. 나 1부.', 1, 2), T('3. 다 1부.', 0, 4), T('끝.', 0, 2)])
    doc('att_para_no_pr', body + [T('붙임 1. 가 1부.', 0, None), T('끝.', 0, None)])
    doc('att_para_missing_pr', body + [T('붙임 1. 가 1부.', 0, 77), T('끝.', 0, 77)])
    doc('att_char_missing', body + [T('붙임 1. 가 1부.', 55), T('끝.', 0)])
    doc('att_char_noid_run', body + [P([R(None, '붙임 1. 가 1부.')]), T('끝.', 0)])
    doc('att_empty_runs', body + [T('붙임 1. 가 1부.', 0), P([R(0, None)]), T('끝.', 0)])
    doc('att_numbered_items_indent', body + [T('붙임　 1. 가 1부.', 0), T('　 2. 나 1부.', 0), T('끝.', 0)])
    doc('att_goal_tie', [T('ㅇ 하나', 3), T('ㅇ 둘', 4), T('붙임 1. 가 1부.', 0), T('끝.', 0)])
    doc('att_goal_weighted', [T('ㅇ 하나', 3), T('ㅇ 둘둘둘둘', 4), T('붙임 1. 가 1부.', 0), T('끝.', 0)])
    doc('att_goal_no_hangul_face', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0)], header=mk_header([C(1000, 0), C(1400, 1), C(1300, 3, bold=True), C(1400, 3)], with_fonts=True).replace('<hh:fontface lang="HANGUL"', '<hh:fontface lang="XXX"'))
    doc('att_no_fonts', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0)], header=mk_header([C(1000, 0), C(1400, 1), C(1300, 3), C(1400, 3)], with_fonts=False))
    doc('att_fontface_empty', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0)], header=mk_header([C(1000, 0), C(1400, 1), C(1300, 3), C(1400, 3)], faces=[]))
    doc('att_no_para_group', body + [T('붙임 1. 가 1부.', 0, 1), T('끝.', 0, 1)], header=mk_header([C(1000, 0), C(1400, 1), C(1300, 3), C(1400, 3)], [PP(), PP('CENTER')], with_para_group=False))
    doc('att_sym_rule', body + [T('붙임 1. 가 1부.', 0, 1), T('끝.', 0)], cfg='sym_rule')
    doc('att_sym_off', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0)], cfg='sym_off')
    doc('att_sym_newface', body + [T('붙임 1. 가 1부.', 0, 1), T('끝.', 0)], cfg='sym_rule_newface')
    doc('att_sym_rule_other_only', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0)], cfg='sym_rule_none_for_o')
    doc('att_scheme_on', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0)], cfg='scheme_on')
    doc('att_two_sections', None, secs={'Contents/section0.xml': sec(*body, T('붙임 1. 가 1부.', 0, 1), T('끝.', 0)),
                                        'Contents/section1.xml': sec(T('붙임 1. 나 1부.', 1), T('끝.', 0, 2))})
    doc('att_second_section_goal', None, secs={'Contents/section0.xml': sec(T('붙임 1. 가 1부.', 0, 1), T('끝.', 0)),
                                               'Contents/section1.xml': sec(*body)})
    doc('att_bad_height', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0)], header=mk_header([C(1000, 0), C(1400, 1), C(1300, 3), C(1400, 3)]).replace('height="1000"', 'height="x"', 1))
    doc('att_height_nonnumeric_other', body + [T('붙임 1. 가 1부.', 0), T('끝.', 0)], header=mk_header([C(1000, 0), C(1400, 1), C(1300, 3), C(1400, 3), C(777, 0)]).replace('height="777"', 'height=" 15 "'))
    rh = real_header()
    n = real_char_count(rh)
    rnd = random.Random(20261124)
    for i in range(8):
        paras = [T('ㅇ 본문 문장', rnd.randrange(n), rnd.randrange(3)) for _ in range(rnd.randint(1, 3))]
        paras.append(T('붙임 1. 계획서 1부.', rnd.randrange(n), rnd.randrange(3)))
        for j in range(rnd.randint(0, 3)):
            paras.append(T(f'{j + 2}. 자료 {j}부.' + rnd.choice(['', ' ', '  ']), rnd.randrange(n), rnd.randrange(3)))
        paras.append(T('끝.', rnd.randrange(n), rnd.randrange(3)))
        out[f'att_random_real_{i:02d}'] = (pack(h, sec(*paras), rh), [(A, None)])
    return out


# ---------------------------------------------------------------- 콜론 라벨

COL = mk_header([C(1000, 0, sp=0, ratio=100), C(1000, 1, bold=True, sp=-5, ratio=95), C(1200, 0, sp=10, ratio=80), C(1000, 3, sp=45, ratio=190)],
                [PP()])


def colon_cases(h):
    out = {}
    A = 'colon_label'

    def doc(name, paras, header=COL, cfg=None, secs=None):
        out[name] = (pack(h, secs if secs is not None else sec(*paras), header), [(A, cfg)])
    g1 = ['- 공사명: 현수식 안내판 철거', '- 공사기간: 2026. 10. 11. ~ 2026. 10. 20.', '- 계약방식: 수의계약']
    doc('col_basic', [T(x, 0) for x in g1])
    doc('col_basic_ratio', [T(x, 0) for x in g1], cfg='ratio_on')
    doc('col_disabled', [T(x, 0) for x in g1], cfg='colon_off')
    doc('col_scheme_on', [T(x, 0) for x in g1], cfg='scheme_on')
    doc('col_single', [T('- 공사명: 현수식', 0)])
    doc('col_equal_width', [T('- 공사명: 가', 0), T('- 계약명: 나', 0)])
    doc('col_two_vs_four', [T('ㅇ 장소: 시청', 0), T('ㅇ 행사일정: 오늘', 0), T('ㅇ 일시: 내일', 0)])
    doc('col_three_vs_six', [T('□ 행사명: 가', 0), T('□ 참석대상자: 나', 0)])
    doc('col_parity', [T('- 일시: 가', 0), T('- 장소명: 나', 0), T('- 참석자: 다', 0), T('- 주관: 라', 0)])
    doc('col_ratio_needed', [T('- 목적: 가', 2), T('- 사업추진배경: 나', 2)], cfg='ratio_on')
    doc('col_ratio_needed_off', [T('- 목적: 가', 2), T('- 사업추진배경: 나', 2)])
    doc('col_long_label_exception', [T('- 일시: 가', 0), T('- 아주아주긴라벨이름입니다: 예외', 0), T('- 장소명: 나', 0)])
    doc('col_label_7', [T('- 일곱글자라벨: 가', 0), T('- 일시: 나', 0)])
    doc('col_label_with_space', [T('- 사 업 명: 가', 0), T('- 일시: 나', 0)])
    doc('col_fullwidth_colon', [T('- 일시： 가', 0), T('- 장소명： 나', 0)])
    doc('col_markers', [T('1) 일시: 가', 0), T('2) 장소명: 나', 0), T('3) 담당: 다', 0)])
    doc('col_markers2', [T('가. 일시: 가', 0), T('나. 장소명: 나', 0)])
    doc('col_markers3', [T('① 일시: 가', 0), T('② 장소명: 나', 0)])
    doc('col_markers4', [T('(1) 일시: 가', 0), T('(2) 장소명: 나', 0)])
    doc('col_stars', [T('* 항세기: 가', 0), T('** 난세기명: 나', 0)])
    doc('col_latin_digits', [T('- ID: 가', 0), T('- Name1: 나', 0), T('- 연락처: 다', 0)])
    doc('col_split_runs', [P([R(0, ' '), R(1, '- '), R(1, '단장'), R(0, ': 가나')]), P([R(0, '- 공사기간: 나')])])
    doc('col_split_runs_mixed_cid', [P([R(1, '- 일시'), R(0, ': 가')]), P([R(2, '- 장소명'), R(3, ': 나')])])
    doc('col_run_other_child', [P(['<hp:run charPrIDRef="0"><hp:t>- 일시: 가</hp:t><hp:t>나</hp:t></hp:run>']), T('- 장소명: 나', 0)])
    doc('col_tab_in_t', [P(['<hp:run charPrIDRef="0"><hp:t>- 일시:<hp:tab/>가</hp:t></hp:run>']), T('- 장소명: 나', 0)])
    doc('col_empty_run_between', [P([R(0, '- 일시: 가'), '<hp:run charPrIDRef="0"/>']), T('- 장소명: 나', 0)])
    doc('col_tbl_in_group', [T('- 일시: 가', 0), P([R(0, '- 장소명: 나'), '<hp:run charPrIDRef="0"><hp:tbl rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:subList/></hp:tc></hp:tr></hp:tbl></hp:run>']), T('- 담당: 다', 0)])
    doc('col_break_group', [T('- 일시: 가', 0), T('그냥 글', 0), T('- 장소명: 나', 0), T('- 담당: 다', 0)])
    doc('col_two_groups', [T('- 일시: 가', 0), T('- 장소명: 나', 0), T('중간 글', 0), T('- 담당: 다', 0), T('- 연락처: 라', 0)])
    doc('col_in_table_cell', [tbl_in_p([[T('- 일시: 가', 0) + T('- 장소명: 나', 0)]])])
    doc('col_nested_tables', [tbl_in_p([[T('- 일시: 가', 0) + T('- 장소명: 나', 0) + tbl_in_p([[T('* 가: 하', 0) + T('* 가나다: 둘', 0)]])]]), T('- 담당: 다', 0), T('- 연락처: 라', 0)])
    doc('col_missing_charpr', [T('- 일시: 가', 77), T('- 장소명: 나', 0)])
    doc('col_noid_run', [P([R(None, '- 일시: 가')]), T('- 장소명: 나', 0)])
    doc('col_no_char_group', [T('- 일시: 가', 0), T('- 장소명: 나', 0)], header=mk_header([C()], with_char_group=False))
    doc('col_two_sections', None, secs={'Contents/section0.xml': sec(T('- 일시: 가', 0), T('- 장소명: 나', 0)),
                                        'Contents/section1.xml': sec(T('- 담당: 가', 0), T('- 연락처번호: 나', 3))})
    doc('col_section_order', None, secs={'Contents/section10.xml': sec(T('- 일시: 가', 0), T('- 장소명: 나', 0)),
                                         'Contents/section2.xml': sec(T('- 담당: 가', 0), T('- 연락처번호: 나', 3))})
    doc('col_nothing', [T('그냥 글', 0), T('또 글', 0)])
    doc('col_noop_equal', [T('- 일시: 가', 0), T('- 일시: 나', 0)])
    doc('col_nbsp_label', [T('- 일 시: 가', 0), T('- 장소명: 나', 0)])
    doc('col_astral', [T('- 𠀀𠀁: 가', 0), T('- 장소명: 나', 0)])
    doc('col_colon_no_space_after', [T('- 일시:가', 0), T('- 장소명:나', 0)])
    doc('col_colon_in_value_only', [T('- 일시 가: 나', 0), T('- 장소명 다: 라', 0)])
    doc('col_spacing_clamp', [T('- 일시: 가', 3), T('- 장소명: 나', 3)])
    doc('col_ratio_clamp', [T('- 일시: 가', 2), T('- 사업추진배경과목적: 나', 2)], cfg='ratio_on')
    rh = real_header()
    n = real_char_count(rh)
    rnd = random.Random(20261125)
    labels = ['일시', '장소', '장소명', '참석대상', '목적', '사업추진배경', '연락처', '담당', '비고', 'ID', '공사명']
    for i in range(10):
        paras = []
        for _ in range(rnd.randint(2, 6)):
            mk = rnd.choice(['- ', 'ㅇ ', '□ ', '* ', '1) ', '가. ', '※ '])
            paras.append(T(f'{mk}{rnd.choice(labels)}: 값{rnd.randint(0, 99)}', rnd.randrange(n)))
            if rnd.random() < 0.15:
                paras.append(T('중간 글', 0))
        out[f'col_random_real_{i:02d}'] = (pack(h, sec(*paras), rh), [(A, rnd.choice([None, 'ratio_on']))])
    return out


def build(h, py):
    cases = {}
    h_ = h
    cases.update(ast_cases(h))
    cases.update(att_cases(h))
    cases.update(colon_cases(h))
    try:
        import rsxml3_cases_b
        cases.update(rsxml3_cases_b.build(h, py, sys.modules[__name__]))
    except ImportError as e:
        print('rsxml3_cases_b 없음:', e)
    return cases


import sys  # noqa: E402


def unsupported_cases(h):
    """반영하지 못하는 설정은 '미지원 입력'(종료 코드 4)으로 거부해야 한다: (이름, 처리기, 설정, 문서 바이트)."""
    doc = pack(h, sec(T('ㅇ 단어* 설명'), T('붙임 1. 가 1부.'), T('끝.')), STD)
    return [
        ('ast_unknown_key', 'asterisk_superscript', {'새_설정': 1}, doc),
        ('att_unknown_key', 'attachment_font', {'font_scale': 90}, doc),
        ('col_unknown_key', 'colon_label', {'x': True}, doc),
        ('att_rules_not_array', 'attachment_font', {'symbol_rules': {'ㅇ': 1}}, doc),
        ('att_rule_not_array', 'attachment_font', {'symbol_rules': ['ㅇ']}, doc),
        ('att_rule_marker_not_str', 'attachment_font', {'symbol_rules': [[1, 0, '글꼴', 12]]}, doc),
        ('att_rule_face_not_str', 'attachment_font', {'symbol_rules': [['ㅇ', 0, 7, 12]]}, doc),
        ('att_rule_size_not_num', 'attachment_font', {'symbol_rules': [['ㅇ', 0, '글꼴', 'x']]}, doc),
        ('att_std_symbols_wrong_type', 'attachment_font', {'std_symbols': 'yes'}, doc),
        ('col_enabled_wrong_type', 'colon_label', {'colon_label_enabled': 1}, doc),
        ('col_ratio_wrong_type', 'colon_label', {'ratio_reduce': 'no'}, doc),
        ('blank_owner_text', 'blank_lines', {'title_owner_text': '담당관'}, doc),
        ('blank_profile_wrong_type', 'blank_lines', {'blank_line_profile': [1]}, doc),
        ('emph_words_wrong_type', 'emphasis', {'emphasis_words': {'a': 1}}, doc),
        ('emph_word_entry_wrong_type', 'emphasis', {'emphasis_words': [1]}, doc),
        ('config_not_object3', 'colon_label', [1, 2], doc),
        # 처리 모드에서만 거부(조사는 성공): 프로필 XML 의 이름공간이 문서에 없으면 파이썬은 선언을 덧붙여 처리하지만 Rust 는 거부한다.
        ('proc_emph_profile_unknown_ns', 'emphasis', {'emphasis_words': [{'marker': 'ㅇ', 'text': '단어', 'font': 'x', 'bold': False, 'size_delta': 0, 'fonts': {'hangul': '<q:font xmlns:q="urn:other" id="0" face="x" type="TTF" />'}}]}, doc),
    ]
