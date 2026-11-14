"""IS1 S0 synthetic benchmark, adapted from the design agent's bench.py.
No documents or COM. First means a fresh process, not a flushed OS cache.
"""
import argparse
import cProfile
import hashlib
import json
import math
import os
import platform
import pstats
import random
import sqlite3
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from docfit_core.timemachine.content_search import ContentSearchIndex
from docfit_core.timemachine.store import Store

WORDS = ["예산", "집행", "계획", "추진", "사업", "주민", "안전", "관리", "점검", "보고", "축제", "도로",
         "공사", "협의", "지원", "결과", "현황", "개선", "청소", "복지", "문화", "교육", "환경", "시설"]
TAILS = ["을", "를", "이", "가", "은", "는", "의", "에", "으로", "과", ""]


def sentence(rng, n=12):
    return " ".join(rng.choice(WORDS) + rng.choice(TAILS) for _ in range(n)) + "."


def summary(samples):
    if not samples:
        raise ValueError("samples must not be empty")
    ordered = sorted(samples)
    return {"samples_ms": samples, "median_ms": statistics.median(samples),
            "p95_ms": ordered[math.ceil(len(ordered) * .95) - 1]}


def sql_key(sql):
    return " ".join(sql.split())


class CursorTimer:
    def __init__(self, cursor, record):
        self.cursor, self.record = cursor, record

    def _call(self, method, *args):
        start = time.perf_counter()
        try:
            return method(*args)
        finally:
            self.record["ms"] += (time.perf_counter() - start) * 1000

    def fetchone(self):
        return self._call(self.cursor.fetchone)

    def fetchall(self):
        return self._call(self.cursor.fetchall)

    def __iter__(self):
        return self

    def __next__(self):
        return self._call(next, self.cursor)

    def __getattr__(self, name):
        return getattr(self.cursor, name)


class SQLTimer:
    """Measure execute and consumption, retaining real bound parameters for EQP."""
    def __init__(self, db):
        self.db, self.records = db, {}

    def execute(self, sql, parameters=()):
        key = sql_key(sql)
        record = self.records.setdefault(key, {"sql": sql, "calls": 0, "ms": 0, "plan": []})
        record["calls"] += 1
        # Explain outside measured execute; never run the statement twice.
        if record["calls"] == 1 and key.upper().startswith(("SELECT", "WITH")):
            try:
                record["plan"] = [list(r) for r in self.db.execute("EXPLAIN QUERY PLAN " + sql, parameters)]
            except sqlite3.Error as exc:
                record["plan_error"] = str(exc)
        start = time.perf_counter()
        try:
            cursor = self.db.execute(sql, parameters)
        finally:
            record["ms"] += (time.perf_counter() - start) * 1000
        return CursorTimer(cursor, record)

    def __enter__(self):
        self.db.__enter__()
        return self

    def __exit__(self, *args):
        return self.db.__exit__(*args)

    def __getattr__(self, name):
        return getattr(self.db, name)


def build(folder, docs, paras, timeline):
    rng = random.Random(7)
    index = ContentSearchIndex(folder / "index.sqlite")
    p_rows, metas, bodies = [], [], []
    for d in range(docs):
        path = str(folder / f"{rng.choice(WORDS)}_{rng.choice(WORDS)}_{d:05}.hwpx")
        texts = [sentence(rng) for _ in range(paras)]
        p_rows.extend((path, Path(path).name, i, "body", 8, t) for i, t in enumerate(texts))
        metas.append((path, Path(path).name, "hwpx", 1, 1., 1., 2))
        bodies.append((path, Path(path).name, "\n".join(texts)))
    index.db.executemany("INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body) VALUES(?,?,?,?,?,?)", p_rows)
    index.db.executemany("INSERT INTO content_meta(path,name,extension,size,mtime,ctime,hierarchy_schema_version) VALUES(?,?,?,?,?,?,?)", metas)
    index.db.executemany("INSERT INTO content_fts(path,name,body) VALUES(?,?,?)", bodies)
    index.db.commit()
    index.close()
    store = Store(folder / "tm.sqlite")
    rows = [{"path": str(folder / f"{i}.hwpx"), "root": str(folder),
             "name": f"{rng.choice(WORDS)} {rng.choice(WORDS)} {i}.hwpx",
             "kind": "hwp", "size": 1, "mtime": 1700000000+i, "ctime": 1700000000+i,
             "work_time": 1700000000+i, "flags": [], "cloud": False} for i in range(timeline)]
    store.upsert_many(rows)
    store.close()


def worker(args):
    folder = Path(args.folder)
    start = time.perf_counter()
    obj = Store(folder / "tm.sqlite") if args.channel == "timeline" else ContentSearchIndex(folder / "index.sqlite")
    open_ms = (time.perf_counter() - start) * 1000
    def query():
        if args.channel == "timeline":
            return obj.range(0, 2000000000, roots=[str(folder)], query=args.query, extensions=["hwpx"])
        return getattr(obj, args.channel)(args.query)
    samples, counts, digests = [], [], []
    for _ in range(args.reps + 1):
        start = time.perf_counter()
        out = query()
        samples.append((time.perf_counter() - start) * 1000)
        counts.append(len(out))
        digests.append(hashlib.sha256(json.dumps(out, sort_keys=True, ensure_ascii=False).encode()).hexdigest())
    obj.close()
    # Separate fresh-object diagnostic call; timings above have no profiler/proxy.
    obj = Store(folder / "tm.sqlite") if args.channel == "timeline" else ContentSearchIndex(folder / "index.sqlite")
    db_attr = "_db" if args.channel == "timeline" else "db"
    raw = getattr(obj, db_attr)
    proxy = SQLTimer(raw)
    setattr(obj, db_attr, proxy)
    profiler = cProfile.Profile()
    profiler.runcall(query)
    stats = pstats.Stats(profiler)
    functions = [{"file": Path(f).name, "line": line, "function": name,
                  "calls": values[1], "self_ms": values[2]*1000, "cumulative_ms": values[3]*1000}
                 for (f, line, name), values in stats.stats.items()]
    setattr(obj, db_attr, raw)
    obj.close()
    return {"channel": args.channel, "query": args.query, "pid": os.getpid(), "open_ms": open_ms,
            "first_ms": samples[0], "repeat": summary(samples[1:]), "counts": counts,
            "results_equal": len(set(digests)) == 1,
            "functions": sorted(functions, key=lambda r: r["cumulative_ms"], reverse=True)[:35],
            "sql": sorted(proxy.records.values(), key=lambda r: r["ms"], reverse=True)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--docs", type=int, default=3000)
    parser.add_argument("--paragraphs", type=int, default=40)
    parser.add_argument("--timeline", type=int, default=100000)
    parser.add_argument("--reps", type=int, default=3)
    parser.add_argument("--temp-dir", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path)
    parser.add_argument("--rust-bench", type=Path, help="Optional executable accepting --bench; stdout retained")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--folder")
    parser.add_argument("--channel")
    parser.add_argument("--query")
    args = parser.parse_args()
    if min(args.docs, args.paragraphs, args.timeline, args.reps) < 1:
        parser.error("scales and reps must be positive")
    if args.worker:
        print(json.dumps(worker(args), ensure_ascii=False))
        return
    result = {"scale": {"docs": args.docs, "paragraphs": args.paragraphs, "timeline": args.timeline},
              "python": sys.version, "platform": platform.platform(), "sqlite": sqlite3.sqlite_version,
              "cache": "fresh process first query; OS disk cache not flushed; diagnostic separate", "runs": []}
    with tempfile.TemporaryDirectory(dir=args.temp_dir, prefix="is1-") as tmp:
        folder = Path(tmp)
        start = time.perf_counter()
        build(folder, args.docs, args.paragraphs, args.timeline)
        result["build_s"] = time.perf_counter() - start
        for channel in ("search_files", "keyword_search", "search", "timeline"):
            for q in ("\uc608", "\uc608\uc0b0", "\uc608\uc0b0\uc744", "\uc608\uc0b0 \uc9d1\ud589", "\uc608\uc0b0 \uc9d1\ud589 \uacc4\ud68d", "\uc608\uc0b0\uc9d1\ud589", "\uac00\ub098\ub2e4\ub77c"):
                cmd = [sys.executable, "-X", "utf8", str(Path(__file__).resolve()), "--worker", "--folder", tmp,
                       "--channel", channel, "--query", q, "--reps", str(args.reps)]
                completed = subprocess.run(cmd, check=True, capture_output=True, text=True, encoding="utf-8")
                result["runs"].append(json.loads(completed.stdout))
                print(f"{channel}: {q}", file=sys.stderr, flush=True)
    if args.rust_bench:
        run = subprocess.run([str(args.rust_bench), "--bench"], check=True, capture_output=True, text=True, encoding="utf-8")
        result["rust_bench"] = run.stdout
    data = json.dumps(result, ensure_ascii=False, indent=2)
    if args.output:
        args.output.write_text(data + "\n", encoding="utf-8")
    else:
        print(data)


if __name__ == "__main__":
    main()
