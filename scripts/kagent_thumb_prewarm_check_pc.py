"""K에이전트 썸네일 미리 만들기(KCAP2) 실측(Windows PC): 캡처 직후 첫 미리보기 지연, 연속 캡처, 쉴 때 CPU·메모리.

사용: .venv\\Scripts\\python.exe scripts\\kagent_thumb_prewarm_check_pc.py <kagent.exe> [임시 폴더 위치]
시험 인스턴스(KAGENT_TEST_ROOT)로 띄워 임시 폴더만 쓴다 — 사용자 기록·실제 스크린샷 폴더·클립보드는 건드리지 않는다.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "scripts"))
import kagent_benchmark_pc as b  # noqa: E402
from docfit_app.kagent_client import KAgentClient  # noqa: E402
from PIL import Image  # noqa: E402


def noise_png(path: Path, w: int, h: int, seed: int):
    # 압축이 잘 안 되는(실제 화면보다 풀기 무거운) 그림 — 보수적인 측정.
    img = Image.effect_noise((w, h), 40 + seed % 20).convert("RGB")
    img.save(path, compress_level=1)


def main():
    exe = Path(sys.argv[1]).resolve()
    parent = sys.argv[2] if len(sys.argv) > 2 else None
    base = Path(tempfile.mkdtemp(prefix="kagent-prewarm-", dir=parent))
    root = base / "root"
    shots = root / "Screenshots"
    stage = base / "stage"
    stage.mkdir(parents=True)
    env = dict(os.environ, KAGENT_TEST_ROOT=str(root))
    env.pop("KAGENT_DATA", None)
    client = KAgentClient(root, timeout=5.0)
    proc = subprocess.Popen([str(exe)], env=env)
    out = {}

    def prewarm():
        return (client.status_file() or {}).get("thumb_prewarm") or {}

    def top():
        items = client.request("list", limit=1)["items"]
        return items[0] if items else None

    def drop(name, w, h, seed):
        src = stage / name
        noise_png(src, w, h, seed)
        t = time.perf_counter()
        os.replace(src, shots / name)
        return t

    def appeared(name):
        item = top()
        return item if item and item["preview"].endswith(name) else None

    def thumb(item):
        t = time.perf_counter()
        r = client.request("thumb", id=item["id"], size=item["size"])
        return round(r["ms"], 1), round((time.perf_counter() - t) * 1000, 1)

    try:
        assert b.wait(client.status_file, 10), "에이전트 시작 실패"
        assert b.wait(lambda: shots.is_dir(), 5)
        time.sleep(0.5)

        # 1) 1080p·4K: 기록 → 미리 만들기 끝 → 첫 미리보기
        for label, (w, h) in {"1080p": (1920, 1080), "4k": (3840, 2160)}.items():
            name = f"캡처 {label}.png"
            done_before = prewarm().get("made", 0)
            t0 = drop(name, w, h, 1)
            item = b.wait(lambda: appeared(name), 10)
            t_rec = time.perf_counter()
            ok = b.wait(lambda: prewarm().get("made", 0) > done_before, 10)
            t_pre = time.perf_counter()
            agent_ms, rt_ms = thumb(item)
            out[label] = dict(record_ms=round((t_rec - t0) * 1000), prewarm_done_after_record_ms=round((t_pre - t_rec) * 1000) if ok else None,
                              first_preview_agent_ms=agent_ms, first_preview_roundtrip_ms=rt_ms)

        # 2) 미리 만들기 전에 바로 가리킴(즉시 생성 경로 그대로)
        name = "캡처 바로.png"
        drop(name, 1920, 1080, 2)
        item = b.wait(lambda: appeared(name), 10)
        agent_ms, rt_ms = thumb(item)
        b.wait(lambda: not prewarm().get("queued") and not prewarm().get("generating"), 10)
        out["hover_before_prewarm"] = dict(agent_ms=agent_ms, roundtrip_ms=rt_ms, status_after=prewarm())

        # 3) 연속 캡처 12장(대기열 상한 8): 최근 것 우선, 넘친 것은 버림
        before = prewarm()
        names = [f"연속 {i:02d}.png" for i in range(12)]
        for i, n in enumerate(names):
            noise_png(stage / n, 1920, 1080, 10 + i)
        for n in names:
            os.replace(stage / n, shots / n)
            time.sleep(0.03)
        b.wait(lambda: appeared(names[-1]), 15)
        b.wait(lambda: (lambda p: p.get("made", 0) + p.get("dropped", 0) + p.get("skipped", 0) + p.get("failed", 0)
                        >= before.get("made", 0) + before.get("dropped", 0) + before.get("skipped", 0) + before.get("failed", 0) + 12
                        and not p.get("queued") and not p.get("generating"))(prewarm()), 30)
        after = prewarm()
        items = {i["preview"].split(" · ", 1)[-1]: i for i in client.request("list", limit=30)["items"]}
        newest_ms = thumb(items[names[-1]])[0] if names[-1] in items else None
        oldest_ms = thumb(items[names[0]])[0] if names[0] in items else None
        out["burst12"] = dict(made=after.get("made", 0) - before.get("made", 0), dropped=after.get("dropped", 0) - before.get("dropped", 0),
                              newest_preview_agent_ms=newest_ms, oldest_preview_agent_ms=oldest_ms, recorded=sum(n in items for n in names))

        # 4) 쉴 때 CPU·메모리(5초)
        time.sleep(1)
        s1 = b.process_stats(proc.pid)
        time.sleep(5)
        s2 = b.process_stats(proc.pid)
        out["idle_cpu_percent_5s"] = round((s2["cpu"] - s1["cpu"]) / 5 * 100, 4)
        out["idle_private_mb"] = round(s2["private"] / 2**20, 2)
        out["idle_working_set_mb"] = round(s2["ws"] / 2**20, 2)

        # 5) 디스크 캐시 평문 없음, 모두 지우기 뒤 삭제
        thumbs = root / "thumbs"
        files = list(thumbs.glob("*.thumb")) if thumbs.exists() else []
        out["disk_cache_files"] = len(files)
        out["disk_cache_kb"] = round(sum(f.stat().st_size for f in files) / 1024, 1)
        out["disk_plain_png"] = sum(f.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n" for f in files)
        client.request("clear", keep_pinned=False)
        out["after_clear_thumbs_dir_exists"] = thumbs.exists()
        out["after_clear_status"] = prewarm()
    finally:
        try:
            client.request("quit")
        except Exception:
            pass
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
        shutil.rmtree(base, ignore_errors=True)
    print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
