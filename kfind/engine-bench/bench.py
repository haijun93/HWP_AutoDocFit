"""IS-S1 본문 검색 엔진 비교(Tantivy 대 fst+roaring) 구동기.

1. S0(`scripts/benchmark_is1.py`)와 같은 생성기·seed로 합성 말뭉치를 만든다(문단을 문장으로 나눔).
2. 새 규칙(같은 문장 안에 모든 낱말, 공백 제거 비교형, 2글자 미만 제외, Q3·Q5·Q8)의 Python 전수 기준 구현으로
   정답을 낸다(느려도 됨).
3. Rust 시제품(`kfind-engine-bench`)으로 두 엔진을 색인·질의하고 정답과 비교한다.

사용: python -X utf8 kfind/engine-bench/bench.py --exe <kfind-engine-bench.exe> --work <임시 폴더>
      --docs 3000 30000 --reps 20 --output <결과 JSON>
"""
import argparse
import json
import random
import re
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
from benchmark_is1 import WORDS, sentence  # noqa: E402  S0와 같은 생성기

SPLIT = re.compile(r"(?<=[.?!。])\s+|\n")


def syl(lead, vowel, tail=0):
    """한글 음절 조합(입력기 중간 상태 흉내용)."""
    return chr(0xAC00 + (lead * 21 + vowel) * 28 + tail)


# 두벌식 입력기의 실제 중간 상태: '예' 다음 'ㅅ'은 받침으로 붙어 '옛', '산' 다음 'ㅈ'은 겹받침 'ㄵ'이 된다.
YE, YET, SA, SAN, SANJ = syl(11, 7), syl(11, 7, 19), syl(9, 0), syl(9, 0, 4), syl(9, 0, 5)
JI, JIB, HAE, HAENG = syl(12, 20), syl(12, 20, 17), syl(18, 1), syl(18, 1, 21)
GYE, GYEH, HO, HOE, HOEK = syl(0, 7), syl(0, 7, 27), syl(18, 8), syl(18, 11), syl(18, 11, 1)
TYPING = [
    {"name": "띄어쓰기 3낱말(입력기 중간 상태 포함)", "steps": [
        "ㅇ", YE, YET, YE + SA, YE + SAN, YE + SAN + " ", YE + SAN + " ㅈ", YE + SAN + " " + JI, YE + SAN + " " + JIB,
        YE + SAN + " " + JIB + "ㅎ", YE + SAN + " " + JIB + HAE, YE + SAN + " " + JIB + HAENG, YE + SAN + " " + JIB + HAENG + " ",
        YE + SAN + " " + JIB + HAENG + " ㄱ", YE + SAN + " " + JIB + HAENG + " " + GYE, YE + SAN + " " + JIB + HAENG + " " + GYEH,
        YE + SAN + " " + JIB + HAENG + " " + GYE + HO, YE + SAN + " " + JIB + HAENG + " " + GYE + HOE, YE + SAN + " " + JIB + HAENG + " " + GYE + HOEK]},
    {"name": "붙여쓰기 1낱말(입력기 중간 상태 포함)", "steps": [
        "ㅇ", YE, YET, YE + SA, YE + SAN, YE + SANJ, YE + SAN + JI, YE + SAN + JIB, YE + SAN + JIB + "ㅎ",
        YE + SAN + JIB + HAE, YE + SAN + JIB + HAENG]},
    {"name": "음절 완성 단위(이상적 입력)", "steps": [
        "예", "예산", "예산 집", "예산 집행", "예산 집행 계", "예산 집행 계획"]},
]
QUERIES = [
    ("2글자·흔함(문장 약 40%)", "예산"),
    ("3글자", "예산을"),
    ("2낱말", "예산 집행"),
    ("3낱말", "예산 집행 계획"),
    ("4낱말", "사업 주민 안전 관리"),
    ("3글자+2글자", "예산을 집행"),
    ("붙여쓰기 4글자", "예산집행"),
    ("붙여쓰기 6글자", "예산집행계획"),
    ("낱말 걸침", "산집"),
    ("없는 말", "가나다라"),
]


def fold(s):
    return "".join(c for c in s.lower() if c.isalnum())


def parse_query(q):
    out = []
    for w in q.split():
        f = fold(w)
        if len(f) >= 2 and f not in out:
            out.append(f)
    return out


def fnv(values):
    h = 0xcbf29ce484222325
    for v in values:
        for b in v.to_bytes(4, "little"):
            h ^= b
            h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def generate(folder, docs, paras):
    """S0 build()와 같은 난수 소비 순서(문서마다 이름 낱말 2개 → 문단 paras개)."""
    folder.mkdir(parents=True, exist_ok=True)
    rng = random.Random(7)
    doc_of, level_of, folds, paragraphs = [], [], [], 0
    with open(folder / "corpus.tsv", "w", encoding="utf-8", newline="\n") as out:
        for d in range(docs):
            rng.choice(WORDS), rng.choice(WORDS)
            for _ in range(paras):
                paragraphs += 1
                for s in SPLIT.split(sentence(rng)):
                    if s.strip():
                        out.write(f"{d}\t8\t{s}\n")
                        doc_of.append(d)
                        level_of.append(8)
                        folds.append(fold(s))
    return doc_of, level_of, folds, paragraphs


def oracle(doc_of, level_of, folds, q):
    words = parse_query(q)
    if not words:
        return None
    sids = [i for i, s in enumerate(folds) if all(w in s for w in words)]
    agg = {}
    for i in sids:
        d = doc_of[i]
        best, n = agg.get(d, (255, 0))
        agg[d] = (min(best, level_of[i]), n + 1)
    hits = sorted(((d, lv, n) for d, (lv, n) in agg.items()), key=lambda h: (h[1], -h[2], h[0]))
    return {"sentences": len(sids), "docs": len(hits), "sid_hash": fnv(sids),
            "rank_hash": fnv(v for h in hits for v in h), "top20": [list(h) for h in hits[:20]]}


def run(exe, *args):
    done = subprocess.run([str(exe), *map(str, args)], capture_output=True, text=True, encoding="utf-8")
    if done.returncode != 0:
        raise RuntimeError(f"{args[0]} failed ({done.returncode}): {done.stderr.strip()[-2000:]}")
    return json.loads(done.stdout)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--work", type=Path, required=True, help="말뭉치·색인을 만들 임시 폴더")
    p.add_argument("--docs", type=int, nargs="+", default=[3000, 30000])
    p.add_argument("--paragraphs", type=int, default=40)
    p.add_argument("--reps", type=int, default=20)
    p.add_argument("--heap-mb", type=int, default=200)
    p.add_argument("--threads", type=int, default=4)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    plan = {"queries": [q for _, q in QUERIES], "typing": TYPING}
    all_q = list(dict.fromkeys(plan["queries"] + [s for t in TYPING for s in t["steps"]]))
    report = {"python": sys.version, "reps": a.reps, "labels": dict((q, l) for l, q in QUERIES), "scales": []}
    for docs in a.docs:
        folder = a.work / f"s1-{docs}"
        t = time.perf_counter()
        doc_of, level_of, folds, paragraphs = generate(folder / "corpus", docs, a.paragraphs)
        gen_s = time.perf_counter() - t
        print(f"[{docs}] corpus {len(folds)} sentences / {paragraphs} paragraphs ({gen_s:.1f}s)", file=sys.stderr, flush=True)
        t = time.perf_counter()
        expect = {q: oracle(doc_of, level_of, folds, q) for q in all_q}
        oracle_s = time.perf_counter() - t
        del folds
        plan_path = folder / "plan.json"
        plan_path.write_text(json.dumps(plan, ensure_ascii=False), encoding="utf-8")
        scale = {"docs": docs, "paragraphs": paragraphs, "sentences": len(doc_of), "gen_s": gen_s, "oracle_s": oracle_s, "engines": {}}
        for eng in ("a", "b"):
            build_args = ["build-a", folder / "corpus", folder / "idx-a", a.heap_mb, a.threads] if eng == "a" else ["build-b", folder / "corpus", folder / "idx-b"]
            built = run(a.exe, *build_args)
            print(f"[{docs}] build {eng}: {built['build_s']:.1f}s", file=sys.stderr, flush=True)
            queried = run(a.exe, "query", eng, folder / f"idx-{eng}", plan_path, a.reps)
            mismatches = []
            for row in queried["queries"]:
                if row["result"] != expect[row["q"]] and not (expect[row["q"]] is None and row["result"]["sentences"] == 0):
                    mismatches.append(row["q"])
            for seq in queried["typing"]:
                for step in seq["steps"]:
                    e = expect[step["q"]]
                    if e is None:
                        if "skipped" not in step:
                            mismatches.append(step["q"])
                        continue
                    if step.get("sid_hash") != e["sid_hash"] or step.get("sentences") != e["sentences"] or not step.get("reuse_equals_fresh"):
                        mismatches.append(step["q"])
            scale["engines"][eng] = {"build": built, "query": queried, "mismatches": mismatches,
                                     "checked": len(queried["queries"]) + sum(len(s["steps"]) for s in queried["typing"])}
            print(f"[{docs}] query {eng}: mismatches={mismatches}", file=sys.stderr, flush=True)
        scale["expected"] = {q: expect[q] for q in plan["queries"]}
        report["scales"].append(scale)
        # 규모마다 저장: 다음 규모가 실패해도 앞 결과가 남는다.
        a.output.write_text(json.dumps(report, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
