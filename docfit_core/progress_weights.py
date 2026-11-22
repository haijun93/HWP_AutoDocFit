"""진행률 가중치·남은 시간 추정(순수 함수, 한/글·화면 의존 없음).

진행 막대가 단계 수로 균등 분할되면 실제로는 시간의 절반을 쓰는 자간 단계에서 멈춘 것처럼 보인다.
단계마다 실제 걸리는 시간 비중(기준선)을 가중치로 쓰고, 지금 단계 안의 진행도 반영한다.
기준선: docs/benchmarks/COM_기준선_v2.2.58_2026-11-12.md (작업 방식 all, 문서 1개 322.8초).
"""
import math

# 안내 키 → 기준선 시간 비중(합계 약 100). 자간 계열 약 44+4, 줄 병합 6, 내어쓰기 계열 24, 페이지 배치 12+1.
STEP_WEIGHTS = {
    "open": 3.0, "pre_format": 3.0, "space": 1.0, "unify_survey": 1.0, "unify_apply": 6.0,
    "standard": 7.0, "body_spacing": 40.0, "control_spacing": 4.0, "short_line": 6.0, "indent": 24.0,
    "page_fit": 1.0, "page_group": 12.0, "table_unify": 3.0, "unify_recheck": 3.0, "save": 2.0,
    "unify_verify": 2.0, "done": 0.0,
}
DEFAULT_WEIGHT = 2.0
# 문서 크기와 무관하게 거의 일정한 시간이 드는 단계(나머지는 문서 크기에 비례해 늘어난다).
FIXED_STEPS = frozenset({"open", "save", "unify_survey", "done"})
SCALE_RANGE = (0.25, 6.0)

MIN_ETA_SECONDS = 5.0       # 이 시간이 지나기 전에는 '계산 중'
MIN_ETA_RATIO = 0.02        # 진행률이 이보다 낮으면 '계산 중'
RANGE_SPREAD = 0.35         # 최근 추정의 퍼짐이 이 비율을 넘으면 범위로 표시
HISTORY_WINDOW = 90.0       # 추정 흔들림을 보는 최근 구간(초)
DEFAULT_TOTAL_SECONDS = 180.0
MAX_STAGE_FRACTION = 0.95   # 시간 기반 점근 증가의 상한(실제 끝나기 전에는 100%가 되지 않는다)


def step_weights(keys, scale=None):
    """단계 키 목록 → {키: 가중치}. scale(문서 크기 보정, 1.0=기준 문서)은 고정 단계를 뺀 단계에 곱한다."""
    factor = 1.0 if scale is None else max(SCALE_RANGE[0], min(SCALE_RANGE[1], float(scale)))
    return {key: STEP_WEIGHTS.get(key, DEFAULT_WEIGHT) * (1.0 if key in FIXED_STEPS else factor)
            for key in keys}


def stage_fraction(elapsed, expected, reported=None):
    """지금 단계 안의 진행(0~1 미만). 엔진이 (처리 수, 전체 수)를 보고하면 그것을, 없으면 시간 기반 점근 증가."""
    if reported:
        try:
            done, total = float(reported[0]), float(reported[1])
        except (TypeError, ValueError, IndexError):
            done = total = 0.0
        if total > 0:
            return max(0.0, min(MAX_STAGE_FRACTION, done / total))
    if elapsed is None or elapsed <= 0:
        return 0.0
    tau = max(float(expected or 0.0), 1.0)
    return min(MAX_STAGE_FRACTION, 1.0 - math.exp(-float(elapsed) / tau))


def weighted_ratio(keys, done_keys, current_key=None, *, stage_elapsed=None, reported=None,
                   total_seconds=None, scale=None):
    """한 문서의 가중 진행률 = (끝난 단계 가중치 + 지금 단계 가중치 × 단계 안 진행) ÷ 전체 가중치."""
    weights = step_weights(keys, scale)
    total_weight = sum(weights.values())
    if total_weight <= 0:
        return 0.0
    done = set(done_keys or ())
    finished = sum(w for key, w in weights.items() if key in done and key != current_key)
    inner = 0.0
    if current_key in weights:
        expected = weights[current_key] / total_weight * (total_seconds or DEFAULT_TOTAL_SECONDS)
        inner = weights[current_key] * stage_fraction(stage_elapsed, expected, reported)
    return max(0.0, min(1.0, (finished + inner) / total_weight))


def _remaining(elapsed, ratio):
    return elapsed * (1.0 - ratio) / ratio


def estimate_remaining(elapsed, ratio, history=()):
    """남은 시간 추정. history: 이전 표본 [(경과 초, 진행률)].

    반환: {"state": "calculating"|"estimate"|"range", "seconds": 중앙값, "low": …, "high": …}.
    처음 몇 초(또는 진행률이 아주 낮을 때)는 '계산 중', 최근 추정이 크게 흔들리면 범위.
    """
    if elapsed is None or ratio is None or elapsed < MIN_ETA_SECONDS or ratio < MIN_ETA_RATIO:
        return {"state": "calculating", "seconds": None, "low": None, "high": None}
    if ratio >= 1.0:
        return {"state": "estimate", "seconds": 0, "low": 0, "high": 0}
    values = [_remaining(elapsed, ratio)]
    for past_elapsed, past_ratio in history or ():
        if (past_ratio >= MIN_ETA_RATIO and past_elapsed >= MIN_ETA_SECONDS
                and 0 <= elapsed - past_elapsed <= HISTORY_WINDOW and past_ratio < 1.0):
            # 그때의 추정에서 그동안 흐른 시간을 뺀 값 = 지금 기준으로 환산한 남은 시간
            values.append(max(0.0, _remaining(past_elapsed, past_ratio) - (elapsed - past_elapsed)))
    low, high = min(values), max(values)
    mid = values[0]
    if len(values) >= 3 and mid > 0 and (high - low) / mid > RANGE_SPREAD:
        return {"state": "range", "seconds": int(round(mid)), "low": int(round(low)), "high": int(round(high))}
    return {"state": "estimate", "seconds": int(round(mid)), "low": int(round(mid)), "high": int(round(mid))}
