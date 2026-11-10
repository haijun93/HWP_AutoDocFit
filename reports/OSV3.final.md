# OSV3 최종 검증 보고

- 검증일: 2026-11-11 KST (OSV 조회 2026-11-10 15:32:30 UTC).
- 작업 폴더: C:/HWP_AUTODOCFIT/v1803/osv2, 브랜치: fix/osv-quickxml.
- 인수 커밋: e2d954cbb8f35d2bd0f570ebeaac02794023b8e2.
- 최초 git status: 변경 없음. git show --stat HEAD: klib/Cargo.toml 및 Cargo.lock만 변경(3 insertions, 3 deletions).
- 기존 수정 유지. 코드·시험 기준값·동작 변경 없음. Cargo 파일 추가 변경 없음.

## 순차 검증 결과

| 단위 | 실행·확인 | 결과 |
|---|---|---|
| 1 | 추적 중인 전체 Cargo.toml/Cargo.lock 확인 | klib 요구 버전 0.41, 잠금 버전 0.41.0. hierarchy, kagent, kagent/ffi, editor_core에 quick-xml 구버전 참조 없음 |
| 2 | klib에서 cargo test | 단위 시험 50 + 통합 시험 76 = 126 통과, 실패 0, 무시 0, 종료 코드 0 |
| 3a | python tools/kidx_regress.py --exe klib/target/debug/klib.exe | 34파일, mismatches/errors 0, 종료 코드 0 |
| 3b | python tools/kidx_index_regress.py | 34파일, Python/Rust 각각 230문단, document/level/body/query diff 각각 0, 검색 10질의, 종료 코드 0 |
| 4 | https://api.osv.dev/v1/querybatch | klib/Cargo.lock의 registry 패키지 189개 조회, 응답 189개, 추가 페이지 없음. quick-xml 0.41.0 취약점 0건, 전체 조회 취약점 0건 |

MinGW PATH: C:/Users/haiju/AppData/Local/DocFitTools/mingw64/bin.
컴파일 오류 없음. quick-xml decode_and_unescape_value 폐기 예정 경고 및 기존 unused/dead-code 등의 경고는 남아 있으며 동작 보존을 위해 변경하지 않았다.

색인 결과는 33개 indexed, 1개 error. timport_too_wide.hwpx는 본문이 없어 Python에서도 empty 오류를 냈으며 양쪽 색인 결과 차이는 0이다.
지정된 두 Python 회귀 도구의 비교 확장자는 TXT/MD/HWPX/DOCX이다. XLSX/PPTX의 Python 대비 회귀는 해당 도구에 포함되지 않으므로 검증했다고 주장하지 않는다.

OSV 원본 응답과 조회 시각: OSV3.osv.json. 다른 Critical/High 잔여 항목 없음(전체 취약점 응답이 비어 있음).

## 판정 및 푸시

요청된 검증 명령 전체 통과. 실패 시험 없음. git push github fix/osv-quickxml 실행 대상으로 확정.
보고서 및 OSV 증거는 UTF-8, CRLF로 저장했다. 기존 파일 줄바꿈은 변경하지 않았다.
