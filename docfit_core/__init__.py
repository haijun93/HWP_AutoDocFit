"""Safe document inspection helpers used by HWP AutoDocFit."""

from .hwpx import (
    HwpxLimits,
    HwpxSecurityError,
    compare_documents,
    export_markdown,
    inspect_hwpx,
    validate_and_inspect_hwpx,
    validate_hwpx,
    validate_hwpx_structure,
    open_hwpx_safely,
)
# kordoc 의존 줄이기 1단계(2026-11-15): 앱이 쓰지 않던 고급 기능 9개(양식 분석·채우기, 패치, 검사, 미리보기,
# 고급 비교·마크다운·IR·RAG 내보내기, 표 분석)는 패키지 공개 목록에서 뺐다. 필요하면 kordoc_bridge에서 직접 불러온다.
from .kordoc_bridge import (
    KordocUnavailableError,
    generate_hwpx,
    parse_document,
)

__all__ = [
    "HwpxLimits",
    "HwpxSecurityError",
    "compare_documents",
    "export_markdown",
    "inspect_hwpx",
    "validate_and_inspect_hwpx",
    "validate_hwpx",
    "validate_hwpx_structure",
    "open_hwpx_safely",
    "KordocUnavailableError",
    "generate_hwpx",
    "parse_document",
]
