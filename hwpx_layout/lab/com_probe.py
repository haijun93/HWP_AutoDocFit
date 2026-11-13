"""COMX1 실험: 한/글 COM으로 읽은 줄 시작·쪽과 HWPX lineseg 대조(사본만 사용).

사용: python com_probe.py REPO LAB_DIR
"""
import sys, json, time, shutil, zipfile, hashlib, re
from pathlib import Path

repo, lab = Path(sys.argv[1]), Path(sys.argv[2])
sys.path.insert(0, str(repo))
sys.path.insert(0, str(lab))
from docfit_core.save_guard import com_test_lock
import pythoncom
import win32com.client as win32
import xmlsegs

OUT = lab / 'out'
OUT.mkdir(exist_ok=True)
LOG = []


def log(*a):
    s = ' '.join(str(x) for x in a)
    LOG.append(s)
    print(s, flush=True)


def start():
    pythoncom.CoInitialize()
    hwp = win32.DispatchEx('HwpFrame.HwpObject')
    hwp.XHwpWindows.Item(0).Visible = True
    ok = hwp.RegisterModule('FilePathCheckDLL', 'HwpAutoDocFitSecurity')
    log('RegisterModule', ok)
    return hwp


def open_doc(hwp, path, fmt='HWPX'):
    ok = hwp.Open(str(path), fmt, 'forceopen:true;versionwarning:false;suspendpassword:true')
    log('Open', path.name, ok)
    return ok


def save_as(hwp, path, fmt='HWPX'):
    ok = hwp.SaveAs(str(path), fmt, '')
    log('SaveAs', path.name, ok)
    return ok


def current_page(hwp):
    return int(hwp.XHwpDocuments.Active_XHwpDocument.XHwpDocumentInfo.CurrentPage) + 1


def sel_text(hwp):
    hwp.InitScan(None, 0xff, None, None, None, None)
    out = []
    try:
        while True:
            st, t = hwp.GetText()
            if st in (0, 1) or st >= 100:
                break
            if st == 2:
                out.append(t)
    finally:
        hwp.ReleaseScan()
    return ''.join(out)


def para_info(hwp, l, p):
    """(길이, 텍스트) 또는 None."""
    hwp.SetPos(l, p, 0)
    g = hwp.GetPos()
    if (g[0], g[1]) != (l, p):
        return None
    hwp.Run('MoveParaEnd')
    e = hwp.GetPos()
    if (e[0], e[1]) != (l, p):
        return None
    n = e[2]
    hwp.SetPos(l, p, 0)
    hwp.Run('MoveSelParaEnd')
    text = sel_text(hwp)
    hwp.Run('Cancel')
    return n, text


def enumerate_lists(hwp, max_list=400, gap=40):
    """list id -> [(para, len, text)]"""
    lists = {}
    miss = 0
    for l in range(max_list):
        paras = []
        p = 0
        while True:
            info = para_info(hwp, l, p)
            if info is None:
                break
            paras.append((p, info[0], info[1]))
            p += 1
        if paras:
            lists[l] = paras
            miss = 0
        else:
            miss += 1
            if miss > gap and l > 0:
                break
    return lists


def probe_chars(hwp, l, p, n):
    """글자마다 KeyIndicator로 (쪽, 줄) 바뀌는 위치 = 줄 시작(정답)."""
    starts = []
    prev = None
    calls = 0
    for c in range(max(n, 1)):
        hwp.SetPos(l, p, c)
        g = hwp.GetPos()
        calls += 2
        if g[2] != c:
            continue
        ki = hwp.KeyIndicator()
        page = current_page(hwp)
        calls += 2
        key = (page, ki[4], ki[5])  # 쪽, 단, 줄
        if key != prev:
            starts.append({'pos': c, 'page': page, 'col': ki[4], 'line': ki[5], 'prnpage': ki[3]})
            prev = key
    return starts, calls


def walk_lines(hwp, l, p, n):
    """일반 COM 방식: MoveLineEnd → MoveNextChar 반복으로 줄 시작 찾기."""
    hwp.SetPos(l, p, 0)
    starts = [{'pos': 0, 'page': current_page(hwp)}]
    calls = 3
    guard = 0
    while guard < 200:
        guard += 1
        hwp.Run('MoveLineEnd')
        e = hwp.GetPos()
        calls += 2
        if (e[0], e[1]) != (l, p) or e[2] >= n:
            break
        hwp.Run('MoveNextChar')
        g = hwp.GetPos()
        calls += 2
        if (g[0], g[1]) != (l, p) or g == e:
            break
        page = current_page(hwp)
        calls += 1
        starts.append({'pos': e[2], 'page': page})
        hwp.SetPos(*g)
        calls += 1
    return starts, calls


def com_layout(hwp, mode='chars', lists=None):
    t0 = time.perf_counter()
    if lists is None:
        lists = enumerate_lists(hwp)
    t1 = time.perf_counter()
    res = {}
    calls = 0
    for l, paras in lists.items():
        for p, n, text in paras:
            fn = probe_chars if mode == 'chars' else walk_lines
            st, c = fn(hwp, l, p, n)
            calls += c
            res[(l, p)] = {'len': n, 'text': text, 'starts': st}
    t2 = time.perf_counter()
    return res, lists, {'enum_s': round(t1 - t0, 3), 'probe_s': round(t2 - t1, 3), 'calls': calls}


def xml_layout(path):
    """lineseg로 줄 시작과 쪽 계산(본문 흐름 vertpos 감소 = 새 쪽)."""
    paras = xmlsegs.paragraphs(path)
    page = 1
    last_body_vpos = -1
    body_page = 1
    for item in paras:
        segs = item['segs']
        if item['ctx'] == ['body']:
            pages = []
            for i, s in enumerate(segs):
                if (last_body_vpos >= 0 and s['vertpos'] < last_body_vpos) or (i == 0 and item['pageBreak'] == '1' and last_body_vpos >= 0):
                    body_page += 1
                last_body_vpos = s['vertpos']
                pages.append(body_page)
            item['pages'] = pages
        else:
            item['pages'] = [body_page] * len(segs)  # 표 칸: 부모 문단 쪽(근사)
        item['starts'] = [s['textpos'] for s in segs]
    return paras


def match(com, lists, xmlp):
    """COM 목록(본문 list0 + 나머지 list id 순)과 XML 문서 순서를 텍스트로 맞춘다."""
    body_x = [x for x in xmlp if x['ctx'] == ['body']]
    sub_x = [x for x in xmlp if x['ctx'] != ['body']]
    pairs = []
    for x, (p, n, text) in zip(body_x, lists.get(0, [])):
        pairs.append((x, com[(0, p)]))
    sub_c = [com[(l, p)] for l in sorted(lists) if l != 0 for p, n, t in lists[l]]
    # 텍스트로 순차 매칭
    j = 0
    for x in sub_x:
        k = j
        while k < len(sub_c) and norm(sub_c[k]['text']) != norm(x['text']):
            k += 1
        if k < len(sub_c):
            pairs.append((x, sub_c[k]))
            j = k + 1
        else:
            pairs.append((x, None))
    return pairs, len(body_x), len(lists.get(0, [])), len(sub_x), len(sub_c)


def norm(s):
    return re.sub(r'\s+', '', s or '')


def compare(pairs):
    rows = []
    for x, c in pairs:
        row = {'ctx': '/'.join(x['ctx']), 'i': x['i'], 'text': x['text'][:24]}
        if c is None:
            row['status'] = 'COM 짝 없음'
            rows.append(row)
            continue
        cs = [s['pos'] for s in c['starts']]
        cp = [s['page'] for s in c['starts']]
        row.update(xml_starts=x['starts'], com_starts=cs, xml_pages=x['pages'], com_pages=cp,
                   lines_diff=len(x['starts']) - len(cs),
                   starts_equal=x['starts'] == cs,
                   pages_equal=x['pages'] == cp,
                   text_equal=norm(x['text']) == norm(c['text']))
        rows.append(row)
    return rows


def summary(rows):
    ok = [r for r in rows if 'starts_equal' in r]
    return {'paragraphs': len(rows), 'paired': len(ok),
            'line_count_equal': sum(r['lines_diff'] == 0 for r in ok),
            'starts_equal': sum(r['starts_equal'] for r in ok),
            'pages_equal': sum(r['pages_equal'] for r in ok),
            'body_pages_equal': sum(r['pages_equal'] for r in ok if r['ctx'] == 'body'),
            'body_paired': sum(1 for r in ok if r['ctx'] == 'body'),
            'text_equal': sum(r['text_equal'] for r in ok)}


def rewrite_zip(src, dst, edit):
    """edit(name, bytes) -> bytes. mimetype 무압축 첫 항목 유지."""
    with zipfile.ZipFile(src) as zi, zipfile.ZipFile(dst, 'w') as zo:
        for info in zi.infolist():
            data = edit(info.filename, zi.read(info.filename))
            zo.writestr(info, data, compress_type=info.compress_type)


def run_case(hwp, name, path, mode='chars'):
    open_doc(hwp, path)
    com, lists, t = com_layout(hwp, mode)
    xmlp = xml_layout(path)
    pairs, nbx, nbc, nsx, nsc = match(com, lists, xmlp)
    rows = compare(pairs)
    s = summary(rows) | {'xml_body': nbx, 'com_body': nbc, 'xml_sub': nsx, 'com_sub': nsc, 'timing': t,
                         'pages_com': max((st['page'] for v in com.values() for st in v['starts']), default=0)}
    log(name, json.dumps(s, ensure_ascii=False))
    (OUT / f'{name}.json').write_text(json.dumps({'summary': s, 'rows': rows}, ensure_ascii=False, indent=1), encoding='utf-8')
    return com, lists, rows, s


def set_charshape(hwp, **kw):
    act = hwp.HAction
    pset = hwp.HParameterSet.HCharShape
    act.GetDefault('CharShape', pset.HSet)
    for k, v in kw.items():
        setattr(pset, k, v)
    return act.Execute('CharShape', pset.HSet)


def set_parashape(hwp, **kw):
    act = hwp.HAction
    pset = hwp.HParameterSet.HParaShape
    act.GetDefault('ParagraphShape', pset.HSet)
    for k, v in kw.items():
        setattr(pset, k, v)
    return act.Execute('ParagraphShape', pset.HSet)


def select_para(hwp, l, p):
    hwp.SetPos(l, p, 0)
    hwp.Run('MoveSelParaEnd')


def main():
    src = lab / 'demo.hwpx'
    digest = hashlib.sha256(src.read_bytes()).hexdigest()
    with com_test_lock('COMX1', 'lineseg 대조', timeout_sec=600):
        hwp = start()
        try:
            # 1) 사본 열고 다시 저장
            work = OUT / 'demo_copy.hwpx'
            shutil.copy2(src, work)
            open_doc(hwp, work)
            t = time.perf_counter(); save_as(hwp, OUT / 'resaved.hwpx'); log('save_s', round(time.perf_counter() - t, 3))
            hwp.Clear(1)
            # 2) 정답(글자별) 대조
            com, lists, rows, s = run_case(hwp, 'A_resaved_chars', OUT / 'resaved.hwpx', 'chars')
            # 2b) 일반 COM 방식(줄 끝 이동) 시간·일치
            hwp.Clear(1)
            run_case(hwp, 'A_resaved_walk', OUT / 'resaved.hwpx', 'walk')
            # 3a) COM으로 서식 변경 후 저장 → lineseg 갱신?
            body = lists[0]
            multi = [p for p, n, txt in body if len(com[(0, p)]['starts']) >= 2][:4]
            log('targets', multi)
            ops = []
            if len(multi) >= 4:
                select_para(hwp, 0, multi[0]); ops.append(('자간-15', set_charshape(hwp, SpacingHangul=-15, SpacingLatin=-15, SpacingHanja=-15, SpacingJapanese=-15, SpacingOther=-15, SpacingSymbol=-15, SpacingUser=-15)))
                select_para(hwp, 0, multi[1]); ops.append(('장평80', set_charshape(hwp, RatioHangul=80, RatioLatin=80, RatioHanja=80, RatioJapanese=80, RatioOther=80, RatioSymbol=80, RatioUser=80)))
                select_para(hwp, 0, multi[2]); ops.append(('크기+3pt', set_charshape(hwp, Height=1800)))
                select_para(hwp, 0, multi[3]); ops.append(('내어쓰기', set_parashape(hwp, Indentation=-3000)))
                hwp.Run('Cancel')
            log('ops', ops)
            save_as(hwp, OUT / 'formatted.hwpx')
            hwp.Clear(1)
            run_case(hwp, 'B_formatted_chars', OUT / 'formatted.hwpx', 'chars')
            hwp.Clear(1)
            # 3b) XML만 고친 파일(lineseg 그대로): resaved의 header charPr 자간을 -40으로
            def edit_spacing(name, data):
                if name == 'Contents/header.xml':
                    return re.sub(rb'(<hh:spacing )hangul="-?\d+" latin="-?\d+" hanja="-?\d+" japanese="-?\d+" other="-?\d+" symbol="-?\d+" user="-?\d+"',
                                  rb'\1hangul="-40" latin="-40" hanja="-40" japanese="-40" other="-40" symbol="-40" user="-40"', data)
                return data
            rewrite_zip(OUT / 'resaved.hwpx', OUT / 'xmlonly_spacing.hwpx', edit_spacing)
            run_case(hwp, 'C_xmlonly_stale_chars', OUT / 'xmlonly_spacing.hwpx', 'chars')
            save_as(hwp, OUT / 'xmlonly_spacing_resaved.hwpx')
            hwp.Clear(1)
            run_case(hwp, 'C2_xmlonly_resaved_chars', OUT / 'xmlonly_spacing_resaved.hwpx', 'chars')
            hwp.Clear(1)
            # 3c) lineseg를 모두 지운 파일 → 열 때 다시 계산? 저장하면 원래와 같나?
            def strip_segs(name, data):
                if name.startswith('Contents/section'):
                    return re.sub(rb'<hp:linesegarray>.*?</hp:linesegarray>', b'', data, flags=re.S)
                return data
            rewrite_zip(OUT / 'resaved.hwpx', OUT / 'noseg.hwpx', strip_segs)
            open_doc(hwp, OUT / 'noseg.hwpx')
            save_as(hwp, OUT / 'noseg_resaved.hwpx')
            hwp.Clear(1)
            run_case(hwp, 'D_noseg_resaved_chars', OUT / 'noseg_resaved.hwpx', 'chars')
            hwp.Clear(1)
            # 3d) 가짜 lineseg(모든 textpos 0, 1줄) → 열 때 무시하는지
            def fake_segs(name, data):
                if name.startswith('Contents/section'):
                    return re.sub(rb'(<hp:linesegarray>)(<hp:lineseg [^>]*/>)(?:<hp:lineseg [^>]*/>)*(</hp:linesegarray>)', rb'\1\2\3', data)
                return data
            rewrite_zip(OUT / 'resaved.hwpx', OUT / 'fakeseg.hwpx', fake_segs)
            run_case(hwp, 'E_fakeseg_chars', OUT / 'fakeseg.hwpx', 'chars')
            hwp.Clear(1)
            # 4) 글상자 문서(저장소 시험 자료 사본)
            shapes = OUT / 'shapes_copy.hwpx'
            shutil.copy2(repo / 'tests/fixtures/hwp_shapes_text_sample.hwpx', shapes)
            open_doc(hwp, shapes)
            save_as(hwp, OUT / 'shapes_resaved.hwpx')
            hwp.Clear(1)
            run_case(hwp, 'F_shapes_chars', OUT / 'shapes_resaved.hwpx', 'chars')
            hwp.Clear(1)
        finally:
            try:
                hwp.Clear(1)
                hwp.Quit()
            except Exception as e:
                log('quit err', e)
    log('original_unchanged', hashlib.sha256(src.read_bytes()).hexdigest() == digest)
    (OUT / 'log.txt').write_text('\n'.join(LOG), encoding='utf-8')


if __name__ == '__main__':
    main()
