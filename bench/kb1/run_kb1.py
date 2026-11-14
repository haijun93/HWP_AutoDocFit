"""KB1 측정 구동기(1회 측정용).

순서: 엔진별 색인(프로세스 하나씩) → 검색어 8개 × 엔진 3개를 검색어마다 새 프로세스로(첫 검색 = 콜드) 실행.
사용: python -X utf8 bench/kb1/run_kb1.py --exe <kb1-bench.exe> --work <폴더(corpus/corpus.tsv 있음)> --reps 20 --output <raw.json>
"""
import argparse
import json
import platform
import subprocess
import time
from pathlib import Path

# IS-BENCH와 같은 8개(카드 KB1 순서). "예산  집행"은 공백 두 칸.
QUERIES = ["예산", "희귀", "예산 집행", "예산 집행 계획", "예산집행", "예산  집행", "가나다라", "예산을"]
# 붙여 쓴 검색어를 나눈 낱말(kfind-core 낱말 사전 나누기 결과와 같게) — 다른 엔진에도 같은 나누기를 준다.
SPLIT = {"예산집행": "예산 집행"}
ENGINES = ["kfind", "tantivy", "seekstorm"]


def run(exe, *args):
    t = time.perf_counter()
    done = subprocess.run([str(exe), *map(str, args)], capture_output=True, text=True, encoding="utf-8")
    wall = time.perf_counter() - t
    if done.returncode != 0:
        raise RuntimeError(f"{args} failed ({done.returncode}): {done.stderr.strip()[-3000:]}")
    out = json.loads(done.stdout)
    out["process_wall_s"] = wall
    return out


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--work", type=Path, required=True)
    p.add_argument("--reps", type=int, default=20)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    corpus = a.work / "corpus" / "corpus.tsv"
    report = {"platform": platform.platform(), "reps": a.reps, "queries": QUERIES, "split": SPLIT, "build": {}, "query": {}}
    for eng in ENGINES:
        report["build"][eng] = run(a.exe, "build", eng, corpus, a.work / f"idx-{eng}")
        print(eng, "build", round(report["build"][eng]["build_s"], 2), "s", flush=True)
    for i, q in enumerate(QUERIES):
        report["query"][q] = {}
        # 엔진 순서를 검색어마다 돌려 순서 효과를 한쪽에 몰지 않는다.
        for eng in ENGINES[i % 3:] + ENGINES[:i % 3]:
            args = ["query", eng, a.work / f"idx-{eng}", q, a.reps]
            if q in SPLIT and eng != "kfind":
                args.append(SPLIT[q])
            report["query"][q][eng] = run(a.exe, *args)
            r = report["query"][q][eng]
            print(repr(q), eng, "cold", round(r["cold_ms"], 3), "sent", r["cold_result"]["sentences"], flush=True)
        a.output.write_text(json.dumps(report, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
