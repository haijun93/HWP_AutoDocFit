"""HRUST1 측정: 문서 N개를 새 임시 색인에 넣는 전체 시간(위계 판정 포함)을 Python·Rust 경로로 잰다.

문서 목록은 본문 색인 DB **사본**의 content_meta에서 읽고, 원본 파일은 읽기만 한다.
경로마다 새 임시 색인을 만든다. 첫 경로 전에 추출만 한 번 돌려 OS 파일 캐시를 같은 조건으로 맞춘다.
결과에는 시간·건수와 두 경로의 위계 결과 일치 여부만 남긴다.
"""
import argparse
import json
import os
import sqlite3
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


def run(db, dll, count, workdir):
    from docfit_app import hierarchy_bridge as hb
    from docfit_core.timemachine import content_search as cs

    src = sqlite3.connect(f"file:{Path(db).as_posix()}?mode=ro", uri=True)
    paths = [r[0] for r in src.execute("SELECT path FROM content_meta ORDER BY path")][:count]
    hb.find_library = lambda: Path(dll)

    original = cs.classify_paragraph_levels
    for path in paths:  # 캐시 맞춤(시간 미측정)
        try:
            with_patch = cs.classify_paragraph_levels
            cs.classify_paragraph_levels = lambda _p, records, **_k: records
            cs.extract_paragraphs(path)
        except Exception:
            pass
        finally:
            cs.classify_paragraph_levels = with_patch

    out = {"documents": len(paths)}
    levels = {}
    for backend in ("python", "rust"):
        settings = {"rust_search_hierarchy": backend == "rust", "hierarchy_engine": False}
        hb._read_settings = lambda settings=settings: settings
        spent = [0.0, 0]

        def timed(path, paragraphs, **kwargs):
            started = time.perf_counter()
            try:
                return original(path, paragraphs, **kwargs)
            finally:
                spent[0] += time.perf_counter() - started
                spent[1] += len(paragraphs)

        cs.classify_paragraph_levels = timed
        index = cs.ContentSearchIndex(Path(workdir) / f"measure-{backend}.sqlite")
        errors = 0
        started = time.perf_counter()
        for path in paths:
            errors += bool(index.index_file(path).get("error"))
        total = time.perf_counter() - started
        cs.classify_paragraph_levels = original
        levels[backend] = index.db.execute(
            "SELECT m.path, m.paragraph_index, f.level FROM content_paragraph_map m"
            " JOIN content_paragraph_fts f ON f.rowid=m.para_rowid ORDER BY 1, 2").fetchall()
        index.close()
        out[backend] = {"total_seconds": round(total, 2), "classify_seconds": round(spent[0], 3),
                        "classified_paragraphs": spent[1], "errors": errors}
    out["same_levels"] = levels["python"] == levels["rust"]
    out["indexed_paragraphs"] = len(levels["rust"])
    return out


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", required=True, type=Path, help="본문 색인 DB 사본")
    parser.add_argument("--dll", required=True, type=Path)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--workdir", type=Path, help="임시 색인 위치(기본: 시스템 임시 폴더)")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(dir=args.workdir) as folder:
        os.environ["APPDATA"] = folder  # 사용자 설정 파일을 읽지 않게 한다.
        result = run(args.db, args.dll, args.count, folder)
    text = json.dumps(result, ensure_ascii=False, indent=2)
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    print(text)
