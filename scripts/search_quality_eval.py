#!/usr/bin/env python3
"""SRCH5 synthetic ranking evaluation for SQLite and Rust backends."""
from __future__ import annotations

import argparse
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from docfit_core.timemachine.content_search import ContentSearchIndex
from docfit_core.timemachine.search_backend import RustBackend, SqliteBackend

QUERIES = {"행정 절차": ("행정 절차",), "예산 편성": ("예산 편성",), "안전 점검": ("안전 점검",)}

# 원래 세 질의는 그대로 두고 서로 어휘가 겹치지 않는 유형별 자료를 추가한다.
SRCH6_CASES = [
    ("제목", "주민 상담"), ("제목", "도시 녹화"), ("제목", "복지 연계"),
    ("밀도", "교통 조사"), ("밀도", "문화 행사"), ("밀도", "체육 시설"),
    ("위계", "재난 대응"), ("위계", "청년 창업"), ("위계", "보육 서비스"),
    ("AND", "공원 정비"), ("AND", "도로 보수"), ("AND", "환경 보전"),
    ("phrase", "디지털 역량"), ("phrase", "자원 순환"), ("phrase", "건강 증진"),
]


def build_srch6_corpus(root: Path):
    gold = build_corpus(root)
    types = {q: "기존 제목·밀도" for q in gold}
    for i, (kind, terms) in enumerate(SRCH6_CASES, 3):
        query = f'"{terms}"' if kind == "phrase" else terms
        types[query] = kind
        gold[query] = []
        for n in range(4):
            path = root / f"gold_{i}_{n}.md"
            title = terms if kind in {"제목", "위계"} else "분야별 업무 기록"
            if kind == "위계":
                body = f"{'#' * (n + 1)} {title}\n\n관계 기관 협의를 진행한다.\n"
            else:
                body = f"# {title}\n\n" + (f"{terms} 실무 내용을 검토한다. " * (4 - n))
            path.write_text(body, encoding="utf-8")
            gold[query].append(path.name)
        for n in range(12):
            path = root / f"body_{i}_{n}.md"
            body = f"# 일반 참고 기록\n\n{terms} 관련 현황을 확인한다.\n"
            path.write_text(body, encoding="utf-8")
        # 정답이 아닌 AND 단일어·phrase 비연속 자료도 둔다.
        a, b = terms.split()
        for n, text in enumerate([a, b, f"{a} 별도 검토 {b}"]):
            (root / f"negative_{i}_{n}.md").write_text(f"# 참고 문서\n\n{text}\n", encoding="utf-8")
    return gold, types


def build_corpus(root: Path) -> dict[str, list[str]]:
    gold: dict[str, list[str]] = {}
    for query_index, (query, (terms,)) in enumerate(QUERIES.items()):
        gold[query] = []
        # 정답: 검색어가 문서 제목에 있고 본문에서도 반복되는 문서.
        for n in range(4):
            path = root / f"gold_{query_index}_{n}.md"
            body = (f"# {terms} 기준과 실행 계획\n\n" + (f"{terms} 추진 절차와 지원 기준을 정리한다. " * (4 - n)))
            path.write_text(body, encoding="utf-8")
            gold[query].append(path.name)
        # 본문에만 검색어가 한 번 나타나는 낮은 우선순위 자료.
        for n in range(36):
            path = root / f"body_{query_index}_{n}.md"
            path.write_text(f"# 일반 업무 기록 {n}\n\n{terms} 관련 현황을 확인한다.\n", encoding="utf-8")
    return gold


def metrics(ranked: list[str], relevant) -> dict[str, float | int]:
    relevant_set = set(relevant)
    top10 = ranked[:10]
    first = next((i for i, name in enumerate(ranked, 1) if name in relevant_set), None)
    result = {"recall_at_10": round(len(set(top10) & relevant_set) / max(1, len(relevant_set)), 4),
            "mrr": round(1 / first, 4) if first else 0.0,
            "relevant_in_top_10": len(set(top10) & relevant_set), "relevant_total": len(relevant_set),
            "first_relevant_rank": first}
    if isinstance(relevant, (list, tuple)):
        observed = [name for name in ranked if name in relevant_set]
        positions = {name: i for i, name in enumerate(observed)}
        pairs = [(a, b) for i, a in enumerate(relevant) for b in relevant[i + 1:]]
        result["gold_order_pairwise_accuracy"] = round(
            sum(positions.get(a, 10**9) < positions.get(b, 10**9) for a, b in pairs) / max(1, len(pairs)), 4)
    return result


def evaluate(backend, queries, gold, root):
    result = {}
    for query in queries:
        rows = backend.search(query, roots=[str(root)], hierarchy=True, limit=100)
        ranking = [Path(row["path"]).name for row in rows]
        result[query] = metrics(ranking, gold[query])
        result[query]["top_10"] = ranking[:10]
        result[query]["top_10_breadcrumbs"] = [row.get("breadcrumb_text", "") for row in rows[:10]]
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--klib-exe")
    parser.add_argument("--suite", choices=("srch5", "srch6"), default="srch5")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="srch5-quality-") as temp:
        root = Path(temp) / "corpus"
        root.mkdir()
        gold, types = build_srch6_corpus(root) if args.suite == "srch6" else (build_corpus(root), {})
        paths = list(root.glob("*.md"))
        sqlite_index = ContentSearchIndex(Path(temp) / "sqlite.sqlite")
        sqlite = SqliteBackend(sqlite_index)
        executable = Path(args.klib_exe).resolve() if args.klib_exe else None
        rust = RustBackend(executable=executable, index_dir=Path(temp) / "tantivy")
        try:
            sqlite.index_files(paths)
            rust_errors = [r for r in rust.index_files(paths) if r.get("error")]
            result = {"documents": len(paths), "queries": len(gold), "types": types, "gold_per_query": 4,
                      "status": "complete" if not rust_errors else "rust_index_errors",
                      "sqlite": evaluate(sqlite, gold, gold, root),
                      "rust": evaluate(rust, gold, gold, root), "rust_index_errors": rust_errors}
            result["result_sets"] = {}
            for query in gold:
                sets = [{Path(r["path"]).name for r in b.search(query, roots=[str(root)], hierarchy=True, limit=500)}
                        for b in (sqlite, rust)]
                result["result_sets"][query] = {"sqlite": len(sets[0]), "rust": len(sets[1]),
                    "equal": sets[0] == sets[1], "missing": sorted(sets[0] - sets[1]), "extra": sorted(sets[1] - sets[0])}
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            print(json.dumps(result, ensure_ascii=False, indent=2))
        finally:
            rust.close()
            sqlite_index.close()
        return 0 if not rust_errors else 2


if __name__ == "__main__":
    raise SystemExit(main())
