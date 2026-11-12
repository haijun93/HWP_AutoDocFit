"""DFB2: Docufinder(DFB1)와 같은 자료·같은 검색어로 우리 엔진 세 가지를 잰다.

- (A) Tantivy 문장 색인, (B) fst+roaring 직접 구현 — IS-S1 시제품 `kfind-engine-bench`
- (P) 현행 앱 Python 본문 검색 `ContentSearchIndex.index_file` + `keyword_search`

자료(읽기만): DFB1 corpus 폴더(txt·md). 시제품용 문장 말뭉치(corpus.tsv)와 Python 색인은 --work에 만든다.
사용: python -X utf8 kfind/engine-bench/dfb2.py --exe <kfind-engine-bench.exe> --corpus <DFB1 corpus>
       --work <임시 폴더> --reps 5 --output <결과 JSON>
"""
import argparse
import ctypes
import json
import re
import statistics
import subprocess
import sys
import time
from ctypes import wintypes
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE))
from bench import SPLIT, TYPING, fold, oracle, parse_query, run  # noqa: E402  IS-S1과 같은 규칙·정답 구현

# DFB1 queries.json 순서 그대로(6번은 공백 두 칸).
QUERIES = ["예산", "희귀", "예산 집행", "예산 집행 계획", "예산집행", "예산  집행", "가나다라", "예산을"]
TYPING_DFB2 = [{"name": "음절 완성 붙여쓰기(예산→예산집→예산집행)", "steps": ["예산", "예산집", "예산집행"]},
               TYPING[1]]  # 참고: 입력기 중간 상태 포함 붙여쓰기(IS-S1과 같은 순서)
DOC_NO = re.compile(r"_(\d{5})$")


class PMC(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
        (n, ctypes.c_size_t) for n in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                                       "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                                       "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")]


def memory_mb():
    c = PMC()
    c.cb = ctypes.sizeof(PMC)
    k32 = ctypes.WinDLL("kernel32")
    k32.GetCurrentProcess.restype = wintypes.HANDLE
    k32.K32GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(PMC), wintypes.DWORD]
    k32.K32GetProcessMemoryInfo(k32.GetCurrentProcess(), ctypes.byref(c), c.cb)
    return {"peak_working_set_mb": c.PeakWorkingSetSize / 1048576, "peak_private_mb": c.PeakPagefileUsage / 1048576}


def corpus_files(corpus):
    files = [p for p in corpus.iterdir() if p.suffix in (".txt", ".md")]
    return sorted(files, key=lambda p: int(DOC_NO.search(p.stem).group(1)))


def prepare(corpus, out):
    """corpus 파일 → 시제품 말뭉치(문서번호\\t위계\\t문장). 문단 = 줄, 문장 나누기는 IS-S1 규칙."""
    out.mkdir(parents=True, exist_ok=True)
    doc_of, level_of, folds, names = [], [], [], []
    with open(out / "corpus.tsv", "w", encoding="utf-8", newline="\n") as w:
        for d, path in enumerate(corpus_files(corpus)):
            names.append(path.name)
            for para in path.read_text(encoding="utf-8").split("\n"):
                for s in SPLIT.split(para):
                    if s.strip():
                        w.write(f"{d}\t8\t{s}\n")
                        doc_of.append(d)
                        level_of.append(8)
                        folds.append(fold(s))
    return doc_of, level_of, folds, names


def summarize(samples):
    return {"n": len(samples), "p50_ms": statistics.median(samples), "min_ms": min(samples), "max_ms": max(samples),
            "samples_ms": samples}


# ---------- 현행 Python(keyword_search) ----------

def py_index(args):
    sys.path.insert(0, str(REPO))
    from docfit_core.timemachine.content_search import ContentSearchIndex
    db = Path(args.work) / "py" / "index.sqlite"
    db.parent.mkdir(parents=True, exist_ok=True)
    for f in db.parent.glob("index.sqlite*"):
        f.unlink()
    files = corpus_files(Path(args.corpus))
    t = time.perf_counter()
    idx = ContentSearchIndex(db)
    errors = []
    with idx.batch():
        for p in files:
            r = idx.index_file(p)
            if r["error"]:
                errors.append((p.name, r["error"]))
    idx.close()
    build_s = time.perf_counter() - t
    size = sum(f.stat().st_size for f in db.parent.glob("index.sqlite*"))
    return {"build_s": build_s, "files": len(files), "errors": errors[:10], "error_count": len(errors),
            "index_bytes": size, "memory": memory_mb()}


def py_query(args):
    sys.path.insert(0, str(REPO))
    from docfit_core.timemachine.content_search import ContentSearchIndex
    plan = json.loads(Path(args.plan).read_text(encoding="utf-8"))
    t = time.perf_counter()
    idx = ContentSearchIndex(Path(args.work) / "py" / "index.sqlite")
    open_ms = (time.perf_counter() - t) * 1000

    def call(q, limit, offset=0, fresh=True):
        if fresh:
            idx._keyword_rank_cache = None  # 순위 보관(쪽 넘김용)을 비워 매번 새로 계산
        try:
            return idx.keyword_search(q, limit=limit, offset=offset)
        except ValueError as exc:  # 검색어 없음 등
            return {"error": str(exc)}

    def top20(q):
        t = time.perf_counter()
        out = call(q, 20)
        return (time.perf_counter() - t) * 1000, out

    def full(q):
        """전체 결과: 500건씩 쪽 넘김으로 끝까지(첫 쪽이 순위 계산, 다음 쪽은 앱과 같이 보관 순위 사용)."""
        t = time.perf_counter()
        out = call(q, 500)
        if isinstance(out, dict):
            return (time.perf_counter() - t) * 1000, out
        rows = list(out)
        while len(out) == 500:
            out = call(q, 500, offset=len(rows), fresh=False)
            rows.extend(out)
        return (time.perf_counter() - t) * 1000, rows

    queries = []
    for q in plan["queries"]:
        first_top, first_res = top20(q)
        first_full, rows = full(q)
        tops = [top20(q)[0] for _ in range(args.reps)]
        fulls = [full(q)[0] for _ in range(args.reps)]
        cached = []
        for _ in range(args.reps):
            t = time.perf_counter()
            call(q, 20, fresh=False)
            cached.append((time.perf_counter() - t) * 1000)
        if isinstance(rows, dict):
            result = {"docs": 0, "error": rows["error"], "top20": []}
        else:
            result = {"docs": len(rows), "top20": [r["name"] for r in first_res[:20]]}
        queries.append({"q": q, "first_ms": first_top, "first_full_ms": first_full, "top20": summarize(tops),
                        "full": summarize(fulls), "top20_rank_cache_hit": summarize(cached), "result": result})
    typing = []
    for seq in plan["typing"]:
        steps = []
        for q in seq["steps"]:
            samples, docs = [], None
            for _ in range(args.reps):
                ms, out = top20(q)
                samples.append(ms)
                docs = 0 if isinstance(out, dict) else len(out)
            steps.append({"q": q, "top20": summarize(samples), "shown": docs})
        typing.append({"name": seq["name"], "steps": steps})
    idx.close()
    return {"open_ms": open_ms, "queries": queries, "typing": typing, "memory": memory_mb()}


def py_run(args, mode):
    cmd = [sys.executable, "-X", "utf8", str(Path(__file__).resolve()), "--worker", mode, "--work", str(args.work),
           "--corpus", str(args.corpus), "--reps", str(args.reps), "--plan", str(args.plan), "--exe", "-", "--output", "-"]
    done = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
    if done.returncode != 0:
        raise RuntimeError(f"python {mode} failed: {done.stderr[-3000:]}")
    return json.loads(done.stdout.strip().splitlines()[-1])


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", required=True)
    p.add_argument("--corpus", type=Path, required=True)
    p.add_argument("--work", type=Path, required=True)
    p.add_argument("--reps", type=int, default=5)
    p.add_argument("--heap-mb", type=int, default=200)
    p.add_argument("--threads", type=int, default=4)
    p.add_argument("--output", required=True)
    p.add_argument("--plan")
    p.add_argument("--worker", choices=["py-index", "py-query"])
    a = p.parse_args()
    if a.worker:
        print(json.dumps(py_index(a) if a.worker == "py-index" else py_query(a), ensure_ascii=False))
        return
    a.work.mkdir(parents=True, exist_ok=True)
    plan = {"queries": QUERIES, "typing": TYPING_DFB2}
    a.plan = a.work / "plan.json"
    a.plan.write_text(json.dumps(plan, ensure_ascii=False), encoding="utf-8")

    t = time.perf_counter()
    doc_of, level_of, folds, names = prepare(a.corpus, a.work / "corpus")
    prep_s = time.perf_counter() - t
    files = corpus_files(a.corpus)
    report = {"python": sys.version, "reps": a.reps, "corpus": str(a.corpus), "files": len(files),
              "bytes": sum(f.stat().st_size for f in files), "sentences": len(doc_of), "prep_s": prep_s,
              "queries": QUERIES, "engines": {}}
    all_q = list(dict.fromkeys(QUERIES + [s for t in TYPING_DFB2 for s in t["steps"]]))
    expect = {q: oracle(doc_of, level_of, folds, q) for q in all_q}
    report["expected"] = expect
    stems = [fold(Path(n).stem) for n in names]
    report["filename_matches"] = {q: sum(all(w in s for w in parse_query(q)) for s in stems) if parse_query(q) else 0
                                  for q in QUERIES}
    print(f"prepared {len(files)} files / {len(doc_of)} sentences in {prep_s:.1f}s", file=sys.stderr, flush=True)

    for eng in ("a", "b"):
        build_args = (["build-a", a.work / "corpus", a.work / "idx-a", a.heap_mb, a.threads] if eng == "a"
                      else ["build-b", a.work / "corpus", a.work / "idx-b"])
        built = run(a.exe, *build_args)
        print(f"build {eng}: {built['build_s']:.2f}s", file=sys.stderr, flush=True)
        queried = run(a.exe, "query", eng, a.work / f"idx-{eng}", a.plan, a.reps)
        mismatches = [row["q"] for row in queried["queries"]
                      if row["result"] != expect[row["q"]]
                      and not (expect[row["q"]] is None and row["result"]["sentences"] == 0)]
        for seq in queried["typing"]:
            for step in seq["steps"]:
                e = expect[step["q"]]
                if e is None:
                    if "skipped" not in step:
                        mismatches.append(step["q"])
                elif (step.get("sid_hash") != e["sid_hash"] or step.get("sentences") != e["sentences"]
                      or not step.get("reuse_equals_fresh")):
                    mismatches.append(step["q"])
        report["engines"][eng] = {"build": built, "query": queried, "mismatches": mismatches}
        print(f"query {eng}: mismatches={mismatches}", file=sys.stderr, flush=True)
        Path(a.output).write_text(json.dumps(report, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")

    built = py_run(a, "py-index")
    print(f"build python: {built['build_s']:.2f}s errors={built['error_count']}", file=sys.stderr, flush=True)
    queried = py_run(a, "py-query")
    report["engines"]["python"] = {"build": built, "query": queried}
    Path(a.output).write_text(json.dumps(report, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print("done", file=sys.stderr)


if __name__ == "__main__":
    main()
