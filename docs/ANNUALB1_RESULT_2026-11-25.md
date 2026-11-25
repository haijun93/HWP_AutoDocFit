# ANNUALB1 결과 — 2026-11-25

## 범위와 검증 상태

작업 브랜치는 `feature/ANNUALB1`. 앞 실행의 미커밋 변경을 검토·보완하고 실제 시험을 수행했다. 이전 결과 문서의 실행하지 않은 ‘완성’ 표기는 이 문서로 대체한다. 변경 크레이트는 `kfind-annual` 1개, Flutter 변경은 없다. 푸시·태그·배포·앱 연결은 하지 않았다. 구현 중간 커밋은 `c762b4b6`. Rust·신규 Python 시험 및 산출물 검증은 통과했으나, 전체 Python 시험은 카드의 20분 상한으로 중단되어 최종 통과 여부를 확인하지 못했다.

## 구현

- `annual synthetic <출력 폴더> [시드]`: 기본 시드 `20261125`, 합성 문서 124건. 기존 분석 엔진으로 주제·버전·통계·폴더 이력·주기 업무를 계산한다. 세 산출물과 `원자료`를 만든다. 원자료에는 재현용 합성 문서 JSON과 시드도 보관한다.
- `annual bundle <입력 JSON> <출력 폴더>` / `output::write_annual_bundle`: 확장 출력을 명시적으로 선택한다. 기존 `annual report` / `write_outputs`는 기존 표 개수와 출력 구성을 유지한다.
- `weekly_heatmap_table`: ISO 연도·주차 × 월~일 9열 표. 분석 기간에 작업이 없는 주도 표시한다. 0 포함 5색 음영, 진한 셀 흰 글자, 숫자 중앙 정렬. `topic_timeline_table`: 주제·연도별 월 격자, 시작~끝 ISO 주와 작성일 순 버전 줄. 다른 해 같은 월을 합치지 않는다.
- HWPX: 제목 16pt·본문 11pt·머리색 `#E7EEF8`, 각 행 폭 합계 48,189 HWPUNIT(170mm). 배경은 `hc:fillBrush`로 기록한다. 인수인계서 2표, 실적보고서 3표.
- XLSX: 기존 `zip` 의존성만 사용한 Rust 생성기. 월별통계·주차별통계·요일열지도·주제타임라인·주기업무·폴더이력 6시트. 문자열은 inlineStr, 계산 결과는 숫자 셀로 기록하며 수식은 생성하지 않는다. Python은 생성기를 중복 구현하지 않고 `defusedxml` 기반 읽기 전용 검증만 담당한다.
- 검증기: ZIP CRC, XML, 관계 대상, content type, 스타일·셀 주소·행 폭·5색 배경 참조 및 수식 부재. COM 검증기는 공용 잠금·독립 인스턴스·입력 사본을 사용해 열기→저장→재열기와 쪽 수·본문·표 개수·입력 해시를 확인한다. 환경 부재나 잠금 실패를 성공으로 취급하지 않는다.

## 실제 시험 결과

`cargo test --manifest-path kfind/annual/Cargo.toml` 최종 실행: 종료 코드 0. 로컬 MinGW PATH 및 외부 `CARGO_TARGET_DIR` 환경변수를 사용했다. 전체 결과 줄(라이브러리·바이너리·통합·문서 시험 순):

```text
test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.11s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`python -m unittest tests.test_annual_synthetic tests.test_annual_xlsx`:

```text
Ran 7 tests in 2.112s
OK
```

동일 시드의 세 산출물 바이트 일치, 124건의 월·주·요일·폴더 합계, 53주(머리 포함 54행), 문서 내 표와 두 검증 CLI, 잘못된 시드, 수식·엔티티·깨진 관계 대상 거부를 확인했다. 초기 통합 시험은 XLSX 주별 행 순서 비결정성으로 1건 실패했고, HashMap에서 모은 주제 행을 정렬한 뒤 통과했다. 기존 Rust 시험 코드는 변경하지 않았다.

최종 합성 출력에 `validate_annual_hwpx.py --charts`와 `validate_annual_xlsx.py`를 실행해 모두 종료 코드 0을 확인했다. HWPX는 2표(3·15열)/3표(2·3·9열), 경고 0. XLSX는 6시트, `has_formulas=false`.

COM 실측은 공용 잠금 아래 순차 실행했으며, 별도 빌드·시험과 겹치지 않았다. 이미 끝난 시험의 오래된 잠금은 실행 중인 시험 프로세스가 없음을 확인한 뒤 파일을 보존하는 이름 변경으로 회수했다.

| 파일 | Open | SaveAs | 재열기 쪽 수 | 네이티브 표 | 저장 크기 | 원본 해시 |
|---|---|---|---:|---:|---:|---|
| 인수인계서.hwpx | 성공 | 성공 | 3 | 2 | 19837 B | 보존 |
| 실적보고서.hwpx | 성공 | 성공 | 2 | 3 | 34092 B | 보존 |

재열기 전후 쪽 수·본문 문자열이 동일했다. 저장본의 제목 16pt·본문 11pt 및 표 폭 48,189 HWPUNIT도 확인했다. 인수인계서 108개 셀·실적보고서 530개 셀의 배경색을 원생성본과 COM 저장본에서 각각 대조해 모두 일치했다. **실제 COM 버전은 `9, 6, 1, 10778`(설치된 한/글 NEO)**이다. 요청한 **한/글 2020에서의 확인은 못 했다**. NEO 결과를 2020 검증으로 대체하지 않는다.

전체 `python -m unittest discover -s tests`: 지정 Python으로 실제 실행했다. `ANNUAL_EXE`와 기존 빌드의 `KLIB_EXE`를 환경변수로 지정했고, 공용 COM 잠금을 잡은 상태에서 다른 빌드·시험 없이 단독 실행했다. 2026-11-25 23:39~23:59 동안 진행했으나 카드의 ‘한 가지 20분 상한’으로 프로세스 트리를 종료했다. 실행 중 소스·브랜치를 수정하지 않았다.

```text
ANNUALB1_EXIT=124 ELAPSED_SECONDS=1200.419
Ran: 출력되지 않음 (상한 중단)
OK: 출력되지 않음 (상한 중단)
```

따라서 **전체 Python 통과·실패 건수는 미확인**이다. 시간 초과 원인의 상세 분석도 미확인으로 남긴다. 입력 문제로 분류하거나 성공으로 처리하지 않았다. 로그에는 기존 SyntaxWarning·ResourceWarning 및 오류 처리 시험의 예외 출력이 포함되어 있으나, 전체 실행의 최종 실패 목록은 나오지 않았다. 실제 원문 로그 `ANNUALB1-python.log`와 Rust 원문 로그 `ANNUALB1-cargo.log`는 저장소 밖에 보존했다. 전체 시험 종료 후 작업 트리는 깨끗했으며 fixture 수정은 없었다. 종료한 시험의 잠금은 소유자·프로세스 종료를 확인하고 보존하는 이름 변경으로 회수했다.

## 남은 작업 설계

스캔 CLI는 `annual scan <폴더> --out <입력 JSON> --period <시작>..<종료>`로 분리한다. Rust가 기존 파일 순회·취소·메타데이터 수집을 재사용하고, HWPX 텍스트 추출은 기존 Python `defusedxml` 경로를 이용한다. HWP/PDF는 기존 읽기 전용 추출기를 재사용하며 추출 실패 사유를 문서별로 남긴다. 원본은 읽기만 한다. 수집 기준(수정일, 문서 내 날짜 보조 정보)을 JSON에 명시한다.

`annual_win`은 Rust+Win32 경량 독립 창으로 폴더·기간 선택, 분석 요약, 세 산출물 저장·열기를 제공한다. 상태는 `Idle → Scanning → Analyzing → Writing → Validating → Completed`이며 각 작업 단계에서 `Cancelled` / `Failed`로 전이한다. 동시 실행은 막고 취소 시 임시 결과만 정리한다. 완성 파일은 검증 후 공개하고 실패 시 오류 내역과 원자료 위치를 유지한다. 기본 앱 라우팅 연결은 후속 카드에서 수행한다.

## 미확인

전체 Python 시험의 최종 통과 여부와 한/글 2020은 미확인(이 PC COM은 NEO). NEO의 열기·저장·재열기·쪽 수는 위 표와 같이 실측했다. Excel/한셀 GUI 열기는 범위 밖이며 구조 검증과 구분한다. 사용자 실문서·스캔 CLI·Win32 창·대량 장문 인쇄 품질은 이번 카드에서 확인하지 않았다.
