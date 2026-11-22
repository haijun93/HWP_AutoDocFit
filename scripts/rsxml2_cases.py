"""RSXML2 합성 HWPX 생성기: 개요 첫 빈칸(overview_lead)·쪽 번호(page_number)·자간 초기화(spacing_reset).

`build(h)` 의 h 는 scripts/rsxml_parity.py 모듈(합성 도우미 t/para/tc/tbl/grid/host/section/hwpx_bytes/one 사용).
반환: {이름: (HWPX 바이트, [(처리기, 설정 이름)])}. 설정 이름은 CONFIGS 의 키(None 이면 설정 없음).
사용자 문서는 쓰지 않는다(전부 합성 문자열).
"""
from __future__ import annotations

import random

PAGE = {'pos': 'BOTTOM_CENTER', 'formatType': 'DIGIT', 'sideChar': '-'}
CONFIGS = {
    'page_std': {'page_number': PAGE},
    'page_pos_only': {'page_number': {'pos': 'TOP_RIGHT'}},
    'page_new_attr': {'page_number': {'pos': 'BOTTOM_CENTER', 'newAttr': 'x', 'sideChar': ''}},
    'page_special': {'page_number': {'pos': 'A&B<C>"D\'', 'sideChar': '–　'}},
    'page_empty': {'page_number': {}},
    'page_roman': {'page_number': {'pos': 'BOTTOM_LEFT', 'formatType': 'ROMAN_CAPITAL', 'sideChar': '~'}},
}

SPACING_ATTRS = ('hangul', 'latin', 'hanja', 'japanese', 'other', 'symbol', 'user')


def header_with_spacings(spacings, ids=None, group=True, extra_attr=''):
    """spacings: 글자 모양별 자간 값(정수 하나 또는 {언어: 값} 사전 또는 None=spacing 요소 없음)."""
    ids = ids if ids is not None else list(range(len(spacings)))
    items = []
    for i, sp in zip(ids, spacings):
        if sp is None:
            body = '<hh:fontRef hangul="0"/>'
        else:
            vals = sp if isinstance(sp, dict) else {k: sp for k in SPACING_ATTRS}
            body = '<hh:fontRef hangul="0"/><hh:spacing ' + ' '.join(f'{k}="{v}"' for k, v in vals.items()) + '/>'
        idattr = '' if i is None else f' id="{i}"'
        items.append(f'<hh:charPr{idattr} height="1000">{body}</hh:charPr>')
    inner = ''.join(items)
    if group:
        inner = f'<hh:charProperties itemCnt="{len(items)}"{extra_attr}>{inner}</hh:charProperties>'
    return (f'<?xml version="1.0" encoding="UTF-8"?><hh:head {NSH}><hh:refList>{inner}</hh:refList></hh:head>')


NSH = ('xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" '
       'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section"')


def build(h):
    cases = {}
    t, para, tc, tbl, grid, host, section, hwpx_bytes, one = h.t, h.para, h.tc, h.tbl, h.grid, h.host, h.section, h.hwpx_bytes, h.one

    # ------------------------------------------------------------ 개요 표 첫 빈칸
    LONG = '개요 본문은 열다섯 글자를 넘는 한 칸 상자 글입니다'
    title1, title3 = h.title_table_type1(), h.title_table_type3()

    def box(*paras, raw=None):
        sub = raw if raw is not None else ''.join(para(p) for p in paras)
        return tbl([[tc('', 0, 0, raw_sub=sub)]], rowcnt=1, colcnt=1)

    def sec(*tables_or_xml):
        return one(''.join(tables_or_xml))

    def pt(*tables):
        return host(*tables)

    ov = {}
    ov['ov_basic'] = one(pt(title1, box(LONG)))
    ov['ov_title3_box'] = one(pt(title3, box(LONG)))
    ov['ov_no_title'] = one(pt(box(LONG)))
    ov['ov_text_between'] = one('<hp:p><hp:run><hp:t>일반 글</hp:t></hp:run></hp:p>'.join([pt(title1), pt(box(LONG))]))
    ov['ov_empty_para_between'] = one(pt(title1) + '<hp:p><hp:run><hp:t>   </hp:t></hp:run></hp:p>' + pt(box(LONG)))
    ov['ov_box_after_box'] = one(pt(title1, box(LONG), box(LONG + ' 둘째')))
    ov['ov_two_titles'] = one(pt(title1, box(LONG), title3, box('둘째 상자 본문도 열다섯 글자를 넘어갑니다')))
    ov['ov_leading_space'] = one(pt(title1, box('  ' + LONG)))
    ov['ov_leading_ideographic_space'] = one(pt(title1, box('　' + LONG)))
    ov['ov_leading_nbsp'] = one(pt(title1, box(' ' + LONG)))
    ov['ov_leading_tab'] = one(pt(title1, box('\t' + LONG)))
    ov['ov_multi_para'] = one(pt(title1, box(LONG + ' 첫째', ' ' + LONG + ' 둘째', '', '   ', LONG + ' 다섯째')))
    ov['ov_t_empty_then_text'] = one(pt(title1, box(None, raw=para(None, raw=t('') + t(LONG)))))
    ov['ov_t_space_then_text_skipped'] = one(pt(title1, box(None, raw=para(None, raw=t('  ') + t(LONG)))))
    ov['ov_t_child_fwspace'] = one(pt(title1, box(None, raw=para(None, raw='<hp:t>' + LONG + '<hp:fwSpace/>끝</hp:t>'))))
    ov['ov_t_child_first'] = one(pt(title1, box(None, raw=para(None, raw='<hp:t><hp:tab/>' + LONG + '</hp:t>'))))
    ov['ov_two_runs'] = one(pt(title1, box(None, raw='<hp:p><hp:run><hp:t>' + LONG[:8] + '</hp:t></hp:run><hp:run><hp:t>' + LONG[8:] + '</hp:t></hp:run></hp:p>')))
    ov['ov_first_run_space_only'] = one(pt(title1, box(None, raw='<hp:p><hp:run><hp:t> </hp:t></hp:run><hp:run><hp:t>' + LONG + '</hp:t></hp:run></hp:p>')))
    ov['ov_linesegarray_removed'] = one(pt(title1, box(None, raw='<hp:p><hp:run><hp:t>' + LONG + '</hp:t></hp:run>'
                                                       '<hp:linesegarray><hp:lineseg textpos="0" vertpos="0"/></hp:linesegarray>\n</hp:p>')))
    ov['ov_linesegarray_in_skipped_para'] = one(pt(title1, box(None, raw='<hp:p><hp:run><hp:t> ' + LONG + '</hp:t></hp:run>'
                                                               '<hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p>')))
    ov['ov_title_and_box_same_paragraph'] = one('<hp:p><hp:run>' + title1 + box(LONG) + '</hp:run></hp:p>')
    ov['ov_title_in_ctrl_not_run_child'] = one('<hp:p><hp:run><hp:ctrl>' + title1 + '</hp:ctrl></hp:run></hp:p>' + pt(box(LONG)))
    ov['ov_multi_section'] = hwpx_bytes({
        'Contents/section0.xml': section(pt(title1, box(LONG))),
        'Contents/section1.xml': section(pt(box(LONG))),
        'Contents/section2.xml': section(pt(title3, box(LONG + ' 셋째 구역'))),
    })
    ov['ov_bullet_start_not_box'] = one(pt(title1, box('※ ' + LONG)))
    ov['ov_short_not_box'] = one(pt(title1, box('짧은 개요 상자')))
    ov['ov_box_exactly_15_chars'] = one(pt(title1, box('열다섯글자짜리상자입니다요요')))
    ov['ov_box_14_chars'] = one(pt(title1, box('열네글자짜리상자입니다요')))
    ov['ov_attach_start_not_box'] = one(pt(title1, box('붙임 ' + LONG)))
    ov['ov_special_chars'] = one(pt(title1, box('&amp; 표시 &lt;꺽쇠&gt; 와 &quot;따옴표&quot; 가 들어간 개요 본문'.replace('&quot;', '"'))))
    ov['ov_first_char_emoji'] = one(pt(title1, box('😀 이모지로 시작하는 개요 본문 입니다 열다섯 글자 이상')))
    ov['ov_first_char_latin'] = one(pt(title1, box('ABC overview text longer than fifteen chars')))
    ov['ov_box_with_nested_table'] = one(pt(title1, box(None, raw=para(LONG) + '<hp:p><hp:run>' + grid([['x', 'y']]) + '</hp:run></hp:p>')))
    ov['ov_non_p_top_level'] = one('<hp:secPr/>' + pt(title1, box(LONG)))
    ov['ov_title_then_midtitle_then_box'] = one(pt(title1, grid([['Ⅰ.', '현황']]), box(LONG)))
    ov['ov_title_then_attach_then_box'] = one(pt(title1, grid([['붙임 1', '서류']]), box(LONG)))
    ov['ov_pretty_printed'] = one('\n <hp:p>\n  <hp:run>\n' + title1 + '\n  </hp:run>\n </hp:p>\n <hp:p>\n  <hp:run>\n' + box(LONG) + '\n  </hp:run>\n </hp:p>\n')
    ov['ov_prefix_variant'] = hwpx_bytes({'Contents/section0.xml': section(pt(title1, box(LONG))).replace('hp:', 'zz:').replace('xmlns:hp=', 'xmlns:zz=')})
    ov['ov_text_after_box_ignored'] = one(pt(title1, box(LONG)) + '<hp:p><hp:run><hp:t>뒷글</hp:t></hp:run></hp:p>' + pt(box(LONG + ' 다시')))
    ov['ov_no_target_all_skipped_copy'] = one(pt(title1, box(' ' + LONG)))
    for k, v in ov.items():
        cases[k] = (v, [('overview_lead', None)])

    # ------------------------------------------------------------ 쪽 번호 모양
    def pn(attrs=None, tag='hp:pageNum'):
        a = ' '.join(f'{k}="{v}"' for k, v in (attrs or {}).items())
        return f'<{tag} {a}/>' if a else f'<{tag}/>'

    def ctrl_p(*xmls):
        return '<hp:p><hp:run><hp:ctrl>' + ''.join(xmls) + '</hp:ctrl></hp:run></hp:p>'

    pg = {}
    pg['pn_none'] = (one('<hp:p><hp:run><hp:t>본문</hp:t></hp:run></hp:p>'), 'page_std')
    pg['pn_already_same'] = (one(ctrl_p(pn(PAGE))), 'page_std')
    pg['pn_pos_differs'] = (one(ctrl_p(pn({**PAGE, 'pos': 'TOP_LEFT'}))), 'page_std')
    pg['pn_missing_attr'] = (one(ctrl_p(pn({'pos': 'BOTTOM_CENTER', 'formatType': 'DIGIT'}))), 'page_std')
    pg['pn_bare_element'] = (one(ctrl_p(pn())), 'page_std')
    pg['pn_two_in_section_one_same'] = (one(ctrl_p(pn(PAGE), pn({**PAGE, 'sideChar': ''}))), 'page_std')
    pg['pn_extra_attrs_kept'] = (one(ctrl_p(pn({'pos': 'TOP_LEFT', 'flag': '1', 'formatType': 'ROMAN_SMALL'}))), 'page_std')
    pg['pn_multi_section'] = (hwpx_bytes({
        'Contents/section0.xml': section(ctrl_p(pn({'pos': 'X'}))),
        'Contents/section1.xml': section('<hp:p/>'),
        'Contents/section2.xml': section(ctrl_p(pn(PAGE))),
        'Contents/section10.xml': section(ctrl_p(pn({'pos': 'Y'}), pn({'pos': 'Z'}))),
    }), 'page_std')
    pg['pn_in_table_cell'] = (one(host(tbl([[tc('', 0, 0, raw_sub=para(None, raw=pn({'pos': 'LEFT'})))]]))), 'page_std')
    pg['pn_namespace_prefix_other'] = (hwpx_bytes({'Contents/section0.xml': section(ctrl_p(pn({'pos': 'LEFT'}))).replace('hp:', 'qq:').replace('xmlns:hp=', 'xmlns:qq=')}), 'page_std')
    pg['pn_with_child'] = (one('<hp:p><hp:run><hp:pageNum pos="A"><hp:x/>자식</hp:pageNum>뒷글</hp:run></hp:p>'), 'page_std')
    pg['pn_similar_tags_ignored'] = (one(ctrl_p(pn({'pos': 'A'}, 'hp:pageNumCtrl'), pn({'pos': 'A'}, 'hp:pagenum'), pn({'pos': 'A'}, 'hp:pageHiding'))), 'page_std')
    pg['pn_many'] = (one(ctrl_p(*[pn({'pos': f'P{i}', 'formatType': 'DIGIT' if i % 3 else 'X'}) for i in range(60)])), 'page_std')
    pg['pn_pos_only_cfg'] = (one(ctrl_p(pn({'pos': 'TOP_RIGHT'}), pn({'pos': 'BOTTOM'}))), 'page_pos_only')
    pg['pn_new_attr_cfg'] = (one(ctrl_p(pn({'sideChar': 'q', 'pos': 'z'}))), 'page_new_attr')
    pg['pn_new_attr_cfg_already_same'] = (one(ctrl_p(pn({'pos': 'BOTTOM_CENTER', 'newAttr': 'x', 'sideChar': ''}))), 'page_new_attr')
    pg['pn_special_values_cfg'] = (one(ctrl_p(pn({'pos': 'old'}))), 'page_special')
    pg['pn_empty_cfg_copy'] = (one(ctrl_p(pn({'pos': 'old'}))), 'page_empty')
    pg['pn_roman_cfg'] = (one(ctrl_p(pn({'pos': 'BOTTOM_LEFT', 'formatType': 'DIGIT', 'sideChar': '~'}))), 'page_roman')
    pg['pn_entities_in_old_attr'] = (one(ctrl_p('<hp:pageNum pos="a&amp;b&#10;c" sideChar="&quot;"/>')), 'page_std')
    pg['pn_tail_text_kept'] = (one('<hp:p><hp:run><hp:ctrl><hp:pageNum pos="A"/>꼬리글</hp:ctrl></hp:run></hp:p>'), 'page_std')
    pg['pn_nested_deep'] = (one('<hp:p><hp:run><hp:a><hp:b><hp:c><hp:pageNum pos="deep"/></hp:c></hp:b></hp:a></hp:run></hp:p>'), 'page_std')
    pg['pn_decl_variant'] = (one(ctrl_p(pn({'pos': 'A'})), decl='<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>'), 'page_std')
    pg['pn_pretty_printed'] = (one('\n<hp:p>\n <hp:run>\n  <hp:ctrl>\n   <hp:pageNum pos="A"/>\n  </hp:ctrl>\n </hp:run>\n</hp:p>\n'), 'page_std')
    pg['pn_binary_and_dirs_preserved'] = (hwpx_bytes({'Contents/section0.xml': section(ctrl_p(pn({'pos': 'A'})))},
                                                    extra_files={'BinData/a.png': bytes(range(256)) * 20, 'Contents/content.hpf': b'<?xml version="1.0"?><x/>'},
                                                    dir_entries=('BinData/',)), 'page_std')
    pg['pn_other_contents_xml_malformed_rejected'] = (hwpx_bytes({'Contents/section0.xml': section(ctrl_p(pn({'pos': 'A'})))},
                                                                extra_files={'Contents/extra.xml': b'<a><b></a>'}), 'page_std')
    pg['pn_other_contents_xml_ok_untouched'] = (hwpx_bytes({'Contents/section0.xml': section(ctrl_p(pn({'pos': 'A'})))},
                                                          extra_files={'Contents/extra.xml': '<a>\n<b>한글</b></a>'.encode('utf-8')}), 'page_std')
    pg['pn_two_sections_both_same_copy'] = (hwpx_bytes({'Contents/section0.xml': section(ctrl_p(pn(PAGE))),
                                                        'Contents/section1.xml': section(ctrl_p(pn(PAGE)))}), 'page_std')
    pg['pn_section_not_name_ignored'] = (hwpx_bytes({'Contents/section0.xml': section('<hp:p/>'),
                                                     'Contents/sectionX.xml': section(ctrl_p(pn({'pos': 'A'})))}), 'page_std')
    pg['pn_unicode_value_compare'] = (one(ctrl_p(pn({'pos': 'BOTTOM_CENTER', 'formatType': 'DIGIT', 'sideChar': '-​'}))), 'page_std')
    pg['pn_cdata_text_around'] = (one('<hp:p><hp:run><hp:t><![CDATA[쪽]]></hp:t><hp:pageNum pos="A"/></hp:run></hp:p>'), 'page_std')
    for k, (v, cfgname) in pg.items():
        cases[k] = (v, [('page_number', cfgname)])

    # ------------------------------------------------------------ 자간 초기화
    def run(text, cid=1, raw=None, attrs=''):
        body = raw if raw is not None else (t(text) if text is not None else '')
        return f'<hp:run charPrIDRef="{cid}"{attrs}>{body}</hp:run>'

    def p(*runs, extra=''):
        return '<hp:p paraPrIDRef="1">' + ''.join(runs) + extra + '</hp:p>'

    SP = header_with_spacings([0, -5, {'hangul': '0', 'latin': '3', 'hanja': '0'}, '', None, 0], ids=[0, 1, 2, 3, 4, 5])
    HDR = SP
    sp = {}

    def spc(name, body, header=HDR, **kw):
        sp[name] = hwpx_bytes({'Contents/section0.xml': section(body)}, header=header, **kw)

    spc('sp_no_spacing_copy', p(run('본문 글', 0), run('둘째', 5)))
    spc('sp_all_plain_text', p(run('본문 글입니다')))
    spc('sp_two_runs_same_char', p(run('본문 글'), run('둘째 글')))
    spc('sp_diff_spacings', p(run('가', 1), run('나', 2), run('다', 3), run('라', 4)))
    spc('sp_marker_box_split', p(run('□ 항목 본문')))
    spc('sp_marker_separate_runs', p(run('□ '), run('본문 글')))
    spc('sp_marker_circle_split', p(run('ㅇ 항목 본문 입니다')))
    spc('sp_marker_dash', p(run('- 하위 항목')))
    spc('sp_run_with_child_straddles_skipped', p(run(None, raw='<hp:t>□ 본문<hp:fwSpace/>끝</hp:t>')))
    spc('sp_run_two_t_straddles_skipped', p(run(None, raw=t('□ ') + t('본문'))))
    spc('sp_paren_label', p(run('ㅇ (라벨) 본문 글')))
    spc('sp_paren_label_colon', p(run('- (방식) : 본문')))
    spc('sp_colon_label', p(run('- 일시: 2026. 10. 10. 본문')))
    spc('sp_label_only_whole_protected', p(run('□ (운영방식)')))
    spc('sp_label_only_colon', p(run('ㅇ (운영방식) :')))
    spc('sp_marker_only', p(run('□')))
    spc('sp_marker_only_trailing_space', p(run('□   ')))
    spc('sp_number_markers', p(run('1. 가나다'), run('가. 라마바'), run('1) 사아자'), run('(1) 차카타'), run('(가) 파하'), run('Ⅰ. 로마')))
    spc('sp_numbers_as_paragraphs', ''.join(p(run(s)) for s in ['1. 가나다', '가. 라마바', '1) 사아자', '(1) 차카타', '(가) 파하', 'Ⅰ. 로마', '1-1. 하위', '1.1. 하위', 'A. 영문', 'a) 영문', 'IV. 로마', 'I. 로마']))
    spc('sp_note_markers', ''.join(p(run(s)) for s in ['※ 참고 사항', '* 별표 글', '** 별표 둘', '＊ 전각 별표', '☞ 손가락', '◦ 작은 원', '• 점 글', '· 가운뎃점']))
    spc('sp_open_quote_skip', ''.join(p(run(s)) for s in ['ㅇ 「법령」을 따름', '□ 『도서』 참고', '- 「 공백 낫표']))
    spc('sp_date_leading', ''.join(p(run(s)) for s in ['2026. 10. 10. 행사', '2026.10.10. 행사', '26. 10. 10.', '2026-10-10 행사']))
    spc('sp_punct_first_char', ''.join(p(run(s)) for s in ['“인용” 글', '「법령」 글', '— 대시 글', '♣ 클럽', '§ 조항', '(괄호) 본문', '[대괄호] 본문', '《서》 본문', '‘작은따옴표’ 본문']))
    spc('sp_leading_spaces', ''.join(p(run(s)) for s in ['  □ 앞 공백', '　ㅇ 전각 공백', '\t- 탭', '   ']))
    spc('sp_empty_and_none_text', p(run(None, raw=''), run('본문'), run(None, raw=t(''))))
    spc('sp_t_text_none_multiple', p(run(None, raw='<hp:t/>'), run('□ 가나')))
    spc('sp_emoji_codepoints', p(run('□ 😀😀 본문 😀 끝')))
    spc('sp_cjk_label', p(run('ㅇ 항목: 본문 설명')))
    spc('sp_colon_label_long_rejected', p(run('ㅇ 아주아주 긴 라벨이 열두 글자를 넘는 경우: 본문')))
    spc('sp_colon_label_url_rejected', p(run('ㅇ http: 본문 글'), run('ㅇ 12: 본문 글'), run('ㅇ a/b: 본문')))
    spc('sp_unknown_charpr_ref', p(run('본문', 99), run('본문', 1)))
    spc('sp_charpr_missing_ref_attr', '<hp:p><hp:run><hp:t>본문</hp:t></hp:run><hp:run charPrIDRef="1"><hp:t>본문</hp:t></hp:run></hp:p>')
    spc('sp_linesegarray_removed', p(run('본문 글'), extra='<hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray>'))
    spc('sp_linesegarray_kept_when_unchanged', p(run('본문', 0), extra='<hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray>'))
    spc('sp_table_cells', host(grid([['본문 가', '본문 나'], ['□ 항목', '- 하위']])).replace('<hp:run>', '<hp:run charPrIDRef="1">'))
    spc('sp_protected_design_table_kept', host(h.design_bar_table().replace('<hp:p>', '<hp:p>')) + p(run('본문')))
    sp['sp_protected_table_text_kept'] = hwpx_bytes({'Contents/section0.xml': section(
        host(tbl([[tc('', 0, 0, rspan=2, raw_sub=p(run('구분')) ), tc('', 0, 1, raw_sub=p(run('', 1)))], [tc('', 1, 1, raw_sub=p(run('', 1)))]], rowcnt=2, colcnt=2))
        + host(tbl([[tc('', 0, 0, raw_sub=p(run('단위 표 글')))]], caption='(단위: 개소)'))
        + p(run('일반 본문')))}, header=HDR)
    sp['sp_nested_table_in_cell'] = hwpx_bytes({'Contents/section0.xml': section(
        host(tbl([[tc('', 0, 0, raw_sub=p(run('바깥 글')) + '<hp:p><hp:run>' + tbl([[tc('', 0, 0, raw_sub=p(run('안쪽 글')))]]) + '</hp:run></hp:p>')]])))}, header=HDR)
    sp['sp_nested_in_protected_flow'] = hwpx_bytes({'Contents/section0.xml': section(
        host(tbl([[tc('', 0, 0, raw_sub=p(run('접수')) ), tc('', 0, 1, raw_sub=p(run('→'))), tc('', 0, 2, raw_sub=p(run('구분')))]]))
        + p(run('본문')))}, header=HDR)
    sp['sp_section_numeric_order'] = hwpx_bytes({
        'Contents/section10.xml': section(p(run('열번째', 2))),
        'Contents/section2.xml': section(p(run('둘째', 1))),
        'Contents/section1.xml': section(p(run('첫째', 3))),
        'Contents/section02.xml': section(p(run('영이', 1))),
    }, header=HDR)
    sp['sp_header_noncontiguous_ids'] = hwpx_bytes({'Contents/section0.xml': section(p(run('가', 5), run('나', 12), run('다', 0)))},
                                                  header=header_with_spacings([0, -3, -7], ids=[0, 5, 12]))
    sp['sp_header_duplicate_ids'] = hwpx_bytes({'Contents/section0.xml': section(p(run('가', 1)))},
                                              header=header_with_spacings([0, -3, 4], ids=[1, 2, 1]))
    sp['sp_header_charpr_missing_id'] = hwpx_bytes({'Contents/section0.xml': section('<hp:p><hp:run><hp:t>가</hp:t></hp:run></hp:p>' + p(run('나', 1)))},
                                                  header=header_with_spacings([0, -3], ids=[1, None]))
    sp['sp_header_no_group_charpr'] = hwpx_bytes({'Contents/section0.xml': section(p(run('가', 1)))},
                                                header=header_with_spacings([0, -3], ids=[0, 1], group=False))
    sp['sp_header_group_attr_order'] = hwpx_bytes({'Contents/section0.xml': section(p(run('가', 1)))},
                                                 header=header_with_spacings([0, -3], extra_attr=' extra="e"'))
    sp['sp_header_spacing_values_variety'] = hwpx_bytes({'Contents/section0.xml': section(p(run('가', 1), run('나', 2), run('다', 3), run('라', 4), run('마', 5), run('바', 6)))},
                                                       header=header_with_spacings([0, 1, {'hangul': '-0', 'latin': '0'}, {'hangul': ' 0'}, {'hangul': '0.0'}, 0, -100], ids=[0, 1, 2, 3, 4, 5, 6]))
    sp['sp_header_malformed_rejected'] = hwpx_bytes({'Contents/section0.xml': section(p(run('가', 1)))}, header='<?xml version="1.0"?><hh:head><a></hh:head>')
    sp['sp_other_contents_xml_malformed_rejected'] = hwpx_bytes({'Contents/section0.xml': section(p(run('가', 1)))}, header=HDR,
                                                               extra_files={'Contents/x.xml': b'<a><b></a>'})
    sp['sp_section_malformed_rejected'] = hwpx_bytes({'Contents/section0.xml': b'<hs:sec><hp:p></hs:sec>'}, header=HDR)
    sp['sp_binary_and_dirs_preserved'] = hwpx_bytes({'Contents/section0.xml': section(p(run('□ 본문')))}, header=HDR,
                                                    extra_files={'BinData/a.png': bytes(range(256)) * 20, 'Preview/PrvText.txt': '미리보기'.encode()},
                                                    dir_entries=('BinData/',))
    sp['sp_no_sections_of_interest_copy'] = hwpx_bytes({'Contents/section0.xml': section('')}, header=HDR)
    sp['sp_tail_text_and_whitespace_runs'] = hwpx_bytes({'Contents/section0.xml': section('<hp:p>\n <hp:run charPrIDRef="1"><hp:t>□ 가나다</hp:t></hp:run>꼬리\n <hp:run charPrIDRef="2"><hp:t>라마</hp:t></hp:run>\n</hp:p>')}, header=HDR)
    for k, v in sp.items():
        cases[k] = (v, [('spacing_reset', None)])

    # 무작위 문단 묶음(보호 구간 규칙의 분기를 폭넓게 건드린다)
    rnd = random.Random(20261123)
    marks = ['□ ', 'ㅇ ', '- ', '○ ', '※ ', '* ', '** ', '1. ', '가. ', '1) ', '(1) ', '(가) ', '1-1. ', 'Ⅰ. ', '① ', '❶ ', '㉠ ', 'A. ', '[가] ', '「', '“', '(', '', '', '', ' ', '  □ ',
             '— ', '→ ', '· ', '2026. 10. 10. ', '☞ ', '＊']
    labels = ['(라벨) ', '(운영방식)', '(2026) ', '(\'26) ', '일시: ', '장소 : ', '(방식) : ', 'http: ', '12: ', '(가나다라마바사아자차카타) ', '(a)(b) ', '((중첩)) ', '(열림']
    bodies = ['본문 글입니다', '서울시 마포구', '', '😀 이모지', '「법령」 글', ' 공백 시작', 'abc def', '2026. 10. 10.', '가', '12:30 시작']
    for n in range(14):
        paras = []
        for _ in range(40):
            nrun = rnd.randint(1, 4)
            text = rnd.choice(marks) + rnd.choice(['', '', rnd.choice(labels)]) + rnd.choice(bodies)
            cuts = sorted(rnd.sample(range(len(text) + 1), min(nrun - 1, len(text) + 1))) if len(text) else []
            pieces, last = [], 0
            for c in cuts + [len(text)]:
                pieces.append(text[last:c])
                last = c
            runs = []
            for piece in pieces:
                cid = rnd.choice([0, 1, 1, 2, 3, 4, 5, 99])
                kind = rnd.random()
                if kind < 0.08 and piece:
                    raw = '<hp:t>' + piece[: len(piece) // 2] + '<hp:fwSpace/>' + piece[len(piece) // 2:] + '</hp:t>'
                    runs.append(run(None, cid, raw=raw))
                elif kind < 0.12:
                    runs.append(run(None, cid, raw=t(piece[: len(piece) // 2]) + t(piece[len(piece) // 2:])))
                else:
                    runs.append(run(piece, cid))
            paras.append(p(*runs))
        in_table = rnd.random() < 0.3
        body = ''.join(paras[:30]) + (host(grid([[x, x] for x in ['□ 칸 글', '본문 칸']])) if in_table else '') + ''.join(paras[30:])
        sp[f'sp_random_{n:02d}'] = hwpx_bytes({'Contents/section0.xml': section(body)}, header=HDR)
        cases[f'sp_random_{n:02d}'] = (sp[f'sp_random_{n:02d}'], [('spacing_reset', None)])
    return cases


def unsupported_cases(h):
    """Rust 가 반영하지 못하는 설정을 '미지원 입력'(종료 코드 4)으로 거부하는지 확인하는 사례: (이름, 처리기, 설정, 문서 바이트)."""
    doc = h.one(h.host(h.title_table_type1(), h.tbl([[h.tc('', 0, 0, raw_sub=h.para('개요 본문은 열다섯 글자를 넘는 한 칸 상자 글입니다'))]], rowcnt=1, colcnt=1)))
    spdoc = h.hwpx_bytes({'Contents/section0.xml': h.section(h.para('□ 본문'))}, header=header_with_spacings([0, -5]))
    pgdoc = h.one('<hp:p><hp:run><hp:ctrl><hp:pageNum pos="A"/></hp:ctrl></hp:run></hp:p>')
    return [
        ('owner_text_overview', 'overview_lead', {'title_owner_text': '담당관'}, doc),
        ('owner_text_twochar', 'twochar', {'title_owner_text': '담당관'}, doc),
        ('indent_rules_spacing', 'spacing_reset', {'indent_rules': {'□': 'after_marker'}}, spdoc),
        ('marker_scheme_spacing', 'spacing_reset', {'marker_scheme_active': True}, spdoc),
        ('page_number_non_string', 'page_number', {'page_number': {'pos': 3}}, pgdoc),
        ('unknown_key_overview', 'overview_lead', {'새_설정': 1}, doc),
        ('unknown_key_date', 'date', {'unknown': True}, doc),
        ('unknown_key_spacing', 'spacing_reset', {'font_scale': 90}, spdoc),
        ('config_not_object', 'page_number', [1, 2], pgdoc),
        ('indent_rules_wrong_type', 'spacing_reset', {'indent_rules': [1]}, spdoc),
        ('marker_scheme_wrong_type', 'spacing_reset', {'marker_scheme_active': 'yes'}, spdoc),
        ('owner_text_wrong_type', 'twochar', {'title_owner_text': 5}, doc),
    ]
