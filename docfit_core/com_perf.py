"""한/글 COM 호출 계측(개발용, 2026-11-11).

문서 처리 단계마다 COM 호출이 몇 번 일어났고 그 안에서 시간이 얼마나 걸렸는지 센다.
"어느 단계가 한/글을 기다리느라 느리고, 어느 단계가 우리 코드 때문에 느린가"를
같은 문서로 비교하려는 것이다.

환경 변수 ``DOCFIT_COM_PERF=1``일 때만 켜진다. 꺼져 있으면 ``wrap_if_enabled``가
받은 객체를 그대로 돌려주므로 평소 동작과 속도에 영향이 없다.

켜져 있으면 한/글 COM 객체를 얇은 대리 객체로 감싼다. 대리 객체는 속성 읽기·쓰기와
메서드 호출을 원래 객체로 그대로 넘기면서 횟수와 시간을 적는다. COM 객체가 결과로
나오면 그것도 감싸고, 감싼 객체가 다른 호출의 인자로 들어가면 원래 객체로 풀어 넘긴다.

이 모듈은 순수 파이썬이며 win32com을 불러오지 않는다(COM 객체 판별은 ``_oleobj_`` 속성).
"""
from __future__ import annotations

import json
import os
import time
from pathlib import Path

ENV_ENABLE = "DOCFIT_COM_PERF"
ENV_OUTPUT_DIR = "DOCFIT_COM_PERF_DIR"
_NO_STAGE = "준비"


def enabled() -> bool:
    return os.environ.get(ENV_ENABLE, "").strip().lower() in ("1", "true", "yes", "on")


class ComCounter:
    """단계별·호출 이름별 COM 호출 횟수와 시간 누적."""

    def __init__(self, clock=time.perf_counter):
        self._clock = clock
        self.reset()

    def reset(self):
        self.stage = _NO_STAGE
        # 단계 이름 -> [호출 수, COM 안에서 보낸 초]
        self.by_stage: dict[str, list] = {}
        # 호출 이름(예: "HAction.Run") -> [호출 수, 초]
        self.by_call: dict[str, list] = {}
        self._depth = 0

    def set_stage(self, name):
        self.stage = str(name) if name else _NO_STAGE

    def record(self, call_name: str, seconds: float):
        stage = self.by_stage.get(self.stage)
        if stage is None:
            stage = self.by_stage[self.stage] = [0, 0.0]
        stage[0] += 1
        stage[1] += seconds
        call = self.by_call.get(call_name)
        if call is None:
            call = self.by_call[call_name] = [0, 0.0]
        call[0] += 1
        call[1] += seconds

    def timed(self, call_name: str, function, *args, **kwargs):
        """function을 실행하고 걸린 시간을 적는다. 겹친 호출은 바깥 것만 시간에 넣는다."""
        outermost = self._depth == 0
        self._depth += 1
        started = self._clock()
        try:
            return function(*args, **kwargs)
        finally:
            elapsed = self._clock() - started
            self._depth -= 1
            # 안쪽 호출 시간은 바깥 호출 시간에 이미 들어 있으므로 횟수만 센다.
            self.record(call_name, elapsed if outermost else 0.0)

    @property
    def total_calls(self) -> int:
        return sum(item[0] for item in self.by_stage.values())

    @property
    def total_seconds(self) -> float:
        return sum(item[1] for item in self.by_stage.values())


def _is_com_object(value) -> bool:
    return hasattr(value, "_oleobj_")


def unwrap(value):
    """대리 객체면 원래 COM 객체를, 아니면 받은 값을 그대로 돌려준다."""
    if isinstance(value, ComProxy):
        return object.__getattribute__(value, "_com_target")
    if isinstance(value, tuple):
        return tuple(unwrap(item) for item in value)
    if isinstance(value, list):
        return [unwrap(item) for item in value]
    return value


class ComProxy:
    """COM 객체 대리. 모든 접근을 원래 객체로 넘기며 횟수·시간을 적는다."""

    __slots__ = ("_com_target", "_com_counter", "_com_path")

    def __init__(self, target, counter: ComCounter, path: str = ""):
        object.__setattr__(self, "_com_target", target)
        object.__setattr__(self, "_com_counter", counter)
        object.__setattr__(self, "_com_path", path)

    def _wrap(self, value, name: str):
        if isinstance(value, ComProxy):
            return value
        if _is_com_object(value):
            return ComProxy(value, object.__getattribute__(self, "_com_counter"), name)
        if isinstance(value, tuple):
            return tuple(self._wrap(item, name) for item in value)
        return value

    def _name(self, attribute: str) -> str:
        path = object.__getattribute__(self, "_com_path")
        return f"{path}.{attribute}" if path else attribute

    def __getattr__(self, attribute):
        target = object.__getattribute__(self, "_com_target")
        counter = object.__getattribute__(self, "_com_counter")
        if attribute.startswith("_"):
            return getattr(target, attribute)
        name = self._name(attribute)
        value = counter.timed(name, getattr, target, attribute)
        if _is_com_object(value):
            return ComProxy(value, counter, name)
        if callable(value):
            def call(*args, **kwargs):
                result = counter.timed(
                    name + "()", value,
                    *[unwrap(arg) for arg in args],
                    **{key: unwrap(item) for key, item in kwargs.items()})
                return self._wrap(result, name)
            return call
        return self._wrap(value, name)

    def __setattr__(self, attribute, value):
        target = object.__getattribute__(self, "_com_target")
        counter = object.__getattribute__(self, "_com_counter")
        counter.timed(self._name(attribute) + "=", setattr, target, attribute, unwrap(value))

    def __call__(self, *args, **kwargs):
        target = object.__getattribute__(self, "_com_target")
        counter = object.__getattribute__(self, "_com_counter")
        name = (object.__getattribute__(self, "_com_path") or "object") + "()"
        result = counter.timed(
            name, target, *[unwrap(arg) for arg in args],
            **{key: unwrap(item) for key, item in kwargs.items()})
        return self._wrap(result, name)

    def __getitem__(self, key):
        target = object.__getattribute__(self, "_com_target")
        counter = object.__getattribute__(self, "_com_counter")
        name = (object.__getattribute__(self, "_com_path") or "object") + "[]"
        return self._wrap(counter.timed(name, lambda: target[key]), name)

    def __len__(self):
        target = object.__getattribute__(self, "_com_target")
        counter = object.__getattribute__(self, "_com_counter")
        name = (object.__getattribute__(self, "_com_path") or "object") + ".len"
        return counter.timed(name, len, target)

    def __iter__(self):
        target = object.__getattribute__(self, "_com_target")
        name = (object.__getattribute__(self, "_com_path") or "object") + "[]"
        for item in target:
            yield self._wrap(item, name)

    def __bool__(self):
        return bool(object.__getattribute__(self, "_com_target"))

    def __eq__(self, other):
        return object.__getattribute__(self, "_com_target") == unwrap(other)

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        return hash(object.__getattribute__(self, "_com_target"))

    def __repr__(self):
        return f"<ComProxy {object.__getattribute__(self, '_com_path') or 'hwp'}>"


_counter = ComCounter()


def counter() -> ComCounter:
    return _counter


def wrap_if_enabled(target, path: str = ""):
    """계측이 켜져 있으면 대리 객체를, 꺼져 있으면 받은 객체를 그대로 돌려준다."""
    if not enabled() or target is None or isinstance(target, ComProxy):
        return target
    return ComProxy(target, _counter, path)


def start_document():
    """문서 하나의 계측을 새로 시작한다(꺼져 있으면 아무것도 하지 않는다)."""
    if enabled():
        _counter.reset()


def set_stage(name):
    if enabled():
        _counter.set_stage(name)


def build_report(file_name: str, stage_table: dict | None, top: int = 25) -> dict:
    """단계 시간표(단계시간_끝의 결과)와 COM 계측을 합쳐 보고서 사전을 만든다(순수 함수)."""
    total = float((stage_table or {}).get("전체", 0.0))
    stage_seconds = (stage_table or {}).get("합", {})
    order = list((stage_table or {}).get("순서", []))
    for name in _counter.by_stage:
        if name not in order:
            order.append(name)
    stages = []
    for name in order:
        seconds, visits = stage_seconds.get(name, (0.0, 0))
        calls, com_seconds = _counter.by_stage.get(name, (0, 0.0))
        stages.append({
            "name": name,
            "seconds": round(float(seconds), 4),
            "visits": int(visits),
            "com_calls": int(calls),
            "com_seconds": round(float(com_seconds), 4),
            "python_seconds": round(max(float(seconds) - float(com_seconds), 0.0), 4),
        })
    calls = sorted(_counter.by_call.items(), key=lambda item: -item[1][1])[:top]
    com_total = _counter.total_seconds
    return {
        "file": file_name,
        "total_seconds": round(total, 4),
        "com_calls": _counter.total_calls,
        "com_seconds": round(com_total, 4),
        "python_seconds": round(max(total - com_total, 0.0), 4),
        "com_share": round(com_total / total, 4) if total > 0 else None,
        "stages": stages,
        "top_calls": [
            {"name": name, "calls": int(count), "seconds": round(float(seconds), 4)}
            for name, (count, seconds) in calls
        ],
    }


def summary_lines(report: dict, top: int = 8) -> list[str]:
    """보고서를 작업 로그용 줄로 바꾼다(순수 함수)."""
    total = report.get("total_seconds") or 0.0
    share = report.get("com_share")
    lines = [
        "[COM 계측] {file}: 전체 {total:.1f}초, COM 호출 {calls}회·{com:.1f}초({share}), 우리 코드 {py:.1f}초".format(
            file=report.get("file", ""), total=total, calls=report.get("com_calls", 0),
            com=report.get("com_seconds", 0.0),
            share=f"{share:.0%}" if share is not None else "-",
            py=report.get("python_seconds", 0.0))
    ]
    slow = sorted(report.get("stages", []), key=lambda item: -item["seconds"])[:top]
    lines.append("[COM 계측] 단계별(상위 %d): " % len(slow) + " / ".join(
        f"{item['name']} {item['seconds']:.1f}초(COM {item['com_calls']}회 {item['com_seconds']:.1f}초)"
        for item in slow))
    calls = report.get("top_calls", [])[:top]
    lines.append("[COM 계측] 오래 걸린 호출(상위 %d): " % len(calls) + " / ".join(
        f"{item['name']} {item['calls']}회 {item['seconds']:.1f}초" for item in calls))
    return lines


def write_report(report: dict, source_path) -> Path | None:
    """``DOCFIT_COM_PERF_DIR``이 있으면 그 폴더에 ``<문서 이름>.comperf.json``으로 저장한다."""
    folder = os.environ.get(ENV_OUTPUT_DIR, "").strip()
    if not folder:
        return None
    directory = Path(folder)
    directory.mkdir(parents=True, exist_ok=True)
    target = directory / (Path(str(source_path)).stem + ".comperf.json")
    target.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    return target


def finish_document(file_name, stage_table, log=None, source_path=None) -> dict | None:
    """문서 하나가 끝났을 때 부른다. 꺼져 있으면 None."""
    if not enabled():
        return None
    report = build_report(str(file_name), stage_table)
    if log is not None:
        for line in summary_lines(report):
            log(line)
    try:
        write_report(report, source_path or file_name)
    except OSError as error:
        if log is not None:
            log(f"[COM 계측] 결과 파일을 저장하지 못했습니다: {error}")
    return report
