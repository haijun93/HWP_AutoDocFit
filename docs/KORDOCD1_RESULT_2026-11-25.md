# KORDOCD1 결과 — 성능 우선 입력 변환과 선택형 라우팅

- 작업 브랜치 `feature/KORDOCD1`, 기준 `github/main=cd138da5`(로컬 참조 일치, 원격 조회 없음). 파일명은 요청한 2026-11-25이며 실제 작업은 한국시간 2026-11-26이다.
- 최신 방침: 의존성 제거 자체를 목표로 삼지 않는다. 같은 품질에서 속도·자원이 유리한 엔진만 권고한다. 기본 `docx_engine=kordoc`, 기존 변환/추출 API 및 호출 지점은 그대로다. 푸시·태그·게시·GitLab 접근 없음.
- 새 의존성·Pandoc 번들 확장·Rust/Flutter 변경 없음. 사용자 문서 대신 합성 입력만 썼다. XML 읽기는 defusedxml을 사용했다(비교 대상인 기존 klib 내부 구현은 변경하지 않음).
- 기계 근거: [입력 해시·측정·품질·COM JSON](KORDOCD1_EVIDENCE_2026-11-25.json). 원시 파일/로그는 저장소 밖 산출 폴더에 보관한다.

## 비교 범위와 측정 방법

Markdown 19종(KORDOCC2 재사용), DOCX·XLSX·PPTX·HTML 각 5종으로 변환 39종이다. HWP/HWPX 본문 추출은 같은 합성 원문 5종을 한/글로 각각 저장한 10개 입력이다. 원문/추출 입력 SHA-256을 근거 JSON에 기록했다. DOCX/HTML은 기본 혼합·표·본문·그림·중첩 목록, XLSX는 복수 시트·빈칸·한영/특수문자·넓은 표·긴 표, PPTX는 혼합·복수 제목·3단 목록·표·그림이다.

각 작업/형식/엔진마다 단건 1회와 연속 20건 1회다. 단건은 해당 묶음 첫 입력, 20건은 동일 5종(또는 MD 19종)을 순서대로 순환한다. 20건은 Python 작업 프로세스 하나를 유지하되 kordoc/klib CLI는 요청마다 실행한다. kordoc 상주형 구현을 실측한 것은 아니다. 양쪽에 같은 새 Python 작업 프로세스를 사용하며 프로세스 시작·import·실제 변환·파일 저장·종료 시간을 모두 포함했다. 이미지 경로와 보고서 프리셋은 같고 HTML 기준은 현재 제품의 번들 Pandoc→MD→kordoc 경로다. DOCX/XLSX/PPTX 기준은 기존 parse→generate 두 CLI 호출 경로다.

최대 메모리는 Windows Job Object의 `PeakJobMemoryUsed`다. Python과 자식 Node/Pandoc/klib의 동시 commit 최대값을 커널에서 얻었다. RSS가 아니며, COM 조판 메모리는 이 표에서 제외한다. 1회 측정이고 OS 파일 캐시는 강제로 비우지 않았다. 후속 프로세스가 이미 데워진 캐시의 영향을 받을 수 있다. 테스트/빌드와 겹치지 않게 순차 실행했다. 기존 MCP/사용자 한/글 프로세스는 건드리지 않았다.

## 처리 시간·메모리

| 작업/입력 | native 단건 초 | kordoc 단건 초 | native 20건 초 | kordoc 20건 초 | native 최대 MiB 단건/20건 | kordoc 최대 MiB 단건/20건 |
|---|---:|---:|---:|---:|---:|---:|
| convert/DOCX | 0.484 | 2.005 | 1.069 | 32.694 | 36.1/39.7 | 135.2/136.4 |
| convert/HTML | 0.585 | 1.561 | 1.043 | 18.932 | 36.2/39.8 | 136.0/136.6 |
| convert/MD | 0.414 | 1.107 | 0.931 | 16.572 | 36.0/38.1 | 136.2/136.5 |
| convert/PPTX | 0.501 | 1.929 | 1.048 | 32.000 | 36.5/39.4 | 136.3/136.9 |
| convert/XLSX | 0.389 | 1.908 | 0.938 | 33.689 | 20.1/20.6 | 135.2/136.6 |
| extract/HWP | 0.331 | 1.163 | 0.884 | 15.662 | 19.5/19.9 | 104.1/106.6 |
| extract/HWPX | 0.335 | 1.135 | 0.723 | 16.582 | 19.4/20.2 | 105.8/107.8 |

native 변환 후보는 전 형식에서 단건·20건과 최대 commit이 유리했다. 이 수치는 어댑터+생성기 경로의 측정이며 후속 라우팅의 추가 사전 검사/게시 비용을 따로 실측한 수치는 아니다. HWPX klib의 거부 입력도 시간에 포함되므로 4/5 품질을 무시하고 더 빠르다고 채택하지 않는다.

## 같은 합격선의 품질과 권고

B2/C2 `conversion_fidelity.compare`를 그대로 사용했다. 항목별 구조 100%, 정규화 문자 재현율 99.5%, 정밀도 99%이며 평균으로 불합격을 상쇄하지 않았다. 추출은 본문 텍스트 작업이라 제목/표 구조는 n/a로 기록하며 구조 보존 합격이라고 부르지 않는다. native 변환 39/39는 해당 모든 지표와 재현율/정밀도 100%다.

| 작업/형식 | native 합격 | kordoc 합격 | 판정과 형식별 기본 전환 권고 |
|---|---:|---:|---|
| convert/DOCX | 5/5 | 3/5 | 단순 본문·Heading1~6·일반 목록·직사각 표·PNG/JPEG는 native/auto 후보. 사용자 정의 스타일·병합·수식 등은 kordoc 유지. |
| convert/HTML | 5/5 | 2/5 | 단순 의미 HTML은 native/auto 후보. CSS·병합·복잡한 DOM·외부 그림은 kordoc/Pandoc 유지. |
| convert/MD | 19/19 | 10/19 | 검증한 Markdown 문법은 native 권고. 링크 대상 필수/미지원 문법은 kordoc 유지. |
| convert/PPTX | 5/5 | 0/5 | 단순 제목·목록·표·PNG/JPEG는 native/auto 후보. 위치 조판·SmartArt·차트·노트 등은 kordoc 유지. |
| convert/XLSX | 5/5 | 0/5 | 시트의 직사각 표 데이터 변환만 native 후보. 시트명·날짜/수식/병합/그림까지 필요한 일반 XLSX 기본은 kordoc 유지. |
| extract/HWP | 5/5 | 5/5 | klib 우선 유지 권고. 같은 품질이며 더 빠르고 적은 commit 사용. kordoc 우선 복원 근거 없음. |
| extract/HWPX | 4/5 | 5/5 | klib 직접 우선 확대 보류. & 엔티티 입력 1건 거부. 기존 추출 경로 유지, kordoc 대체 가능; 앱 기본 변경은 PM 판정. |

**현재 변환 설정 기본값은 모든 형식에서 kordoc 그대로다.** 위 native 권고는 합성 시험을 통과한 기능 범위에 한하며 실문서 전면 전환 권고가 아니다. XLSX 기대 명세는 기존 B2 관례상 시트명을 본문에서 제외한다. kordoc이 보존한 시트명이 추가 글자로 계산되어 정밀도 80.00~97.37%로 불합격한 것이므로, 이를 kordoc 데이터 손실이나 native의 완전한 XLSX 의미 보존으로 해석하면 안 된다. 시트명 보존이 필요한 사용은 kordoc을 유지한다. XLSX 그림은 native 미지원으로 명시 경고/auto 폴백한다.

kordoc 불합격 원인: DOCX의 H3→기호 목록과 번호 목록 유형 변경, HTML의 제목/목록 변형, PPTX 제목 수준 변형 및 3단 목록 깊이 일부, XLSX의 추가 시트명이다. Markdown은 기존 C2와 같은 10/19다. 본문 추출은 최종 정상 비교에서 kordoc HWP/HWPX 각 5/5(재현율/정밀도 100%)이다. klib HWP는 5/5, HWPX는 정상 XML의 `&amp;`까지 GeneralRef로 거부하는 기존 Rust 코드 때문에 1건 실패했다. Rust 코드/우선순위는 이번에 고치지 않았다.

## 한/글 2020 검증

공용 `com_test_lock` 기본 잠금 하나 아래 독립 COM 인스턴스 하나씩 순차 실행했다. native HWPX 39개 모두 열기·별도 HWPX 저장·저장본 ZIP/XML·저장 전후 동일 harness 품질이 통과했다. 한/글 버전 `9, 6, 1, 10778`, 모두 1쪽이다. 그림의 바이너리 SHA-256·대체글·크기 목록도 저장 전후 39/39 같았다(그림 없는 문서 포함). 추출 입력 5종은 먼저 native 합성 HWPX를 열고 각각 HWP/HWPX로 새로 저장했으며 본문·제목·표·개행 입력은 1쪽, 200문단은 페이지 수를 근거 JSON에 기록했다. 사용자 원본은 열거나 고치지 않았다. 화면 조판/픽셀 동등성·실문서·긴 오피스 문서 조판은 확인 못 함.

## 선택형 API와 안전 경계

`docfit_core/input_markdown.py`: `adapt_to_markdown(source, new_image_dir)`는 Markdown·그림 작업 디렉터리·손실 경고를 돌려준다. 기존 안전 ZIP 검사 및 defusedxml로 Office를 읽고, HTML은 표준 HTMLParser를 쓴다. 새 패키지/다운로드 없이 PNG/JPEG 원본 바이트를 작업 폴더에 복사한다. 외부 관계/URL·상위 경로·그림 형식/용량 초과는 손실 경고다. 수식/날짜 표시·병합·숨김·지원 밖 개체를 완전 변환했다고 하지 않는다.

`docfit_core.kordoc_bridge.convert_to_hwpx_routed(path, preset="보고서", engine="auto", target=...)`는 `RoutedConversion(path, engine, states, warnings, fallback_reason, quality)`를 돌려준다. 설정 소비자는 `engine=configured_engine(settings)`를 명시하며 키 없는 설정도 kordoc이다. 새 함수의 명시 auto 기본과 앱 설정 기본 kordoc을 구분한다. 설정 스키마/기본값 명세에 키만 추가했고 기존 호출은 연결하지 않았다.

상태는 `prepare → native_attempt → native_validation → publish → complete`, 대체 시 `native_attempt → fallback → kordoc_attempt → publish → complete`다. native는 보고서 프리셋만 지원하며 계획서/기안문은 auto에서 kordoc으로 간다. 손실 경고·미지원 구조·생성 후 품질 미달·파싱 오류는 auto에서 kordoc을 단 한 번 시도한다. 명시 native는 오류를 그대로 낸다. 품질 검사는 Markdown 의미 모델과 별도 HWPX 추출 경로를 비교하며, 원본 Office 전체 의미를 독립적으로 증명하는 런타임 검증은 아니다. 원본→결과 보존은 합성 정답 시험으로 별도 확인했다.

모든 결과는 같은 볼륨 임시 폴더에서 검증한 뒤 hard link로 새 최종 이름을 배타적으로 게시한다. 원본/기존 파일 덮어쓰기·충돌 때 폴백·게시 전 미검증 결과 노출·무한 재시도는 없다. 경로 충돌/접근권한/취소는 엔진 폴백으로 숨기지 않는다. 최종 게시가 hard link를 지원하지 않는 파일시스템에서 성공하는지는 확인 못 함. 기존 API의 동작은 변경하지 않는다.

정적 호출 확인: `kordoc_bridge.convert_to_hwpx/generate_hwpx`를 사용하는 `file_conversion.py`, `docfit_engine/formatter_text.py`(요청의 core 경로가 아닌 실제 위치), MCP는 기존 경로를 유지했다. 본문 추출의 klib 우선/실패 kordoc 대체, Pandoc GPL 번들은 변경하지 않았다.

## 도구 오류와 무효 시도

성능 측정 전 사전 품질 검사에서 목록 문단 사이 빈 줄 때문에 깊이가 0으로 초기화되는 어댑터 오류와 `< >` 이스케이프를 수정했다. 이 사전 생성은 시간/메모리 판정에 쓰지 않았다. 첫 사전 검사 자체의 기대 form 누락도 고쳤다.

추출 첫 호출의 `--format text`는 kordoc 4.24.9에서 지원하지 않아 무효다. 지원하는 Markdown으로 바꾼 뒤 기존 B2 글 정규화가 리터럴 `< >`를 태그로 지우는 도구 오류를 발견했다. 원시 kordoc Markdown에는 글자가 살아 있었다. 의미 모델을 통한 본문/표 셀 정규화로 바로잡았으며 해당 두 무효 시도의 시간은 근거 JSON에 따로 남기고 최종 비교에서 제외했다. 성공한 변환 5종/klib 측정은 반복하지 않았다. 정상 절차의 단건/20건은 각각 1회다. klib HWPX 엔티티 거부는 입력 문제로 돌리지 않고 제품 결함/미합격으로 기록한다.

## 시험

관련 시험(입력 어댑터·라우팅·기존 Markdown/C2/B2·설정 폼/줄 간격):

```text
Ran 103 tests in 7.817s
OK
```

첫 전체 실행은 새 내부 설정 키를 기존 폼이 화면 항목으로 해석하여 `KeyError`가 발생하는 문제를 확인하고 119.101초에 중단했다. `configuration_service.EXCLUDED`에 키를 넣어 기존 화면을 유지하고 회귀 시험을 추가했다. 위 103개는 수정 후 결과다. 중단 실행은 완전한 전체 결과로 세지 않으며 원시 로그/메타데이터를 별도로 보존했다.

<!-- FULL_TEST_RESULT -->

Rust 변경 크레이트 0개로 cargo test 대상 없음. Flutter 변경 0개로 analyze/test 대상 없음. 새 시험은 형식별 5종 이상의 합성 문서, 원본 바이트 불변·그림 해시, 외부 이미지/상위 경로/엔티티 거절, 손실 경고, native 성공/실패·한 번 폴백·kordoc 실패·기존 결과·게시 경합·권한 오류·설정 기본값을 검사한다.

## 남은 확인과 성능 개선 여지

- 실제 사용자 문서, 복잡한 DOCX 스타일/필드/각주·병합/중첩 표, XLSX 수식 재계산·날짜/표시 서식·차트/그림·시트명 의미, PPTX 위치/테마/SmartArt/노트, HTML CSS·복잡한 DOM·원격 자원은 이번 합성 묶음에서 확인 못 함. 검출되는 미지원 요소는 경고/폴백하며 완전한 지원 범위 판별기는 아니다.
- kordoc은 parse→generate를 한 프로세스/일괄 요청으로 묶거나 상주 워커에서 모듈을 재사용할 여지가 있다. 이번 20건은 CLI를 매번 시작했으므로 상주 kordoc과 native의 우열은 확인 못 함. 실제 설치 번들의 CLI에 없는 API를 가정해 기본으로 연결하지 않았다.
- HWP는 현재 klib 우선 유지. HWPX klib 엔티티 처리는 별도 수정/크레이트 전체 시험 필요. kordoc 우선으로 전체를 되돌리는 안은 HWP에서 품질 이득 없이 더 느려 권고하지 않는다. HWPX 직접 klib 확대 대신 기존 안전 추출/대체 경로를 유지하며 최종 기본 정책은 PM 판정이다.
- 이번 커밋은 선택형 기능과 근거를 제공하며 앱 기본 전환·소비자 연결·패키지/번들 분할·게시를 수행하지 않는다.
