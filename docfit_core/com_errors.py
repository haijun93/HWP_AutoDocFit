"""사용자에게 보여 줄 한/글 COM 오류 안내와 호출 문맥."""

RPC_E_SERVERFAULT = -2147417851


def hresult_of(error) -> int | None:
    """pywin32 예외의 HRESULT를 정수로 반환한다."""
    value = getattr(error, "hresult", None)
    if value is None:
        value = getattr(error, "args", (None,))[0]
    try:
        value = int(value)
    except (TypeError, ValueError):
        return None
    return value - 0x100000000 if value > 0x7FFFFFFF else value


def user_message(error, *, stage=None, call=None) -> str:
    """COM 예외를 원인 후보와 조치가 포함된 짧은 한국어 안내로 바꾼다."""
    code = hresult_of(error)
    if code == RPC_E_SERVERFAULT:
        message = (
            "한/글이 문서를 처리하는 중 내부 오류를 냈어요. 문서 안의 표·그림·도형 같은 개체나 "
            "한/글의 열린 창·대화상자 상태가 원인일 수 있습니다. 한/글을 모두 닫고 다시 시도해 주세요. "
            "계속 실패하면 문서 사본으로 재현 여부를 확인해 주세요."
        )
        message += f" (COM 오류 0x80010105 / {code})"
    else:
        return str(error)
    if stage:
        message += f" 단계: {stage}."
    if call:
        message += f" COM 호출: {call}."
    return message
