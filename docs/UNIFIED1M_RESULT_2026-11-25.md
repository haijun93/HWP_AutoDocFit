# UNIFIED1M — v2.2.101 기준 통합 검색 병합·재시험

`feature/UNIFIED1M`에서 v2.2.101 기준 `e550ad48`에 `feature/UNIFIED1`(`a56e7c60`)을 `git merge --no-ff`로 병합했다. 병합 커밋은 `158650bb`다. 설정 4조합·ktm 전체 276개·release 빌드·전체 Python 3,695개 검증을 통과했다(기존 Rust 1개·Python 22개 제외). 로컬 기능 브랜치에 커밋했으며 main 브랜치 이동·푸시·태그·배포·설치본 교체는 하지 않았다.

## 병합과 검색 의미

- 충돌 파일은 `ktm/src/window.rs` 하나다. SEMI2의 질의 칩·묶음별 색·연관 검색 근거·첫 입력 그리기 계측을 유지하면서 UNIFIED1의 단계 스트리밍·세대별 취소·단 안 정렬·단별 그리기 계측을 함께 보존했다.
- 자동 병합만으로는 통합 경로의 파일명이 일반 `names_query`로 가고, pipe 모드의 본문이 일반 검색 명령으로 가는 차이가 있었다. 두 설정을 켜면 main과 같은 **원문 질의를 `names_related`에 그대로 전달**하고, 위계·본문은 main과 같은 **연관 검색이 켜진 원문 문단 backend**를 사용하게 고쳤다. `file:` 등을 덧붙여 연관 검색의 리터럴 의미를 바꾸지 않는다.
- 원천 선택을 `unified::source`에 명시했다. 연관 검색이 켜지면 pipe 모드에서도 원문 문단 backend를 선택한다. 기존 공개 검색 API의 기본값·응답은 바꾸지 않았다.
- 상태는 Names → Hierarchy → Body → Complete다. 실패한 단도 완료 처리하고 오류를 보존하며, 취소된 세대는 발행하지 않는다. 통합을 끄면 main과 같은 Names 20·Hierarchy 20·Body 40 작업을 큐에 넣는다.
- `unified_search=false`, `related_queries=false` 기본값을 유지한다. 기존 첫 표시 측정은 19.768/15.844ms로 **16ms 목표 미달**이며, 이번에는 속도를 재측정하지 않았다.

## 설정 4조합 회귀 시험

새 시험 `ktm/tests/unified_related.rs`는 합성 문서 8개와 별도 원문 문단 SQLite·파일명 색인을 만들고 실제 Windows 격리 파이프로 명령·원문 질의·상한을 확인한다. 설치 서비스나 사용자 문서는 쓰지 않는다. 기존 시험은 수정하지 않았다.

| unified_search | related_queries | 고정한 동작 |
|---|---|---|
| false | false | main의 세 작업·일반 파일명/본문 원천·기존 중복 합성 유지 |
| false | true | main의 names_related·문장 단위 AND/문서 교집합·묶음별 근거 유지 |
| true | false | 일반 원천을 통합 스트리밍하고 최고 단 하나에 배정 |
| true | true | names_related 및 main과 같은 문단 검색을 통합하고 근거 보존 |

4개 시험 모두 설정 저장 왕복, 기본/pipe 모드의 원천 선택을 검사한다. 질의는 `2026; 공사 계약`, `;2026;; 공사 계약;`, `R&D; "a;b"`, `2026; 없는검색어`다. 두 설정을 켠 최종 응답의 경로·순서·문단 번호·위계·묶음별 근거를 통합을 끈 main 원천 응답의 최고 단 합성과 대조했다.

`2026; 공사 계약`의 독립 기대값은 파일명 2개·본문 2개, 합친 문서 3개다. 파일명 전체를 한 단위로 보아 `2026 공사. 계약.txt`도 포함한다. 본문은 공사/계약이 서로 다른 문단 또는 문장에 나뉜 제어 문서를 제외한다. 파일명에 2026만 있고 본문에 공사 계약만 있는 문서는 단 사이를 합쳐 포함하지 않는다. 같은 문서는 이름 단 하나에 남고 파일명·문단의 두 묶음 근거가 유지된다.

기존 UNIFIED1의 합성 1천 문서 독립 기대값 시험 및 취소·부분 실패·정렬 5종·중복 제거·상한 시험도 전체 ktm 시험에 포함했다. 기존 SEMI2의 원문 문단·묶음 색/칩 레이아웃 시험도 유지했다.

## Rust 전체 시험

변경한 코드·예제·시험은 모두 `ktm` 크레이트 소속이다. 로컬 MinGW를 PATH 앞에 두고 카드 전용 `CARGO_TARGET_DIR` 하나를 환경변수로 지정했다. 저장소 설정에는 PC 전용 경로를 넣지 않았다.

`cargo test --manifest-path ktm/Cargo.toml --no-fail-fast -- --test-threads=1` — 종료 코드 0. 모든 test result 줄:

```text
test result: ok. 268 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 2.46s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.28s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

276개 통과·기존 수동 벤치마크 1개 제외·실패 0이다. 별도 4조합 집중 시험도 종료 코드 0이다.

```text
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.47s
```

새 시험의 최초 컴파일에서 `CombinedRow`를 내부 `Row`처럼 접근한 오류 3개가 있었다. 새 시험의 필드 접근을 고친 뒤 위 집중 시험과 전체 시험을 실행했다. 제품 코드·기존 시험의 기준을 완화하지 않았다.

## Release 빌드·전체 Python

`cargo build --release --manifest-path ktm/Cargo.toml --bin ktm --example unified_bench` — 종료 코드 0.

```text
Finished `release` profile [optimized] target(s) in 2m 10s
```

ktm.exe: 2,108,928바이트, SHA-256 `BED4D37B6FF9259EC7ED8548EB2BF9EA2DA422CC577BE4D46976495A2A9F2C23`.

`python -m unittest discover -s tests` — 종료 코드 0. 지정된 Python 3.15.2 가상환경으로 실행했다. 실제 Ran/OK 줄:

```text
Ran 3695 tests in 1494.272s
OK (skipped=22)
```

3,673개 통과·조건부 제외 22개·실패/오류 0이다. 제품·시험 소스는 `158650bb`로 고정했다. 전체 실행 중의 중간 커밋 `8357f270`은 문서만 변경했다. 다른 전체 Python 시험이 끝난 뒤 실행했고, 이번 카드의 다른 빌드·시험은 동시에 실행하지 않았다.

필수 전체 시험은 약 24분 54초 걸렸다. 20분 내 완료를 확인하지 못한 사실을 진행 문서에 남겼고, 한 차례의 읽기 전용 비정지 프로파일에서 기존 `test_thumbnails_are_made_once_and_cached`의 `defusedxml` 파싱이 실행 중임을 확인했다. 장시간 원인의 추가 조사는 하지 않았으며, 실행 중인 전체 시험의 완료 결과를 위와 같이 받았다. 시험 생략·기준 완화·기존 시험 수정·재실행은 하지 않았다.

실제 한/글 COM 활성화를 차단한 별도 시험 프로세스를 사용했고 공통 COM 잠금을 실행 내내 보유했다. 차단 요청 기록은 **0건**이며, 종료 후 이번 실행이 만든 잠금을 해제했다. 사용자 문서는 사용하지 않았다. 시험이 다시 만든 docx/hwpx/pptx/xlsx fixture 4개는 원복했다.

기존 ResourceWarning 및 종료 후 sandbox의 열린 `timeline-content.sqlite` 정리 PermissionError는 출력됐다. unittest 결과와 프로세스 종료 코드는 성공이며 정리 경고를 고쳤다고 주장하지 않는다.

기존 SEMI2 release klib를 `KLIB_EXE`와 `SEMI1_KLIB_EXE`로 지정했다. SHA-256은 시험 전후 동일한 `6F122D935B8AD00C83FCE7ECD850E356136EBE11ACBCDE52E2DE652FE4FC215B`다. klib는 이번 변경 크레이트가 아니며 이번 카드에서 새로 빌드한 산출물이 아니다.

UTF-8 엄격 디코딩, 추가 diff의 한글과 PC 전용 경로 부재, `git diff --check`, 기존 Python 시험·Flutter·본체 Python 파일 무변경을 확인했다. 본체의 CRLF는 유지했다. 빌드 산출물은 저장소 밖 카드 전용 target에 남겼으며 PM 정리 대상이다.

## 확인하지 않은 범위

- 실제 사용자 문서·한/글 COM·설치된 파일명 서비스/SID 검증·실제 드라이브 색인·실제 ktm 화면 조작/캡처·IME·맥·Everything/Docufinder 비교는 확인하지 않았다.
- 16ms 목표와 대기 메모리를 다시 측정하지 않았으며 기본 활성화 근거로 삼지 않는다. 후보 단별 200개 밖의 전역 최고 단 배정·페이지 처리는 기존 UNIFIED1 한계로 남는다.
- Flutter 파일은 변경하지 않아 flutter analyze·flutter test 대상이 아니다. 새 XML 파싱은 추가하지 않았으며 본체 Python 파일의 CRLF는 변경하지 않았다.
