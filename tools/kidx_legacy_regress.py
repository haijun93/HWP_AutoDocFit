#!/usr/bin/env python3
"""Compare every field emitted by legacy HWPX paragraph extraction."""
import argparse, json, os, subprocess, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
def main():
 p=argparse.ArgumentParser();p.add_argument("paths",nargs="*",type=Path);p.add_argument("--exe",type=Path,default=ROOT/"klib"/"target"/"debug"/"klib.exe");p.add_argument("--dump",action="store_true");a=p.parse_args();os.environ["PYTHONIOENCODING"]="utf-8"
 from docfit_core.timemachine.content_search import extract_paragraphs_legacy
 files=sorted({x.resolve() for item in (a.paths or [ROOT/"tests"/"fixtures"]) for x in (item.rglob("*.hwpx") if item.is_dir() else [item]) if x.is_file() and x.suffix.lower()==".hwpx"})
 diffs=errors=0
 for f in files:
  py=extract_paragraphs_legacy(f,"hwpx"); expected=[{"paragraph_index":r.paragraph_index,"text":r.text,"level":r.level,"level_rank":r.level_rank,"is_table_cell":r.is_table_cell,"style_name":r.style_name,"outline_level":r.outline_level} for r in py]
  run=subprocess.run([str(a.exe.resolve()),"legacy-records",str(f)],capture_output=True,text=True,encoding="utf-8")
  if run.returncode: errors+=1;print(f"RUST_ERROR {f}: {run.stderr.strip()}");continue
  actual=json.loads(run.stdout)
  if expected!=actual:
   diffs+=1
   if a.dump:
    for i in range(max(len(expected),len(actual))):
     x=expected[i] if i<len(expected) else None;y=actual[i] if i<len(actual) else None
     if x!=y: print(json.dumps({"file":str(f),"index":i,"python":x,"rust":y},ensure_ascii=False))
 print(f"files={len(files)} differences={diffs} errors={errors}");return 1 if diffs or errors else 0
if __name__=="__main__":raise SystemExit(main())
