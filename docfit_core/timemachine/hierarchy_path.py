"""문단 위계 수준으로 문서 나무를 세우고, 검색 결과에 상위 경로를 붙인다.

본문 색인은 문단마다 위계 순위(0=문서 제목 … 8=본문·표 칸)를 저장한다. 이 모듈은 그 값만으로
"이 문단은 어느 장·절·항 아래에 있는가"를 계산한다. 원본 문서를 다시 열지 않는다.

규칙: 문단의 부모는 앞쪽에서 가장 가까운, 자기보다 위계가 높은(순위 숫자가 작은) 문단이다.
같은 순위가 이어지면 형제다. 본문·표 칸(가장 낮은 순위)은 부모가 될 수 없다.
"""
from __future__ import annotations

from typing import Iterable, Sequence

# content_search.LEVEL_ORDER의 "body" 순위와 같다(순환 import를 피하려고 값으로 둔다).
BODY_RANK = 8
MAX_HEADING_CHARS = 60


def parents(level_ranks: Sequence[int]) -> list[int | None]:
    """문단 순서대로 준 위계 순위에서 각 문단의 부모 위치(없으면 None)를 구한다."""
    result: list[int | None] = []
    stack: list[int] = []  # 위계가 점점 낮아지는(순위가 커지는) 조상 위치
    for position, rank in enumerate(level_ranks):
        rank = BODY_RANK if rank is None else int(rank)
        while stack and int(level_ranks[stack[-1]]) >= rank:
            stack.pop()
        result.append(stack[-1] if stack else None)
        if rank < BODY_RANK:
            stack.append(position)
    return result


def ancestors(level_ranks: Sequence[int], position: int) -> list[int]:
    """position 문단의 조상 위치를 위에서 아래 순서로 돌려준다(자기 자신은 뺀다)."""
    if position < 0 or position >= len(level_ranks):
        return []
    target = BODY_RANK if level_ranks[position] is None else int(level_ranks[position])
    chain: list[int] = []
    need = target
    for index in range(position - 1, -1, -1):
        rank = level_ranks[index]
        rank = BODY_RANK if rank is None else int(rank)
        if rank < need:
            chain.append(index)
            need = rank
            if rank == 0:
                break
    chain.reverse()
    return chain


def shorten(text: str, limit: int = MAX_HEADING_CHARS) -> str:
    """제목 한 줄을 화면용으로 줄인다(줄바꿈·연속 공백 정리)."""
    cleaned = " ".join(str(text or "").split())
    # Markdown 제목 표시(#, ##…)는 경로에 보일 필요가 없다.
    stripped = cleaned.lstrip("#")
    if stripped != cleaned and stripped.startswith(" "):
        cleaned = stripped.strip()
    return cleaned if len(cleaned) <= limit else cleaned[: limit - 1].rstrip() + "…"


def breadcrumb(rows: Sequence[dict], paragraph_index: int) -> list[dict]:
    """한 문서의 문단 행에서 paragraph_index 문단의 상위 경로를 만든다.

    rows: ``paragraph_index``·``level_rank``·``body``(선택 ``level``)를 가진 행. 순서는 상관없다.
    돌려주는 값: 위에서 아래 순서의 ``{"paragraph_index", "level", "level_rank", "text"}`` 목록.
    """
    ordered = sorted(rows, key=lambda row: int(row["paragraph_index"]))
    position = next((i for i, row in enumerate(ordered)
                     if int(row["paragraph_index"]) == int(paragraph_index)), None)
    if position is None:
        return []
    ranks = [row["level_rank"] for row in ordered]
    path = []
    for index in ancestors(ranks, position):
        row = ordered[index]
        text = shorten(row["body"])
        if not text:
            continue
        path.append({
            "paragraph_index": int(row["paragraph_index"]),
            "level": row["level"] if "level" in row.keys() else None,
            "level_rank": int(row["level_rank"]) if row["level_rank"] is not None else BODY_RANK,
            "text": text,
        })
    return path


def breadcrumb_text(path: Iterable[dict], separator: str = " > ") -> str:
    return separator.join(item["text"] for item in path if item.get("text"))


def context_term_hits(path: Iterable[dict], terms: Iterable[str]) -> int:
    """상위 경로(조상 제목)에 들어 있는 검색어 수. 상위 문맥이 질의와 맞는 정도의 근거로 쓴다."""
    joined = " ".join(item.get("text", "") for item in path).casefold()
    return sum(1 for term in {str(t).casefold() for t in terms if t} if term in joined)
