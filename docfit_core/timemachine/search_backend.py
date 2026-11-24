"""몽땅검색 색인 backend 계약과 기존 SQLite 구현 어댑터."""
from __future__ import annotations

from abc import ABC, abstractmethod
from threading import Event
from typing import Any


class SearchBackend(ABC):
    """문서 색인·검색 backend의 service 경계."""

    @abstractmethod
    def index_file(self, path: str) -> dict[str, Any]:
        """파일을 색인하고 기존 색인 응답 사전을 돌려준다."""

    @abstractmethod
    def remove_file(self, path: str) -> None:
        """파일의 색인 데이터를 제거한다."""

    @abstractmethod
    def search(self, query: str, *, roots=None, hierarchy: bool = False,
               limit: int = 100, offset: int = 0,
               sort: str = "hierarchy_relevance", start=None, end=None) -> list[dict[str, Any]]:
        """기존 본문검색 또는 위계검색 결과 형식으로 검색한다."""

    @abstractmethod
    def status(self, *, roots=None) -> dict[str, Any]:
        """색인 상태 요약을 돌려준다."""

    @abstractmethod
    def cancel(self, cancel_event: Event) -> bool:
        """진행 중 작업의 취소 이벤트를 설정하고 취소 요청 여부를 돌려준다."""


class SqliteBackend(SearchBackend):
    """ContentSearchIndex를 감싸는 하위 호환 어댑터."""

    def __init__(self, index):
        self.index = index

    def index_file(self, path: str) -> dict[str, Any]:
        return self.index.index_file(path)

    def remove_file(self, path: str) -> None:
        return self.index.remove_file(path)

    def search(self, query: str, *, roots=None, hierarchy: bool = False,
               limit: int = 100, offset: int = 0,
               sort: str = "hierarchy_relevance", start=None, end=None) -> list[dict[str, Any]]:
        if hierarchy:
            return self.index.search(query, roots=roots, limit=limit, sort=sort, start=start, end=end)
        return self.index.keyword_search(query, roots=roots, limit=limit, offset=offset,
                                         start=start, end=end)

    def status(self, *, roots=None) -> dict[str, Any]:
        return self.index.status_summary(roots=roots)

    def cancel(self, cancel_event: Event) -> bool:
        was_cancelled = cancel_event.is_set()
        cancel_event.set()
        return not was_cancelled
