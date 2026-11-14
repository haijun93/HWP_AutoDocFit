# SPD2 자간 기억 재사용 범위 확대 (2026-11-13)

- 작업 브랜치: `perf/spacing-com-design`, 시작점 `d83360e3`.
- 먼저 `github/main`을 병합했다(`6810f631`). 충돌 없음. GitLab 접근 없음.
- 산출물: `E:/docfit-tmp/perf2/spd2-off`, `spd2-on`, `spd2-diff.json`, `spd2-tests.txt`.
- 합격 기준: 시연 문서 사본의 all 결과 구조 차이 0, COM 기록 감소, Python 전체 시험 통과.

## 구현 범위

1단계에는 이미 A(줄범위)·B(한글자)의 정확한 인자별 재사용이 들어 있다.
이번에는 설계 2단계 A의 줄범위 내부 경계 읽기를 확장했다. 다른 커서 위치에서
같은 화면줄을 검사할 때 전체 줄범위는 다시 재되, 실제 `raw_end`와 `next_start`가
둘 다 같은 경계의 `두_위치_사이_줄바꿈_문자인가`만 재사용한다.
이로써 SetPos·MoveSelRight·InitScan/GetText/ReleaseScan·Cancel·복원 SetPos 묶음을 생략한다.
화면줄 시작을 다른 인자의 별칭으로 저장하는 후보 F는 적용하지 않는다.

- 기본 꺼짐 유지. `DOCFIT_SPACING_MEMO=reuse`에서만 생략한다.
- `verify`는 실제로 다시 읽고 불일치를 집계한다.
- 상태는 기존 `없음 → 편집전 → 편집후 → 없음`을 따른다. 편집 후 기억을 다시 채우지 않는다.
- 경계 검사의 후속 코드는 커서 위치를 읽지 않고 결과를 반환하거나 SetPos로 위치를 정한다.
- 공용 page_fit 함수 자체를 변경하지 않아 다른 호출자의 커서 복원 계약을 유지한다.
- 3단계 B의 추가 확대, 장평 필드 읽기 생략(C), 복원 이동 정리(D), 탐색 알고리즘 변경 없음.
- 신규 시험은 별도 파일에 추가했다. 기존 시험 수정 없음, PC 전용 경로 없음.
- `hwp-auto-docfit.py`는 구현에서 수정하지 않았다(main 병합 변경만 포함).

## 측정·결과 동일성

저장소 `release-assets/[모의데이터]문서편집_작업방식(한번에적용) 기능시연용문서.txt`를
`scripts/perf_baseline_pc.py --mode all`로 꺼짐/켜짐 각각 1회 처리한다.
도구가 원본을 복사하고 `C:/HWP_AUTODOCFIT/.com-test.lock`을 잡는다.
두 실행 모두 `DOCFIT_COM_PERF=1`, 같은 코드·같은 옵션이다. 측정 중 다른 시험·빌드를 실행하지 않는다.
COM 기록 수는 메서드 속성 읽기와 호출을 각각 세는 기존 계측기 단위이다.

| 측정 | 전체 COM 기록 | 전체 초 | 본문 자간 1차 COM 기록 |
|---|---:|---:|---:|
| 꺼짐 | 144,274 | 347.485 | 67,086 |
| reuse | 128,702 | 324.037 | 55,752 |

전체 15,572건(10.793%) 감소, 본문 자간 1차 11,334건(16.895%) 감소.
시간은 각 1회 관찰값이며 반복 검증한 속도 보장이 아니다.

이번 원본(5,451바이트)은 1단계 보고서의 사본(5,712바이트)과 다르므로,
1단계 보고서 숫자와 직접 차감해 이번 확장만의 절감량이라고 표현하지 않는다.
이번 꺼짐/켜짐 차이는 기존 1단계 재사용을 포함한 opt-in 전체 효과이다.

## 검증 결과

- `analyze_hwpx_diff.py`: 문단 49→49, `changed_paragraphs=[]`.
- `Contents/header.xml` 및 유일한 구역 `Contents/section0.xml` SHA-256 각각 동일.
  ZIP 자체 크기는 1바이트 다르다. 패키지 내부 차이는 `Preview/PrvImage.png`와
  `Contents/content.hpf`이며, 후자는 생성·수정 시각 및 날짜 문자열만 다르다.
  비교 대상 본문·서식 XML은 바이트까지 같다.
- 두 실행 모두 오류 0, 저장 1개, `original_unchanged=true`, `copy_unchanged=true`.
- TXT 대비 `text_preserved_ignoring_whitespace=false`는 양쪽에 동일하게 기록됐다.
  변환 원문 글자 보존 통과로 표현하지 않고, 원본 파일 불변과 전후 결과 동일성만 판정한다.
- reuse 줄 요약 133개, 적중 741, 실제 검사 867. 불일치 집계 0은 reuse 로그 수치이며
  이번에 verify COM 실행을 추가하지 않았으므로 실제 재읽기 검증 0건으로 일반화하지 않는다.
- 비교 요약: `E:/docfit-tmp/perf2/spd2-comparison.json`.
- 전체 Python: `python -X utf8 -m unittest discover -s tests -v`,
  **2,923건 / 1,098.064초 / OK (skipped=11)**. 측정 완료 뒤 단독 실행했다.
  시험이 재생성한 fixture 바이너리 4개(docx·hwpx·pptx·xlsx)는 HEAD로 복원했다.
- 신규 경계 시험 4건: 서로 다른 줄범위 요청의 공유, 경계 키 분리·편집 후 무효화,
  verify 실제 읽기·불일치 기록, 기본 꺼짐. 초기 관련 시험 9건도 통과했다.
- 본체 CRLF 3,506개, bare LF 0개. `git diff --check` 통과.
- COM 종료 뒤 Hwp 프로세스·잠금 잔존 없음. 다른 회귀 문서 묶음은 이번 범위에서 미실행.
- 기본 활성화 및 설계 3단계 이후 작업은 후속 범위로 남긴다.
