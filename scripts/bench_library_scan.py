"""문서뷰·사진뷰 검색의 단계별 시간 측정(SCANSPEED1). 파일 이름·내용은 출력하지 않고 개수·시간만 낸다.

    python scripts/bench_library_scan.py --klib <klib.exe> --code <측정할 저장소> --count 10000 50000
    python scripts/bench_library_scan.py --klib <klib.exe> --code <측정할 저장소> --real

--code 로 변경 전 코드(git archive 로 푼 폴더)와 변경 후 코드를 같은 방법으로 잰다.
합성 파일과 DB·캐시는 --work 아래 임시 폴더에 만들고 끝나면 함께 지운다.
"""
from __future__ import annotations

import argparse
import io
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

POLL = 0.12  # 화면의 진행 확인 간격
DOC_EXT = "hwp,hwpx,pdf,ppt,pptx,xls,xlsx"
PHOTO_EXT = "bmp,heic,jpeg,jpg,png,tif,tiff,webp"


def make_files(root: Path, count: int, kind: str) -> None:
    data = b""
    if kind == "photo":
        from PIL import Image
        image = Image.new("RGB", (32, 24), (90, 120, 150))
        exif = image.getexif()
        exif[0x0132] = "2025:03:14 10:20:30"
        buffer = io.BytesIO()
        image.save(buffer, format="JPEG", exif=exif)
        data = buffer.getvalue()
    suffix = ".jpg" if kind == "photo" else ".pdf"
    for index in range(count):
        folder = root / f"folder{index // 100:04}"
        if index % 100 == 0:
            folder.mkdir()
        (folder / f"item{index:06}{suffix}").write_bytes(data)


def win32_scan(klib: str, roots: list[str], extensions: str, excluded: list[str]) -> dict:
    """win32 창과 같은 klib 호출: 줄이 오는 대로 읽는다."""
    command = [klib, "scan", "--roots", *roots, "--ext", extensions, "--json-lines"]
    if excluded:
        command += ["--exclude", *excluded]
    started = time.perf_counter()
    first = None
    rows = 0
    with subprocess.Popen(command, stdout=subprocess.PIPE, text=True, encoding="utf-8") as process:
        for line in process.stdout:
            json.loads(line)
            rows += 1
            if first is None:
                first = time.perf_counter() - started
    return {"rows": rows, "first": first, "total": time.perf_counter() - started}


def app_scan(command, start_action: str, *, streaming: bool, list_action: str | None) -> dict:
    """화면과 같은 순서로 부른다. 응답을 JSON으로 만들고 읽는 시간이 브리지 전달 비용이다."""
    bridge = {"seconds": 0.0, "bytes": 0, "largest": 0}

    def call(action, payload):
        response = command(action, payload)
        t0 = time.perf_counter()
        text = json.dumps(response, ensure_ascii=False)
        decoded = json.loads(text)
        bridge["seconds"] += time.perf_counter() - t0
        size = len(text.encode("utf-8"))
        bridge["bytes"] += size
        bridge["largest"] = max(bridge["largest"], size)
        return decoded

    started = time.perf_counter()
    response = call(start_action, {})
    first = None
    all_rows = None
    rows = 0
    if response.get("state") == "cached":
        result = response.get("result", {})
        rows = len(result.get("documents", result.get("photos", [])))
        first = time.perf_counter() - started
    cached = first is not None
    since = 0
    state = {"state": "running"}
    backlog = False
    while state["state"] in ("running", "cancelling") or backlog:
        if not backlog:
            time.sleep(POLL)
        state = call("scan_progress", {"since": since} if streaming and not cached else {})
        backlog = False
        if "rows" in state:
            backlog = since < state["next"] < state["total"]
            since = state["next"]
            rows += len(state["rows"])
            if state["rows"]:
                first = first or time.perf_counter() - started
                all_rows = time.perf_counter() - started
        if state["state"] == "failed":
            raise RuntimeError(state.get("error"))
    scan_done = time.perf_counter() - started
    if "documents" in state.get("result", {}):
        rows = len(state["result"]["documents"])
        first = first or time.perf_counter() - started
        all_rows = scan_done
    if list_action:
        listed = call(list_action, {})
        rows = sum(len(group["items"]) for group in listed["groups"])
        first = first or time.perf_counter() - started
    return {"rows": rows, "first": first, "all_rows": all_rows or time.perf_counter() - started,
            "scan_done": scan_done, "total": time.perf_counter() - started,
            "bridge_s": bridge["seconds"], "bridge_mb": bridge["bytes"] / 1e6,
            "largest_mb": bridge["largest"] / 1e6, "engine": state.get("engine")}


def measure(kind: str, roots: list[str] | None, work: Path, streaming: bool, runs: tuple[str, ...]) -> None:
    from docfit_app import library_scan_cache

    data = work / f"data-{kind}"
    library_scan_cache._data_root = lambda: data
    drive_roots = [Path(root) for root in roots] if roots else None
    if kind == "doc":
        from docfit_app.doc_grid_service import DocGridService
        make = lambda: DocGridService(thumb_dir=data / "thumbs", drive_roots=drive_roots)
        start, listing = "list_default", None
    else:
        from docfit_app.photo_album_service import PhotoAlbumService
        make = lambda: PhotoAlbumService(db_path=data / "photos.db", thumb_dir=data / "thumbs",
                                         drive_roots=drive_roots)
        start, listing = "scan_default", "list_by_date"
    service = make()
    for label in runs:
        if label == "again":  # 저장된 목록 없이 다시 검색(DB는 남아 있음)
            (data / "scan-cache" / ("documents.json" if kind == "doc" else "photos.json")).unlink(missing_ok=True)
        report(kind, label, app_scan(service.command, start, streaming=streaming, list_action=listing))
        service = make()
    if kind == "photo":
        for label, payload in (("search name", {"query": "item0001"}), ("search all", {"query": "item"})):
            t0 = time.perf_counter()
            found = service.command("search", payload)["results"]
            queried = time.perf_counter() - t0
            t0 = time.perf_counter()
            text = json.dumps(found, ensure_ascii=False)
            json.loads(text)
            report(kind, label, {"rows": len(found), "total": queried, "bridge_s": time.perf_counter() - t0})


def report(kind: str, label: str, values: dict) -> None:
    shown = {key: (round(value, 3) if isinstance(value, float) else value) for key, value in values.items()}
    print(json.dumps({"kind": kind, "run": label, **shown}, ensure_ascii=False), flush=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--klib", required=True)
    parser.add_argument("--code", default=str(Path(__file__).resolve().parents[1]))
    parser.add_argument("--work", default=".")
    parser.add_argument("--count", type=int, nargs="*", default=[10000, 50000])
    parser.add_argument("--kind", nargs="*", default=["doc", "photo"])
    parser.add_argument("--runs", nargs="*", default=["first", "cached", "again"])
    parser.add_argument("--source", help="이미 만든 합성 폴더를 그대로 쓴다(전후 비교용)")
    parser.add_argument("--real", action="store_true", help="실제 기본 검색 범위를 한 번 잰다(개수·시간만)")
    args = parser.parse_args()
    sys.path.insert(0, str(Path(args.code).resolve()))
    os.environ["KLIB_EXE"] = str(Path(args.klib).resolve())
    os.environ.pop("DOCFIT_SCAN_ENGINE", None)
    from docfit_app import background_scan, doc_grid_service
    streaming = hasattr(background_scan, "PAGE_ROWS")
    work_parent = Path(args.work).resolve()
    if not args.real:
        doc_grid_service.query_names_service = lambda *a, **k: None  # 색인 서비스는 실제 드라이브를 답하므로 끈다
    for kind in args.kind:
        extensions = DOC_EXT if kind == "doc" else PHOTO_EXT
        for count in [None] if args.real else args.count:
            with tempfile.TemporaryDirectory(prefix=".bench-", dir=work_parent) as temp:
                work = Path(temp)
                if args.real:
                    from docfit_app.library_scope import current_default_roots
                    roots = None
                    report(kind, "win32 real", win32_scan(args.klib, [str(r) for r in current_default_roots()
                                                                      if r.is_dir()], extensions, []))
                else:
                    source = Path(args.source).resolve() if args.source else work / "files"
                    if not args.source:
                        source.mkdir()
                        make_files(source, count, kind)
                    roots = [os.path.normcase(str(source))]  # 실제 기본 루트처럼 소문자 표기
                    win32_scan(args.klib, roots, extensions, [])  # 파일 시스템 캐시를 같은 상태로 만든다
                    report(kind, f"win32 {count}", win32_scan(args.klib, roots, extensions, []))
                print(f"# {kind} {count or 'real'}", flush=True)
                measure(kind, roots, work, streaming, ("first",) if args.real else tuple(args.runs))


if __name__ == "__main__":
    main()
