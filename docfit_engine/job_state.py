"""파일별 작업 상태(JOBSTATE1)."""
from dataclasses import dataclass


class DocumentStatus:
    SAVED = "saved"                          # 결과 저장(검수 실패·무결성 문제 없음)
    INSPECTION_FAILED = "inspection_failed"  # 저장됐지만 저장 후 검수를 마치지 못함
    INTEGRITY_FAILED = "integrity_failed"    # 저장됐지만 무결성 확인 필요
    CANCELLED = "cancelled"                  # 저장 후 검수 중 사용자 중단
    FAILED = "failed"                        # 저장하지 못함
    UNPROCESSED = "unprocessed"              # 시작하지 못함


@dataclass(frozen=True)
class DocumentCancelled:
    """사용자 중단: 남은 문서를 처리하지 않는다."""


@dataclass(frozen=True)
class DocumentFailed:
    """중단이 아닌 실패: 이 파일만 실패로 남기고 다음 문서를 처리한다."""
    error: str


def classify_result(result, cancel_requested):
    """문서_처리의 반환값을 구분한다. 처리를 마쳤으면 None."""
    if result is not False:
        return None
    if cancel_requested:
        return DocumentCancelled()
    return DocumentFailed("처리 단계가 실패를 반환했습니다. 작업 로그를 확인해 주세요.")


def close_failed_document(hwp):
    """실패한 문서를 저장하지 않고 닫는다(hwpDiscard=1). 다음 문서에 섞이지 않게 한다."""
    hwp.Clear(1)
