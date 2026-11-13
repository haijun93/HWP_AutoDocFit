"""Rust hwpx-layout 출력과 COM 정답(글자별 KeyIndicator) 대조. 본문/하위 목록을 따로 짝짓는다."""
import sys, json, subprocess, re, time
from pathlib import Path

exe, out = sys.argv[1], Path(sys.argv[2])
cases = [('A_resaved_chars', 'resaved.hwpx'), ('B_formatted_chars', 'formatted.hwpx'),
         ('C2_xmlonly_resaved_chars', 'xmlonly_spacing_resaved.hwpx'), ('D_noseg_resaved_chars', 'noseg_resaved.hwpx'),
         ('G_partial', 'G_partial_resaved.hwpx'), ('F_shapes_chars', 'shapes_resaved.hwpx'),
         ('C_xmlonly_stale_chars', 'xmlonly_spacing.hwpx'), ('G_stale', 'G_stale_resaved.hwpx'),
         ('E_fakeseg_chars', 'fakeseg.hwpx')]
norm = lambda s: re.sub(r'\s+', '', s or '')
report = {}
for name, f in cases:
    rows = json.load(open(out / f'{name}.json', encoding='utf-8'))['rows']
    t = time.perf_counter()
    p = subprocess.run([exe, str(out / f)], capture_output=True)
    wall = time.perf_counter() - t
    r = json.loads(p.stdout.decode('utf-8'))
    rp = r['paras']
    rb = [a for a in rp if not a['ctx']]
    rs = [a for a in rp if a['ctx']]
    cb = [c for c in rows if c['ctx'] == 'body']
    cs_ = [c for c in rows if c['ctx'] != 'body']
    pairs = list(zip(rb, cb)) + list(zip(rs, cs_))
    st = dict(paras=len(rp), com_rows=len(rows), paired=0, lines_equal=0, starts_equal=0, starts_equal_ctrl_prefix=0,
              pages_equal=0, text_equal=0)
    diffs = []
    for a, c in pairs:
        if 'com_starts' not in c:
            diffs.append({'no_com': c.get('text'), 'ctx': c['ctx']}); continue
        st['paired'] += 1
        cs, cp = c['com_starts'], c['com_pages']
        st['lines_equal'] += len(a['starts']) == len(cs)
        st['starts_equal'] += a['starts'] == cs
        ok_pref = len(a['starts']) == len(cs) and a['starts'][1:] == cs[1:]
        st['starts_equal_ctrl_prefix'] += ok_pref
        st['pages_equal'] += a['pages'][:len(cp)] == cp
        st['text_equal'] += norm(a['text']) == norm(c['text'])
        if not ok_pref or a['pages'][:len(cp)] != cp:
            diffs.append({'ctx': a['ctx'], 'i': a['index'], 'rust': [a['starts'], a['pages']], 'com': [cs, cp], 'text': a['text'][:20]})
    report[name] = {'stat': st, 'diffs': diffs, 'rust_pages': r['pages'], 'missing': r['missing_lineseg'], 'wall_s': round(wall, 4)}
    print(name, json.dumps(st, ensure_ascii=False), 'rust_pages', r['pages'], 'missing', r['missing_lineseg'], 'wall_s', round(wall, 4))
    for d in diffs[:8]:
        print('   ', json.dumps(d, ensure_ascii=False))
(out / 'rust_vs_com.json').write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding='utf-8')
