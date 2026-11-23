"""RSXML1·RSXML2 차이 0 시험 하네스: 파이썬 처리기와 Rust CLI(hwpx_fmt)를 같은 HWPX 에 돌려 결과 XML 을 트리 단위로 비교한다.

처리기: 표날짜칸_hwpx_처리(date), 두글자칸_hwpx_처리(twochar), 개요첫칸_hwpx_처리(overview_lead),
        쪽번호_hwpx_처리(page_number), 자간초기화_hwpx_처리(spacing_reset).
설정: 파이썬 전역 설정(표준서식_설정 등)을 사례별로 바꿔 부르고, 같은 값을 --config JSON 으로 Rust 에 넘긴다.
      Rust 가 반영하지 못하는 설정은 '미지원 입력'(종료 코드 4, 출력 파일 없음)으로 거부해야 한다(unsupported_cases).
입력: 합성 HWPX(scripts 안에서 생성, 사용자 문서 없음) + tests/fixtures 의 기존 HWPX.
비교: 압축 항목 이름·순서, 바이너리 항목 바이트, XML 항목은 정규화한 트리(네임스페이스 접두사 무시·속성 순서 무시·
      t 요소 밖 공백뿐인 글/꼬리 무시)가 같은지. 한 건이라도 다르면 차이를 그대로 출력하고 종료 코드 1.
사용: python scripts/rsxml_parity.py [--bin 경로] [--json 결과.json] [--bench] [--keep 폴더]
Rust 바이너리: --bin, 환경변수 HWPX_FMT_BIN, 또는 CARGO_TARGET_DIR/release/hwpx_fmt(.exe).
"""
from __future__ import annotations

import argparse
import hashlib
import html
import io
import json
import os
import random
import runpy
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path

from defusedxml import ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
if str(Path(__file__).resolve().parent) not in sys.path:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
NS = ('xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" '
      'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section"')
PROCESSORS = {'date': '표날짜칸_hwpx_처리', 'twochar': '두글자칸_hwpx_처리', 'overview_lead': '개요첫칸_hwpx_처리',
              'page_number': '쪽번호_hwpx_처리', 'spacing_reset': '자간초기화_hwpx_처리',
              'asterisk_superscript': '별표위첨자_hwpx_처리', 'attachment_font': '붙임글꼴_hwpx_처리',
              'colon_label': '콜론라벨_hwpx_처리', 'blank_lines': '빈줄서식_hwpx_처리', 'emphasis': '강조낱말_hwpx_처리',
              'table_width': '표너비_hwpx_처리', 'table_style': '기본표서식_hwpx_처리', 'table_style_align': '기본표서식_hwpx_처리',
              'connector_tab': '연결부호탭_hwpx_처리'}
ONLY = None   # --only 로 정한 처리기 이름 집합(None 이면 전부)
PY_NS = {}    # 파이썬 처리기 이름공간(학습용 함수 접근)
# 모든 문서(합성 공통·fixture)에 처리기별로 거는 기본 설정 이름(rsxml2_cases.CONFIGS 의 키)
DEFAULT_CONFIG = {'page_number': 'page_std', 'table_style': 'ts_default', 'table_style_align': 'ts_default'}
HEADER = f'<?xml version="1.0" encoding="UTF-8"?><hh:head {NS}><hh:refList/></hh:head>'


# ---------------------------------------------------------------- 합성 HWPX 생성

def t(text, extra=''):
    return f'<hp:t{extra}>{text}</hp:t>' if text is not None else ''


def para(text, raw=None):
    body = raw if raw is not None else t(text)
    return f'<hp:p paraPrIDRef="1"><hp:run charPrIDRef="1">{body}</hp:run></hp:p>'


def tc(content, row=0, col=0, rspan=1, cspan=1, sub_attr='', raw_sub=None, with_sublist=True):
    """content: 문자열이면 한 문단, 리스트면 문단 여럿, 이미 만든 XML 이면 raw_sub 로."""
    if raw_sub is None:
        paras = [content] if isinstance(content, str) else content
        raw_sub = ''.join(para(p) for p in paras)
    sub = f'<hp:subList{sub_attr}>{raw_sub}</hp:subList>' if with_sublist else ''
    return (f'<hp:tc borderFillIDRef="1">{sub}<hp:cellAddr colAddr="{col}" rowAddr="{row}"/>'
            f'<hp:cellSpan colSpan="{cspan}" rowSpan="{rspan}"/><hp:cellSz width="5669" height="2835"/></hp:tc>')


def tbl(rows, caption=None, rowcnt=None, colcnt=None, attrs=''):
    """rows: tc XML 목록의 목록."""
    rc = len(rows) if rowcnt is None else rowcnt
    cc = max((len(r) for r in rows), default=0) if colcnt is None else colcnt
    cap = f'<hp:caption>{para(html.escape(caption, quote=False))}</hp:caption>' if caption is not None else ''
    trs = ''.join('<hp:tr>' + ''.join(r) + '</hp:tr>' for r in rows)
    return f'<hp:tbl rowCnt="{rc}" colCnt="{cc}"{attrs}>{cap}{trs}</hp:tbl>'


def grid(texts, **kw):
    """texts: 2차원 글 목록 → 표."""
    rows = [[tc(x, r, c) for c, x in enumerate(row)] for r, row in enumerate(texts)]
    return tbl(rows, **kw)


def host(*tables):
    """표를 문단 안에 둔다."""
    return ''.join(f'<hp:p><hp:run>{x}</hp:run></hp:p>' for x in tables)


def section(body, decl='<?xml version="1.0" encoding="UTF-8"?>', extra=''):
    return f'{decl}<hs:sec {NS}>{extra}{body}</hs:sec>'


def hwpx_bytes(sections, extra_files=None, header=HEADER, dir_entries=()):
    bio = io.BytesIO()
    with zipfile.ZipFile(bio, 'w') as z:
        z.writestr('mimetype', 'application/hwp+zip', compress_type=zipfile.ZIP_STORED)
        z.writestr('Contents/header.xml', header, compress_type=zipfile.ZIP_DEFLATED)
        for name, xml in sections.items():
            data = xml.encode('utf-8') if isinstance(xml, str) else xml
            z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
        for name, data in (extra_files or {}).items():
            z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)
        for name in dir_entries:
            z.writestr(name, b'')
    return bio.getvalue()


def one(body, **kw):
    return hwpx_bytes({'Contents/section0.xml': section(body, **kw)})


def title_table_type1():
    # 2x2: 제목(2열 병합) / 날짜 / 담당자
    cells = [[tc('보고서 제목', 0, 0, cspan=2)], [tc('2026. 10. 10.', 1, 0), tc('담당 과장 홍길동', 1, 1)]]
    return tbl(cells, rowcnt=2, colcnt=2)


def title_table_type3():
    return tbl([[tc('제목입니다', 0, 0)], [tc('담당: 홍길동 ☎ 3153-8660', 1, 0)]], rowcnt=2, colcnt=1)


def design_bar_table():
    # 장식 막대 표: 글 칸이 첫 행에서 모든 행을 덮고, 빈 점선 칸이 있다
    return tbl([[tc('구분', 0, 0, rspan=2), tc('', 0, 1)], [tc('', 1, 1)]], rowcnt=2, colcnt=2)


def diagram_table():
    cells = []
    for r in range(3):
        cells.append([tc('구분' if (r, c) == (0, 0) else '', r, c) for c in range(4)])
    return tbl(cells)


def flow_table():
    return grid([['접수', '→', '구분']])


def synthetic_cases():
    cases = {}
    cases['two_hangul_basic'] = one(host(grid([['구분', '내용'], ['비고', '확인']])))
    cases['mixed_digits'] = one(host(grid([['10', '1부', '가1'], ['ab', '한글a', '구분']])))
    cases['already_spaced'] = one(host(grid([['구 분', '내 용'], ['구분', '비고']])))
    cases['three_hangul'] = one(host(grid([['구분란', '구분'], ['가나다라', '나']])))
    cases['whitespace_padded'] = one(host(grid([[' 구분 ', '\u3000내용\u3000'], ['\t비고', '확인']])))
    cases['split_runs_not_target'] = one(host(tbl([[tc('', 0, 0, raw_sub=para(None, raw=t('구') + t('분')))]])))
    cases['t_with_child_skipped'] = one(host(tbl([[tc('', 0, 0, raw_sub=para(None, raw='<hp:t>구<hp:fwSpace/>분</hp:t>')),
                                                  tc('', 0, 1, raw_sub=para(None, raw='<hp:t>구분<hp:lineBreak/></hp:t>'))]])))
    cases['blank_t_siblings'] = one(host(tbl([[tc('', 0, 0, raw_sub=para(None, raw=t('  ') + t('구분') + t('')))]])))
    cases['protected_design_bar'] = one(host(design_bar_table(), grid([['구분', '내용']])))
    cases['protected_diagram_with_nested'] = one(host(
        tbl([[tc('', 0, 0, raw_sub=para(None, raw=grid([['구분', '내용']])))] + [tc('', 0, c) for c in range(1, 4)]] +
            [[tc('', r, c) for c in range(4)] for r in range(1, 3)])))
    cases['protected_flow'] = one(host(flow_table(), grid([['구분', '내용']])))
    cases['protected_unit_caption'] = one(host(grid([['구분', '내용']], caption='(단위: 개소)'),
                                               grid([['구분', '내용']], caption='<표 1> 현황')))
    cases['format_title1'] = one(host(title_table_type1(), grid([['구분', '내용']])))
    cases['format_title3'] = one(host(title_table_type3()))
    cases['format_title_twochar_title'] = one(host(tbl([[tc('보고', 0, 0, cspan=2)],
                                                        [tc('2026. 10. 10.', 1, 0), tc('담당 홍길동', 1, 1)]], rowcnt=2, colcnt=2)))
    cases['format_midtitle_roman'] = one(host(grid([['Ⅰ.', '개요']]), grid([['Ⅱ.', '', '현황']])))
    cases['midtitle_digit_not_format'] = one(host(grid([['1', '개요']])))
    cases['format_attach'] = one(host(grid([['붙임 1', '서류']]), grid([['붙임', '', '서류']]), grid([['붙임2', '서류']]),
                                      grid([['붙임물', '서류']])))
    cases['overview_box_vs_short'] = one(host(grid([['구분']]), grid([['이것은 열다섯 글자가 넘는 개요 상자 글입니다']])))
    cases['nested_duplicate_target'] = one(host(
        tbl([[tc('', 0, 0, raw_sub=para(None, raw=grid([['구분']])))]])))
    cases['nested_in_cell_with_text'] = one(host(
        tbl([[tc('', 0, 0, raw_sub=para('구분') + para(None, raw=grid([['내용']])))]])))
    # 날짜 칸
    cases['date_basic'] = one(host(grid([['’26. 10. 10.(토)', '2026년 10월 10일(토) 14:00'], ['10. 6.(화) 10:00 ~', '구분']])))
    cases['date_negative'] = one(host(grid([['관광정책과장: 조희옥 ☎3153-8660', '구분', '', '2026. 10. 10.부터 민원 안내 서비스를 시범 운영하여 대기 시간을 줄임']])))
    cases['date_already_squeeze'] = one(host(tbl([[tc('2026. 10. 10.', 0, 0, sub_attr=' lineWrap="SQUEEZE"'),
                                                  tc('2026. 10. 11.', 0, 1, sub_attr=' lineWrap="BREAK"')]])))
    cases['date_no_sublist'] = one(host(tbl([[tc('x', 0, 0, with_sublist=False), tc('2026. 10. 10.', 0, 1)]])))
    cases['date_cell_with_nested_table'] = one(host(
        tbl([[tc('', 0, 0, raw_sub=para('2026. 10. 10.') + para(None, raw=grid([['2026. 10. 11.']]))),
              tc('2026. 10. 12.', 0, 1)]])))
    cases['date_in_protected'] = one(host(tbl([[tc('2026. 10. 10.', 0, 0, rspan=2), tc('', 0, 1)], [tc('', 1, 1)]],
                                             rowcnt=2, colcnt=2), grid([['2026. 10. 10.']])))
    cases['date_in_format_title'] = one(host(title_table_type1()))
    cases['date_nested_outer_inner'] = one(host(
        tbl([[tc('', 0, 0, raw_sub=para(None, raw=grid([['2026. 10. 10.', '구분']]))), tc('2026. 10. 11.', 0, 1)]])))
    cases['date_multi_paragraph_cell'] = one(host(grid([[['2026. 10. 10.', '(토)'], ['10:00', '2026. 10. 10.', 'x']]])))
    cases['date_weekday_forms'] = one(host(grid([['10.6.(화)', '2026-10-10', '26/10/10 10:00', '2026.10.10.(토)~11.(일)', '10월 6일(화)']])))
    cases['date_long_remainder'] = one(host(grid([['2026. 10. 10. 참석자 명단 확인 필요', '2026. 10. 10.']])))
    cases['no_tables'] = one('<hp:p><hp:run><hp:t>구분</hp:t></hp:run></hp:p>')
    cases['empty_body'] = one('')
    # 여러 구역
    cases['multi_section_ordering'] = hwpx_bytes({
        'Contents/section0.xml': section(host(grid([['구분', '내용']]))),
        'Contents/section1.xml': section(host(grid([['2026. 10. 10.']]))),
        'Contents/section10.xml': section(host(grid([['비고', '확인']]))),
        'Contents/section2.xml': section('<hp:p/>'),
        'Contents/sectionX.xml': section(host(grid([['구분', '내용']]))),   # 구역 이름 아님 → 손대지 않음
    })
    cases['targets_only_in_second_section'] = hwpx_bytes({
        'Contents/section0.xml': section(host(grid([['10', 'ab']]))),
        'Contents/section1.xml': section(host(grid([['구분', '2026. 10. 10.']]))),
    })
    cases['no_targets_anywhere_copy'] = one(host(grid([['10', 'ab'], ['text', 'x']])))
    cases['binary_and_dirs'] = hwpx_bytes({'Contents/section0.xml': section(host(grid([['구분', '내용']])))},
                                          extra_files={'BinData/image1.png': bytes(range(256)) * 40,
                                                       'Preview/PrvText.txt': '미리보기'.encode('utf-8')},
                                          dir_entries=('BinData/',))
    # XML 표현 차이
    cases['entities_cdata_attrs'] = one(host(tbl([[tc('', 0, 0, raw_sub=para(None, raw='<hp:t><![CDATA[구분]]></hp:t>')),
                                                  tc('', 0, 1, raw_sub=para(None, raw='<hp:t>&#xAC00;&#xB098;</hp:t>')),
                                                  tc('', 0, 2, raw_sub=para(None, raw='<hp:t>a &amp; b &lt; c &gt; d 2026. 10. 10.</hp:t>'))]],
                                                attrs=' memo="따옴표&quot;와 &amp; 줄바꿈&#10;탭&#9;"')))
    cases['comments_and_pi'] = one(host('<!-- 주석 -->' + grid([['구분', '<!--x-->내용']]) + '<?pi data?>'),
                                    extra='<!-- 앞 주석 --><?target value?>')
    cases['pretty_printed_whitespace'] = one('\n  <hp:p>\n    <hp:run>\n      ' + grid([['구분', '내용']]).replace('><', '>\n      <') + '\n    </hp:run>\n  </hp:p>\n')
    cases['declaration_standalone'] = one(host(grid([['구분', '내용']])), decl='<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>')
    cases['bom_utf8_section'] = hwpx_bytes({'Contents/section0.xml': b'\xef\xbb\xbf' + section(host(grid([['구분', '내용']]))).encode('utf-8')})
    cases['unusual_prefixes'] = hwpx_bytes({'Contents/section0.xml': (
        '<?xml version="1.0"?><a:sec xmlns:a="http://www.hancom.co.kr/hwpml/2011/section" xmlns:b="http://www.hancom.co.kr/hwpml/2011/paragraph">'
        '<b:p><b:run><b:tbl rowCnt="1" colCnt="2"><b:tr><b:tc><b:subList><b:p><b:run><b:t>구분</b:t></b:run></b:p></b:subList></b:tc>'
        '<b:tc><b:subList lineWrap="BREAK"><b:p><b:t>2026. 10. 10.</b:t></b:p></b:subList></b:tc></b:tr></b:tbl></b:run></b:p></a:sec>')})
    cases['tail_text_in_cells'] = one(host(tbl([[tc('', 0, 0, raw_sub='<hp:p><hp:run><hp:t>구분</hp:t>꼬리</hp:run></hp:p>')]])))
    cases['rowcnt_missing_attrs'] = one(host('<hp:tbl><hp:tr><hp:tc><hp:subList><hp:p><hp:t>구분</hp:t></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl>'))
    cases['unicode_whitespace_cells'] = one(host(grid([[' 구분 ', '구분', '구​분', ' 내용 ', '　비고']])))
    cases['invalid_control_char_rejected'] = one(host(grid([['구분', '내용']])))
    cases['invalid_char_reference_rejected'] = one(host(grid([['구분&#1;', '내용']])))
    cases['hangul_boundaries'] = one(host(grid([['가힣', '\uac00\ud7a3', '가', '힣a', 'ㄱㄴ', '漢字', '구분.', '구.분']])))
    cases['one_two_hangul_in_t_with_extra'] = one(host(tbl([[tc('', 0, 0, raw_sub=para(None, raw=t('구분') + t('   ')))]])))
    return cases


def random_cases(count=12, seed=20261122):
    rnd = random.Random(seed)
    pool = ['구분', '내용', '비고', '확인', '10', '1부', '가나다', '', '2026. 10. 10.', '’26. 10. 10.(토)', '10. 6.(화)',
            '이것은 긴 글입니다 열다섯 글자 이상', '붙임 1', 'Ⅰ.', 'Ⅱ', '→', '담당: 홍길동', '(단위: 개)', ' 구분 ', 'ab']

    def rand_table(depth=0):
        rows, cols = rnd.randint(1, 3), rnd.randint(1, 4)
        trs = []
        for r in range(rows):
            cells = []
            for c in range(cols):
                if depth < 2 and rnd.random() < 0.12:
                    cells.append(tc('', r, c, raw_sub=para(rnd.choice(pool)) + para(None, raw=rand_table(depth + 1))))
                else:
                    cells.append(tc(rnd.choice(pool), r, c, sub_attr=rnd.choice(['', '', ' lineWrap="SQUEEZE"', ' lineWrap="BREAK"'])))
            trs.append(cells)
        cap = rnd.choice([None, None, None, '(단위: 건)', '<표 1> 현황'])
        return tbl(trs, caption=cap, rowcnt=rows, colcnt=cols)

    out = {}
    for i in range(count):
        secs = {}
        for s in range(rnd.randint(1, 3)):
            secs[f'Contents/section{s}.xml'] = section(host(*[rand_table() for _ in range(rnd.randint(1, 8))]))
        out[f'random_{i:02d}'] = hwpx_bytes(secs)
    return out


def malicious_cases():
    """둘 다 거부해야 하는 입력(안전 검사 일치 확인)."""
    ok_sec = section(host(grid([['구분', '내용']])))

    def make(mutator):
        bio = io.BytesIO()
        with zipfile.ZipFile(bio, 'w') as z:
            mutator(z)
        return bio.getvalue()

    def base(z, skip=()):
        if 'header' not in skip:
            z.writestr('Contents/header.xml', HEADER)
        if 'section' not in skip:
            z.writestr('Contents/section0.xml', ok_sec)

    out = {}
    out['evil_path_traversal'] = make(lambda z: (base(z), z.writestr('../evil.xml', b'x')))
    out['evil_path_absolute'] = make(lambda z: (base(z), z.writestr('/abs.xml', b'x')))
    out['evil_path_drive'] = make(lambda z: (base(z), z.writestr('C:/win.xml', b'x')))
    out['evil_path_backslash_dotdot'] = make(lambda z: (base(z), z.writestr('a\\..\\b.xml', b'x')))
    out['missing_header'] = make(lambda z: base(z, skip=('header',)))
    out['missing_section'] = make(lambda z: base(z, skip=('section',)))

    def dup(z):
        base(z)
        with __import__('warnings').catch_warnings():
            __import__('warnings').simplefilter('ignore')
            z.writestr('Contents/section0.xml', ok_sec)
    out['duplicate_entry'] = make(dup)

    def symlink(z):
        base(z)
        info = zipfile.ZipInfo('link')
        info.external_attr = (0o120777 << 16)
        z.writestr(info, 'Contents/header.xml')
    out['symlink_entry'] = make(symlink)

    def bomb(z):
        base(z)
        z.writestr(zipfile.ZipInfo('bomb.bin'), b'\0' * (120 * 1024 * 1024), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    out['zip_bomb_ratio'] = make(bomb)
    out['not_a_zip'] = b'this is not a zip file'
    return out


# ---------------------------------------------------------------- 정규화·비교

def _ws_only(s):
    return s is None or s == '' or s.strip() == ''


def norm_tree(el, strict=False):
    """(확장 이름, 정렬한 속성, 글, 꼬리, 자식들). t 요소 밖 공백뿐인 글/꼬리는 ''로 본다(strict 면 그대로)."""
    is_t = el.tag.rsplit('}', 1)[-1] == 't'

    def clean(s, keep):
        if s is None:
            return ''
        return s if (strict or keep or not _ws_only(s)) else ''
    return (el.tag, tuple(sorted(el.attrib.items())), clean(el.text, is_t), clean(el.tail, False),
            tuple(norm_tree(c, strict) for c in el))


def first_diff(a, b, path='/'):
    if a[0] != b[0]:
        return f'{path}: 이름 {a[0]!r} != {b[0]!r}'
    if a[1] != b[1]:
        return f'{path}{a[0]}: 속성 {a[1]!r} != {b[1]!r}'
    if a[2] != b[2]:
        return f'{path}{a[0]}: 글 {a[2]!r} != {b[2]!r}'
    if a[3] != b[3]:
        return f'{path}{a[0]}: 꼬리 {a[3]!r} != {b[3]!r}'
    if len(a[4]) != len(b[4]):
        return f'{path}{a[0]}: 자식 수 {len(a[4])} != {len(b[4])}'
    for i, (x, y) in enumerate(zip(a[4], b[4])):
        d = first_diff(x, y, f'{path}{a[0].rsplit("}", 1)[-1]}[{i}]/')
        if d:
            return d
    return None


def read_zip(path):
    with zipfile.ZipFile(path) as z:
        return [(i.filename, z.read(i.filename), i.compress_type) for i in z.infolist()]


def compare_outputs(py_path, rs_path):
    """(같은가, 차이 설명 목록, 엄격 비교 차이 수)."""
    diffs, strict_diffs = [], 0
    a, b = read_zip(py_path), read_zip(rs_path)
    if [x[0] for x in a] != [x[0] for x in b]:
        diffs.append(f'압축 항목 이름·순서 다름: {[x[0] for x in a]} != {[x[0] for x in b]}')
        return False, diffs, strict_diffs
    for (name, da, ca), (_, db, cb) in zip(a, b):
        if name == 'mimetype' and (ca != cb):
            diffs.append(f'mimetype 압축 방식 다름 {ca} != {cb}')
        if name.endswith('.xml') and name.startswith('Contents/'):
            try:
                ta, tb = ET.fromstring(da), ET.fromstring(db)
            except Exception as exc:
                diffs.append(f'{name}: XML 읽기 실패 {exc}')
                continue
            na, nb = norm_tree(ta), norm_tree(tb)
            if na != nb:
                diffs.append(f'{name}: {first_diff(na, nb)}')
            elif norm_tree(ta, True) != norm_tree(tb, True):
                strict_diffs += 1
        elif da != db:
            diffs.append(f'{name}: 바이트 다름 ({len(da)} vs {len(db)})')
    return not diffs, diffs, strict_diffs


# ---------------------------------------------------------------- 실행

def find_bin(arg):
    cands = [arg, os.environ.get('HWPX_FMT_BIN')]
    tgt = os.environ.get('CARGO_TARGET_DIR')
    exe = 'hwpx_fmt.exe' if os.name == 'nt' else 'hwpx_fmt'
    if tgt:
        cands.append(str(Path(tgt) / 'release' / exe))
    cands.append(str(ROOT / 'hwpx_fmt' / 'target' / 'release' / exe))
    for c in cands:
        if c and Path(c).is_file():
            return str(Path(c).resolve())
    return None


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_python_processors():
    old = sys.stdout
    sys.stdout = io.StringIO()
    try:
        ns = runpy.run_path(str(ROOT / 'hwp-auto-docfit.py'))
    finally:
        sys.stdout = old
    PY_NS.clear()
    PY_NS.update(ns)
    return {k: ns[v] for k, v in PROCESSORS.items()}


_MISSING = object()


def apply_py_settings(fn, cfg):
    """파이썬 처리기가 읽는 전역 설정을 cfg(Rust --config 와 같은 사전)로 맞추고, 되돌리는 함수를 돌려준다."""
    g = fn.__globals__
    keys = ('표준서식_설정', '활성_항목기호_체계', '제목_담당자_글', '표준서식_기호_사용', '알파_콜론라벨_정렬_사용', '장평_줄이기_사용',
            '활성_표서식_프로필', '알파_연결부호_탭정렬_사용')
    saved = {k: g.get(k, _MISSING) for k in keys}
    std = {}
    if cfg:
        if cfg.get('page_number'):
            std['쪽번호'] = dict(cfg['page_number'])
        if cfg.get('indent_rules'):
            std['내어쓰기_규칙'] = dict(cfg['indent_rules'])
        g['제목_담당자_글'] = cfg.get('title_owner_text') or ''
    else:
        g['제목_담당자_글'] = ''
    c3 = cfg or {}
    if c3.get('symbol_rules'):
        std['기호_규칙'] = [list(r) for r in c3['symbol_rules']]
    if c3.get('blank_line_profile'):
        std['빈줄_서식'] = c3['blank_line_profile']
    if c3.get('emphasis_words'):
        std['강조_낱말'] = c3['emphasis_words']
    g['표준서식_설정'] = std
    g['표준서식_기호_사용'] = c3.get('std_symbols', True)
    g['알파_콜론라벨_정렬_사용'] = c3.get('colon_label_enabled', True)
    g['장평_줄이기_사용'] = c3.get('ratio_reduce', False)
    g['활성_표서식_프로필'] = c3.get('table_style')
    g['알파_연결부호_탭정렬_사용'] = c3.get('connector_tab_enabled', True)
    if c3.get('marker_scheme_active') and fn.__name__ in ('별표위첨자_hwpx_처리', '붙임글꼴_hwpx_처리', '콜론라벨_hwpx_처리', '빈줄서식_hwpx_처리', '강조낱말_hwpx_처리'):
        from docfit_core.marker_scheme import detect_scheme
        g['활성_항목기호_체계'] = detect_scheme(['1. 가', '2. 나', '1) 다', '2) 라', '가. 마', '나. 바'])
    else:
        g['활성_항목기호_체계'] = None

    def restore():
        for k, v in saved.items():
            if v is _MISSING:
                g.pop(k, None)
            else:
                g[k] = v
    return restore


def run_py(fn, src, dst, cfg=None):
    """파이썬: (조사 결과, 개수, 오류). 실패하면 dst 가 없어야 한다."""
    restore = apply_py_settings(fn, cfg)
    try:
        old = sys.stdout
        sys.stdout = io.StringIO()
        try:
            sel = fn(str(src))
            n = fn(str(src), str(dst), selections=sel)
        finally:
            sys.stdout = old
        return json.loads(json.dumps(sel)), n, None
    except Exception as exc:
        return None, None, f'{type(exc).__name__}: {exc}'
    finally:
        restore()


def cfg_args(cfg, cfg_file):
    if cfg is None:
        return []
    Path(cfg_file).write_text(json.dumps(cfg, ensure_ascii=False), encoding='utf-8')
    return ['--config', str(cfg_file)]


def run_rs(binpath, name, src, dst, cfg=None, cfg_file=None):
    extra = cfg_args(cfg, cfg_file)
    s = subprocess.run([binpath, 'survey', name, str(src)] + extra, capture_output=True, text=True, encoding='utf-8')
    if s.returncode != 0:
        return None, None, s.stderr.strip()
    sel = json.loads(s.stdout)
    p = subprocess.run([binpath, name, str(src), str(dst)] + extra, capture_output=True, text=True, encoding='utf-8')
    if p.returncode != 0:
        return sel, None, p.stderr.strip()
    return sel, json.loads(p.stdout)['count'], None


def mutated_fixtures():
    """tests/fixtures 의 실제 HWPX 구조를 쓰되, 대상이 생기도록 일부만 바꾼 변형(자간 -5 / 쪽 번호 요소 삽입)."""
    import re
    out = {}
    for f in sorted((ROOT / 'tests' / 'fixtures').glob('*.hwpx')):
        with zipfile.ZipFile(f) as z:
            items = [(i, z.read(i.filename)) for i in z.infolist()]

        def rebuild(fn):
            bio = io.BytesIO()
            with zipfile.ZipFile(bio, 'w') as z:
                for info, data in items:
                    z.writestr(info.filename, fn(info.filename, data),
                               compress_type=zipfile.ZIP_STORED if info.filename == 'mimetype' else zipfile.ZIP_DEFLATED)
            return bio.getvalue()

        def sp(name, data):
            if name == 'Contents/header.xml':
                text = data.decode('utf-8')
                text = re.sub(r'(<hh:spacing [^>]*>)', lambda m: re.sub(r'="-?\d+"', '="-5"', m.group(1)), text)
                return text.encode('utf-8')
            return data

        def pn(name, data):
            if re.fullmatch(r'Contents/section\d+\.xml', name):
                text = data.decode('utf-8')
                text = re.sub(r'(<hp:run [^>]*>)', r'\1<hp:ctrl><hp:pageNum pos="TOP_LEFT" formatType="X" sideChar=""/></hp:ctrl>', text, count=1)
                return text.encode('utf-8')
            return data
        out[f'fixturesp_{f.stem}'] = rebuild(sp)
        out[f'fixturepn_{f.stem}'] = rebuild(pn)
    return out


def build_corpus(work):
    """{문서 이름: (종류, 경로, [(처리기, 설정 이름)])}. 공통 문서는 모든 처리기에, 처리기 전용 사례는 정해진 처리기에 건다."""
    import rsxml2_cases
    everyone = [x for x in PROCESSORS if ONLY is None or x in ONLY]
    corpus = {}
    (work / 'in').mkdir(exist_ok=True)
    for name, data in {**synthetic_cases(), **random_cases()}.items():
        p = work / 'in' / f'{name}.hwpx'
        p.write_bytes(data)
        corpus[name] = ('synthetic', p, [(x, DEFAULT_CONFIG.get(x)) for x in everyone])
    for f in sorted((ROOT / 'tests' / 'fixtures').glob('*.hwpx')):
        p = work / 'in' / f'fixture_{f.stem}.hwpx'
        shutil.copyfile(f, p)
        corpus[f'fixture_{f.stem}'] = ('fixture', p, [(x, DEFAULT_CONFIG.get(x)) for x in everyone])
    for name, data in mutated_fixtures().items():
        p = work / 'in' / f'{name}.hwpx'
        p.write_bytes(data)
        corpus[name] = ('fixture_mutated', p, [(x, DEFAULT_CONFIG.get(x)) for x in everyone])
    for name, data in malicious_cases().items():
        p = work / 'in' / f'mal_{name}.hwpx'
        p.write_bytes(data)
        corpus[f'mal_{name}'] = ('malicious', p, [(x, DEFAULT_CONFIG.get(x)) for x in everyone])
    for name, (data, jobs) in rsxml2_cases.build(sys.modules[__name__]).items():
        jobs = [j for j in jobs if ONLY is None or j[0] in ONLY]
        p = work / 'in' / f'{name}.hwpx'
        p.write_bytes(data)
        corpus[name] = ('rsxml2', p, jobs)
    import rsxml3_cases
    for name, (data, jobs) in rsxml3_cases.build(sys.modules[__name__], PY_NS).items():
        jobs = [j for j in jobs if ONLY is None or j[0] in ONLY]
        if not jobs:
            continue
        p = work / 'in' / f'{name}.hwpx'
        p.write_bytes(data)
        corpus[name] = ('rsxml3', p, jobs)
    import rsxml4_cases
    for name, (data, jobs) in rsxml4_cases.build(sys.modules[__name__], PY_NS).items():
        jobs = [j for j in jobs if ONLY is None or j[0] in ONLY]
        if not jobs:
            continue
        p = work / 'in' / f'{name}.hwpx'
        p.write_bytes(data)
        corpus[name] = ('rsxml4', p, jobs)
    return corpus


def rsxml2_cases_module():
    import rsxml2_cases
    return rsxml2_cases


def config_dict(name):
    if name is None:
        return None
    if isinstance(name, dict):
        return name
    import rsxml3_cases
    import rsxml4_cases
    return {**rsxml2_cases_module().CONFIGS, **rsxml3_cases.CONFIGS, **rsxml4_cases.CONFIGS}[name]


def run_parity(binpath, work, procs):
    corpus = build_corpus(work)
    results, failures = [], 0
    (work / 'out').mkdir(exist_ok=True)
    for name, (kind, src, jobs) in corpus.items():
        before = sha(src)
        for pname, cfgname in jobs:
            fn = procs[pname]
            cfg = config_dict(cfgname)
            out_py, out_rs = work / 'out' / f'{name}.{pname}.py.hwpx', work / 'out' / f'{name}.{pname}.rs.hwpx'
            psel, pn, perr = run_py(fn, src, out_py, cfg)
            rsel, rn, rerr = run_rs(binpath, pname, src, out_rs, cfg, work / 'out' / f'{name}.{pname}.cfg.json')
            rec = {'doc': name, 'kind': kind, 'processor': pname, 'config': cfgname if isinstance(cfgname, (str, type(None))) else 'inline', 'py_count': pn, 'rs_count': rn,
                   'py_error': perr, 'rs_error': rerr, 'ok': True, 'diffs': [], 'strict_noise': 0}
            if perr or rerr:
                # 둘 다 거부해야 같다. 한쪽만 실패하거나, 실패했는데 출력 파일이 남으면 차이.
                if not (perr and rerr):
                    rec['ok'] = False
                    rec['diffs'].append(f'한쪽만 실패: 파이썬={perr!r} Rust={rerr!r}')
                if perr and out_py.exists():
                    rec['ok'] = False
                    rec['diffs'].append('파이썬이 실패했는데 출력 파일이 남음')
                if rerr and out_rs.exists():
                    rec['ok'] = False
                    rec['diffs'].append('Rust 가 실패했는데 출력 파일이 남음')
            else:
                if psel != rsel:
                    rec['ok'] = False
                    rec['diffs'].append(f'조사 결과 다름: 파이썬={psel} Rust={rsel}')
                if pn != rn:
                    rec['ok'] = False
                    rec['diffs'].append(f'바꾼 개수 다름: 파이썬={pn} Rust={rn}')
                same, diffs, noise = compare_outputs(out_py, out_rs)
                rec['strict_noise'] = noise
                if not same:
                    rec['ok'] = False
                    rec['diffs'].extend(diffs)
                if (all(not v for v in psel.values()) or (pname == 'spacing_reset' and not pn)) and out_rs.read_bytes() != src.read_bytes():
                    rec['ok'] = False
                    rec['diffs'].append('대상이 없는데 출력이 입력과 바이트 단위로 다름(복사해야 함)')
            if sha(src) != before:
                rec['ok'] = False
                rec['diffs'].append('입력 원본이 바뀜')
            if not rec['ok']:
                failures += 1
            results.append(rec)
    return results, failures


def run_unsupported(binpath, work):
    """반영하지 못하는 설정은 종료 코드 4 와 '미지원 입력' 메시지로 거부하고 출력 파일을 만들지 않아야 한다."""
    import rsxml2_cases
    import rsxml3_cases
    import rsxml4_cases
    out, failures = [], 0
    (work / 'uns').mkdir(exist_ok=True)
    for name, pname, cfg, data in (rsxml2_cases.unsupported_cases(sys.modules[__name__]) + rsxml3_cases.unsupported_cases(sys.modules[__name__])
                                   + rsxml4_cases.unsupported_cases(sys.modules[__name__])):
        if ONLY is not None and pname not in ONLY:
            continue
        src, dst, cf = work / 'uns' / f'{name}.hwpx', work / 'uns' / f'{name}.out.hwpx', work / 'uns' / f'{name}.json'
        src.write_bytes(data)
        cf.write_text(json.dumps(cfg, ensure_ascii=False), encoding='utf-8')
        rec = {'case': name, 'processor': pname, 'ok': True, 'diffs': []}
        modes = (('survey', [binpath, 'survey', pname, str(src), '--config', str(cf)]),
                 ('process', [binpath, pname, str(src), str(dst), '--config', str(cf)]))
        if name.startswith('proc_'):
            modes = modes[1:]      # 처리 모드에서만 거부하는 사례
        for mode, argv in modes:
            r = subprocess.run(argv, capture_output=True, text=True, encoding='utf-8')
            if r.returncode != 4 or not r.stderr.startswith('미지원 입력'):
                rec['ok'] = False
                rec['diffs'].append(f'{mode}: 종료 코드 {r.returncode}, 오류 {r.stderr.strip()!r} (4 와 미지원 입력이어야 함)')
        if dst.exists():
            rec['ok'] = False
            rec['diffs'].append('미지원 입력인데 출력 파일이 만들어짐')
        failures += not rec['ok']
        out.append(rec)
    return out, failures


HEADER_SP = (f'<?xml version="1.0" encoding="UTF-8"?><hh:head {NS}><hh:refList><hh:charProperties itemCnt="3">'
             '<hh:charPr id="0"><hh:spacing hangul="0" latin="0"/></hh:charPr>'
             '<hh:charPr id="1"><hh:spacing hangul="-5" latin="-5"/></hh:charPr>'
             '<hh:charPr id="2"><hh:spacing hangul="3" latin="0"/></hh:charPr>'
             '</hh:charProperties></hh:refList></hh:head>')


def large_doc(tables=500):
    """표 500개: 보통 표 3×3(날짜·두 글자 칸·자간 대상 글 포함) + 제목/개요 쌍 + 쪽 번호."""
    body = []
    pairs = tables // 10
    for i in range(tables - 2 * pairs):
        body.append(grid([['구분', '내용', '비고'], ['2026. 10. 10.', '10', f'항목{i}'], ['확인', 'ab', '’26. 10. 11.(일)']]))
    for i in range(pairs):
        body.append(title_table_type1())
        body.append(tbl([[tc('', 0, 0, raw_sub=para(f'개요 본문은 열다섯 글자를 넘는 한 칸 상자 글입니다 {i}'))]], rowcnt=1, colcnt=1))
    pn = ''.join('<hp:p><hp:run><hp:ctrl><hp:pageNum pos="TOP_LEFT" formatType="X" sideChar=""/></hp:ctrl></hp:run></hp:p>' for _ in range(20))
    return hwpx_bytes({'Contents/section0.xml': section(host(*body) + pn)}, header=HEADER_SP)


def large_doc3(paras=3000):
    """RSXML3 속도 시험용: 별표·붙임·콜론 라벨·빈 줄·강조 대상이 모두 많은 문서(문단 수천 개). (바이트, 프로필 설정)"""
    import rsxml3_cases as m
    body = []
    for i in range(paras // 6):
        body += [m.T(f'ㅇ 단어{i}* 설명입니다', 0), m.T(f'- 일시: 값{i}', 0), m.T(f'- 장소명: 값{i}', 0), m.blank(0, 0),
                 m.T('□ 사업 개요: 중요 부분은 진하게 표시', 0), m.T('붙임 1. 계획서 1부.', 0) if i % 50 == 0 else m.T('ㅇ 본문', 3)]
        if i % 50 == 0:
            body.append(m.T('끝.', 0))
    return m.pack(sys.modules[__name__], m.sec(*body), m.ATT)


def bench3(binpath, work, procs):
    import rsxml3_cases as m
    import rsxml3_cases_b
    src = work / 'large3.hwpx'
    src.write_bytes(large_doc3())
    cfgs = {'blank_lines': {'blank_line_profile': {'version': 1, 'slots': {'-|□|0': {
                'para': '<hh:paraPr xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" id="9" tabPrIDRef="0"><hh:align horizontal="RIGHT" vertical="BASELINE"/></hh:paraPr>',
                'char': '<hh:charPr xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" id="9" height="500"><hh:fontRef hangul="0"/></hh:charPr>', 'fonts': {}}}, 'after': {}}},
            'emphasis': {'emphasis_words': [{'marker': '□', 'text': '개요', 'bold': True, 'size_delta': 0, 'context': '□사업개요:중요부분은진하게표시'}]}}
    out = {}
    for pname in ('asterisk_superscript', 'attachment_font', 'colon_label', 'blank_lines', 'emphasis'):
        fn = procs[pname]
        cfg = cfgs.get(pname)
        dp, dr = work / f'b3.{pname}.py.hwpx', work / f'b3.{pname}.rs.hwpx'
        restore = apply_py_settings(fn, cfg)
        try:
            t0 = time.perf_counter()
            sel = fn(str(src))
            n = fn(str(src), str(dp), selections=sel)
            t_py = time.perf_counter() - t0
        finally:
            restore()
        t0 = time.perf_counter()
        r = subprocess.run([binpath, pname, str(src), str(dr)] + cfg_args(cfg, work / f'b3.{pname}.json'), capture_output=True, text=True, encoding='utf-8')
        t_rs = time.perf_counter() - t0
        same, diffs, _ = compare_outputs(dp, dr)
        out[pname] = {'paragraphs': 3000, 'py_seconds': round(t_py, 3), 'rs_seconds': round(t_rs, 3), 'speedup': round(t_py / t_rs, 1) if t_rs else None,
                      'py_count': n, 'rs_count': json.loads(r.stdout)['count'] if r.returncode == 0 else None, 'identical': same}
    return out


def large_doc4(tables=400, toc_groups=500):
    """RSXML4 속도 시험용: 3×3 데이터 표 tables 개(칸 너비·기본 표 서식 대상) + 연결 부호 문단 묶음 toc_groups 개(문단 3000개)."""
    import rsxml4_cases as m4
    import rsxml4_cases_b as b4
    body = [m4.pagepr(), m4.T('□ 추진 배경')]
    for i in range(tables):
        body.append(m4.host(b4.data_table([['구분', '내용', '비고'], [f'가{i}', '긴 문장이 들어 있는 칸입니다 이 글은 길다', '10'], ['나', '짧음', '2026. 10. 10.']])))
    for i in range(toc_groups):
        body += [m4.T(f'제{i}장 총칙 ........ {i}'), m4.T(f'제{i}장 목적과 범위 ........ {i + 10}'), m4.T(f'제{i}장 부칙 ........ {i + 20}'),
                 m4.T('중간 글'), m4.T('가나다'), m4.T('라마바')]
    return m4.pack(sys.modules[__name__], m4.sec(*body), b4.DOC_HEADER)


def bench4(binpath, work, procs):
    src = work / 'large4.hwpx'
    src.write_bytes(large_doc4())
    out = {}
    for pname in ('table_width', 'table_style', 'connector_tab'):
        fn = procs[pname]
        cfg = config_dict(DEFAULT_CONFIG.get(pname))
        dp, dr = work / f'b4.{pname}.py.hwpx', work / f'b4.{pname}.rs.hwpx'
        restore = apply_py_settings(fn, cfg)
        try:
            t0 = time.perf_counter()
            sel = fn(str(src))
            n = fn(str(src), str(dp), selections=sel)
            t_py = time.perf_counter() - t0
        finally:
            restore()
        t0 = time.perf_counter()
        r = subprocess.run([binpath, pname, str(src), str(dr)] + cfg_args(cfg, work / f'b4.{pname}.json'), capture_output=True, text=True, encoding='utf-8')
        t_rs = time.perf_counter() - t0
        same, diffs, _ = compare_outputs(dp, dr)
        out[pname] = {'tables': 400, 'toc_paragraphs': 3000, 'py_seconds': round(t_py, 3), 'rs_seconds': round(t_rs, 3), 'speedup': round(t_py / t_rs, 1) if t_rs else None,
                      'py_count': n, 'rs_count': json.loads(r.stdout)['count'] if r.returncode == 0 else None, 'identical': same}
    return out


def bench(binpath, work, procs):
    src = work / 'large.hwpx'
    src.write_bytes(large_doc(500))
    out = {}
    for pname, fn in procs.items():
        cfg = config_dict(DEFAULT_CONFIG.get(pname))
        dp, dr = work / f'bench.{pname}.py.hwpx', work / f'bench.{pname}.rs.hwpx'
        restore = apply_py_settings(fn, cfg)
        try:
            t0 = time.perf_counter()
            sel = fn(str(src))
            n = fn(str(src), str(dp), selections=sel)
            t_py = time.perf_counter() - t0
        finally:
            restore()
        t0 = time.perf_counter()
        r = subprocess.run([binpath, pname, str(src), str(dr)] + cfg_args(cfg, work / f'bench.{pname}.json'),
                           capture_output=True, text=True, encoding='utf-8')
        t_rs = time.perf_counter() - t0
        same, diffs, _ = compare_outputs(dp, dr)
        out[pname] = {'tables': 500, 'py_seconds': round(t_py, 3), 'rs_seconds': round(t_rs, 3),
                      'speedup': round(t_py / t_rs, 1) if t_rs else None, 'py_count': n,
                      'rs_count': json.loads(r.stdout)['count'] if r.returncode == 0 else None, 'identical': same}
    return out


# ---------------------------------------------------------------- 정규화가 실제 서식 차이를 가리지 않는지 확인(일부러 망가뜨린 출력)

SABOTAGE_PAIRS = [('ast_lines_all', 'asterisk_superscript', None), ('att_align_two_kinds', 'attachment_font', None),
                  ('col_basic', 'colon_label', None), ('blk_basic_same', 'blank_lines', 'inline'), ('emp_plain_same', 'emphasis', 'inline'),
                  ('w_two_col', 'table_width', None), ('w_random_03', 'table_width', None), ('k_toc_dots', 'connector_tab', None), ('k_pull_forward_cell', 'connector_tab', None),
                  ('s_basic_3x3', 'table_style', 'ts_default'), ('s_profile_learned', 'table_style', 'ts_learned'), ('a_after_width_w_two_col', 'table_style_align', 'ts_default')]


def _mut_zip(path, out, fn):
    """path 의 압축을 읽어 fn(이름, 바이트)->바이트 로 바꾼 사본을 out 에 쓴다. 바뀐 항목이 없으면 False."""
    changed = False
    with zipfile.ZipFile(path) as z:
        items = [(i, z.read(i.filename)) for i in z.infolist()]
    with zipfile.ZipFile(out, 'w') as z:
        for info, data in items:
            new = fn(info.filename, data)
            changed |= new != data
            z.writestr(info.filename, new, compress_type=info.compress_type)
    return changed


def _re_sub_first(pattern, repl):
    import re

    def go(name, data):
        if not (name.startswith('Contents/') and name.endswith('.xml')) or name == 'Contents/header.xml' and not _MUT_HEADER[0]:
            return data
        if name != 'Contents/header.xml' and _MUT_HEADER[0]:
            return data
        text, n = re.subn(pattern, repl, data.decode('utf-8'), count=1)
        return text.encode('utf-8') if n else data
    return go


_MUT_HEADER = [False]


def run_sabotage(binpath, work, procs):
    """Rust 출력을 일부러 망가뜨려 비교기가 잡는지(M1~M7) 확인하고, 일부러 무시하기로 한 변형(N1~N3)은 같다고 보는지 확인한다."""
    import re
    (work / 'sab').mkdir(exist_ok=True)
    corpus = build_corpus(work)
    must_detect = [
        ('M1 글자 모양 참조 번호 바꿈', False, r'charPrIDRef="(\d+)"', lambda m: f'charPrIDRef="{int(m.group(1)) + 1}"'),
        ('M2 문단 모양 참조 번호 바꿈', False, r'paraPrIDRef="(\d+)"', lambda m: f'paraPrIDRef="{int(m.group(1)) + 1}"'),
        ('M3 머리 itemCnt 바꿈', True, r'itemCnt="(\d+)"', lambda m: f'itemCnt="{int(m.group(1)) + 1}"'),
        ('M4 머리 새 정의 id 바꿈', True, r'(<hh:charPr id=")(\d+)(")(?![\s\S]*<hh:charPr id=")', lambda m: f'{m.group(1)}{int(m.group(2)) + 5}{m.group(3)}'),
        ('M5 머리 글자 크기 바꿈', True, r'height="(\d+)"', lambda m: f'height="{int(m.group(1)) + 100}"'),
        ('M6 글 안 글자 바꿈', False, r'(<hp:t>)([^<]+)', lambda m: f'{m.group(1)}{m.group(2)[:-1]}X'),
        ('M7 글 안 공백 하나 추가', False, r'(<hp:t>)([^<]+)', lambda m: f'{m.group(1)} {m.group(2)}'),
        ('M8 이름공간 주소 바꿈', False, r'(xmlns:hp="[^"]+)(")', lambda m: f'{m.group(1)}X{m.group(2)}'),
        ('M10 칸 너비 바꿈', False, r'(<hp:cellSz width=")(\d+)(")', lambda m: f'{m.group(1)}{int(m.group(2)) + 1}{m.group(3)}'),
        ('M11 표 폭 바꿈', False, r'(<hp:sz width=")(\d+)(")', lambda m: f'{m.group(1)}{int(m.group(2)) + 1}{m.group(3)}'),
        ('M12 탭 위치 바꿈', True, r'(<hh:tabItem pos=")(\d+)(")', lambda m: f'{m.group(1)}{int(m.group(2)) + 1}{m.group(3)}'),
        ('M13 칸 테두리 참조 바꿈', False, r'(<hp:tc borderFillIDRef=")(\d+)(")', lambda m: f'{m.group(1)}{int(m.group(2)) + 1}{m.group(3)}'),
        ('M14 테두리 정의 바꿈', True, r'(<hh:leftBorder type=")(\w+)(")', lambda m: f'{m.group(1)}X{m.group(3)}'),
        ('M15 탭 요소 종류 바꿈', False, r'(<hp:tab width="0" leader=")(\d+)(")', lambda m: f'{m.group(1)}{int(m.group(2)) + 1}{m.group(3)}'),
        ('M16 머리 새 글꼴 이름 바꿈', True, r'(<hh:font id="\d+" face=")([^"]+)(")(?![\s\S]*<hh:font id=")', lambda m: f'{m.group(1)}X{m.group(3)}'),
        ('M9 run 하나 삭제', False, r'<hp:run [^>]*>(?:(?!</hp:run>).)*</hp:run>', lambda m: ''),
    ]
    must_ignore = [
        ('N1 속성 순서 바꿈(의미 같음)', False, r'(<hp:p )(paraPrIDRef="[^"]*") (styleIDRef="[^"]*")', lambda m: f'{m.group(1)}{m.group(3)} {m.group(2)}'),
        ('N2 요소 사이 공백뿐인 글 추가', False, r'(</hp:run>)(<hp:linesegarray>)', lambda m: m.group(1) + chr(10) + '  ' + m.group(2)),
        ('N3 접두사 이름 바꿈(주소 같음)', False, None, None),
    ]
    rows, bad = [], 0
    for doc, pname, cfgname in SABOTAGE_PAIRS:
        if pname not in procs or doc not in corpus:
            continue
        _, src, jobs = corpus[doc]
        cfg = config_dict(dict(jobs).get(pname))
        out_py, out_rs = work / 'sab' / f'{doc}.py.hwpx', work / 'sab' / f'{doc}.rs.hwpx'
        _, pn, perr = run_py(procs[pname], src, out_py, cfg)
        _, rn, rerr = run_rs(binpath, pname, src, out_rs, cfg, work / 'sab' / f'{doc}.json')
        base_same = bool(not perr and not rerr and compare_outputs(out_py, out_rs)[0])
        rec = {'doc': doc, 'processor': pname, 'base_identical': base_same, 'py_count': pn, 'detected': [], 'missed': [], 'not_applicable': [], 'ignored_ok': [], 'ignored_wrong': []}
        if not base_same:
            bad += 1
        for label, hdr, pat, fn in must_detect + must_ignore:
            m = work / 'sab' / f'{doc}.mut.hwpx'
            _MUT_HEADER[0] = hdr
            if label.startswith('N3'):
                def swap(name, data):
                    if name.startswith('Contents/') and name.endswith('.xml'):
                        t = data.decode('utf-8').replace('hp:', 'zz:').replace('xmlns:hp=', 'xmlns:zz=')
                        return t.encode('utf-8')
                    return data
                changed = _mut_zip(out_rs, m, swap)
            else:
                changed = _mut_zip(out_rs, m, _re_sub_first(pat, fn))
            if not changed:
                rec['not_applicable'].append(label)
                continue
            same = compare_outputs(out_py, m)[0]
            if label.startswith('M'):
                (rec['missed'] if same else rec['detected']).append(label)
            else:
                (rec['ignored_ok'] if same else rec['ignored_wrong']).append(label)
        if rec['missed'] or rec['ignored_wrong']:
            bad += 1
        rows.append(rec)
    return rows, bad


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument('--bin')
    ap.add_argument('--json')
    ap.add_argument('--bench', action='store_true')
    ap.add_argument('--bench3', action='store_true', help='RSXML3 처리기 속도만 잰다')
    ap.add_argument('--bench4', action='store_true', help='RSXML4 처리기 속도만 잰다')
    ap.add_argument('--keep')
    ap.add_argument('--sabotage', action='store_true', help='출력을 일부러 망가뜨려 비교기가 잡는지 확인')
    ap.add_argument('--only', help='처리기 이름을 쉼표로(기본: 전부)')
    args = ap.parse_args(argv)
    global ONLY
    ONLY = set(args.only.split(',')) if args.only else None
    binpath = find_bin(args.bin)
    if not binpath:
        print('hwpx_fmt 바이너리를 찾지 못했습니다(--bin 또는 HWPX_FMT_BIN).', file=sys.stderr)
        return 3
    work = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix='rsxml_parity_'))
    work.mkdir(parents=True, exist_ok=True)
    procs = load_python_processors()
    if ONLY:
        procs = {k: v for k, v in procs.items() if k in ONLY}
    results, failures = run_parity(binpath, work, procs)
    if args.bench4:
        print(json.dumps(bench4(binpath, work, procs), ensure_ascii=False))
        shutil.rmtree(work, ignore_errors=True)
        return 0
    if args.bench3:
        print(json.dumps(bench3(binpath, work, procs), ensure_ascii=False))
        shutil.rmtree(work, ignore_errors=True)
        return 0
    if args.sabotage:
        rows, bad = run_sabotage(binpath, work, procs)
        for r in rows:
            print(json.dumps(r, ensure_ascii=False))
        print('sabotage_bad', bad)
        if not args.keep:
            shutil.rmtree(work, ignore_errors=True)
        return 1 if bad else 0
    unsupported, unsupported_failures = run_unsupported(binpath, work)
    docs = {r['doc'] for r in results}
    failed_docs = {r['doc'] for r in results if not r['ok']}
    changed = sum(1 for r in results if r['py_count'])
    summary = {'docs': len(docs), 'doc_processor_pairs': len(results), 'passed_pairs': len(results) - failures,
               'failed_pairs': failures, 'passed_docs': len(docs) - len(failed_docs), 'failed_docs': len(failed_docs),
               'pairs_with_changes': changed, 'strict_whitespace_noise_pairs': sum(1 for r in results if r['strict_noise']),
               'both_rejected_pairs': sum(1 for r in results if r['py_error'] and r['rs_error']),
               'unsupported_checks': len(unsupported), 'unsupported_failed': unsupported_failures}
    per = {}
    for r in results:
        d = per.setdefault(r['processor'], {'pairs': 0, 'passed': 0, 'changed': 0, 'docs_targeted': 0, 'both_rejected': 0})
        d['pairs'] += 1
        d['passed'] += bool(r['ok'])
        d['changed'] += bool(r['py_count'])
        d['docs_targeted'] += r['kind'] in ('rsxml2', 'rsxml3', 'rsxml4', 'fixture_mutated')
        d['both_rejected'] += bool(r['py_error'] and r['rs_error'])
    summary['per_processor'] = per
    print(json.dumps(summary, ensure_ascii=False))
    for r in unsupported:
        if not r['ok']:
            print(f"미지원 검사 실패: {r['case']} [{r['processor']}]")
            for d in r['diffs']:
                print('   ', d)
    for r in results:
        if not r['ok']:
            print(f"차이: {r['doc']} [{r['processor']}]")
            for d in r['diffs']:
                print('   ', d)
    report = {'summary': summary, 'results': results, 'unsupported': unsupported}
    if args.bench:
        report['bench'] = bench(binpath, work, procs)
        report['bench3'] = bench3(binpath, work, procs)
        print(json.dumps(report['bench3'], ensure_ascii=False))
        print(json.dumps(report['bench'], ensure_ascii=False))
    if args.json:
        Path(args.json).write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding='utf-8')
    if not args.keep:
        shutil.rmtree(work, ignore_errors=True)
    return 1 if (failures or unsupported_failures) else 0


if __name__ == '__main__':
    sys.exit(main())
