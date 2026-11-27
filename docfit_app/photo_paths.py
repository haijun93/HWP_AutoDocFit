"""사진 DB의 기존 경로를 보존하면서 Windows 별칭을 조회한다. 스키마/행 이동 없음."""
from __future__ import annotations

import os


def photo_key(path: str) -> str:
    if os.name != "nt":
        return os.path.abspath(path)
    if path.startswith("\\\\?\\UNC\\"):
        path = "\\\\" + path[8:]
    elif path.startswith("\\\\?\\"):
        path = path[4:]
    return os.path.normcase(os.path.realpath(path))


class PhotoPathLookup:
    """정확한 열쇠를 먼저 조회하고, 없으면 한 묶음 안에서 별칭 색인을 한 번 만든다."""

    def __init__(self, connection):
        self.connection = connection
        self._aliases: dict[str, str] | None = None

    def find(self, path: str) -> str:
        if self.connection.execute("SELECT 1 FROM photos WHERE path=?", (path,)).fetchone():
            return path
        if self._aliases is None:
            self._aliases = {}
            for (stored,) in self.connection.execute("SELECT path FROM photos ORDER BY path"):
                self._aliases.setdefault(photo_key(stored), stored)
        return self._aliases.setdefault(photo_key(path), path)
