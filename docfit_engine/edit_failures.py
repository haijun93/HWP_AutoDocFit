"""필수 COM 수정 실패는 호출부가 예외를 잡아도 문서 저장을 차단한다."""
from docfit_engine.context import current_context


class RequiredEditError(RuntimeError):
    pass


def fail_required_edit(message, cause=None):
    context = current_context()
    if context is not None and not context.state.get('_필수수정_실패'):
        context.state['_필수수정_실패'] = str(message)
    raise RequiredEditError(message) from cause


def check_required_edits():
    context = current_context()
    if context is not None and context.state.get('_필수수정_실패'):
        raise RequiredEditError(context.state['_필수수정_실패'])
