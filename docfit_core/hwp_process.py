"""Track only HWP processes created by this application.

소유 상태(PID 하나당):
  owned(앱 소유, 강제 정리 대상) --hand_over--> 목록에서 빠짐(사용자 창, 앱이 다시 죽이지 않음)
  owned --kill_owned/죽은 PID 발견--> 목록에서 빠짐
작업 한/글 창은 만들 때 bind_owner로 그 COM 객체에 PID를 묶어 두고,
자동닫기 끔으로 창을 사용자에게 넘길 때(작업창_분리) hand_over로 소유를 푼다.
"""
import csv
import io
from threading import Lock, local

_lock = Lock()
_owned = set()
_bound = {}            # id(작업 COM 객체) -> 그 객체를 만들 때 등록된 PID들
_thread = local()      # 이 스레드에서 마지막으로 등록한 PID들(take_registered가 가져감)


def _run(args):
    import subprocess
    return subprocess.run(args, capture_output=True, text=True, check=False,
                          creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0),
                          timeout=3)


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
