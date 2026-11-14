"""업무자료함(노트북) 출처의 변경분 색인을 쉬는 시간에(NBIDX1, 2026-11-13).

klib가 출처마다 원본 크기·시각을 저장해 두므로, 비교·상태 고치기·다시 색인은 모두 klib가 한다(Python은 호출만).

1. `nb_source_check`: 출처 상태를 고친다(ready → stale·missing, missing → ready·stale). 다시 색인할 출처(stale·failed) 목록.
2. 출처마다 `ctx.checkpoint()` 뒤 `NotebookService.refresh_source` → klib `nb_source_refresh`(같은 출처 번호로 문단 교체,
   인용 연결 유지). klib가 못 읽는 hwp·pdf만 Python이 추출해 나눠 보낸다.

- 상태 파일이 따로 없다: 기준(크기·시각)과 상태는 klib 업무자료함 DB(`nb_source`)에 있다. 멈추거나 실패한 출처는 상태가
  stale·failed로 남아 다음 쉬는 시간에 다시 한다.
- klib를 이번 실행에서 아직 쓰지 않았으면(브리지가 없으면) 아무것도 하지 않는다(쉬는 시간에 klib를 새로 띄우지 않음).
- 업무자료함 저장소를 열지 못한 상태(옮기기 실패, code NB_UNAVAILABLE)면 그 사실만 남기고 끝낸다.
"""
from __future__ import annotations

import logging

from .library_bridge import LibraryBridgeError

NOTEBOOK_IDLE_JOB = "notebook-sources"
NOTEBOOK_IDLE_EVERY_SECONDS = 1800


class NotebookSourceRefreshJob:
    """IdleJob: 업무자료함 출처 중 원본이 바뀐 것만 같은 번호로 다시 색인한다."""

    name = NOTEBOOK_IDLE_JOB

    def __init__(self, bridge_provider, service_factory):
        self._bridge_provider = bridge_provider
        self._service_factory = service_factory
        self.last_result = None

    def pending(self):
        try:
            return self._bridge_provider() is not None
        except Exception:
            return False

    def run(self, ctx):
        bridge = self._bridge_provider()
        if bridge is None:
            return
        result = {"checked": 0, "changed": 0, "refreshed": 0, "errors": 0, "missing": 0, "relinked": 0,
                  "unresolved": 0, "unavailable": False}
        try:
            reply = bridge.request({"cmd": "nb_source_check"})
        except LibraryBridgeError:
            logging.getLogger(__name__).debug("업무자료함 원본 검사 실패", exc_info=True)
            result["errors"] = 1
            self.last_result = result
            return
        if not isinstance(reply, dict) or not reply.get("ok"):
            result["unavailable"] = isinstance(reply, dict) and reply.get("code") == "NB_UNAVAILABLE"
            result["errors"] = 0 if result["unavailable"] else 1
            self.last_result = result
            return
        targets = [row for row in reply.get("refresh") or () if isinstance(row, dict)]
        counts = reply.get("counts") if isinstance(reply.get("counts"), dict) else {}
        result.update(checked=int(reply.get("checked") or 0), changed=len(targets), missing=int(counts.get("missing") or 0))
        service = self._service_factory(bridge)
        ctx.progress(0, len(targets))
        for done, row in enumerate(targets, start=1):
            if not ctx.checkpoint():
                break
            try:
                outcome = service.refresh_source(row.get("notebook_id"), row.get("source_id"))
            except LibraryBridgeError:
                logging.getLogger(__name__).debug("업무자료함 출처 다시 색인 실패", exc_info=True)
                outcome = None
            if isinstance(outcome, dict) and outcome.get("ok"):
                result["refreshed"] += 1
                result["relinked"] += int(outcome.get("relinked") or 0)
                result["unresolved"] += int(outcome.get("unresolved") or 0)
            else:
                result["errors"] += 1
            ctx.progress(done, len(targets))
        self.last_result = result
