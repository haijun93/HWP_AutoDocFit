"""SRCH6 합성 5천 문서의 일반/문서 AND warm 지연을 한 번씩 측정한다."""
import argparse
import json
from pathlib import Path
import statistics
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from docfit_core.timemachine.search_backend import RustBackend
from scripts.search_bench import make_corpus


def run(executable, count=5000):
    with tempfile.TemporaryDirectory(prefix="semi2-warm-") as temp:
        root = Path(temp)
        corpus = root / "corpus"
        corpus.mkdir()
        paths = make_corpus(corpus, count, 20261124)
        backend = RustBackend(executable, index_dir=root / "index", timeout=90)
        try:
            started = time.perf_counter()
            indexed = backend.index_files(paths, batch_size=250)
            if len(indexed) != count or any(row.get("error") for row in indexed):
                raise RuntimeError("성능 코퍼스 색인 실패")
            index_seconds = time.perf_counter() - started
            cases = []
            for label, query, hierarchy, samples in (
                ("short", "행정", False, 5),
                ("long", "행정 정책 문서 안전 지원", False, 5),
                ("and", "행정 정책", False, 5),
                ("phrase", '"행정 절차 지원"', False, 5),
                ("hierarchy_and2", "행정 정책", True, 7),
                ("hierarchy_and5", "행정 정책 문서 안전 지원", True, 7),
            ):
                def measure():
                    started = time.perf_counter()
                    rows = backend.search(query, hierarchy=hierarchy, limit=20)
                    return (time.perf_counter() - started) * 1000, rows
                cold, expected = measure()
                warm = []
                for _ in range(samples):
                    elapsed, rows = measure()
                    if rows != expected:
                        raise RuntimeError(f"warm 결과 변경: {label}")
                    warm.append(elapsed)
                cases.append({"case": label, "query": query, "hierarchy": hierarchy,
                    "returned_documents": len(expected), "cold_ms": round(cold, 3),
                    "warm_samples_ms": [round(v, 3) for v in warm],
                    "warm_median_ms": round(statistics.median(warm), 3)})
            return {"seed": 20261124, "documents": count, "limit": 20,
                "index_seconds": round(index_seconds, 3), "cases": cases,
                "ok": all(case["warm_median_ms"] <= 30 for case in cases)}
        finally:
            backend.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--klib-exe", required=True)
    parser.add_argument("--documents", type=int, default=5000)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.klib_exe, args.documents)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    raise SystemExit(0 if result["ok"] else 1)
