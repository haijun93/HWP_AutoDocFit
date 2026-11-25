"""HWPXFMT5 구조 처리기: Python/Rust 선택·변경 수·정규화 바이트·원본 해시 대조.

실행 산출물은 --out 외부 폴더에만 저장한다. 사용자 문서는 사용하지 않는다.
"""
from __future__ import annotations

import argparse
import contextlib
import copy
import hashlib
import io
import json
import os
import re
import random
from pathlib import Path
import subprocess
import sys
import time

from defusedxml import ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parent))
import rsxml_parity as base

PROCESSORS = {'attachment': '붙임_hwpx_처리', 'box': '상자서식_hwpx_처리',
              'title': '제목_hwpx_처리', 'midtitle': '중제목_hwpx_처리',
              'report_header': '보고서머리_hwpx_처리'}


def load():
    base.load_python_processors()
    return {key: base.PY_NS[name] for key, name in PROCESSORS.items()}


@contextlib.contextmanager
def settings(fn, cfg):
    g = fn.__globals__
    changes = {'활성_서식표_프로필': cfg.get('format_samples'),
               '제목_담당자_글': cfg.get('title_owner_text', ''),
               '중제목_번호굵게': cfg.get('midtitle_number_bold', True),
               '괄호_축소_사용': cfg.get('title_parenthesis_reduce', True),
               '괄호_축소_pt': cfg.get('title_parenthesis_pt', 2),
               '제목_부제_크기_pt': cfg.get('title_subtitle_pt', 15),
               '활성_항목기호_체계': None}
    saved = {k: g.get(k) for k in changes}
    saved['표준서식_설정'] = g.get('표준서식_설정')
    if cfg.get('marker_roles'):
        changes['표준서식_설정'] = dict(g.get('표준서식_설정') or {}, 문두기호_역할=cfg['marker_roles'])
    g.update(changes)
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            yield
    finally:
        g.update(saved)


def run_python(fn, src, dst, cfg):
    with settings(fn, cfg):
        sel = fn(src)
        count = fn(src, dst, sel)
    return json.loads(json.dumps(sel)), count


def examples():
    h, s = base.PY_NS['개요붙임_원본자료']()
    tables = [x for x in s.iter() if x.tag.rsplit('}', 1)[-1] == 'tbl']
    def sample(t):
        return {'header_xml': ET.tostring(h, encoding='unicode'),
                'table_xml': ET.tostring(t, encoding='unicode')}
    return {'box': sample(tables[0]), 'attach1': sample(tables[1]), 'attach2': sample(tables[2])}


def cases():
    h, _ = base.PY_NS['개요붙임_원본자료']()
    header = ET.tostring(h, encoding='unicode')
    samples = examples()
    for i, text in enumerate(['□추진', 'ㅁ추진', 'ㅇ본문', '○본문', '-본문', '※주석', '*주석', '**주석',
                              '□ 추진', 'ㅇ 본문', '- 본문', '* 주석']):
        body = base.host(base.title_table_type1()) + base.para('') + base.para(text)
        yield f'title_marker_boundary_{i}', base.hwpx_bytes({'Contents/section0.xml': base.section(body)}, header=header), 'title', {}
    # 실제 내장 표의 sz/cellMargin/pagePr로 폭 맞춤도 실행한다.
    for proc, loader, ordinal in [('attachment', '개요붙임_원본자료', 1),
                                   ('box', '개요붙임_원본자료', 0),
                                   ('midtitle', '중제목_원본자료', 0),
                                   ('title', '제목_원본자료', 0)]:
        sh, ss = base.PY_NS[loader]()
        source_table = [x for x in ss.iter() if x.tag.rsplit('}', 1)[-1] == 'tbl'][ordinal]
        for width in (30001, 42520, 60000):
            for custom in (False, True):
                table = copy.deepcopy(source_table)
                for cellsize in (x for x in table.iter() if x.tag.rsplit('}', 1)[-1] == 'cellSz'):
                    cellsize.set('width', '7001')
                    cellsize.set('height', '100')
                body = base.para('', raw=f'<hp:secPr><hp:pagePr width="{width + 2000}" height="80000"><hp:margin left="1000" right="1000"/></hp:pagePr></hp:secPr>') + base.host(ET.tostring(table, encoding='unicode'))
                key = {'attachment': 'attach1', 'box': 'box', 'midtitle': 'midtitle1', 'title': 'title1'}[proc]
                cfg = {'format_samples': {key: {'header_xml': ET.tostring(sh, encoding='unicode'), 'table_xml': ET.tostring(source_table, encoding='unicode')}}} if custom or proc == 'box' else {}
                yield f'full_geometry_{proc}_{width}_{custom}', base.hwpx_bytes({'Contents/section0.xml': base.section(body)}, header=ET.tostring(sh, encoding='unicode')), proc, cfg
    rng = random.Random(20261124)
    for i in range(24):
        proc = ('attachment', 'box', 'midtitle', 'title')[i % 4]
        text = rng.choice(['핵심 추진 계획', '따옴표 ‘계획’과 본문', '행정 업무(부연 설명)', '한글 😀 여러 글'])
        if proc == 'attachment': body = base.host(base.grid([['붙임 2', '', text]]))
        elif proc == 'box': body = base.host(base.grid([[text + ' 핵심 내용을 충분히 길게 기록합니다.']]))
        elif proc == 'midtitle': body = base.host(base.grid([['Ⅱ', '', text]]))
        else: body = base.host(base.title_table_type1().replace('보고서 제목', text))
        # 섹션 압축 순서와 숫자 순서가 다른 경우, 병합 캐시 재사용, 이모지·따옴표.
        data = base.hwpx_bytes({'Contents/section10.xml': base.section(body),
                                'Contents/section2.xml': base.section(body)}, header=header,
                               extra_files={'BinData/synthetic.bin': b'\x00\xffunchanged'})
        cfg = {'format_samples': samples} if proc == 'box' or i % 2 else {}
        yield f'random_multisection_{i}_{proc}', data, proc, cfg
    for cols in (2, 3, 4):
        for number in ('Ⅰ', '2'):
            for bold in (False, True):
                row = [number, '추진 계획'] if cols == 2 else [number, '', '추진 계획']
                if cols == 4: row.append('추가')
                yield f'mid_{cols}_{number}_{bold}', base.hwpx_bytes({'Contents/section0.xml': base.section(base.host(base.grid([row])))}, header=header), 'midtitle', {'midtitle_number_bold': bold}
    for kind in (1, 2, 3):
        for overview in (False, True):
            for parens in (False, True):
                table = base.title_table_type1() if kind in (1, 2) else base.title_table_type3()
                if kind == 2:
                    table = table.replace(base.para('보고서 제목'), base.para('부제입니다') + base.para('보고서 제목'))
                if parens:
                    table = table.replace('보고서 제목', "추진 계획(부연 설명) ‘제목’").replace('제목입니다', '제목입니다(부연 설명)')
                body = base.host(table)
                if overview:
                    body += base.para('') + base.host(base.grid([['충분히 긴 개요 내용입니다(부연 설명) 핵심 내용입니다.']]))
                body += base.para('') + base.para('□ 추진 내용')
                yield f'title_{kind}_{overview}_{parens}', base.hwpx_bytes({'Contents/section0.xml': base.section(body)}, header=header), 'title', {}
    # 별도 예시 서식으로 실제 병합·런 분할·텍스트 보존 경로도 밟는다.
    for cols in (2, 3):
        for texts in (['붙임', '계획서'], ['붙임 12', "‘추진’ '계획'"], ['일반', '본문']):
            row = texts if cols == 2 else [texts[0], '', texts[1]]
            for profile in (False, True):
                name = f'attachment_{cols}_{texts[0]}_{profile}'
                body = base.host(base.grid([row]))
                cfg = {'format_samples': samples} if profile else {}
                yield name, base.hwpx_bytes({'Contents/section0.xml': base.section(body)}, header=header), 'attachment', cfg
    for count in (1, 2, 3, 4):
        for title_before in (False, True):
            for profile in (False, True):
                cell = base.tc(['내용을 보존하는 충분히 긴 한 칸 상자 본문입니다.'] * count)
                box = base.tbl([[cell]])
                body = (base.host(base.title_table_type1()) if title_before else '') + base.host(box)
                cfg = {'format_samples': {'box': samples['box']}} if profile else {}
                yield f'box_{count}_{title_before}_{profile}', base.hwpx_bytes({'Contents/section0.xml': base.section(body)}, header=header), 'box', cfg
    # 기존 합성·fixture도 원본을 읽기만 하고 외부 폴더의 사본으로 처리한다.
    for name, data in base.synthetic_cases().items():
        if name.startswith('invalid_'):
            continue
        for proc in ('attachment', 'box'):
            # 빈 최소 header는 복제 대상 서식을 담지 않는다. 변경 사례는 유효한
            # 내장 header로 감싸 실제 서식 병합 경로를 검사한다.
            from zipfile import ZipFile
            with ZipFile(io.BytesIO(data)) as z:
                files = {n: z.read(n) for n in z.namelist()}
            files['Contents/header.xml'] = header.encode('utf-8')
            yield f'common_{name}_{proc}', base.hwpx_bytes(
                {n: v for n, v in files.items() if re.fullmatch(r'Contents/section\d+\.xml', n)},
                extra_files={n: v for n, v in files.items() if n not in ('mimetype', 'Contents/header.xml') and not re.fullmatch(r'Contents/section\d+\.xml', n)}, header=header), proc, {}
    for path in sorted((base.ROOT / 'tests' / 'fixtures').glob('*.hwpx')):
        for proc in ('attachment', 'box'):
            yield f'fixture_{path.stem}_{proc}', path.read_bytes(), proc, {}


def unsupported_cases():
    for name, proc, body, cfg in [
        ('report_header', 'report_header', base.para('보고서 제목입니다'), {}),
        ('roman_text', 'midtitle', base.para('Ⅰ 추진 계획'), {}),
        ('roman_alias', 'midtitle', base.para('로1 추진 계획'), {}),
        ('boxed_title', 'title', base.host(base.grid([['한 칸 제목입니다']])), {}),
        ('owner_setting', 'attachment', base.host(base.grid([['붙임', '계획서']])), {'title_owner_text': '사용자 담당자'}),
        ('unknown_config', 'attachment', '', {'unknown_key': True}),
        ('custom_marker_roles', 'title', base.host(base.title_table_type1()), {'marker_roles': {'□': ''}}),
        ('box_bad_sample', 'box', '', {'format_samples': {'box': {'header_xml': '<head/>', 'table_xml': '<tbl/>'}}}),
        ('overview_across_body', 'title', base.host(base.title_table_type1()) + base.para('중간 본문') + base.host(base.grid([['한 칸 제목 후보가 될 수 있는 충분히 긴 내용입니다']])), {}),
    ]:
        yield name, proc, base.one(body), cfg


def check_unsupported(binary, out):
    rows = []
    for i, (name, proc, data, cfg) in enumerate(unsupported_cases()):
        folder = out / f'unsupported{i:02d}'
        folder.mkdir(exist_ok=True)
        source = folder / 'source.hwpx'; source.write_bytes(data)
        config = folder / 'config.json'; config.write_text(json.dumps(cfg, ensure_ascii=False), encoding='utf-8')
        selected = folder / 'selections.json'; selected.write_text('{}', encoding='utf-8')
        target = folder / 'output.hwpx'
        target.unlink(missing_ok=True)
        checks = []
        for command in ([str(binary), 'survey', proc, str(source)],
                        [str(binary), proc, str(source), str(target)]):
            result = subprocess.run(command + ['--config', str(config)], capture_output=True, text=True, encoding='utf-8')
            checks.append(result.returncode == 4 and '미지원 입력:' in result.stderr and not target.exists() and source.read_bytes() == data)
        if proc in ('title', 'midtitle'):
            result = subprocess.run([str(binary), proc, str(source), str(target), '--selections', str(selected), '--config', str(config)], capture_output=True, text=True, encoding='utf-8')
            checks.append(result.returncode == 4 and not target.exists())
        rows.append({'name': name, 'ok': all(checks)})
    return rows


def normalized_bytes(path):
    """접두사·속성 순서·t 밖 들여쓰기만 정규화한 패키지 바이트."""
    parts = []
    for name, data, compression in base.read_zip(path):
        if name.startswith('Contents/') and name.endswith('.xml'):
            data = json.dumps(base.norm_tree(ET.fromstring(data)), ensure_ascii=False,
                              separators=(',', ':')).encode('utf-8')
        key = name.encode('utf-8')
        parts += [len(key).to_bytes(8, 'big'), key, len(data).to_bytes(8, 'big'), data]
        if name == 'mimetype': parts.append(compression.to_bytes(2, 'big'))
    return b''.join(parts)


def run(binary, out, bench=False):
    procs = load()
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    for i, (name, data, proc, cfg) in enumerate(cases()):
        folder = out / f'case{i:04d}'
        folder.mkdir(exist_ok=True)
        src, py, rs = [folder / x for x in ('source.hwpx', 'python.hwpx', 'rust.hwpx')]
        src.write_bytes(data)
        before = base.sha(src)
        try:
            ps, pn = run_python(procs[proc], src, py, cfg)
            rsel, rn, err = base.run_rs(str(binary), proc, src, rs, cfg, folder / 'config.json')
            same, diffs, strict = base.compare_outputs(py, rs) if err is None else (False, [err], None)
            if err is None and normalized_bytes(py) != normalized_bytes(rs):
                diffs.append('정규화 바이트 불일치')
            if ps != rsel: diffs.append('선택 목록 불일치')
            if pn != rn: diffs.append('변경 수 불일치')
            if base.sha(src) != before: diffs.append('원본 변경')
            rows.append({'name': name, 'processor': proc, 'ok': same and not diffs,
                         'count': pn, 'strict_noise': strict, 'diffs': diffs})
        except Exception as exc:
            # 양쪽 실패는 동등성 통과 건수에서 분리한다. 차이 0 검증으로 세지 않는다.
            rsel, rn, err = base.run_rs(str(binary), proc, src, rs, cfg, folder / 'config.json')
            rejected = err is not None and not py.exists() and not rs.exists()
            rows.append({'name': name, 'processor': proc, 'ok': None if rejected else False,
                         'both_rejected': rejected, 'error': f'{type(exc).__name__}: {exc}',
                         'rust_error': err, 'source_unchanged': base.sha(src) == before})
    measurements = []
    if bench:
        h, _ = base.PY_NS['개요붙임_원본자료']()
        header = ET.tostring(h, encoding='unicode')
        for proc, table in [('attachment', base.grid([['붙임', '', '추진 계획서']])),
                            ('box', base.grid([['내용을 보존하는 충분히 긴 한 칸 상자 본문입니다.']])),
                            ('midtitle', base.grid([['Ⅰ', '', '추진 계획']])),
                            ('title', base.title_table_type1())]:
            cfg = {'format_samples': examples()}
            src, py, rs = [out / f'bench_{proc}_{x}.hwpx' for x in ('source', 'python', 'rust')]
            src.write_bytes(base.hwpx_bytes({'Contents/section0.xml': base.section(base.host(*([table] * 400)))}, header=header))
            start = time.perf_counter(); ps, pn = run_python(procs[proc], src, py, cfg); pt = time.perf_counter() - start
            start = time.perf_counter(); ss, sn, err = base.run_rs(str(binary), proc, src, rs, cfg, out / 'bench_config.json'); rt = time.perf_counter() - start
            same, diffs, _ = base.compare_outputs(py, rs) if err is None else (False, [err], None)
            measurements.append({'processor': proc, 'python_seconds': pt, 'rust_seconds': rt,
                                 'speedup': pt / rt, 'count': pn, 'ok': same and pn == sn and ps == ss, 'diffs': diffs})
        for proc, loader, ordinal in [('attachment', '개요붙임_원본자료', 1),
                                       ('box', '개요붙임_원본자료', 0),
                                       ('midtitle', '중제목_원본자료', 0),
                                       ('title', '제목_원본자료', 0)]:
            sh, ss = base.PY_NS[loader]()
            table = [x for x in ss.iter() if x.tag.rsplit('}', 1)[-1] == 'tbl'][ordinal]
            body = base.para('', raw='<hp:secPr><hp:pagePr width="44520" height="80000"><hp:margin left="1000" right="1000"/></hp:pagePr></hp:secPr>')
            body += base.host(*([ET.tostring(table, encoding='unicode')] * 400))
            cfg = {'format_samples': examples()}
            src, py, rs = [out / f'bench_geometry_{proc}_{x}.hwpx' for x in ('source', 'python', 'rust')]
            src.write_bytes(base.hwpx_bytes({'Contents/section0.xml': base.section(body)}, header=ET.tostring(sh, encoding='unicode')))
            start = time.perf_counter(); ps, pn = run_python(procs[proc], src, py, cfg); pt = time.perf_counter() - start
            start = time.perf_counter(); ss, sn, err = base.run_rs(str(binary), proc, src, rs, cfg, out / 'bench_config.json'); rt = time.perf_counter() - start
            same, diffs, _ = base.compare_outputs(py, rs) if err is None else (False, [err], None)
            measurements.append({'processor': proc, 'geometry': True, 'python_seconds': pt,
                                 'rust_seconds': rt, 'speedup': pt / rt, 'count': pn,
                                 'ok': same and pn == sn and ps == ss, 'diffs': diffs})
    unsupported = check_unsupported(binary, out)
    result = {'pairs': len(rows), 'passed': sum(r['ok'] is True for r in rows),
              'failed': sum(r['ok'] is False or r.get('source_unchanged') is False for r in rows),
              'both_rejected': sum(r.get('both_rejected', False) for r in rows),
              'changed': sum(bool(r.get('count')) for r in rows), 'results': rows, 'bench': measurements,
              'unsupported': unsupported, 'unsupported_failed': sum(not r['ok'] for r in unsupported)}
    (out / 'parity.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--bin', type=Path, default=base.find_bin(None))
    ap.add_argument('--out', type=Path, required=True)
    ap.add_argument('--bench', action='store_true')
    args = ap.parse_args()
    result = run(args.bin, args.out, args.bench)
    print(json.dumps({k: v for k, v in result.items() if k != 'results'}, ensure_ascii=False))
    for row in result['results']:
        if row['ok'] is not True: print(json.dumps(row, ensure_ascii=False))
    return int(bool(result['failed'] or result['unsupported_failed'] or any(not r['ok'] for r in result['bench'])))


if __name__ == '__main__':
    raise SystemExit(main())
