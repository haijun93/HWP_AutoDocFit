"""Python 문단 기록을 kfind ingest 문서로 변환한다."""
from __future__ import annotations

import os

from .content_search import CURRENT_HIERARCHY_SCHEMA_VERSION, _document_date


def doc_key(path=None, memo=None):
    """파일 경로 또는 memo:<box>:<item>/nbmemo:<nb>:<id> 식별자."""
    if (path is None) == (memo is None):
        raise ValueError("path 또는 memo 중 하나가 필요합니다")
    if memo is not None:
        parts = str(memo).split(":")
        if len(parts) != 3 or parts[0] not in {"memo", "nbmemo"} or not all(parts[1:]):
            raise ValueError("잘못된 메모 식별자")
        return str(memo)
    if not os.fspath(path):
        raise ValueError("빈 파일 경로")
    return os.path.normcase(os.path.abspath(path))


def doc_from_records(path, records, stat, ref_date, scopes):
    """문장을 나누지 않고 문단 번호·원문·위계를 그대로 전달한다."""
    return {
        "doc_key": doc_key(path=path),
        "path": os.fspath(path),
        "size": int(getattr(stat, "st_size", 0) or 0),
        "mtime": int((stat.st_mtime or 0) * 1000),
        "ref_date": ref_date,
        "date": int(_document_date(stat.st_ctime, stat.st_mtime) * 1000),
        "level_src": "py",
        "level_rule": CURRENT_HIERARCHY_SCHEMA_VERSION,
        "scopes": list(scopes),
        "paras": [{"p": record.paragraph_index, "lv": record.level_rank,
                   "t": record.text} for record in records],
    }
