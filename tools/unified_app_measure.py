"""앱 화면 첫 결과 시간 측정용 연결(UNIFIEDAPP1). 합성 색인·격리 파이프·임시 설정만 쓴다.

flutter_ui/test/timemachine_unified_measure_test.dart 가 이 파일을 띄우고 한 줄 JSON
``[action, payload]`` 을 보내면 실제 TimemachineService 와 ``ktm.exe --search-host`` 가 답한다.
설치 서비스·사용자 설정·사용자 색인은 읽지도 쓰지도 않는다.

사용: python tools/unified_app_measure.py UNIFIED_BENCH_EXE KTM_EXE
      python tools/unified_app_measure.py UNIFIED_BENCH_EXE KTM_EXE --direct 결과.json
--direct 는 화면 없이 서비스 응답 시간(2ms 간격 조회)만 질의마다 5회 잰다.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

REPO = Path(__file__).resolve().parents[1]


def direct(service, out):
    """첫 부분 결과와 전체 결과까지. 호스트 프로세스를 띄우는 첫 실행은 따로 적는다."""
    def run(query):
        began = time.perf_counter()
        generation = service.command("unified_start", {"query": query})["generation"]
        first = None
        while True:
            result = service.command("unified_poll", {"generation": generation})["result"]
            elapsed = round((time.perf_counter() - began) * 1000, 1)
            if result and first is None and result["answer"]["rows"]:
                first = elapsed
            if result and result["event"] != "part":
                assert result["event"] == "page" and not result["incomplete"], result
                return {"first_ms": first, "complete_ms": elapsed, "totals": result["totals"]}
            time.sleep(0.002)

    report = {"documents": 1000, "cold": run("예산집행")}
    for query in ("예산집행", "2026; 공사 계약"):
        report[query] = [run(query) for _ in range(5)]
    Path(out).write_text(json.dumps(report, ensure_ascii=False, indent=2) + chr(10), encoding="utf-8")


def main(bench, ktm, mode=None, out=None):
    with tempfile.TemporaryDirectory(prefix="unified-measure-", ignore_cleanup_errors=True) as folder:
        base = Path(folder)
        # 앱 데이터 경로를 임시 폴더로 돌린 뒤에 서비스를 읽는다.
        for name in ("LOCALAPPDATA", "APPDATA", "USERPROFILE", "HOME"):
            os.environ[name] = str(base / name.lower())
        sys.path.insert(0, str(REPO))
        from docfit_app.timemachine_service import TimemachineService
        from docfit_app.unified_search_host import UnifiedSearchHost

        corpus = base / "corpus"
        pipe = rf"\\.\pipe\unified-measure-{os.getpid()}"
        fixture = subprocess.Popen([bench, str(corpus), pipe], stdout=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
        service = None
        try:
            while not (corpus / "server-ready").exists():
                if fixture.poll() is not None:
                    raise SystemExit("합성 색인 준비 실패")
                time.sleep(0.05)
            host = UnifiedSearchHost(executable=lambda: Path(ktm), overrides={
                "pipe": pipe, "index_root": str(corpus), "paragraphs": str(corpus / "timeline-content.sqlite")})
            service = TimemachineService(base / "settings", db_path=base / "timeline.db", unified_host=host)
            service.command("settings_set", {"roots": [str(corpus / "documents")], "time_mode": "max",
                                             "enabled": False, "extensions": ["txt"]})
            service.command("unified_view_set", {"enabled": True})
            if mode == "--direct":
                direct(service, out)
                return
            print(json.dumps({"ready": True}), flush=True)
            for line in sys.stdin.buffer:
                action, payload = json.loads(line.decode("utf-8"))
                if action == "settings_get":
                    # 색인 감시는 켜지 않고 화면만 켠 상태로 보인다.
                    answer = service.command(action, payload)
                    answer["settings"]["enabled"] = True
                elif action.startswith("unified_"):
                    answer = service.command(action, payload)
                elif action == "status":
                    answer = {"ok": True, "watch": "app", "scan": {"running": False}}
                elif action == "content_index_status":
                    answer = {"ok": True, "running": False}
                elif action == "scan":
                    answer = {"ok": True, "by": "app"}
                elif action == "range":
                    answer = {"ok": True, "items": []}
                else:
                    answer = {"ok": True, "days": {}, "hours": {}}
                sys.stdout.buffer.write((json.dumps(answer, ensure_ascii=True) + "\n").encode("ascii"))
                sys.stdout.buffer.flush()
        finally:
            if service is not None:
                service.close()
            fixture.kill()
            fixture.wait()


if __name__ == "__main__":
    main(*sys.argv[1:5])
