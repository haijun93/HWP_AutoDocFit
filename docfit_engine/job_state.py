"""파일별 작업 상태(JOBSTATE1)."""


class DocumentStatus:
    SAVED = "saved"                          # 결과 저장(검수 실패·무결성 문제 없음)
    INSPECTION_FAILED = "inspection_failed"  # 저장됐지만 저장 후 검수를 마치지 못함
    INTEGRITY_FAILED = "integrity_failed"    # 저장됐지만 무결성 확인 필요
    CANCELLED = "cancelled"                  # 저장 후 검수 중 사용자 중단
    FAILED = "failed"                        # 저장하지 못함
    UNPROCESSED = "unprocessed"              # 시작하지 못함
