# ALLQA5 현재 main 전후 측정 — 2026-11-23

## 측정 개요

- 대상: `docs/ALLQA_DOCSET.md`의 고정 픽스처 6개, 개선 전 결과 `E:/docfit-tmp/allqa/before`와 현재 브랜치 기준 결과 `after5` 비교.
- 브랜치: `feature/ALLQA5` (GitHub main 기반). 실행 모드 `all`, label `after5`, `DOCFIT_SPACING_MEMO`는 지정하지 않음(기본 reuse).
- 실행 Python: `C:/HWP_AUTODOCFIT/venv315/Scripts/python.exe`; 시작 전 `win32com` 및 `defusedxml` import 성공.
- 실행 시작: `2026-11-23 02:17:21.619 +09:00`; 종료: `2026-11-23 02:23:32.085 +09:00`; 실측 경과 370.5초.
- COM 잠금: `C:/HWP_AUTODOCFIT/.com-test.lock` 획득 후 실행, 종료 시 해제 확인.
- 시작 전 `find`, `grep`, `tail`, `cargo`, `flutter_tester`, `rustc` 잔여 프로세스 없음. 측정 중 별도 빌드·시험은 실행하지 않음.
- 기존 한/글 PID 13648은 시작 전부터 존재했고 제목은 `tables.hwpx [E:\docfit-tmp\hwp_table_guard_ccc_2yrj\] - 한글`. 종료하지 않았으며 측정 중 창 제목 유지, CPU 증가는 1.56초.
- 측정 중 시험 외 프로세스에서도 CPU 사용이 관찰됨. 측정기는 시험 Python 프로세스 트리 밖의 CPU 증가를 합산해 294.48초로 기록했다. 이 중 이름이 `Hwp`인 모든 프로세스가 222.85초였고, 기존 PID 13648의 증가량은 1.56초였다. 나머지 221.29초는 새 한/글 인스턴스 등 다른 Hwp 프로세스에서 발생했으며, 측정 도구가 PID별 소유 관계를 저장하지 않아 기존 창 이외 인스턴스의 정확한 귀속은 판별할 수 없다. Hwp 이외 프로세스의 합산 CPU 증가는 71.63초였고, 주요 증가는 `Code` 17.18초, `explorer` 10.82초, `logioptionsplus_agent` 6.60초, 모니터링 PowerShell 5.52초, `StSess32` 3.91초, `ChatGPT` 3.80초, `claude` 3.22초, `jabra-direct` 3.04초, `stsess` 2.82초였다. 시험 외 프로세스 활동이 관찰되어 완전히 유휴 PC 조건은 아니다.
- COM 생성 코드에서 `DispatchEx`를 우선 호출하고 실패한 경우에만 `Dispatch`로 재시도하도록 확인했다. 측정 중 기존 사용자 HWP 창은 건드리지 않았다.
- 재시도 없이 1회 실행. 실행 종료 코드는 1이며 `hwpx_table` 저장 실패가 원인이다. 나머지 5개 문서는 종료 코드 0으로 결과를 저장했다.

## 요약

| 지표 | 개선 전 | after5 | 증감 |
|---|---:|---:|---:|
| 전체 처리 시간(6개 입력, 실패한 hwpx_table 시도 포함) | 638.666초 | 355.238초 | -44.4% |
| COM 계측 시간 합 | 578.929초 | 338.125초 | -41.6% |
| COM 호출 수 | 154,212 | 141,720 | -8.1% |
| 결과 저장 성공 | 5/6 | 5/6 | 동일 (`hwpx_table` 실패) |

`txt_long`은 보고된 250.360초로 완료되어 5분 정체 한도에 걸리지 않았다(폴더 생성 후 다음 문서 시작까지 약 251초). 한/글 대화상자 발생 여부는 화면을 관찰하지 않아 확인하지 못했다. 실행 중 처리 문서별 창 제목을 기록하지는 못했으며 기존 HWP 제목은 위와 같다.

## 기능 검수 판정

- 원본 픽스처 해시: 6/6 전·후 동일.
- 처리 사본 해시: 6/6 전·후 동일.
- 파이프라인 오류: 성공한 5개는 오류 0. `hwpx_table`은 오류 종료 1, 저장 0, 쪽 수·문단 수 측정 불가.
- 선택된 단계 실행 기록: 성공한 5개 모두 선택 단계 로그 검수 통과. `hwpx_table`은 저장 실패로 검수 미통과.
- 쪽 수: `txt_short` 1, `txt_long` 2, `hwp_reference` 1, `hwpx_continuation` 1, `hwpx_shapes` 1. `hwpx_table` 미측정.
- 원문 텍스트 보존: 엔진 검수 플래그는 `txt_short`, `txt_long`에서 실패(각각 공백 제거 길이 71→71, 1105→884; 같은 길이여도 내용은 불일치). `hwpx_continuation`, `hwpx_shapes` 통과. `hwp_reference`의 단순 플래그는 HWP 바이너리를 UTF-8 텍스트처럼 읽어 산출하므로 판정 불가. `hwpx_table`은 저장본이 없어 판정 불가.
- 전후 결과 비교: 저장된 5개에서 변경 문단 0, 텍스트 XML 해시 동일, 원시 서식 XML 해시 동일, 정규화 서식 XML 해시 동일. 이는 기준선 출력과의 동일성을 뜻하며 입력 원문의 보존 실패를 통과로 바꾸지는 않는다.

아래 단계별 표와 상세 동일성/자동 검수 내역은 `scripts/allqa_compare.py` 출력이다. 정규화 규칙은 `docs/SPDMEMO2_RESULT_2026-11-22.md`와 동일하다: `paraPr` 정의 id 제거 후 정의 목록 안에서만 정렬, `paraPrIDRef`를 정의 트리로 치환, 참조되지 않는 `tbl/@id`만 제외하고, 나머지 `Contents/*.xml` 구조·속성을 비교한다. `<t>` 텍스트는 별도 해시로 비교한다.

---

# ALLQA 전후 비교

- 전: `before`
- 후: `after5`

## 문서별 시간·COM

| 문서 | 단계/전체 | 전(초) | 후(초) | 증감(%) | 전 COM | 후 COM |
|---|---|---:|---:|---:|---:|---:|
| hwp_reference | 개별 문단 페이지 배치 | 0.225 | 0.167 | -25.7% | 238 | 238 |
| hwp_reference | 개요·한 칸 표 자간 조정 | 0.002 | 0.006 | +181.0% | 10 | 10 |
| hwp_reference | 검수 집계 | 0.000 | 0.000 | — | 0 | 0 |
| hwp_reference | 공백·문장부호·라벨(XML) | 2.701 | 2.443 | -9.6% | 8 | 8 |
| hwp_reference | 기본 표 서식 | 0.077 | 0.054 | -29.8% | 0 | 0 |
| hwp_reference | 무결성·숫자 대조 | 0.006 | 0.004 | -23.2% | 0 | 0 |
| hwp_reference | 문단 아래 간격 페이지 맞춤 | 0.691 | 0.584 | -15.5% | 677 | 677 |
| hwp_reference | 별표 위첨자 | 0.073 | 0.036 | -51.0% | 0 | 0 |
| hwp_reference | 보고서 표준서식 | 1.136 | 0.809 | -28.8% | 471 | 471 |
| hwp_reference | 본문 자간·단어 분리 조정 | 0.025 | 0.015 | -40.5% | 140 | 74 |
| hwp_reference | 부연설명 들여쓰기 | 0.007 | 0.007 | +1.4% | 36 | 36 |
| hwp_reference | 셀 너비 본문 맞춤 | 0.001 | 0.003 | +211.1% | 4 | 4 |
| hwp_reference | 셀 안쪽 여백 강제 축소 | 0.001 | 0.003 | +123.1% | 4 | 4 |
| hwp_reference | 연결 부호 맞춤 | 0.007 | 0.006 | -4.4% | 34 | 34 |
| hwp_reference | 열기 | 2.019 | 1.370 | -32.1% | 6 | 6 |
| hwp_reference | 원본 쪽 구성 기록 | 0.007 | 0.003 | -50.7% | 0 | 0 |
| hwp_reference | 자간 조정 후 내어쓰기 | 0.011 | 0.010 | -6.5% | 52 | 52 |
| hwp_reference | 자간 초기화 | 0.073 | 0.032 | -56.7% | 0 | 0 |
| hwp_reference | 저장 | 0.602 | 0.468 | -22.2% | 14 | 14 |
| hwp_reference | 저장 준비(쪽 보호·쪽 나누기 해제) | 0.029 | 0.017 | -44.1% | 23 | 23 |
| hwp_reference | 저장 후 규칙 검수 | 0.000 | 0.000 | +0.0% | 0 | 0 |
| hwp_reference | 제목·개요 가로 크기 | 0.192 | 0.074 | -61.5% | 18 | 18 |
| hwp_reference | 제목·개요·붙임 선행 서식 | 0.095 | 0.044 | -53.4% | 0 | 0 |
| hwp_reference | 준말 변환 | 0.040 | 0.016 | -60.6% | 0 | 0 |
| hwp_reference | 준비 | 0.000 | 0.000 | +0.0% | 0 | 0 |
| hwp_reference | 짧은 마지막 줄 병합 | 0.003 | 0.002 | -25.0% | 16 | 16 |
| hwp_reference | 쪽 보호 해제 | 0.052 | 0.037 | -28.9% | 23 | 23 |
| hwp_reference | 최종 내어쓰기 재확인 | 0.021 | 0.015 | -29.8% | 74 | 74 |
| hwp_reference | 최종 서식 기준 내어쓰기 | 0.016 | 0.025 | +59.6% | 72 | 72 |
| hwp_reference | 텍스트 표(박스 그림) 변환 | 0.497 | 0.367 | -26.3% | 28 | 28 |
| hwp_reference | 표 칸 너비 | 2.378 | 2.197 | -7.6% | 2 | 2 |
| hwp_reference | 표 테두리 선 굵기 통일 | 0.001 | 0.002 | +166.7% | 4 | 4 |
| hwp_reference | 표/컨트롤 자간·단어 분리 조정 | 55.537 | 27.564 | -50.4% | 30174 | 24090 |
| hwp_reference | 표/컨트롤 줄 병합 | 15.091 | 12.263 | -18.7% | 11368 | 11368 |
| hwp_reference | 한 칸 표 영역 조사 | 1.273 | 0.818 | -35.8% | 206 | 206 |
| hwp_reference | **전체** | **87.194** | **52.541** | **-39.7%** | **43702** | **37552** |
| hwpx_continuation | 개별 문단 페이지 배치 | 0.336 | 0.242 | -27.9% | 246 | 246 |
| hwpx_continuation | 개요·한 칸 표 자간 조정 | 0.010 | 0.009 | -10.8% | 10 | 10 |
| hwpx_continuation | 검수 집계 | 0.000 | 0.000 | — | 0 | 0 |
| hwpx_continuation | 공백·문장부호·라벨(XML) | 2.469 | 2.350 | -4.8% | 8 | 8 |
| hwpx_continuation | 기본 표 서식 | 0.017 | 0.014 | -17.9% | 0 | 0 |
| hwpx_continuation | 무결성·숫자 대조 | 0.000 | 0.000 | -100.0% | 0 | 0 |
| hwpx_continuation | 문단 아래 간격 페이지 맞춤 | 0.147 | 0.112 | -24.4% | 93 | 93 |
| hwpx_continuation | 별표 위첨자 | 0.035 | 0.033 | -6.8% | 0 | 0 |
| hwpx_continuation | 보고서 표준서식 | 6.109 | 5.671 | -7.2% | 616 | 616 |
| hwpx_continuation | 본문 자간·단어 분리 조정 | 0.214 | 0.123 | -42.4% | 226 | 160 |
| hwpx_continuation | 부연설명 들여쓰기 | 0.168 | 0.128 | -24.0% | 126 | 126 |
| hwpx_continuation | 셀 너비 본문 맞춤 | 0.005 | 0.006 | +28.6% | 4 | 4 |
| hwpx_continuation | 셀 안쪽 여백 강제 축소 | 0.007 | 0.009 | +39.4% | 4 | 4 |
| hwpx_continuation | 연결 부호 맞춤 | 0.278 | 0.119 | -57.4% | 118 | 118 |
| hwpx_continuation | 열기 | 0.410 | 0.331 | -19.4% | 2 | 2 |
| hwpx_continuation | 원본 쪽 구성 기록 | 0.005 | 0.004 | -27.3% | 0 | 0 |
| hwpx_continuation | 자간 초기화 | 0.031 | 0.024 | -21.4% | 0 | 0 |
| hwpx_continuation | 저장 | 0.582 | 0.358 | -38.4% | 14 | 14 |
| hwpx_continuation | 저장 준비(쪽 보호·쪽 나누기 해제) | 0.155 | 0.085 | -45.1% | 74 | 74 |
| hwpx_continuation | 저장 후 규칙 검수 | 0.000 | 0.000 | +0.0% | 0 | 0 |
| hwpx_continuation | 제목·개요 가로 크기 | 0.107 | 0.089 | -16.1% | 18 | 18 |
| hwpx_continuation | 제목·개요·붙임 선행 서식 | 0.036 | 0.032 | -11.2% | 0 | 0 |
| hwpx_continuation | 준말 변환 | 0.027 | 0.026 | -3.7% | 0 | 0 |
| hwpx_continuation | 준비 | 0.104 | 0.069 | -34.4% | 0 | 0 |
| hwpx_continuation | 짧은 마지막 줄 병합 | 0.201 | 0.146 | -27.7% | 178 | 178 |
| hwpx_continuation | 쪽 보호 해제 | 0.098 | 0.091 | -7.1% | 74 | 74 |
| hwpx_continuation | 최종 내어쓰기 재확인 | 2.021 | 1.234 | -38.9% | 582 | 582 |
| hwpx_continuation | 최종 서식 기준 내어쓰기 | 1.452 | 1.249 | -14.0% | 580 | 580 |
| hwpx_continuation | 텍스트 표(박스 그림) 변환 | 0.704 | 0.515 | -26.9% | 94 | 94 |
| hwpx_continuation | 표 칸 너비 | 2.035 | 2.004 | -1.5% | 2 | 2 |
| hwpx_continuation | 표 테두리 선 굵기 통일 | 0.008 | 0.006 | -27.8% | 4 | 4 |
| hwpx_continuation | 표/컨트롤 자간·단어 분리 조정 | 0.663 | 0.468 | -29.3% | 16 | 16 |
| hwpx_continuation | 표/컨트롤 줄 병합 | 0.015 | 0.012 | -19.0% | 12 | 12 |
| hwpx_continuation | 한 칸 표 영역 조사 | 0.016 | 0.032 | +100.6% | 9 | 9 |
| hwpx_continuation | **전체** | **22.612** | **18.367** | **-18.8%** | **3110** | **3044** |
| hwpx_shapes | 개별 문단 페이지 배치 | 0.102 | 0.055 | -46.0% | 66 | 66 |
| hwpx_shapes | 개요·한 칸 표 자간 조정 | 0.003 | 0.003 | -18.8% | 10 | 10 |
| hwpx_shapes | 검수 집계 | 0.000 | 0.000 | — | 0 | 0 |
| hwpx_shapes | 공백·문장부호·라벨(XML) | 2.782 | 2.442 | -12.2% | 8 | 8 |
| hwpx_shapes | 기본 표 서식 | 0.031 | 0.015 | -51.6% | 0 | 0 |
| hwpx_shapes | 무결성·숫자 대조 | 0.000 | 0.000 | — | 0 | 0 |
| hwpx_shapes | 문단 아래 간격 페이지 맞춤 | 0.293 | 0.087 | -70.4% | 85 | 85 |
| hwpx_shapes | 별표 위첨자 | 0.088 | 0.047 | -46.9% | 0 | 0 |
| hwpx_shapes | 보고서 표준서식 | 1.281 | 0.915 | -28.5% | 401 | 401 |
| hwpx_shapes | 본문 자간·단어 분리 조정 | 0.027 | 0.014 | -47.8% | 140 | 74 |
| hwpx_shapes | 부연설명 들여쓰기 | 0.008 | 0.009 | +8.5% | 36 | 36 |
| hwpx_shapes | 셀 너비 본문 맞춤 | 0.001 | 0.001 | +0.0% | 4 | 4 |
| hwpx_shapes | 셀 안쪽 여백 강제 축소 | 0.001 | 0.001 | +7.7% | 4 | 4 |
| hwpx_shapes | 연결 부호 맞춤 | 0.007 | 0.006 | -9.9% | 34 | 34 |
| hwpx_shapes | 열기 | 0.487 | 0.358 | -26.5% | 2 | 2 |
| hwpx_shapes | 원본 쪽 구성 기록 | 0.007 | 0.004 | -42.9% | 0 | 0 |
| hwpx_shapes | 자간 초기화 | 0.068 | 0.032 | -53.5% | 0 | 0 |
| hwpx_shapes | 저장 | 0.541 | 0.386 | -28.5% | 14 | 14 |
| hwpx_shapes | 저장 준비(쪽 보호·쪽 나누기 해제) | 0.021 | 0.013 | -36.5% | 23 | 23 |
| hwpx_shapes | 저장 후 규칙 검수 | 0.000 | 0.000 | +0.0% | 0 | 0 |
| hwpx_shapes | 제목·개요 가로 크기 | 0.467 | 0.261 | -44.1% | 18 | 18 |
| hwpx_shapes | 제목·개요·붙임 선행 서식 | 0.104 | 0.048 | -54.0% | 0 | 0 |
| hwpx_shapes | 준말 변환 | 0.029 | 0.013 | -53.9% | 0 | 0 |
| hwpx_shapes | 준비 | 0.125 | 0.073 | -41.3% | 0 | 0 |
| hwpx_shapes | 짧은 마지막 줄 병합 | 0.004 | 0.003 | -18.4% | 16 | 16 |
| hwpx_shapes | 쪽 보호 해제 | 0.022 | 0.014 | -37.8% | 23 | 23 |
| hwpx_shapes | 최종 내어쓰기 재확인 | 0.018 | 0.013 | -30.4% | 74 | 74 |
| hwpx_shapes | 최종 서식 기준 내어쓰기 | 0.018 | 0.017 | -6.6% | 72 | 72 |
| hwpx_shapes | 텍스트 표(박스 그림) 변환 | 0.848 | 0.562 | -33.7% | 28 | 28 |
| hwpx_shapes | 표 칸 너비 | 2.161 | 2.066 | -4.4% | 2 | 2 |
| hwpx_shapes | 표 테두리 선 굵기 통일 | 0.001 | 0.001 | -15.4% | 4 | 4 |
| hwpx_shapes | 표/컨트롤 자간·단어 분리 조정 | 1.176 | 0.896 | -23.8% | 1304 | 908 |
| hwpx_shapes | 표/컨트롤 줄 병합 | 0.309 | 0.228 | -26.2% | 364 | 364 |
| hwpx_shapes | 한 칸 표 영역 조사 | 0.101 | 0.059 | -41.6% | 57 | 57 |
| hwpx_shapes | **전체** | **15.710** | **11.522** | **-26.7%** | **2789** | **2327** |
| hwpx_table | 열기 | 0.096 | 0.067 | -29.9% | 2 | 2 |
| hwpx_table | 준비 | 0.058 | 0.047 | -18.9% | 0 | 0 |
| hwpx_table | **전체** | **4.117** | **2.727** | **-33.8%** | **2** | **2** |
| txt_long | 개별 문단 페이지 배치 | 29.171 | 18.695 | -35.9% | 5402 | 5402 |
| txt_long | 개요·한 칸 표 자간 조정 | 13.136 | 6.858 | -47.8% | 4176 | 3574 |
| txt_long | 검수 집계 | 0.000 | 0.000 | — | 0 | 0 |
| txt_long | 공백·문장부호·라벨(XML) | 2.790 | 2.584 | -7.4% | 8 | 8 |
| txt_long | 기본 표 서식 | 0.029 | 0.021 | -29.8% | 0 | 0 |
| txt_long | 무결성·숫자 대조 | 0.003 | 0.004 | +15.6% | 0 | 0 |
| txt_long | 문단 아래 간격 페이지 맞춤 | 121.382 | 78.752 | -35.1% | 27322 | 27322 |
| txt_long | 변환 | 21.970 | 5.490 | -75.0% | 497 | 497 |
| txt_long | 별표 위첨자 | 0.082 | 0.034 | -58.4% | 0 | 0 |
| txt_long | 보고서 표준서식 | 21.131 | 14.795 | -30.0% | 5463 | 5463 |
| txt_long | 본문 자간·단어 분리 조정 | 73.789 | 39.359 | -46.7% | 25976 | 21986 |
| txt_long | 본문 자간·단어 분리 조정 (2/3차) | 9.331 | 5.377 | -42.4% | 4294 | 3510 |
| txt_long | 부연설명 들여쓰기 | 2.707 | 1.829 | -32.5% | 1116 | 1116 |
| txt_long | 셀 너비 본문 맞춤 | 0.004 | 0.008 | +81.0% | 4 | 4 |
| txt_long | 셀 안쪽 여백 강제 축소 | 0.006 | 0.014 | +118.5% | 4 | 4 |
| txt_long | 연결 부호 맞춤 | 2.339 | 1.814 | -22.5% | 1042 | 1042 |
| txt_long | 연결 부호 맞춤 (2/3차) | 2.648 | 1.655 | -37.5% | 1042 | 1042 |
| txt_long | 열기 | 0.625 | 0.261 | -58.3% | 2 | 2 |
| txt_long | 원본 쪽 구성 기록 | 0.008 | 0.004 | -52.4% | 0 | 0 |
| txt_long | 자간 조정 후 내어쓰기 | 5.829 | 4.917 | -15.6% | 2116 | 2116 |
| txt_long | 자간 조정 후 별표(**) 정렬 | 2.805 | 1.783 | -36.5% | 1116 | 1116 |
| txt_long | 자간 조정 후 부연설명 들여쓰기 | 2.375 | 1.590 | -33.0% | 1116 | 1116 |
| txt_long | 자간 초기화 | 0.052 | 0.029 | -44.4% | 0 | 0 |
| txt_long | 저장 | 0.622 | 0.520 | -16.5% | 14 | 14 |
| txt_long | 저장 준비(쪽 보호·쪽 나누기 해제) | 0.873 | 0.747 | -14.4% | 635 | 635 |
| txt_long | 저장 후 규칙 검수 | 0.000 | 0.000 | +0.0% | 0 | 0 |
| txt_long | 제목·개요 가로 크기 | 0.245 | 0.112 | -54.5% | 18 | 18 |
| txt_long | 제목·개요·붙임 선행 서식 | 0.183 | 0.087 | -52.6% | 0 | 0 |
| txt_long | 준말 변환 | 0.069 | 0.020 | -71.1% | 0 | 0 |
| txt_long | 준비 | 0.009 | 0.000 | -98.9% | 0 | 0 |
| txt_long | 짧은 마지막 줄 병합 | 2.835 | 2.323 | -18.1% | 1882 | 1882 |
| txt_long | 짧은 마지막 줄 병합 (2/3차) | 2.435 | 1.823 | -25.1% | 1650 | 1650 |
| txt_long | 쪽 보호 해제 | 1.145 | 0.745 | -34.9% | 635 | 635 |
| txt_long | 최종 내어쓰기 재확인 | 33.268 | 19.502 | -41.4% | 5186 | 5186 |
| txt_long | 최종 서식 기준 내어쓰기 | 35.959 | 19.239 | -46.5% | 5184 | 5184 |
| txt_long | 텍스트 표(박스 그림) 변환 | 2.995 | 2.154 | -28.1% | 820 | 820 |
| txt_long | 표 칸 너비 | 2.206 | 2.095 | -5.0% | 2 | 2 |
| txt_long | 표 테두리 선 굵기 통일 | 0.004 | 0.005 | +18.6% | 4 | 4 |
| txt_long | 표/컨트롤 자간·단어 분리 조정 | 15.646 | 10.684 | -31.7% | 4428 | 4056 |
| txt_long | 표/컨트롤 자간·단어 분리 조정 (2/3차) | 1.171 | 0.611 | -47.8% | 36 | 36 |
| txt_long | 표/컨트롤 줄 병합 | 0.304 | 0.215 | -29.4% | 32 | 32 |
| txt_long | 표/컨트롤 줄 병합 (2/3차) | 0.274 | 0.272 | -0.6% | 32 | 32 |
| txt_long | 한 칸 표 영역 조사 | 0.756 | 0.600 | -20.7% | 30 | 30 |
| txt_long | **전체** | **425.150** | **250.360** | **-41.1%** | **101284** | **95536** |
| txt_short | 개별 문단 페이지 배치 | 1.182 | 0.224 | -81.1% | 246 | 246 |
| txt_short | 개요·한 칸 표 자간 조정 | 0.015 | 0.009 | -37.7% | 10 | 10 |
| txt_short | 검수 집계 | 0.000 | 0.000 | -100.0% | 0 | 0 |
| txt_short | 공백·문장부호·라벨(XML) | 3.633 | 2.333 | -35.8% | 8 | 8 |
| txt_short | 기본 표 서식 | 0.043 | 0.016 | -63.0% | 0 | 0 |
| txt_short | 무결성·숫자 대조 | 0.007 | 0.004 | -47.0% | 0 | 0 |
| txt_short | 문단 아래 간격 페이지 맞춤 | 0.422 | 0.109 | -74.2% | 93 | 93 |
| txt_short | 변환 | 9.389 | 2.449 | -73.9% | 62 | 62 |
| txt_short | 별표 위첨자 | 0.188 | 0.037 | -80.3% | 0 | 0 |
| txt_short | 보고서 표준서식 | 12.888 | 4.626 | -64.1% | 771 | 771 |
| txt_short | 본문 자간·단어 분리 조정 | 0.887 | 0.147 | -83.4% | 226 | 160 |
| txt_short | 부연설명 들여쓰기 | 0.327 | 0.139 | -57.5% | 126 | 126 |
| txt_short | 셀 너비 본문 맞춤 | 0.026 | 0.005 | -80.8% | 4 | 4 |
| txt_short | 셀 안쪽 여백 강제 축소 | 0.131 | 0.007 | -94.7% | 4 | 4 |
| txt_short | 연결 부호 맞춤 | 0.224 | 0.136 | -39.2% | 118 | 118 |
| txt_short | 원본 쪽 구성 기록 | 0.045 | 0.009 | -81.3% | 0 | 0 |
| txt_short | 자간 초기화 | 0.118 | 0.021 | -82.2% | 0 | 0 |
| txt_short | 저장 | 0.876 | 0.361 | -58.8% | 14 | 14 |
| txt_short | 저장 준비(쪽 보호·쪽 나누기 해제) | 0.343 | 0.075 | -78.2% | 74 | 74 |
| txt_short | 저장 후 규칙 검수 | 0.000 | 0.000 | +0.0% | 0 | 0 |
| txt_short | 제목·개요 가로 크기 | 3.337 | 0.086 | -97.4% | 18 | 18 |
| txt_short | 제목·개요·붙임 선행 서식 | 0.130 | 0.045 | -65.1% | 0 | 0 |
| txt_short | 준말 변환 | 0.026 | 0.010 | -61.8% | 0 | 0 |
| txt_short | 준비 | 0.000 | 0.000 | -50.0% | 0 | 0 |
| txt_short | 짧은 마지막 줄 병합 | 0.928 | 0.168 | -81.9% | 178 | 178 |
| txt_short | 쪽 보호 해제 | 0.491 | 0.079 | -83.9% | 74 | 74 |
| txt_short | 최종 내어쓰기 재확인 | 5.223 | 1.225 | -76.5% | 582 | 582 |
| txt_short | 최종 서식 기준 내어쓰기 | 6.387 | 1.240 | -80.6% | 580 | 580 |
| txt_short | 텍스트 표(박스 그림) 변환 | 1.463 | 0.553 | -62.2% | 94 | 94 |
| txt_short | 표 칸 너비 | 2.842 | 2.065 | -27.3% | 2 | 2 |
| txt_short | 표 테두리 선 굵기 통일 | 0.015 | 0.005 | -65.5% | 4 | 4 |
| txt_short | 표/컨트롤 자간·단어 분리 조정 | 1.422 | 0.464 | -67.3% | 16 | 16 |
| txt_short | 표/컨트롤 줄 병합 | 0.023 | 0.011 | -52.0% | 12 | 12 |
| txt_short | 한 칸 표 영역 조사 | 0.041 | 0.032 | -21.2% | 9 | 9 |
| txt_short | **전체** | **83.883** | **19.721** | **-76.5%** | **3325** | **3259** |

## 결과 문서 동일성

서식 동일성은 `paraPr` 정의 내용으로 ID 참조를 풀고, 정의 순서를 정렬한 뒤 비교한다. 다른 서식 값·요소 순서는 그대로 비교하며, 참조되지 않는 표 `id`만 해시에서 제외한다. 원시 서식 해시는 진단용으로 함께 표시한다.

| 문서 | 문단 수 전→후 | 변경 문단 | 텍스트 XML 해시 | 원시 서식 해시 | 정규화 서식 해시 |
|---|---:|---:|---|---|---|
| hwp_reference | 16→16 | 0 | 일치 | 일치 | 일치 |
| hwpx_continuation | 4→4 | 0 | 일치 | 일치 | 일치 |
| hwpx_shapes | 7→7 | 0 | 일치 | 일치 | 일치 |
| hwpx_table | — | 측정 불가 | 측정 불가 | 측정 불가 |
| txt_long | 38→38 | 0 | 일치 | 일치 | 일치 |
| txt_short | 4→4 | 0 | 일치 | 일치 | 일치 |

## 자동 기능 검수

| 문서 | 항목 | 판정 |
|---|---|---|
| before/hwp_reference | 원본 해시 불변 | PASS |
| before/hwp_reference | 처리 사본 해시 불변 | PASS |
| before/hwp_reference | 오류 0 | PASS |
| before/hwp_reference | 결과 저장 | PASS |
| before/hwp_reference | 원문 텍스트 보존(공백 무시) | FAIL |
| before/hwp_reference | 문단 수 기록 | PASS |
| before/hwp_reference | 쪽 수 기록 | PASS |
| before/hwp_reference | 선택된 단계 실행 로그 | PASS |
| before/hwpx_continuation | 원본 해시 불변 | PASS |
| before/hwpx_continuation | 처리 사본 해시 불변 | PASS |
| before/hwpx_continuation | 오류 0 | PASS |
| before/hwpx_continuation | 결과 저장 | PASS |
| before/hwpx_continuation | 원문 텍스트 보존(공백 무시) | PASS |
| before/hwpx_continuation | 문단 수 기록 | PASS |
| before/hwpx_continuation | 쪽 수 기록 | PASS |
| before/hwpx_continuation | 선택된 단계 실행 로그 | PASS |
| before/hwpx_shapes | 원본 해시 불변 | PASS |
| before/hwpx_shapes | 처리 사본 해시 불변 | PASS |
| before/hwpx_shapes | 오류 0 | PASS |
| before/hwpx_shapes | 결과 저장 | PASS |
| before/hwpx_shapes | 원문 텍스트 보존(공백 무시) | PASS |
| before/hwpx_shapes | 문단 수 기록 | PASS |
| before/hwpx_shapes | 쪽 수 기록 | PASS |
| before/hwpx_shapes | 선택된 단계 실행 로그 | PASS |
| before/hwpx_table | 원본 해시 불변 | PASS |
| before/hwpx_table | 처리 사본 해시 불변 | PASS |
| before/hwpx_table | 오류 0 | FAIL |
| before/hwpx_table | 결과 저장 | FAIL |
| before/hwpx_table | 원문 텍스트 보존(공백 무시) | FAIL |
| before/hwpx_table | 문단 수 기록 | UNMEASURED |
| before/hwpx_table | 쪽 수 기록 | UNMEASURED |
| before/hwpx_table | 선택된 단계 실행 로그 | FAIL |
| before/txt_long | 원본 해시 불변 | PASS |
| before/txt_long | 처리 사본 해시 불변 | PASS |
| before/txt_long | 오류 0 | PASS |
| before/txt_long | 결과 저장 | PASS |
| before/txt_long | 원문 텍스트 보존(공백 무시) | FAIL |
| before/txt_long | 문단 수 기록 | PASS |
| before/txt_long | 쪽 수 기록 | PASS |
| before/txt_long | 선택된 단계 실행 로그 | PASS |
| before/txt_short | 원본 해시 불변 | PASS |
| before/txt_short | 처리 사본 해시 불변 | PASS |
| before/txt_short | 오류 0 | PASS |
| before/txt_short | 결과 저장 | PASS |
| before/txt_short | 원문 텍스트 보존(공백 무시) | FAIL |
| before/txt_short | 문단 수 기록 | PASS |
| before/txt_short | 쪽 수 기록 | PASS |
| before/txt_short | 선택된 단계 실행 로그 | PASS |
| after5/hwp_reference | 원본 해시 불변 | PASS |
| after5/hwp_reference | 처리 사본 해시 불변 | PASS |
| after5/hwp_reference | 오류 0 | PASS |
| after5/hwp_reference | 결과 저장 | PASS |
| after5/hwp_reference | 원문 텍스트 보존(공백 무시) | FAIL |
| after5/hwp_reference | 문단 수 기록 | PASS |
| after5/hwp_reference | 쪽 수 기록 | PASS |
| after5/hwp_reference | 선택된 단계 실행 로그 | PASS |
| after5/hwpx_continuation | 원본 해시 불변 | PASS |
| after5/hwpx_continuation | 처리 사본 해시 불변 | PASS |
| after5/hwpx_continuation | 오류 0 | PASS |
| after5/hwpx_continuation | 결과 저장 | PASS |
| after5/hwpx_continuation | 원문 텍스트 보존(공백 무시) | PASS |
| after5/hwpx_continuation | 문단 수 기록 | PASS |
| after5/hwpx_continuation | 쪽 수 기록 | PASS |
| after5/hwpx_continuation | 선택된 단계 실행 로그 | PASS |
| after5/hwpx_shapes | 원본 해시 불변 | PASS |
| after5/hwpx_shapes | 처리 사본 해시 불변 | PASS |
| after5/hwpx_shapes | 오류 0 | PASS |
| after5/hwpx_shapes | 결과 저장 | PASS |
| after5/hwpx_shapes | 원문 텍스트 보존(공백 무시) | PASS |
| after5/hwpx_shapes | 문단 수 기록 | PASS |
| after5/hwpx_shapes | 쪽 수 기록 | PASS |
| after5/hwpx_shapes | 선택된 단계 실행 로그 | PASS |
| after5/hwpx_table | 원본 해시 불변 | PASS |
| after5/hwpx_table | 처리 사본 해시 불변 | PASS |
| after5/hwpx_table | 오류 0 | FAIL |
| after5/hwpx_table | 결과 저장 | FAIL |
| after5/hwpx_table | 원문 텍스트 보존(공백 무시) | FAIL |
| after5/hwpx_table | 문단 수 기록 | UNMEASURED |
| after5/hwpx_table | 쪽 수 기록 | UNMEASURED |
| after5/hwpx_table | 선택된 단계 실행 로그 | FAIL |
| after5/txt_long | 원본 해시 불변 | PASS |
| after5/txt_long | 처리 사본 해시 불변 | PASS |
| after5/txt_long | 오류 0 | PASS |
| after5/txt_long | 결과 저장 | PASS |
| after5/txt_long | 원문 텍스트 보존(공백 무시) | FAIL |
| after5/txt_long | 문단 수 기록 | PASS |
| after5/txt_long | 쪽 수 기록 | PASS |
| after5/txt_long | 선택된 단계 실행 로그 | PASS |
| after5/txt_short | 원본 해시 불변 | PASS |
| after5/txt_short | 처리 사본 해시 불변 | PASS |
| after5/txt_short | 오류 0 | PASS |
| after5/txt_short | 결과 저장 | PASS |
| after5/txt_short | 원문 텍스트 보존(공백 무시) | FAIL |
| after5/txt_short | 문단 수 기록 | PASS |
| after5/txt_short | 쪽 수 기록 | PASS |
| after5/txt_short | 선택된 단계 실행 로그 | PASS |

## 결함 목록

- hwpx_table: 결과 HWPX 누락
- before/hwp_reference: 원문 텍스트 보존(공백 무시) FAIL
- before/hwpx_table: 오류 0 FAIL
- before/hwpx_table: 결과 저장 FAIL
- before/hwpx_table: 원문 텍스트 보존(공백 무시) FAIL
- before/hwpx_table: 문단 수 기록 UNMEASURED
- before/hwpx_table: 쪽 수 기록 UNMEASURED
- before/hwpx_table: 선택된 단계 실행 로그 FAIL
- before/txt_long: 원문 텍스트 보존(공백 무시) FAIL
- before/txt_short: 원문 텍스트 보존(공백 무시) FAIL
- after5/hwp_reference: 원문 텍스트 보존(공백 무시) FAIL
- after5/hwpx_table: 오류 0 FAIL
- after5/hwpx_table: 결과 저장 FAIL
- after5/hwpx_table: 원문 텍스트 보존(공백 무시) FAIL
- after5/hwpx_table: 문단 수 기록 UNMEASURED
- after5/hwpx_table: 쪽 수 기록 UNMEASURED
- after5/hwpx_table: 선택된 단계 실행 로그 FAIL
- after5/txt_long: 원문 텍스트 보존(공백 무시) FAIL
- after5/txt_short: 원문 텍스트 보존(공백 무시) FAIL
