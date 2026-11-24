"""SRCH6 합성 warm 계측. 작업 자료·원시 로그는 지정한 외부 폴더에만 둔다."""
from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "scripts"))
from search_bench import make_corpus
from docfit_core.timemachine.search_backend import RustBackend


class ProfileBackend(RustBackend):
    def _request(self, payload):
        payload = dict(payload, profile=payload.get("op") == "search")
        started = time.perf_counter()
        result = super()._request(payload)
        self.request_ms = (time.perf_counter() - started) * 1000
        self.profile = result.get("_profile", {})
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--klib-exe", required=True)
    parser.add_argument("--work-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--sizes", nargs="+", type=int, default=[1000, 5000])
    args = parser.parse_args()
    work = args.work_dir.resolve()
    if work == ROOT or ROOT in work.parents:
        parser.error("work-dir는 저장소 밖이어야 합니다")
    result = {"seed": 20261124, "warm_samples": 7, "runs": []}
    for count in args.sizes:
        corpus = work / str(count) / "corpus"
        corpus.mkdir(parents=True, exist_ok=True)
        paths = sorted(corpus.glob("*.txt"))
        if len(paths) != count:
            paths = make_corpus(corpus, count, 20261124)
        backend = ProfileBackend(args.klib_exe, index_dir=corpus.parent / "index")
        try:
            started = time.perf_counter()
            errors = [r for r in backend.index_files(paths) if r.get("error")]
            if errors:
                raise RuntimeError(errors)
            run = {"documents": count, "index_request_seconds": time.perf_counter() - started, "queries": {}}
            for name, query in {"short": "행정", "long": "행정 정책 문서 안전 지원", "and": "행정 정책", "phrase": '"행정 절차 지원"'}.items():
                backend.search(query, limit=20)
                samples = []
                for _ in range(7):
                    started = time.perf_counter()
                    rows = backend.search(query, limit=20)
                    elapsed = (time.perf_counter() - started) * 1000
                    samples.append(dict(backend.profile, python_conversion_ms=elapsed - backend.request_ms,
                                        client_request_ms=backend.request_ms, total_ms=elapsed, returned=len(rows)))
                run["queries"][name] = {"query": query, "median": {
                    key: round(statistics.median(s[key] for s in samples), 4) for key in samples[0]}, "samples": samples}
            result["runs"].append(run)
        finally:
            backend.close()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": args.output.name, "runs": [{"documents": r["documents"], "ms": {
        k: v["median"]["total_ms"] for k, v in r["queries"].items()}} for r in result["runs"]]}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
