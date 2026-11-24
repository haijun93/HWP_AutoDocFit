"""Track only HWP processes created by this application.

소유 상태(PID 하나당):
  owned(앱 소유, 강제 정리 대상) --hand_over--> 목록에서 빠짐(사용자 창, 앱이 다시 죽이지 않음)
  owned --kill_owned/죽은 PID 발견--> 목록에서 빠짐
작업 한/글 창은 만들 때 bind_owner로 그 COM 객체에 PID를 묶어 두고,
자동닫기 끔으로 창을 사용자에게 넘길 때(작업창_분리) hand_over로 소유를 푼다.
"""
import csv
import io
import os
from threading import Lock, local

_lock = Lock()
_owned = set()
_bound = {}            # id(작업 COM 객체) -> 그 객체를 만들 때 등록된 PID들
_thread = local()      # 이 스레드에서 마지막으로 등록한 PID들(take_registered가 가져감)


def restart_between_documents_enabled(
    document_count: int,
    *,
    env_value: str | None = None,
    auto_close: bool = True,
    compare_view: bool = False,
) -> bool:
    """문서 간 한/글 재시작 정책(PM 결정 2026-11-24).

    여러 문서를 한 한/글로 이어 처리하면 앞 문서의 상태가 뒤 문서 산출물에 남아
    결과가 달라지므로(SEQDIFF1) 기본은 재시작한다. 다만 자동 닫기를 끈 경우(창이
    문서 수만큼 쌓임)와 비교 보기(좌우 창 배치 미검증)에서는 기본을 끄고,
    환경변수 DOCFIT_RESTART_BETWEEN_DOCS=1 로 강제, 0 으로 해제할 수 있다.
    """
    if document_count < 2:
        return False
    value = os.environ.get("DOCFIT_RESTART_BETWEEN_DOCS") if env_value is None else env_value
    if value is not None:
        return str(value).strip().casefold() in {"1", "true", "yes", "on"}
    return bool(auto_close) and not compare_view


def _run(args):
    import locale
    import subprocess
    result = subprocess.run(args, capture_output=True, text=False, check=False,
                            creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0),
                            timeout=3)
    encoding = locale.getpreferredencoding(False)
    for stream in ('stdout', 'stderr'):
        value = getattr(result, stream)
        if isinstance(value, bytes):
            try:
                decoded = value.decode(encoding)
            except (LookupError, UnicodeDecodeError):
                try:
                    decoded = value.decode('cp949')
                except (LookupError, UnicodeDecodeError):
                    decoded = value.decode(encoding if encoding else 'utf-8', errors='replace')
            setattr(result, stream, decoded)
    return result


def hwp_pids(*, runner=None) -> set[int]:
    result = (runner or _run)(['tasklist', '/FI', 'IMAGENAME eq Hwp.exe', '/FO', 'CSV', '/NH'])
    if result.returncode:
        raise OSError('tasklist failed')
    pids = set()
    for row in csv.reader(io.StringIO(result.stdout)):
        if len(row) >= 2 and row[0].casefold() == 'hwp.exe':
            try:
                pids.add(int(row[1]))
            except ValueError:
                pass
    return pids


def register_owned(pids):
    added = {pid for pid in pids if isinstance(pid, int) and pid > 0}
    with _lock:
        _owned.update(added)
    _thread.last = added


def take_registered():
    """이 스레드가 마지막으로 register_owned한 PID들을 꺼낸다(한 번만)."""
    pids = getattr(_thread, 'last', set())
    _thread.last = set()
    return pids


def bind_owner(app, pids):
    """작업 COM 객체와 그 한/글 PID를 묶는다. 객체가 살아 있는 동안만 유효하다."""
    if app is None:
        return
    with _lock:
        _bound[id(app)] = set(pids)


def unbind_owner(app):
    """묶음만 푼다. PID는 계속 앱 소유(작업창_닫기: Quit 실패로 남은 창은 정리 대상)."""
    with _lock:
        return _bound.pop(id(app), set()) if app is not None else set()


def hand_over(app):
    """창을 사용자에게 넘긴다: 그 객체의 PID를 소유 목록에서 빼 강제 정리 대상에서 제외한다."""
    with _lock:
        pids = _bound.pop(id(app), set()) if app is not None else set()
        _owned.difference_update(pids)
    return pids


def owned_pids():
    with _lock:
        return set(_owned)


def forget(pid):
    with _lock:
        _owned.discard(pid)


def kill_owned(log=None, *, runner=None) -> list[int]:
    runner = runner or _run
    killed = []
    # Serialize cleanup so concurrent callers cannot kill the same registration twice.
    with _lock:
        for pid in sorted(_owned.copy()):
            try:
                if pid not in hwp_pids(runner=runner):
                    _owned.discard(pid)
                    continue
                result = runner(['taskkill', '/PID', str(pid), '/T', '/F'])
                if result.returncode == 0:
                    killed.append(pid)
                    _owned.discard(pid)
            except Exception:
                continue
    if log and killed:
        log(f'앱이 띄운 한/글 강제 종료: {killed}')
    return killed
