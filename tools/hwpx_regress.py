"""Fixture comparator for Python and the resident Rust HWPX DLL."""
from __future__ import annotations
import argparse, ctypes, json, os, sys, time
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
from docfit_core.style_inventory import analyze_style_inventory
from docfit_core.format_elements import analyze_format_elements

def rust_path():
    p=os.environ.get("HWPX_CORE_DLL")
    return Path(p) if p else ROOT/"hwpx_core"/"target"/"release"/"hwpx_core.dll"

def flatten(a,b,path="$",out=None):
    out=[] if out is None else out
    if type(a) is not type(b): out.append((path,a,b))
    elif isinstance(a,dict):
        for k in sorted(set(a)|set(b)):
            if k not in a: out.append((f"{path}.{k}","<missing>",b[k]))
            elif k not in b: out.append((f"{path}.{k}",a[k],"<missing>"))
            else: flatten(a[k],b[k],f"{path}.{k}",out)
    elif isinstance(a,list):
        if len(a)!=len(b): out.append((f"{path}.length",len(a),len(b)))
        for i,(x,y) in enumerate(zip(a,b)): flatten(x,y,f"{path}[{i}]",out)
    elif a!=b: out.append((path,a,b))
    return out

def load_dll(path):
    if not path.exists(): return None
    lib=ctypes.CDLL(str(path)); lib.hwpx_analyze.argtypes=[ctypes.c_char_p];lib.hwpx_analyze.restype=ctypes.c_void_p
    lib.hwpx_last_error.restype=ctypes.c_char_p;lib.hwpx_free.argtypes=[ctypes.c_void_p]
    return lib

def main():
    ap=argparse.ArgumentParser();ap.add_argument("--fixtures",type=Path,default=ROOT/"tests"/"fixtures");ap.add_argument("--out",type=Path);ap.add_argument("--dll",type=Path,default=rust_path());ap.add_argument("--measure",action="store_true")
    ns=ap.parse_args();lib=load_dll(ns.dll);files=sorted(ns.fixtures.rglob("*.hwpx"));reports=[];diffs=0
    if lib is None: print(f"Rust DLL unavailable: {ns.dll}; comparisons marked unavailable",file=sys.stderr)
    for p in files:
        t=time.perf_counter(); py={"style_inventory":analyze_style_inventory(p),"format_elements":analyze_format_elements(p)}; py_ms=(time.perf_counter()-t)*1000
        item={"file":str(p.relative_to(ns.fixtures)),"python":py,"rust":None,"differences":[],"rust_error":None,"python_ms":py_ms}
        if lib:
            ptr=lib.hwpx_analyze(os.fsencode(p))
            if ptr:
                try: item["rust"]=json.loads(ctypes.string_at(ptr).decode("utf-8"))
                finally: lib.hwpx_free(ptr)
                item["differences"]=flatten({"style_inventory":py["style_inventory"],"format_elements":py["format_elements"]},item["rust"])
            else:item["rust_error"]=(lib.hwpx_last_error() or b"unknown").decode("utf-8","replace")
        diffs+=len(item["differences"])+(1 if item["rust_error"] else 0);reports.append(item)
    summary={"files":len(files),"dll_available":lib is not None,"difference_count":diffs,"results":reports}
    encoded=json.dumps(summary,ensure_ascii=False,indent=2)
    if ns.out: ns.out.parent.mkdir(parents=True,exist_ok=True);ns.out.write_text(encoded+"\n",encoding="utf-8")
    print(json.dumps({k:summary[k] for k in ("files","dll_available","difference_count")},ensure_ascii=False))
    if ns.measure and files:
        sizes=sorted(files,key=lambda p:p.stat().st_size,reverse=True); large=sizes[0];small=sizes[-min(10,len(sizes)):]
        print(json.dumps({"large":str(large),"large_bytes":large.stat().st_size,"small_files":[str(p) for p in small],"small_bytes":sum(p.stat().st_size for p in small)},ensure_ascii=False))
    return 0 if lib and diffs==0 else 2
if __name__=="__main__": raise SystemExit(main())
