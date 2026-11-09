#!/usr/bin/env python3
"""style_hierarchy의 함수별 Python/Rust 일치 회귀 도구."""
from __future__ import annotations
import argparse, json, os, subprocess, sys
from pathlib import Path

REPO=Path(__file__).resolve().parents[1]; sys.path.insert(0,str(REPO))
from docfit_core import style_hierarchy as py
from tools.hier_regress import source_corpus, normalize_paragraphs

ORDER=("canonical_marker","normalize_leading_dot","leading_marker","display_role","stored_role","most_common","document_type","analyze_hierarchy","hierarchy_summary")
ROLES=("장","중제목","소제목","본문","내용","부연설명","제목","1단계","2단계","3단계","4단계","5단계","6단계","부연설명","기타")

def cases(name, paragraphs):
    if name=="canonical_marker": return [{"marker":x} for x in ("•","·","‧","∙","⋅","ㆍ","●","□","ㅁ","■","ㅇ","x","")]
    if name=="normalize_leading_dot": return [{"text":p["text"]} for p in paragraphs]+[{"text":x} for x in ("  • foo","  ·","a·b","·x","",None)]
    if name=="leading_marker": return [{"text":p["text"]} for p in paragraphs]
    if name=="display_role": return [{"role":x} for x in ROLES]
    if name=="stored_role": return [{"label":x} for x in ROLES]
    if name=="most_common":
        return [{"values":x,"default":d} for x,d in [([],None),([1,1,2],None),(["a","b"],None),([None,None,1],7),([[1],[2],[1]],None)]]
    if name=="document_type":
        from itertools import combinations
        base=("장","중제목","소제목")
        subsets=[list(c) for n in range(4) for c in combinations(base,n)]
        return [{"roles":r,"has_midtitle_table":b} for r in subsets for b in (False,True)]
    if name=="analyze_hierarchy":
        corpus=[paragraphs[i:i+25] for i in range(0,len(paragraphs),25)]
        return [{"paragraphs":x} for x in corpus]
    if name=="hierarchy_summary":
        return [{"analysis":py.analyze_hierarchy(paragraphs)}]
    return []

def python_value(name, c):
    if name=="canonical_marker": return py.canonical_marker(c["marker"])
    if name=="normalize_leading_dot": return py.normalize_leading_dot(c["text"])
    if name=="leading_marker": return list(py.leading_marker(c["text"]))
    if name=="display_role": return py.display_role(c["role"])
    if name=="stored_role": return py.stored_role(c["label"])
    if name=="most_common":
        from collections import Counter
        return Counter(c["values"]).most_common(1)[0][0] if c["values"] else c["default"]
    if name=="document_type": return py.document_type(c["roles"],c["has_midtitle_table"])
    if name=="analyze_hierarchy": return py.analyze_hierarchy(c["paragraphs"])
    return py.hierarchy_summary(c["analysis"])

def run(exe,name,c):
    p=subprocess.run([str(exe),"fn",name],input=json.dumps(c,ensure_ascii=False,allow_nan=False),text=True,encoding="utf-8",capture_output=True)
    if p.returncode: raise RuntimeError(p.stderr)
    return json.loads(p.stdout)

def main():
    ap=argparse.ArgumentParser();ap.add_argument("--function",choices=ORDER);ap.add_argument("--executable",type=Path);ap.add_argument("--no-build",action="store_true");ap.add_argument("--report",type=Path,default=REPO.parent/"reports"/"HIER6.final.md");args=ap.parse_args()
    exe=args.executable or REPO/"hierarchy"/"target"/"debug"/("hierarchy.exe" if os.name=="nt" else "hierarchy")
    if not args.no_build:
        env=os.environ.copy();env["PATH"]=str(Path(os.environ.get("LOCALAPPDATA",""))/"DocFitTools"/"mingw64"/"bin")+os.pathsep+str(Path.home()/".cargo"/"bin")+os.pathsep+env.get("PATH","")
        subprocess.run(["cargo","build","--manifest-path",str(REPO/"hierarchy"/"Cargo.toml")],cwd=REPO,env=env,check=True)
    corpus=source_corpus();all_paras=normalize_paragraphs([p for _,ps,_ in corpus for p in ps]);assert len(all_paras)==549,len(all_paras)
    rows=[]
    for name in (args.function,) if args.function else ORDER:
        cs=cases(name,all_paras);diffs=[]
        for i,c in enumerate(cs):
            expected=python_value(name,c);actual=run(exe,name,c)
            if expected!=actual: diffs.append((i,c,expected,actual))
        rows.append((name,len(cs),len(diffs),diffs[:3]));print(f"{name}: cases={len(cs)} differences={len(diffs)}")
        if diffs:
            for i,c,e,a in diffs[:3]: print(f"  case {i}: input={str(c)[:180]}\n    python={str(e)[:300]}\n    rust={str(a)[:300]}")
            break
    lines=["# HIER6 함수별 이식 결과","","기준: `docfit_core/style_hierarchy.py`; 비교 자료: fixture 502문단 + 골든 47문단 = 549문단.","","| 함수 | 비교 사례 | 차이 | 상태 |","|---|---:|---:|---|"]
    for n,total,diff,_ in rows: lines.append(f"| `{n}` | {total} | {diff} | {'일치' if diff==0 else '진행 필요'} |")
    lines.extend(["","전체 bridge 경로 측정 결과는 `hier_regress.py` 실행 후 별도로 기록합니다."])
    args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text("\n".join(lines)+"\n",encoding="utf-8")
    return int(any(r[2] for r in rows))
if __name__=="__main__": raise SystemExit(main())
