# CILINUX2 결과 — Linux fixture 회귀·의존성 추출 오류

- 대상: `feature/CILINUX2`, 기준 `cd138da5` (`github/main`). 로컬 커밋만 수행하며 푸시·태그·병합·배포는 하지 않는다.
- 범위: 합성 변환 fixture 11개(Office ZIP 5개·Markdown/HTML 6개)와 별도 PNG 1개, 관련 시험 2모듈, MCP 의존성 및 OSV 워크플로.
- 판정: 회귀 원인과 의존성 추출 오류를 수정했다. **Linux에서 수정 후 전체 CI 통과를 직접 확인하지 못했다. OSV의 기존 취약점·유지보수 경고도 남아 있으므로 의존성 점검 전체 성공으로 보고하지 않는다.**
- Windows 전체 시험: **3,751개 실행, 3,722개 통과·29개 조건부 제외, 실패·오류 0, 종료 코드 0**.

## 원인 확인과 비교 기준 수정

- [기존 Linux CI 45674579386](https://github.com/haijun93/hwp-auto-docfit/actions/runs/45674579386): Python 3.14.10에서 `Ran 3744 tests in 429.443s / FAILED (failures=4, skipped=67)`를 확인했다. manifest 시험 1건과 OS 모의 시험의 3개 subtest이며 파이프 시험 실패는 없다.
- 로컬 지정 Python은 3.15.2, zlib-ng 2.2.4(`ZLIB_RUNTIME_VERSION=1.3.1.zlib-ng`)다. 일반 zlib 1.3.1의 `deflateInit2_`/`deflate`로 ZIP 압축기만 교체한 대조를 수행했다. 실제 Linux 실행이나 Linux의 zlib 런타임 버전 확인을 뜻하지 않는다.
- `doc_basic.docx`의 일반 zlib SHA-256은 `1cc7c2c3168e215488193dbba08f5f993723126443f8bea5f6238b9a5feaf132`다. Linux CI의 축약된 assertion 출력에 노출된 앞 40자리와 일치한다. 커밋본은 `5bc5ab479197f1001475142c09bc28b0e9a82da9711d45f6cfaf0cc1aa1f3b42`다.
- 일반 zlib 대조에서 Office ZIP 5개 모두 해시·압축 크기가 달랐고, 압축을 푼 **모든 ZIP 항목 바이트는 동일**했다. 기본 PNG의 zlib 압축 결과도 두 구현에서 같았다. 생성 OS 메타데이터는 이미 `0`으로 고정되어 있었다. 원인은 문서 내용 변화가 아닌 deflate 구현 차이다.
- 수정 전 시험 소스를 읽기 전용으로 로드해 일반 zlib에서 `Ran 2 tests / FAILED (failures=4)`를 재현했다. 동일한 압축 환경의 수정 후 관련 46시험은 모두 통과했다.

`tests/conversion_fixture_checks.py`가 두 시험의 공통 검증 기준이다.

1. 생성본과 커밋본 각각의 실제 SHA-256·파일 크기를 각자의 manifest와 대조한다.
2. fixture ID·형식·파일명·기대 구조·텍스트 해시는 동일해야 한다. Office 항목의 압축 파일 해시·크기만 환경 간 동일 조건에서 분리한다.
3. ZIP 항목 이름·순서·중복·추가·누락, archive comment 및 항목 메타데이터 15필드를 대조한다. 압축에 종속된 `compress_size`·`header_offset`은 비교하지 않는다.
4. XML·관계·내장 그림을 포함한 모든 항목의 압축 해제 바이트를 그대로 대조하고 CRC를 검증한다. XML 정규화나 텍스트 유사도 허용치를 사용하지 않는다. Markdown·HTML·별도 PNG도 바이트 전체를 대조한다.
5. 다른 deflate 출력은 허용하면서 XML 변경·그림 변경·항목 누락/추가/중복/순서 변경·생성 OS 변경·archive comment 변경은 검출하는 음성 대조 8종을 추가했다. manifest 해시·크기를 임의로 바꾼 대조 2종도 실패한다.

fixture 바이너리와 커밋된 manifest는 갱신하지 않았다. 생성기 실행 동작도 유지하며, 압축 환경이 다르면 파일 해시가 달라질 수 있음을 생성기 문서에 명시했다. 기존 동일 환경 내 재생성 결정성 시험과 XML 정합성 시험을 유지했다. skip을 추가하거나 기존 skip 정책을 바꾸지 않았다.

## 의존성 추출 오류 수정과 점검 범위

- [기존 감사 CI 45674579530](https://github.com/haijun93/hwp-auto-docfit/actions/runs/45674579530)와 `osv-report` 산출물(ID `15051286959`)의 Markdown·JSON을 확인했다. 전이 해석에서 `pywin32[Concrete:312.0.0]: not found`가 발생했고, 취약점 보고서도 별도로 생성되어 있었다.
- `requirements-mcp.txt`의 pywin32에 기존 앱 잠금 파일과 같은 `sys_platform == "win32"` 마커를 붙였다. 버전은 312로 유지했다. Linux 설치 대상과 Windows 전용 의존성을 올바르게 표시한다.
- 워크플로의 JSON 출력에 `--all-packages`를 추가해 실제 검사 패키지를 남긴다. JSON 스캔 오류 출력을 숨기지 않고 두 스캔 중 하나라도 실패하면 최종 실패 상태를 보존한다.
- `--recursive`, 두 명시적 Python 파일, 기존 모든 생태계·잠금 파일 및 전이 의존성 해석을 유지한다. 제외 목록·무시 advisory·`--no-resolve`는 추가하지 않았다.
- Windows용 **동일 OSV-Scanner v2.7.0**에서 수정 후 전체 명령을 실행했다. 기존 CI 로그의 스캔 파일 36개와 수정 후 스캔 파일 36개를 상대 경로로 대조해 추가·누락 0개를 확인했다. 37 Extract 호출, 결과 36개 source 경로, 2,120 package 행(중복 포함): Rust 1,641·npm 361·Pub 43·PyPI 75. 생태계·이름·버전 기준 고유 조합은 875개다. 추출 오류·해석 실패는 0건이고 종료 코드 1의 원인은 아래의 보고 항목이다.
- MCP 결과에는 고정 패키지 28개와 전이 결과 5행이 들어 있다. **pywin32 312도 고정 버전 검사 결과에 남는다.** cffi·pycparser·exceptiongroup 등 전이 의존성도 검사한다. PyPI의 pywin32 312 메타데이터에는 `requires_dist`가 없으므로 해당 패키지의 전이 의존성을 빠뜨리지 않는다.
- 검토 중 `--data-source native`도 시험했다. MCP 단독은 통과했으나 전체에서는 레거시 `proxy_tools 0.1.0` sdist의 `setup.py` 의존성 메타데이터를 읽지 못했다. 따라서 최종 워크플로에는 채택하지 않았다.
- OSV의 manifest/lockfile 및 전이 해석 방식: [공식 지원 형식 문서](https://google.github.io/osv-scanner/supported-languages-and-lockfiles/). pywin32 선언 확인: [PyPI 312 메타데이터](https://pypi.org/pypi/pywin32/312/json).

## 발견 항목과 조치안

기존 CI Markdown 요약은 **영향받는 source/package 8건, 알려진 취약점 11건**(Critical 1·High 3·Medium 1·Unknown 6)이다. 동일 패키지의 여러 source, 유지보수 중단 advisory, 별칭 ID를 구분해야 한다. 수정 후 로컬 전체 스캔도 같은 항목을 보고했다.

| 패키지·해석 버전 | 보고 ID | 위치·상태와 조치안 |
|---|---|---|
| bottle 0.9.8 | [PYSEC-2014-77](https://osv.dev/PYSEC-2014-77), [PYSEC-2021-129](https://osv.dev/PYSEC-2021-129) / GHSA-qhx9-7hx7-cp4r, [PYSEC-2022-227](https://osv.dev/PYSEC-2022-227) / GHSA-xhp9-4947-rq78 | `requirements.txt`의 전이 해석 결과. `constraints.txt`·`requirements-lock.txt`는 **0.13.4**이며, 배포 잠금 파일에서 0.9.8이 확인된 것은 아니다. 일반 requirements 설치와 검사도 같은 고정 그래프를 쓰게 정리하거나 bottle 0.13.4를 명시해 해석 결과를 맞출 것. 보고된 수정 버전은 각각 0.10.12·0.12.19·0.12.20. 이번에는 경고를 숨기거나 선언 범위를 바꾸지 않았다. |
| chrono 0.4.45 | [RUSTSEC-2026-0543](https://osv.dev/RUSTSEC-2026-0543) | 유지보수 중단 경고. `bench/kb1`의 seekstorm 4.0.1과 `pdf_edit_core`의 lopdf 0.34.0에서 유입되어 source 2건이다. 상위 의존성 갱신·유지보수되는 대체 라이브러리 이관을 검토할 것. 보고된 수정 버전 없음. |
| number_prefix 0.4.0 | [RUSTSEC-2025-0119](https://osv.dev/RUSTSEC-2025-0119) | 유지보수 중단 경고. `bench/kb1`의 indicatif 0.17.11에서 유입. 상위 indicatif 갱신 및 해당 의존성 제거 여부 확인. 수정 버전 없음. |
| paste 1.0.15 | [RUSTSEC-2024-0436](https://osv.dev/RUSTSEC-2024-0436) | 유지보수 중단 경고. `bench/kb1`의 tokenizers 0.21.4에서 유입. 상위 tokenizers 갱신 및 유지보수되는 대체 매크로로의 이관 검토. 수정 버전 없음. |
| url 2.5.8 | [RUSTSEC-2026-0592](https://osv.dev/RUSTSEC-2026-0592) / GHSA-hrj5-67jp-wqp9 | 중첩 blob URL 처리의 스택 오버플로. `bench/kb1/Cargo.lock`; seekstorm·ureq 의존성이다. **2.5.9 이상**으로 잠금 갱신 후 벤치 회귀 확인. |
| quick-xml 0.37.5 | [RUSTSEC-2026-0194](https://osv.dev/RUSTSEC-2026-0194), [RUSTSEC-2026-0195](https://osv.dev/RUSTSEC-2026-0195) | 속성 중복 검사 시간 복잡도·namespace 할당에 의한 서비스 거부. `hwpx_fmt` 직접 의존성. **0.41.0 이상**으로 API 이관·잠금 갱신 후 기존 HWPX 내용·서식 보존 회귀 확인. |
| lopdf 0.34.0 | [RUSTSEC-2026-0187](https://osv.dev/RUSTSEC-2026-0187) / GHSA-6fj4-rcr3-r7rr | 깊게 중첩된 PDF 객체 처리의 스택 오버플로. `pdf_edit_core` 직접 의존성. **0.42.0 이상**으로 API 이관·잠금 갱신 후 PDF 편집·왕복 보존 회귀 확인. |

이번 카드에서는 Rust 의존성의 API 이관이나 잠금 갱신을 수행하지 않았다. 위 목록이 남아 있으므로 추출 오류 수정 후에도 감사 작업의 실패는 유지되어야 한다. 벤치 경로도 기존 검사 범위에 포함되어 있다.

## 시험 결과

- 변경 전 Windows 관련 시험: `Ran 43 tests in 4.050s / OK`.
- 변경 후 기본 zlib-ng 환경: `Ran 46 tests in 4.126s / OK`.
- 일반 zlib 1.3.1 대조: 수정 전 `Ran 2 tests in 0.469s / FAILED (failures=4)`, 수정 후 `Ran 46 tests in 4.210s / OK`.
- Windows 전체 시험: 사용자 지정 venv315 Python 3.15.2에서 `python -m unittest discover -s tests` 실행. 기존 release 빌드 klib 실행 파일을 `KLIB_EXE`로 지정하고 COM 공통 잠금 획득·갱신, 다른 전체 시험과의 비중첩을 확인했다. 임시 폴더와 UTF-8 원시 로그는 작업 트리 밖에 보관한다. 실제 출력과 프로세스 종료 코드 **0**을 확인했다.

```text
Ran 3751 tests in 929.047s

OK (skipped=29)
```

- 위 Windows 전체 시험의 OS·실제 엔진 구성·동적 시험 수는 기존 Linux CI와 달라 전체 건수를 같은 모집단으로 직접 비교하지 않는다. 신규 시험 3개에는 skip이 없다.
- `OK` 뒤 atexit 샌드박스 정리에서 `timeline-content.sqlite` 사용 중에 따른 `PermissionError [WinError 32]`가 두 번 출력됐다. 기존 CILINUX1 결과에서도 보고된 문제이며, unittest 실패·오류에 포함되지 않았고 종료 코드도 0이었다. 기존 ResourceWarning도 출력됐다. 이번 수정 범위의 해결로 보고하지 않는다.
- Linux 직접 검증: 사용 가능한 WSL 배포판이 없어 수행하지 못했다. 수정 후 Linux 전체 Tk CI·Linux OSV 실행·Linux zlib 런타임 버전은 미확인이다. 푸시나 원격 CI 실행을 하지 않았다.
- 시험이 재생성한 `timemachine_extract` 바이너리 4개(DOCX/HWPX/PPTX/XLSX)는 원복했다. 공통 잠금 해제 및 시작 전부터 있던 한/글 프로세스 2개 보존을 확인했다. 사용자 원본은 사용하지 않았다. 본체 CRLF·변환 fixture 바이너리·manifest는 수정하지 않았으며 UTF-8을 유지한다. PC 전용 절대 경로를 저장소에 추가하지 않는다.
- 최종 정적 확인: 변경·신규 파일 9개 UTF-8 읽기 성공, 추가한 PC 전용 경로 0건, fixture 차이 0개, 본체 CRLF 3,523줄 보존, `git diff --check` 통과.
