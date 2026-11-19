# BENCH2A 구현 보고서

## 변경 파일

- `qms_bench/Cargo.toml`: serde·serde_json만 사용하는 Rust 2021 크레이트 및 바이너리 선언
- `qms_bench/Cargo.lock`: 잠금 파일
- `qms_bench/query-set.schema.json`: 버전 1 질의 세트 JSON Schema
- `qms_bench/src/lib.rs`: 질의 세트 로더/검증, 고정 시드 순수 표본 추출, SHA-256 지문, 어댑터/가짜 어댑터, 주입형 시계·취소 계측 루프, 통계·이상치, 리소스 샘플 요약, 결과 검증·임시 파일 저장·CSV
- `qms_bench/src/main.rs`: JSON 출력 `sample-queries`, `run`, `detect` CLI 골격 및 인자 처리
- `qms_bench/src/tests.rs`: 30개 단위 시험

## 시험

지정된 홈 디렉터리 아래 Cargo 타깃 경로는 이 실행 환경에서 쓰기 거부되어 `/private/tmp/cargo-target-BENCH2A`를 사용했습니다. crates.io DNS가 차단된 상태에서 로컬 의존성 캐시로 `--offline` 실행했습니다.

```text
running 30 tests

 test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

 test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

 test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 남은 격차

- Windows API 기반 프로세스 어댑터·카운터 리더와 Everything/kfind 연결은 BENCH2B 및 후속 카드 범위로 남았습니다.
- `run`과 `detect`는 유효한 JSON 응답을 내는 CLI 골격입니다. 실제 엔진 연결, 실행 상태 이벤트, 결과 경로 인자 연결은 아직 없습니다.
- 측정 루프는 cold 반복 수를 별도 인자로 받아 warm과 구분합니다. 호출자가 실제 캐시 조건을 보장해야 하며 자동 cold 보장은 하지 않습니다.
- 저장 함수는 스키마 필수 필드와 버전을 검사하고 임시 파일에서 최종 경로로 교체합니다. 결과 객체의 상세 §6 필드 전체를 구성하는 빌더 및 Windows에서 기존 파일 교체 동작은 아직 실기기 검증 전입니다.
- 표본 추출은 입력 파일명에서 확장자를 제외한 파일 stem을 사용합니다. 의미 동등성 판정과 한국어/영문/필터 질의 세트의 고정 기본 자료는 추가 구현이 필요합니다.
