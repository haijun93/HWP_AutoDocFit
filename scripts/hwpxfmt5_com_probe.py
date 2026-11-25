"""내장 합성 구조의 Python/Rust 변경 산출물을 한/글 COM으로 순차 재열기.

사용: python scripts/hwpxfmt5_com_probe.py --bin <실행 파일> --out <외부 폴더>
원본 리소스는 읽기만 하며, COM은 기본 공통 잠금 하나를 소유한다.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[1]
ap=argparse.ArgumentParser(description=__doc__)
ap.add_argument('--bin',type=Path,required=True)
ap.add_argument('--out',type=Path,required=True)
ap.add_argument('--only',nargs='+',choices=('attachment','box','midtitle','title'),default=('attachment','box','midtitle','title'))
args=ap.parse_args()
args.bin=args.bin.resolve()
OUT=args.out.resolve()
sys.path.insert(0,str(ROOT))
sys.path.insert(0,str(ROOT/'scripts'))
os.chdir(ROOT)
import hwpxfmt5_parity as h5
from defusedxml import ElementTree as ET
from docfit_core.hwpx import open_hwpx_safely
from docfit_core.save_guard import com_test_lock
from docfit_core import hwp_process
import pythoncom
import zipfile

OUT.mkdir(parents=True,exist_ok=True)
procs=h5.load()
ns=h5.base.PY_NS
resource=ROOT/'resources'/'서식예시'/'기본 서식.hwpx'
resource_hash=h5.base.sha(resource)
with open_hwpx_safely(resource) as archive:
    original={name:archive.read(name) for name in archive.namelist()}

def local(n): return n.tag.rsplit('}',1)[-1]

def inspect(path):
    app=ns['한글_COM_인스턴스_생성'](독립=True)
    try:
        app.XHwpWindows.Item(0).Visible=False
        if not app.RegisterModule(ns['REGISTER_MODULE_NAME'],ns['REGISTER_MODULE_VALUE']):
            raise RuntimeError('보안 모듈 등록 실패')
        app.SetMessageBoxMode(0x00020000)
        if not ns['한글_문서_열기'](app,path,'HWPX','forceopen:true'):
            raise RuntimeError('Open 실패')
        text=app.GetTextFile('UNICODE','') or ''
        return {'pages':int(app.PageCount),'text_sha256':hashlib.sha256(text.encode('utf-8')).hexdigest(),'text_length':len(text)}
    finally:
        try: app.Clear(1)
        finally:
            try: app.Quit()
            finally: hwp_process.kill_owned()

previous=json.loads((OUT/'result.json').read_text(encoding='utf-8')) if (OUT/'result.json').exists() else []
rows=[r for r in previous if r['processor'] not in args.only]
pythoncom.CoInitialize()
try:
    with com_test_lock('HWPXFMT5','4종 변경 산출물 Python/Rust COM 재열기',timeout_sec=5):
        for proc,loader,index in [('attachment','개요붙임_원본자료',1),('box','개요붙임_원본자료',0),('midtitle','중제목_원본자료',0),('title','제목_원본자료',0)]:
            if proc not in args.only: continue
            row={'processor':proc}
            try:
                sh,section=ns[loader]()
                tables=[t for p in section for r in p for t in r if local(t)=='tbl']
                table=tables[index]
                host=next(p for p in section if any(t is table for r in p for t in r))
                control=next((copy.deepcopy(x) for x in section.iter() if local(x)=='secPr'),None)
                root=copy.deepcopy(section)
                for p in list(root): root.remove(p)
                if control is not None and not any(local(x)=='secPr' for x in host.iter()):
                    p=type(section)('{http://www.hancom.co.kr/hwpml/2011/paragraph}p',{'id':'1','paraPrIDRef':'0','styleIDRef':'0','pageBreak':'0','columnBreak':'0','merged':'0'})
                    r=type(section)('{http://www.hancom.co.kr/hwpml/2011/paragraph}run',{'charPrIDRef':'0'})
                    p.append(r)
                    r.append(control);root.append(p)
                root.append(copy.deepcopy(host))
                contents=dict(original)
                contents['Contents/header.xml']=ET.tostring(sh,encoding='utf-8',xml_declaration=True)
                contents['Contents/section0.xml']=ET.tostring(root,encoding='utf-8',xml_declaration=True)
                src,py,rs=[OUT/f'{proc}_{name}.hwpx' for name in ('source','python','rust')]
                with zipfile.ZipFile(src,'w',zipfile.ZIP_DEFLATED) as z:
                    for name,data in contents.items():
                        z.writestr(name,data,compress_type=zipfile.ZIP_STORED if name=='mimetype' else zipfile.ZIP_DEFLATED)
                cfg={'format_samples':{'box':{'header_xml':ET.tostring(sh,encoding='unicode'),'table_xml':ET.tostring(table,encoding='unicode')}}} if proc=='box' else {}
                before=h5.base.sha(src)
                ps,pn=h5.run_python(procs[proc],src,py,cfg)
                ss,sn,err=h5.base.run_rs(str(args.bin.resolve()),proc,src,rs,cfg,OUT/f'{proc}.json')
                if err: raise RuntimeError(err)
                same,diffs,strict=h5.base.compare_outputs(py,rs)
                if not same or ps!=ss or pn!=sn or not pn: raise RuntimeError(f'차이/변경수 오류: {diffs}/{pn}/{sn}')
                row.update(count=pn,normalized_equal=same,python=inspect(py),rust=inspect(rs),source_unchanged=h5.base.sha(src)==before)
                row['ok']=row['python']==row['rust'] and row['source_unchanged']
            except Exception as exc:
                row.update(ok=False,error=f'{type(exc).__name__}: {exc}')
            rows.append(row)
            (OUT/'result.json').write_text(json.dumps(rows,ensure_ascii=False,indent=2),encoding='utf-8')
            print(json.dumps(row,ensure_ascii=False),flush=True)
finally:
    hwp_process.kill_owned()
    pythoncom.CoUninitialize()
assert h5.base.sha(resource)==resource_hash
raise SystemExit(int(any(not r['ok'] for r in rows)))
