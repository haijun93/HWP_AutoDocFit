"""HWPX section XML에서 문단별 lineseg를 읽는다(실험용, defusedxml)."""
import zipfile, re
from defusedxml import ElementTree as ET

HP = '{http://www.hancom.co.kr/hwpml/2011/paragraph}'


def _text(p):
    out = []
    for run in p.findall(HP + 'run'):
        for ch in run:
            if ch.tag == HP + 't':
                out.append(''.join(ch.itertext()))
    return ''.join(out)


def paragraphs(path):
    """문서 순서로 (context, index_in_list, text, pageBreak, segs, para_elem_info)."""
    z = zipfile.ZipFile(path)
    names = sorted(n for n in z.namelist() if re.match(r'Contents/section\d+\.xml$', n))
    result = []
    for name in names:
        root = ET.fromstring(z.read(name))

        def walk(list_elem, ctx):
            idx = 0
            for p in list_elem.findall(HP + 'p'):
                segs = []
                arr = p.find(HP + 'linesegarray')
                if arr is not None:
                    for s in arr.findall(HP + 'lineseg'):
                        segs.append({k: int(v) for k, v in s.attrib.items()})
                result.append({'ctx': ctx, 'i': idx, 'text': _text(p),
                               'pageBreak': p.get('pageBreak'), 'segs': segs,
                               'has_linesegarray': arr is not None})
                # nested sublists (table cells, textboxes) in document order
                for sub in p.iter(HP + 'subList'):
                    pass
                for run in p.findall(HP + 'run'):
                    for ctrl in run:
                        tag = ctrl.tag.replace(HP, '')
                        for sub in _direct_sublists(ctrl):
                            walk(sub, ctx + [tag])
                idx += 1
        walk(root, ['body'] if True else [])
    return result


def _direct_sublists(elem):
    """elem 아래 subList 중 다른 subList 안에 있지 않은 것(문서 순서)."""
    out = []
    def rec(e):
        for c in e:
            if c.tag == HP + 'subList':
                out.append(c)
            else:
                rec(c)
    rec(elem)
    return out


if __name__ == '__main__':
    import sys, json
    for item in paragraphs(sys.argv[1]):
        print(json.dumps({k: item[k] for k in ('ctx', 'i', 'pageBreak')} | {'n': len(item['segs']), 'text': item['text'][:30]}, ensure_ascii=False))
