# KIDX7C — unify_marker 잔여 5건 해결

- 기준: `feature/klib-index-rust`, 시작 커밋 `a95fdd80d2040747284d94261d8a43bbb8fe9952`
- 회귀 비교: 502개 문단, 수정 전 5건 불일치 → 수정 후 0건 불일치

## 발견한 차이

남은 다섯 입력은 PUA 원문자, 추가 점, tight-number 보충 경로에서 발생하지 않았다. `leading_marker`가 이미 `2.` 또는 `3.`을 인식한 경우 `marker_class`가 이를 공통 `1.` 그룹으로 변환해야 하는데, Rust가 원래 표식을 그룹으로 남기고 있었다.

사례 인덱스는 10, 42, 43, 123, 133이다. `--dump` 옵션은 각 입력과 Python/Rust 결과를 표시한다. 회귀 도구는 debug 바이너리를 우선 선택한다.

## 변경

- `klib/src/unify_marker.rs`: Python `_CLASS_PATTERNS`의 `\d{1,2}[.．]?`와 일치하도록 숫자 뒤 선택적 마침표를 그룹화한다.
- `tools/kidx_unify_regress.py`: `--dump` 전체 사례 표시, debug 우선 바이너리 선택.
- `hierarchy::compat::leading_marker` 및 `canonical_marker`는 수정하지 않았다.

## 검증

- `cargo build --manifest-path klib/Cargo.toml`: 통과 (debug)
- `cargo test --manifest-path klib/Cargo.toml`: 통과, 49 unit + 71 integration
- `tools/kidx_unify_regress.py --dump`: 502 paragraphs, 0 differences
- `tools/kidx_unify_regress.py`: 502 paragraphs, 0 differences

빌드에서 기존 경고가 출력되었으나 실패는 없었다.

## 커밋 및 푸시

- 코드 커밋: `d0a291ca` — `Fix unify marker numeric class parity`
- 코드 변경 GitHub `feature/klib-index-rust` 푸시 완료. 원격에서 저장소 이전 안내가 출력됐으나 push는 성공했다.
- 이 보고서는 저장소에서 `reports/`가 ignore되어 작업 경로에만 기록되었다.
