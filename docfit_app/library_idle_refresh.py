"""자료함(가상 폴더) 자료의 변경분 본문 색인을 쉬는 시간에(IDLE1, 2026-11-13).

klib는 자료를 넣을 때(item_add·item_update) 본문을 색인하지만, 넣은 뒤 원본 파일이 바뀌면 다시 색인하지 않는다.
이 작업은 쉬는 시간마다 자료 목록(`item_search` 빈 검색어)을 읽어 원본의 수정 시각·크기가 바뀐 파일만
`index_file`로 다시 넣는다. 원본 파일은 읽기만 한다(klib가 읽음).

- 처음 한 번은 기준만 잡는다(지금 색인이 넣을 때 만든 최신이라고 보고 다시 색인하지 않음).
- 한 파일이 여러 자료함에 들어 있으면 건너뛴다: 파이프 `index_file`은 자료함 id 하나만 받아, 다시 넣으면
  다른 자료함 소속이 지워진다(klib에 여러 id를 받는 명령이 생기면 풀 것 — `skipped_multi_box`로 수를 남김).
- klib를 이번 실행에서 아직 쓰지 않았으면(브리지가 없으면) 아무것도 하지 않는다(쉬는 시간에 klib를 새로 띄우지 않음).
"""
from __future__ import annotations

import json
import logging
import os
import tempfile
from pathlib import Path

from .library_bridge import LibraryBridgeError

LIBRARY_IDLE_JOB = "library-items"
LIBRARY_IDLE_EVERY_SECONDS = 1800
INDEXED_KINDS = frozenset(("file", "text", "rich_text", "note"))
STATE_VERSION = 1


def _key(path):
    return os.path.normcase(os.path.abspath(path))


class LibraryItemRefreshJob:
    """IdleJob: 자료함 자료 중 바뀐 원본만 다시 색인한다."""

    name = LIBRARY_IDLE_JOB

    def __init__(self, bridge_provider, state_path, *, stat=os.stat):
        self._bridge_provider = bridge_provider
        self.state_path = Path(state_path)
        self._stat = stat
        self.last_result = None

    def pending(self):
        try:
            return self._bridge_provider() is not None
        except Exception:
            return False

    # 기준 상태 ---------------------------------------------------------
    def _load_state(self):
        try:
            data = json.loads(self.state_path.read_text(encoding="utf-8"))
            if data.get("version") == STATE_VERSION and isinstance(data.get("files"), dict):
                return data["files"]
        except (OSError, ValueError, AttributeError):
            pass
        return None

    def _save_state(self, files):
        self.state_path.parent.mkdir(parents=True, exist_ok=True)
        fd, temporary = tempfile.mkstemp(prefix="library-idle-", suffix=".tmp", dir=self.state_path.parent)
        try:
            with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
                json.dump({"version": STATE_VERSION, "files": files}, stream, ensure_ascii=False)
            os.replace(temporary, self.state_path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)

    # 목록 --------------------------------------------------------------
    def _items(self, bridge):
        reply = bridge.request({"cmd": "item_search", "query": ""})
        if not isinstance(reply, dict) or reply.get("ok") is False:
            raise LibraryBridgeError("자료 목록을 읽지 못했습니다.")
        groups = {}
        for item in reply.get("items") or ():
            path = item.get("source_path") if isinstance(item, dict) else None
            if item.get("kind") not in INDEXED_KINDS or not isinstance(path, str) or not path:
                continue
            groups.setdefault(_key(path), []).append(item)
        return groups

    def _signature(self, path):
        try:
            info = self._stat(path)
        except OSError:
            return None
        return [int(info.st_mtime_ns if hasattr(info, "st_mtime_ns") else info.st_mtime * 1e9), int(info.st_size)]

    # 실행 --------------------------------------------------------------
    def run(self, ctx):
        bridge = self._bridge_provider()
        if bridge is None:
            return
        groups = self._items(bridge)
        previous = self._load_state()
        current, changed = {}, []
        multi_box = 0
        for key, items in groups.items():
            signature = self._signature(items[0]["source_path"])
            if signature is None:
                continue  # 원본이 없거나 닿지 않음(연결 끊긴 드라이브): 기준에서 빼고 다음에 다시 본다
            boxes = {item.get("box_id") for item in items}
            # 새로 들어온 자료는 klib가 넣을 때 색인했으므로 기준만 잡는다. 기준이 있던 자료만 비교한다.
            if previous is not None and key in previous and previous[key] != signature:
                if len(boxes) > 1:
                    multi_box += 1
                    current[key] = previous.get(key)  # 기준을 옮기지 않아 klib가 고쳐지면 그때 색인
                    continue
                changed.append((key, items[0], signature))
                continue
            current[key] = signature
        result = {"checked": len(groups), "changed": len(changed), "indexed": 0, "errors": 0,
                  "skipped_multi_box": multi_box, "baseline": previous is None}
        ctx.progress(0, len(changed))
        done = 0
        for key, item, signature in changed:
            if not ctx.checkpoint():
                break
            try:
                reply = bridge.request({"cmd": "index_file", "path": item["source_path"],
                                        "name": item.get("display_name") or Path(item["source_path"]).name,
                                        "box_id": item.get("box_id")})
                if isinstance(reply, dict) and reply.get("ok") is not False:
                    current[key] = signature
                    result["indexed"] += 1
                else:
                    result["errors"] += 1
            except LibraryBridgeError:
                logging.getLogger(__name__).debug("자료 변경분 색인 실패", exc_info=True)
                result["errors"] += 1
            done += 1
            ctx.progress(done, len(changed))
        # 멈췄거나 실패한 파일은 기준을 옮기지 않아 다음 쉬는 시간에 다시 한다.
        for key, item, signature in changed:
            if key not in current and previous is not None and key in previous:
                current[key] = previous[key]
        self._save_state({key: value for key, value in current.items() if value is not None})
        self.last_result = result
