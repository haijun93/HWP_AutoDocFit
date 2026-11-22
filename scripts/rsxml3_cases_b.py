"""RSXML3 합성 사례 2부: 빈 줄 서식(blank_lines)·강조 낱말(emphasis).

두 처리기는 서식 프로필을 입력으로 받는다. 프로필은 파이썬 `blank_lines.learn`·`emphasis.learn` 이 '예시 문서'에서 실제로 만든 값이며
(내가 손으로 쓴 값이 아니다), 같은 값을 파이썬 전역 설정과 Rust `--config` 에 넘긴다. 예시와 대상 문서는 모두 합성이다.
"""
from __future__ import annotations

import random
import re

from defusedxml import ElementTree as ET


def build(h, py, m):
    """h: rsxml_parity 모듈, py: 파이썬 처리기 이름공간, m: rsxml3_cases 모듈."""
    from docfit_core import blank_lines, emphasis
    C, PP, R, P, T, blank, sec, pack, mk_header = m.C, m.PP, m.R, m.P, m.T, m.blank, m.sec, m.pack, m.mk_header
    kind = py['서식표_종류판별']
    register = py['XML_네임스페이스_등록']
    out = {}

    def parse(x):
        return ET.fromstring(x.encode('utf-8') if isinstance(x, str) else x)

    def host(tbl_xml, pp=0, cid=0):
        return f'<hp:p paraPrIDRef="{pp}"><hp:run charPrIDRef="{cid}">{tbl_xml}</hp:run></hp:p>'

    title = h.title_table_type1()
    box = h.tbl([[h.tc('', 0, 0, raw_sub=h.para('개요 본문은 열다섯 글자를 넘는 한 칸 상자 글입니다'))]], rowcnt=1, colcnt=1)
    mid = h.grid([['Ⅰ.', '추진 배경']])
    att = h.grid([['붙임 1', '서류']])
    plain = h.grid([['구분', '내용'], ['가', '나']])

    def learn_blank(header_xml, sections_xml, reg):
        if reg:
            for pre, uri in (('hp', 'http://www.hancom.co.kr/hwpml/2011/paragraph'), ('hh', 'http://www.hancom.co.kr/hwpml/2011/head'),
                             ('hc', 'http://www.hancom.co.kr/hwpml/2011/core'), ('hs', 'http://www.hancom.co.kr/hwpml/2011/section')):
                register(pre, uri)
        return blank_lines.learn(parse(header_xml), [parse(x) for x in sections_xml], kind)

    # ------------------------------------------------------------ 빈 줄 서식 예시 문서
    faces_extra = m.FACES + ['새로운글꼴']

    def ex_header(variant):
        chars = [C(1000, 0), C(500, 3), C(1500, 0), C(1000, 1, bf=2), C(800, 5 if variant == 'newfont' else 4, bold=True)]
        paras = [PP(), PP('LEFT', prev=0, switch=True), PP('RIGHT', prev=300, switch=True, tab='1', border=2), PP('CENTER', prev=200, switch=False, tab='1', border=2, left=500),
                 PP('JUSTIFY', prev=700, switch=True)]
        if variant == 'newfont':
            return mk_header(chars, paras, faces=faces_extra)
        return mk_header(chars, paras)

    def ex_section(variant):
        b = lambda cid, pp, text='': blank(cid, pp, text=text)
        items = [b(2, 1), host(title, 2), b(1, 3), host(box, 3), b(4, 4),
                 T('Ⅰ 추진 배경', 0, 0), b(1, 1), host(mid, 2), T('□ 첫 문장', 0, 4), b(1, 3), T('ㅇ 항목', 0, 0), b(1, 3), b(4, 2), T('ㅇ 둘째 항목', 0, 0),
                 T('- 하위', 0, 0), b(1, 3), host(att, 2), b(2, 1), host(plain, 3), b(1, 2)]
        if variant == 'after':
            items = [host(mid, 2), T('□ 첫 문장', 0, 4), T('ㅇ 항목', 0, 0), host(mid, 2), b(1, 1), T('ㅇ 항목', 0, 0), b(1, 3)]
        return sec(*items)

    examples = {}
    for variant, reg in (('basic', False), ('registered', True), ('newfont', False), ('after', False)):
        hx = ex_header(variant)
        sx = ex_section(variant)
        prof = learn_blank(hx, [sx], reg)
        examples[variant] = prof
        out[f'blk_example_{variant}'] = (pack(h, sx, hx), [('blank_lines', {'blank_line_profile': prof})])

    def targets(variant):
        prof = examples[variant]
        hdr_same = ex_header(variant)
        hdr_tgt = mk_header([C(1000, 0), C(1000, 1), C(1500, 3), C(1000, 2), C(900, 4)], [PP(), PP('LEFT'), PP('RIGHT', prev=100)])  # 서식이 다른 대상
        b = lambda cid=0, pp=0: blank(cid, pp)
        docs = {
            'same': (sec(*(ex_section(variant) if False else [b(), host(title), b(), host(box), b(), T('□ 문장'), b(), T('ㅇ 항목'), b(), b(), T('- 하위'), b()])), hdr_tgt),
            'fewer': (sec(T('□ 문장'), b(), T('ㅇ 항목')), hdr_tgt),
            'long_blank_run': (sec(T('ㅇ 항목'), b(), b(), b(), b(), T('ㅇ 둘째')), hdr_tgt),
            'no_blank_no_table': (sec(T('ㅇ 항목'), T('ㅇ 둘째')), hdr_tgt),
            'tables_only': (sec(host(title), host(box), host(mid), host(att), host(plain)), hdr_tgt),
            'blank_between_tables': (sec(host(title), b(), host(box), b(), host(mid), b(), host(att), b(), host(plain), b()), hdr_tgt),
            'mid_after': (sec(host(mid), T('□ 첫 문장'), b(), T('ㅇ 항목'), host(mid), b(), T('□ 다른 문장')), hdr_tgt),
            'two_sections': ({'Contents/section0.xml': sec(b(), host(title), b()), 'Contents/section1.xml': sec(T('ㅇ 항목'), b(), T('ㅇ 둘째'), b())}, hdr_tgt),
            'section_order': ({'Contents/section10.xml': sec(b(), T('ㅇ 열')), 'Contents/section2.xml': sec(T('ㅇ 둘'), b(), host(box))}, hdr_tgt),
            'blank_with_space_text': (sec(T('ㅇ 항목'), blank(0, 0, text='   '), T('ㅇ 둘째')), hdr_tgt),
            'blank_two_runs': (sec(T('ㅇ 항목'), P([R(0, ''), R(1, '')]), T('ㅇ 둘째')), hdr_tgt),
            'blank_no_run': (sec(T('ㅇ 항목'), '<hp:p paraPrIDRef="0"></hp:p>', T('ㅇ 둘째')), hdr_tgt),
            'object_para': (sec(T('ㅇ 항목'), host('<hp:pic/>'), b(), T('ㅇ 둘째')), hdr_tgt),
            'same_header_as_example': (sec(b(), T('ㅇ 항목'), b(), host(title), b()), hdr_same),
            'no_fontfaces': (sec(b(), T('ㅇ 항목'), b()), mk_header([C()], [PP()], with_fonts=False)),
            'no_border_tab': (sec(b(), host(title), b(), T('ㅇ 항목'), b()), hdr_tgt.replace('<hh:borderFills', '<hh:xborderFills').replace('</hh:borderFills>', '</hh:xborderFills>').replace('<hh:tabProperties', '<hh:xtabProperties').replace('</hh:tabProperties>', '</hh:xtabProperties>')),
            'no_char_group': (sec(b(), T('ㅇ 항목'), b()), mk_header([C()], [PP()], with_char_group=False)),
            'no_para_group': (sec(b(), T('ㅇ 항목'), b()), mk_header([C()], [PP()], with_para_group=False)),
            'para_noid': (sec(b(), T('ㅇ 항목'), b()), mk_header([C()], [PP()]).replace('<hh:paraPr id="0"', '<hh:paraPr')),
            'bad_table': (sec(host('<hp:tbl rowCnt="x" colCnt="1"><hp:tr><hp:tc><hp:subList/></hp:tc></hp:tr></hp:tbl>'), b()), hdr_tgt),
            'missing_para_pr': (sec(b(1, 9), T('ㅇ 항목', 0, 9), b(1, 9)), hdr_tgt),
            'noid_para_ref': (sec(b(), '<hp:p><hp:run charPrIDRef="0"><hp:t></hp:t></hp:run></hp:p>'), hdr_tgt),
        }
        return docs

    k = 0
    for variant in ('basic', 'registered', 'newfont', 'after'):
        for name, (secs, hdr) in targets(variant).items():
            if variant in ('registered', 'after') and name in ('no_fontfaces', 'no_border_tab', 'no_char_group', 'no_para_group', 'para_noid', 'bad_table', 'missing_para_pr', 'noid_para_ref', 'blank_two_runs', 'blank_no_run', 'object_para', 'blank_with_space_text'):
                continue
            out[f'blk_{variant}_{name}'] = (pack(h, secs, hdr), [('blank_lines', {'blank_line_profile': examples[variant]})])
            k += 1
    # 프로필 없음(조사 결과 빈 목록 → 원본 복사)·빈 프로필·슬롯 없는 프로필·다른 처리기 설정 반영
    sample = pack(h, sec(blank(0, 0), T('ㅇ 항목'), blank(0, 0)), ex_header('basic'))
    out['blk_no_profile'] = (sample, [('blank_lines', None)])
    out['blk_empty_profile'] = (sample, [('blank_lines', {'blank_line_profile': {}})])
    out['blk_version_only_profile'] = (sample, [('blank_lines', {'blank_line_profile': {'version': 1}})])
    prof = examples['basic']
    out['blk_scheme_on'] = (sample, [('blank_lines', {'blank_line_profile': prof, 'marker_scheme_active': True})])
    out['blk_after_only'] = (pack(h, sec(host(mid), T('□ 첫 문장'), T('ㅇ 항목')), ex_header('basic')), [('blank_lines', {'blank_line_profile': {'version': 1, 'slots': {}, 'after': {'after|midtitle': 123}}})])
    out['blk_after_same_value'] = (pack(h, sec(host(mid), T('□ 첫 문장', 0, 1)), ex_header('basic')), [('blank_lines', {'blank_line_profile': {'version': 1, 'slots': {}, 'after': {'after|midtitle': 0}}})])
    out['blk_after_default_double'] = (pack(h, sec(host(mid), T('□ 첫 문장', 0, 2), host(mid), T('ㅇ 둘', 0, 2)), ex_header('basic')), [('blank_lines', {'blank_line_profile': {'version': 1, 'slots': {}, 'after': {'after|midtitle': 450}}})])

    # ------------------------------------------------------------ 강조 낱말
    def emp_header(newfont=False):
        chars = [C(1000, 0), C(1000, 0, bold=True), C(1000, 4, bold=True), C(1200, 0, bold=True), C(1000, 0, color='#0000FF'), C(1000, 3, color='#FF0000'), C(1400, 0)]
        return mk_header(chars, [PP()])

    sentences = ['사업 개요: 중요 부분은 진하게 표시', '핵심 과제는 한컴돋움으로 강조', '파란색 글자 예시 문장', '두 번 나오는 낱말 낱말 예시', '전체가 붉은 문장']
    ex_paras = [
        P([R(0, '□ 사업 개요: 중요 부분은 '), R(1, '진하게'), R(0, ' 표시')]),
        P([R(0, 'ㅇ 핵심 과제는 '), R(2, '한컴돋움'), R(0, '으로 강조')]),
        P([R(0, '- 파란색 '), R(4, '글자'), R(0, ' 예시 문장')]),
        P([R(0, 'ㅇ 두 번 나오는 낱말 '), R(3, '낱말'), R(0, ' 예시')]),
        P([R(5, '※ 전체가 붉은 문장')]),
        P([R(0, '* 크게 '), R(6, '보이게'), R(0, ' 한다')]),
    ]
    ehdr = emp_header()
    esec = sec(*ex_paras)
    eprof = emphasis.learn(parse(ehdr), [parse(esec)])
    out['emp_example'] = (pack(h, esec, ehdr), [('emphasis', {'emphasis_words': eprof})])

    def tgt(*paras_xml, header=None, secs=None):
        return pack(h, secs if secs is not None else sec(*paras_xml), header or ehdr)
    E = 'emphasis'
    cfg = {'emphasis_words': eprof}
    docs = {
        'plain_same': tgt(T('□ 사업 개요: 중요 부분은 진하게 표시', 0), T('ㅇ 핵심 과제는 한컴돋움으로 강조', 0), T('- 파란색 글자 예시 문장', 0), T('ㅇ 두 번 나오는 낱말 낱말 예시', 0), T('※ 전체가 붉은 문장', 0), T('* 크게 보이게 한다', 0)),
        'one_para': tgt(T('□ 사업 개요: 중요 부분은 진하게 표시', 0)),
        'extra_spaces': tgt(T('□  사업  개요:  중요 부분은   진하게 표시', 0)),
        'other_marker': tgt(T('ㅇ 사업 개요: 중요 부분은 진하게 표시', 0)),
        'no_marker': tgt(T('사업 개요: 중요 부분은 진하게 표시', 0)),
        'repeated_para': tgt(T('□ 사업 개요: 중요 부분은 진하게 표시', 0), T('□ 사업 개요: 중요 부분은 진하게 표시', 1)),
        'split_runs': tgt(P([R(0, '□ 사업 개요: 중요 '), R(0, '부분은 진하게'), R(1, ' 표시')])),
        'split_inside_word': tgt(P([R(0, '□ 사업 개요: 중요 부분은 진'), R(0, '하게 표시')])),
        'already_bold_run': tgt(P([R(0, '□ 사업 개요: 중요 부분은 '), R(1, '진하게'), R(0, ' 표시')])),
        'run_with_ctrl': tgt(P(['<hp:run charPrIDRef="0"><hp:ctrl><hp:colPr/></hp:ctrl><hp:t>□ 사업 개요: 중요 부분은 진하게 표시</hp:t></hp:run>'])),
        'tab_in_t': tgt(P(['<hp:run charPrIDRef="0"><hp:t>□ 사업 개요: 중요 부분은<hp:tab/> 진하게 표시</hp:t></hp:run>'])),
        'two_t_in_run': tgt(P(['<hp:run charPrIDRef="0"><hp:t>□ 사업 개요: 중요 부분은 </hp:t><hp:t>진하게 표시</hp:t></hp:run>'])),
        'empty_run_present': tgt(P([R(0, '□ 사업 개요: 중요 부분은 진하게 표시'), '<hp:run charPrIDRef="0"/>'])),
        'ctrl_only_run': tgt(P(['<hp:run charPrIDRef="0"><hp:ctrl/></hp:run>', R(0, '□ 사업 개요: 중요 부분은 진하게 표시')])),
        'color_whole': tgt(T('※ 전체가 붉은 문장', 0), T('※ 다른 문장도 같은 계층', 0)),
        'color_part': tgt(T('- 파란색 글자 예시 문장', 0)),
        'color_other_group': tgt(T('ㅇ 파란색 글자 예시 문장', 0)),
        'bigger_delta': tgt(T('* 크게 보이게 한다', 2)),
        'duplicate_word': tgt(T('ㅇ 두 번 나오는 낱말 낱말 예시', 0)),
        'font_only': tgt(T('ㅇ 핵심 과제는 한컴돋움으로 강조', 0)),
        'base_cid_missing': tgt(T('□ 사업 개요: 중요 부분은 진하게 표시', 99)),
        'base_cid_noid': tgt(P([R(None, '□ 사업 개요: 중요 부분은 진하게 표시')])),
        'two_sections': tgt(secs={'Contents/section0.xml': sec(T('□ 사업 개요: 중요 부분은 진하게 표시', 0)), 'Contents/section1.xml': sec(T('ㅇ 핵심 과제는 한컴돋움으로 강조', 3))}),
        'section_order': tgt(secs={'Contents/section10.xml': sec(T('□ 사업 개요: 중요 부분은 진하게 표시', 0)), 'Contents/section2.xml': sec(T('ㅇ 핵심 과제는 한컴돋움으로 강조', 3))}),
        'no_char_group': tgt(T('□ 사업 개요: 중요 부분은 진하게 표시', 0), header=mk_header([C()], [PP()], with_char_group=False)),
        'no_fonts_header': tgt(T('ㅇ 핵심 과제는 한컴돋움으로 강조', 0), header=mk_header([C(), C(bold=True), C(bold=True)], [PP()], with_fonts=False)),
        'fonts_missing_face': tgt(T('ㅇ 핵심 과제는 한컴돋움으로 강조', 0), header=emp_header().replace('<hh:fontface lang="HANGUL"', '<hh:fontface lang="XHANGUL"')),
        'fontface_empty': tgt(T('ㅇ 핵심 과제는 한컴돋움으로 강조', 0), header=mk_header([C(), C(bold=True), C(bold=True)], [PP()], faces=[])),
        'new_font_added': tgt(T('ㅇ 핵심 과제는 한컴돋움으로 강조', 0), header=mk_header([C(), C(bold=True), C(bold=True), C(), C(), C(), C()], [PP()], faces=['함초롬바탕', '함초롬돋움', '휴먼명조'])),
        'height_missing': tgt(T('* 크게 보이게 한다', 0), header=ehdr.replace('height="1000"', 'height=""', 1)),
        'scheme_on': tgt(T('□ 사업 개요: 중요 부분은 진하게 표시', 0)),
        'tables_ignored': tgt(h.one('') and '<hp:p><hp:run charPrIDRef="0"><hp:tbl rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:subList>' + T('□ 사업 개요: 중요 부분은 진하게 표시', 0) + '</hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>'),
        'empty_doc': tgt(),
        'space_only_paragraph': tgt(T('   ', 0), T('□ 사업 개요: 중요 부분은 진하게 표시', 0)),
        'astral_before': tgt(T('□ 𠀀 사업 개요: 중요 부분은 진하게 표시', 0)),
    }
    for name, data in docs.items():
        c = dict(cfg)
        if name == 'scheme_on':
            c['marker_scheme_active'] = True
        out[f'emp_{name}'] = (data, [(E, c)])
    # 손으로 정한 프로필(학습기가 만들지 않는 모양·가장자리)
    hand = [
        ('bold_off', [{'marker': '□', 'text': '개요', 'font': None, 'bold': False, 'size_delta': 0, 'fonts': {}}], '□ 사업 개요 입니다'),
        ('bold_on_plain', [{'marker': '□', 'text': '개요', 'font': None, 'bold': True, 'size_delta': 0, 'fonts': {}}], '□ 사업 개요 입니다'),
        ('size_neg_clamp', [{'marker': '□', 'text': '개요', 'bold': True, 'size_delta': -5000}], '□ 사업 개요 입니다'),
        ('size_float', [{'marker': '□', 'text': '개요', 'bold': False, 'size_delta': 12.9}], '□ 사업 개요 입니다'),
        ('color_whole_hand', [{'kind': 'color', 'marker': '', 'text': '전체 문장', 'color': '#123456', 'whole': True}], '전체 문장'),
        ('color_part_nomarker', [{'kind': 'color', 'marker': '', 'text': '부분', 'color': '#654321', 'whole': False}], '글 부분 글'),
        ('context_mismatch', [{'marker': '□', 'text': '개요', 'bold': True, 'size_delta': 0, 'context': 'xxx'}], '□ 사업 개요 입니다'),
        ('overlap', [{'marker': '□', 'text': '사업 개요', 'bold': True, 'size_delta': 0}, {'marker': '□', 'text': '개요 입', 'bold': False, 'size_delta': 100}], '□ 사업 개요 입니다'),
        ('word_in_marker', [{'marker': '□', 'text': '□', 'bold': True, 'size_delta': 0}], '□ 사업 개요 입니다'),
        ('whole_with_spaces', [{'kind': 'color', 'marker': '□', 'text': '□ 사업', 'color': '#111111', 'whole': True}], '  □ 사업  '),
        ('font_new_face', [{'marker': '□', 'text': '개요', 'font': '새글꼴', 'bold': False, 'size_delta': 0, 'fonts': {'hangul': '<ns0:font xmlns:ns0="http://www.hancom.co.kr/hwpml/2011/head" id="0" face="새글꼴" type="TTF" isEmbedded="0" />', 'LATIN': '<ns0:font xmlns:ns0="http://www.hancom.co.kr/hwpml/2011/head" id="0" face="새글꼴" type="TTF" isEmbedded="0" />'}}], '□ 사업 개요 입니다'),
        ('font_same_face', [{'marker': '□', 'text': '개요', 'font': '함초롬돋움', 'bold': True, 'size_delta': 0, 'fonts': {'hangul': '<hh:font xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" id="9" face="함초롬돋움" type="TTF" isEmbedded="0" />'}}], '□ 사업 개요 입니다'),
        ('font_bad_xml', [{'marker': '□', 'text': '개요', 'font': 'x', 'bold': False, 'size_delta': 0, 'fonts': {'hangul': '<font'}}], '□ 사업 개요 입니다'),
        ('font_flag_without_fonts', [{'marker': '□', 'text': '개요', 'font': 'x', 'bold': False, 'size_delta': 0}], '□ 사업 개요 입니다'),
    ]
    for name, entries, text in hand:
        out[f'emp_hand_{name}'] = (pack(h, sec(T(text, 0)), ehdr), [(E, {'emphasis_words': entries})])
    out['emp_no_profile'] = (pack(h, sec(T('□ 사업 개요: 중요 부분은 진하게 표시', 0)), ehdr), [(E, None)])
    out['emp_hand_text_missing'] = (pack(h, sec(T('□ 사업 개요', 0)), ehdr), [(E, {'emphasis_words': [{'marker': '□'}]})])
    # 무작위: 예시 학습 프로필을 임의 문장 문서에 적용
    rnd = random.Random(20261126)
    pool = sentences + ['아무 관계 없는 문장', '□ 사업 개요: 중요 부분은 진하게 표시']
    marks = ['□ ', 'ㅇ ', '- ', '* ', '※ ', '']
    for i in range(10):
        paras = []
        for _ in range(rnd.randint(2, 6)):
            text = rnd.choice(marks) + rnd.choice(pool)
            if rnd.random() < 0.4:
                cut = rnd.randint(0, len(text))
                paras.append(P([R(rnd.randrange(7), text[:cut]), R(rnd.randrange(7), text[cut:])]))
            else:
                paras.append(T(text, rnd.randrange(7)))
        out[f'emp_random_{i:02d}'] = (pack(h, sec(*paras), ehdr), [(E, cfg)])
    # 빈 줄 서식 무작위(예시 프로필 적용)
    for i in range(8):
        paras = []
        for _ in range(rnd.randint(3, 9)):
            c = rnd.random()
            if c < 0.35:
                paras.append(blank(rnd.randrange(5), rnd.randrange(5)))
            elif c < 0.5:
                paras.append(host(rnd.choice([title, box, mid, att, plain]), rnd.randrange(5)))
            else:
                paras.append(T(rnd.choice(['□ 제목', 'ㅇ 항목', '- 하위', '일반 글', 'Ⅰ 중제목', '1. 번호', '* 주석']), rnd.randrange(5), rnd.randrange(5)))
        v = rnd.choice(['basic', 'registered', 'newfont', 'after'])
        out[f'blk_random_{i:02d}'] = (pack(h, sec(*paras), ex_header(v)), [('blank_lines', {'blank_line_profile': examples[v]})])
    return out
