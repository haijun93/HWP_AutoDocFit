"""COMX1 추가 실험: 고친 문단만 lineseg 삭제(부분), 열기·저장 시간."""
import sys, json, time, re, hashlib
from pathlib import Path
repo, lab = Path(sys.argv[1]), Path(sys.argv[2])
sys.path.insert(0, str(repo)); sys.path.insert(0, str(lab))
import com_probe as C
from docfit_core.save_guard import com_test_lock

OUT = C.OUT


def main():
    base = OUT / 'resaved.hwpx'
    # 문단 3,4,7,9(본문 직속, 0부터)의 charPr를 새 charPr(-40 자간)로 바꾸고 그 문단 lineseg만 삭제
    import zipfile
    z = zipfile.ZipFile(base)
    sec = z.read('Contents/section0.xml').decode('utf-8')
    head = z.read('Contents/header.xml').decode('utf-8')
    # 새 charPr: 첫 charPr 복제 + id 새 번호
    m = re.search(r'<hh:charPr id="(\d+)".*?</hh:charPr>', head, re.S)
    ids = [int(x) for x in re.findall(r'<hh:charPr id="(\d+)"', head)]
    new_id = max(ids) + 1
    targets = [3, 4, 7, 9]

    def body_paras(xml):
        # 최상위 hp:p 위치(깊이 1) 찾기
        out, depth, i = [], 0, 0
        for mm in re.finditer(r'<(/?)hp:(p|subList)\b[^>]*?(/?)>', xml):
            closing, tag, selfc = mm.group(1), mm.group(2), mm.group(3)
            if tag == 'p' and not closing and not selfc:
                if depth == 0:
                    out.append([mm.start(), None])
                depth += 1 if True else 0
            elif tag == 'p' and closing:
                depth -= 1
                if depth == 0:
                    out[-1][1] = mm.end()
        return out
    spans = body_paras(sec)
    print('body spans', len(spans))
    variants = {}
    for strip in ('stale', 'partial'):
        s = sec
        for t in sorted(targets, reverse=True):
            a, b = spans[t]
            para = s[a:b]
            # 원래 charPr을 복제한 새 charPr 하나에 자간 -40을 주고 그 문단 모든 run에 적용
            para2 = re.sub(r'charPrIDRef="\d+"', f'charPrIDRef="{new_id}"', para)
            if strip == 'partial':
                # 그 문단 자신의 linesegarray(마지막)만 삭제
                k = para2.rfind('<hp:linesegarray>')
                e = para2.find('</hp:linesegarray>', k) + len('</hp:linesegarray>')
                para2 = para2[:k] + para2[e:]
            s = s[:a] + para2 + s[b:]
        # 새 charPr: 대상 첫 문단의 첫 run charPr를 본떠 자간만 -40
        first_ref = re.search(r'charPrIDRef="(\d+)"', sec[spans[targets[0]][0]:spans[targets[0]][1]]).group(1)
        proto = re.search(rf'<hh:charPr id="{first_ref}".*?</hh:charPr>', head, re.S).group(0)
        newpr = proto.replace(f'id="{first_ref}"', f'id="{new_id}"', 1)
        newpr = re.sub(r'<hh:spacing [^/]*/>', '<hh:spacing hangul="-40" latin="-40" hanja="-40" japanese="-40" other="-40" symbol="-40" user="-40"/>', newpr)
        h = head.replace('</hh:charProperties>', newpr + '</hh:charProperties>')
        h = re.sub(r'(<hh:charProperties itemCnt=")(\d+)"', lambda mm: f'{mm.group(1)}{int(mm.group(2))+1}"', h)
        dst = OUT / f'G_{strip}.hwpx'
        def edit(name, data, s=s, h=h):
            if name == 'Contents/section0.xml':
                return s.encode('utf-8')
            if name == 'Contents/header.xml':
                return h.encode('utf-8')
            return data
        C.rewrite_zip(base, dst, edit)
        variants[strip] = dst
    timing = {}
    with com_test_lock('COMX1', 'lineseg 부분 삭제', timeout_sec=600):
        hwp = C.start()
        try:
            for strip, path in variants.items():
                t = time.perf_counter(); C.open_doc(hwp, path); timing[f'open_{strip}'] = round(time.perf_counter() - t, 3)
                com, lists, tt = C.com_layout(hwp, 'chars')
                t = time.perf_counter(); C.save_as(hwp, OUT / f'G_{strip}_resaved.hwpx'); timing[f'save_{strip}'] = round(time.perf_counter() - t, 3)
                hwp.Clear(1)
                xmlp = C.xml_layout(OUT / f'G_{strip}_resaved.hwpx')
                pairs, *_ = C.match(com, lists, xmlp)
                rows = C.compare(pairs)
                s = C.summary(rows)
                base_rows = json.load(open(OUT / 'A_resaved_chars.json', encoding='utf-8'))['rows']
                changed = [(r['i'], b['com_starts'], r['com_starts']) for r, b in zip(rows, base_rows) if r['ctx'] == 'body' and r['com_starts'] != b['com_starts']]
                pages = max(st['page'] for v in com.values() for st in v['starts'])
                C.log(f'G_{strip}', json.dumps(s | {'pages_com': pages, 'com_changed_vs_A': changed}, ensure_ascii=False))
                (OUT / f'G_{strip}.json').write_text(json.dumps({'summary': s, 'rows': rows, 'changed': changed}, ensure_ascii=False, indent=1), encoding='utf-8')
            # 열기·저장 시간(작업 문서 기준, 측정 1회)
            t = time.perf_counter(); C.open_doc(hwp, base); timing['open_resaved'] = round(time.perf_counter() - t, 3)
            t = time.perf_counter(); C.save_as(hwp, OUT / 'timing_save.hwpx'); timing['saveas_hwpx'] = round(time.perf_counter() - t, 3)
            hwp.Clear(1)
        finally:
            try:
                hwp.Clear(1); hwp.Quit()
            except Exception:
                pass
    C.log('timing', json.dumps(timing))
    (OUT / 'log2.txt').write_text('\n'.join(C.LOG), encoding='utf-8')


main()
