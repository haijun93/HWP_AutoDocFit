"""DOCDBPERF1 합성 전용 측정. 설치 서비스·사용자 자료·설정을 사용하지 않는다.

앱 가상환경 Python으로 --klib <개발 빌드> --work <저장소 밖 임시 폴더>를 지정한다.
--code로 전후 코드를 고르고 각각 3회 실행한다. 파일/DB/화면 결과의 해시는 경로를
상대화하고 시각을 고정해 전후 대조한다. SQL/행 계측은 양쪽에 동일하게 적용한다.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time
from unittest.mock import patch


def digest(value, root):
    def relative(item):
        if isinstance(item, str):
            return item.replace(str(root), "<synthetic>").replace(str(root).lower(), "<synthetic>")
        if isinstance(item, dict):
            return {k: relative(v) for k, v in item.items()}
        if isinstance(item, (tuple, list)):
            return [relative(v) for v in item]
        return item
    text = json.dumps(relative(value), ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(text.encode()).hexdigest()


def fixture(root, count):
    def folder_files(first):
        folder = root / f"folder{first // 100:04}"
        folder.mkdir()
        for index in range(first, min(first + 100, count)):
            path = folder / f"item{index:06}.pdf"
            path.write_bytes(b"")
            os.utime(path, (1700000000, 1700000000))
    # 준비만 병렬화한다. 모든 생성 스레드가 끝난 뒤 측정은 순차 실행한다.
    with ThreadPoolExecutor(max_workers=16) as pool:
        list(pool.map(folder_files, range(0, count, 100)))


def change(root, count):
    for number, index in enumerate(range(0, count, 100)):
        path = root / f"folder{index // 100:04}" / f"item{index:06}.pdf"
        if number % 10 < 4:
            path.write_bytes(b"changed")
            os.utime(path, (1700000010, 1700000010))
        elif number % 10 < 7:
            added = path.with_name(f"new{number:06}.pdf")
            added.write_bytes(b"")
            os.utime(added, (1700000000, 1700000000))
        else:
            path.unlink()


def restore(root, count):
    """직접 생성한 1%만 원상 복원한다. 측정 사이 파일 생성 부하를 줄인다."""
    for number, index in enumerate(range(0, count, 100)):
        path = root / f"folder{index // 100:04}" / f"item{index:06}.pdf"
        if 4 <= number % 10 < 7:
            path.with_name(f"new{number:06}.pdf").unlink(missing_ok=True)
        else:
            path.write_bytes(b"")
            os.utime(path, (1700000000, 1700000000))


def source_rows(root, legacy=False):
    rows = []
    for folder in sorted(root.iterdir()):
        with os.scandir(folder) as entries:
            for entry in entries:
                st = entry.stat()
                row = dict(path=entry.path, name=entry.name, ext="pdf")
                if not legacy:
                    row.update(size=st.st_size, mtime=st.st_mtime, cloud=False)
                rows.append(row)
    return sorted(rows, key=lambda row: row["name"].casefold())


def measure(root, data, source, klib, count):
    from docfit_app import doc_grid_service as grid, library_db as lib, library_scan_cache

    prepared = source_rows(root, source == "legacy")  # 측정 밖에서 모의 색인 준비

    def query(extensions, excluded=(), on_row=None):
        for row in prepared:
            if on_row:
                on_row(row)
        return prepared

    timers, calls = {}, {}

    def timed(cls, name, label):
        original = getattr(cls, name)

        def wrapper(*args, **kwargs):
            start = time.perf_counter()
            try:
                return original(*args, **kwargs)
            finally:
                timers[label] = timers.get(label, 0) + time.perf_counter() - start
                calls[label] = calls.get(label, 0) + 1
        return patch.object(cls, name, wrapper)

    # 포함 관계: snapshot>load, source>feed, finish>write>sql, items는 별도.
    patches = [timed(lib.LibraryDB, "load", "load"), timed(lib.LibraryDB, "write", "write"),
               timed(lib.LibraryDB, "_run", "sql"), timed(lib.ViewSync, "publish_snapshot", "snapshot"),
               timed(lib.ViewSync, "feed", "feed"), timed(lib.ViewSync, "finish", "finish"),
               timed(lib.ViewSync, "items", "items"), timed(lib.FolderStats, "stat", "stat"),
               timed(grid.DocGridService, "list_default_documents", "source"),
               patch.object(grid, "query_names_service", query if source != "klib" else lambda *a, **k: None),
               patch.object(library_scan_cache, "_data_root", lambda: data)]
    for item in patches:
        item.start()
    try:
        service = grid.DocGridService(thumb_dir=data / "thumbs", drive_roots=[root], klib_path=klib)
        first = all_shown = last_event = None
        bridge = 0.0
        events, shown = [], {}
        since = 0
        started = time.perf_counter()
        service.command("list_default", {})
        while True:
            time.sleep(0.12)
            while True:
                response = service.command("scan_progress", {"since": since})
                t0 = time.perf_counter()
                response = json.loads(json.dumps(response, ensure_ascii=False))
                bridge += time.perf_counter() - t0
                assert response["state"] != "failed", response.get("error")
                since = response["next"]
                now = time.perf_counter() - started
                for row in response["rows"]:
                    events.append(row)
                    first = first if first is not None else now
                    last_event = now
                    if row.get("op") == "remove":
                        shown.pop(row["path"], None)
                    else:
                        if row.get("old_path"):
                            shown.pop(row["old_path"], None)
                        shown[row["path"]] = {k: v for k, v in row.items() if k not in ("op", "old_path")}
                if all_shown is None and len(shown) >= count:
                    all_shown = now
                if since >= response["total"]:
                    break
            if response["state"] not in ("running", "cancelling"):
                break
        total = time.perf_counter() - started
    finally:
        for item in reversed(patches):
            item.stop()
    # 측정 뒤 결과·DB 대조. 목록 순서와 모든 공개 필드, 썸네일 키를 포함한다.
    final = service._background_scan.progress()["result"]
    ordered = sorted(shown.values(), key=lambda row: row["name"].casefold())
    assert final["documents"] == ordered
    records = service.db.load()
    assert len(records) == count == len(ordered)
    assert sorted((r[1], r[4], r[5]) for r in records) == sorted(
        (r["path"], r["size"], r["mtime"]) for r in source_rows(root))
    return dict(total=total, first=first, all_shown=all_shown, last_event=last_event, bridge=bridge,
                timings=timers, calls=calls, changes=final["changes"], count=len(ordered),
                events_hash=digest(sorted(events, key=lambda r: (r["path"], r.get("op", ""))), root), db_hash=digest(records, root),
                list_hash=digest(ordered, root), db_bytes=service.db.path.stat().st_size)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--code", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--klib", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--count", type=int, default=65000)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--sources", nargs="+", choices=("service", "klib", "legacy"), default=["service", "klib"])
    args = parser.parse_args()
    if args.count < 1000 or args.count % 1000 or args.repeats < 1:
        parser.error("건수는 1000 이상의 1000 배수, 반복은 1 이상이어야 합니다.")
    if any(args.work.resolve().is_relative_to(code.resolve()) for code in
           (args.code, Path(__file__).resolve().parents[1])):
        parser.error("합성 자료·DB는 저장소 밖 --work에 생성해야 합니다.")
    sys.path.insert(0, str(args.code.resolve()))
    os.environ["DOCFIT_FORBID_REAL_DATA"] = "1"
    os.environ["KLIB_EXE"] = str(args.klib.resolve())
    os.environ.pop("DOCFIT_SCAN_ENGINE", None)
    args.work.mkdir(parents=True, exist_ok=True)
    root = args.work / "synthetic-files"
    marker = args.work / "synthetic-count.json"
    if root.exists():
        if not marker.is_file() or json.loads(marker.read_text()) != args.count:
            raise ValueError("이 도구가 완성한 합성 자료만 재사용할 수 있습니다.")
        restore(root, args.count)
    else:
        root.mkdir()
        fixture(root, args.count)
        marker.write_text(json.dumps(args.count))
    for repeat in range(1, args.repeats + 1):
        for source in args.sources:
            with tempfile.TemporaryDirectory(prefix="docdbperf1-", dir=args.work) as temp:
                base = Path(temp)
                for phase in ("first", "cached", "changed"):
                    if phase == "changed":
                        change(root, args.count)
                    result = measure(root, base / "data", source, args.klib, args.count)
                    print(json.dumps(dict(repeat=repeat, source=source, phase=phase, **result)), flush=True)
                restore(root, args.count)


if __name__ == "__main__":
    main()
