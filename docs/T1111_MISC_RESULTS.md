# T1111-MISC 작업 결과 (2026-11-11)

## 1. kagent 상태 파일

- 원인: `main.rs::write_status`는 이미 임시 파일+rename을 사용했으나, 모든 호출이 `status.json.tmp`를 공유하면서 잠금이 없었다. 타임라인 작업자, 클립보드 저장 작업자의 `refresh_recent`, 주 창 메시지/시작 경로에서 동시에 호출 가능하다. 한 스레드가 rename한 파일의 열린 핸들에 다른 스레드가 계속 쓰는 경합이 가능했다.
- 조치: 공용 Mutex로 스냅샷 생성부터 임시 파일 저장·Windows 원자 교체까지 직렬화. 경로와 직전 성공 내용이 같으면 쓰지 않음. `updated`는 비교에서 제외하고 실제 교체 때만 갱신. 실패한 저장은 캐시에 반영하지 않고 임시 파일 정리. 타이머·폴링 추가 없음.
- 환경변수 시험: `util::TestEnvironment`의 공용 Mutex와 Drop 복원으로 KAGENT_TEST_ROOT/KAGENT_DATA 변경 시험 2개를 직렬화. 기존 검증은 유지.
- 시험: 기본 병렬 `cargo test --offline --manifest-path kagent/Cargo.toml` 통과. 라이브러리 16, kagent 109, xl_regress 7 = **132개 통과, 1개 기존 제외, 실패 0**. kagent 시험 31.94초. 새 시험은 8스레드×300회=2,400회 저장 중 별도 읽기 스레드로 JSON 파싱·재직렬화 일치 검사, 마지막 동일 내용 저장 생략 및 임시 파일 미잔존 확인.
- 커밋: `23c1a158`.

## 2. 위계 브리지 캐시

- 원인: 기본 설정 경로에서 문서마다 SettingsService.read, DLL 경로에서 문서마다 CDLL 및 ABI 함수 설정을 반복.
- 조치: 정규화한 DLL 절대 경로별 핸들을 잠금으로 한 번만 로딩·초기화하고 재사용. 실패한 로딩은 캐시하지 않음. 설정은 경로·st_mtime_ns·st_size가 같으면 직전 결과 재사용. 삭제·재생성도 반영하며, 읽기 오류는 기존 예외 처리대로 분석을 건너뜀.
- Python 시험 명령: `C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 -m unittest tests.test_hierarchy_bridge tests.test_document_review tests.test_hierarchy_page_fit_wire tests.test_timemachine_content_search`
- 결과: **69개 실행, 65개 통과, POSIX 전용 4개 제외, 실패 0**, 14.040초. 새 시험 3개: 8스레드 300회 DLL 호출의 단일 로딩/경로 분리/메모리 해제, 로딩 실패 재시도, 설정 300회 재사용 및 mtime만 변경/크기만 변경/삭제/재생성 즉시 반영.

## 시간 측정

- 실제 hierarchy DLL을 `cargo build --offline --release --lib --manifest-path hierarchy/Cargo.toml`로 준비. 기존 Python 브리지는 로컬 커밋 `23c1a158`에서 읽어 동일 프로세스·DLL로 비교. 사용자 설정 대신 작업 폴더 내 임시 설정 사용.
- 재현: `C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 scripts/benchmark_hierarchy_bridge.py`
- 동일 4문단(□ title / ㅇ body / - content / * explanation, indent 0/10/20/30, font_size 15/13/13/13)으로 각 300회, 5라운드, 전후 순서 교대. import/native warmup 이후 측정. 첫 변경 후 라운드는 캐시 미스 포함. 시간 측정에는 mock을 사용하지 않고 읽기/로딩 횟수는 따로 계수.

| 구분 | 변경 전(초) | 변경 후(초) |
|---|---:|---:|
| 1 | 0.494424 | 0.431272 |
| 2 | 0.456013 | 0.424216 |
| 3 | 0.471567 | 0.479351 |
| 4 | 0.498520 | 0.498447 |
| 5 | 0.494998 | 0.489000 |
| 중앙값 | **0.494424** | **0.479351** |

- 중앙값 약 **3.05% 감소**(300회당 약 15.1ms). 편차가 있으므로 처리량 전체의 큰 개선으로 확대 해석하지 않음.
- 별도 300회 계수: SettingsService.read **300→1**, ctypes.CDLL **300→1**. 측정 3,000개 결과 전체 일치(별도 계수 600개도 일치).

## 남은 문제와 범위

- 최초 측정 문단(1. 추진 배경 / 가. 세부 계획 / ㅇ 사업의 추진 현황을 다음과 같이 보고합니다. / - 세부 내용 및 향후 일정)에서 변경 전 브리지 자체의 반복 호출 결과가 달라져 동일성 검사가 실패했다. 역할·styles·variants·warnings·문서 유형 차이를 관찰했다. 위계 엔진 자체는 이번 범위에서 수정하지 않았고, 결과가 일정한 기호 문단 묶음으로 전후 측정했다. 해당 엔진 현상의 원인은 미확정이며 별도 조사 필요.
- 설정 mtime와 크기를 모두 의도적으로 보존한 변경은 요구된 캐시 키로 식별할 수 없다.
- 실제 실행 중인 kagent.exe 종료·설정 변경, 클립보드 쓰기·키 입력, 한/글 COM 시험은 수행하지 않음. Rust 시험 환경은 KAGENT_TEST_ROOT=E:/docfit-tmp/kstat-root, TEMP=TMP=E:/docfit-tmp. 실제 에이전트 격리 실행은 하지 않음.
- content_search.py 미수정. 기존 파일 줄바꿈 유지. 지정 브랜치 integ/todo-1111에서 항목별 로컬 커밋, 푸시·태그·fetch 없음.
