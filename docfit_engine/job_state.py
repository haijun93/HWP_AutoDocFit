"""파일별 작업 상태(JOBSTATE1)."""
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path


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


def file_key(path):
    """같은 파일을 가리키는 경로 표기(대소문자·상대 경로)를 하나로 본다."""
    return os.path.normcase(os.path.abspath(str(path)))


def options_fingerprint(values):
    """결과에 영향을 주는 실행 인자가 같은지 비교하는 지문."""
    raw = json.dumps(values, ensure_ascii=False, sort_keys=True, default=repr)
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def resumed_records(files, completed):
    """이어하기에서 건너뛸 파일의 기록. 결과 파일이 남아 있는 항목만 인정한다."""
    by_key = {file_key(item["source"]): item for item in completed or ()}
    records = {}
    for source in files:
        item = by_key.get(file_key(source))
        if not item or not Path(item["output"]).is_file():
            continue
        status = item.get("status") or DocumentStatus.SAVED
        records[file_key(source)] = {
            "source": str(source), "output": str(item["output"]), "status": status,
            "success": status in (DocumentStatus.SAVED, DocumentStatus.INTEGRITY_FAILED),
            "integrity_ok": False if status == DocumentStatus.INTEGRITY_FAILED else None,
            "resumed_result": True,
        }
    return records
