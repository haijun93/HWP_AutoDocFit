# RUSTVIEW2 결과 (2026-11-26)

브랜치: `feature/RUSTVIEW2`. 설정·파일변환·파일명 창을 Rust+Win32 별도 창으로 확장했다. **로컬 커밋만 하며 푸시·태그·배포·설치·기본 실행 방식 변경은 하지 않는다.** Flutter 화면은 보존했다.

## 선행 병합·비교

- `git merge github/main`: 실제 출력 `Already up to date.` 기준 HEAD는 `20b3042d`(v2.2.107).
- `git merge feature/SIDEMERGE1`: TODO와 부트스트랩 버전 줄 충돌 2곳. 양쪽 작업 기록과 v2.2.107을 보존해 `a5c35bb9`로 병합했다. `feature/RUSTSIDE2`가 HEAD의 조상임을 종료 코드 0으로 확인했다.
- 병합 직후 지정된 `tests.test_default_library_scan`, `tests.test_library_exclusion_settings`: `Ran 5 tests in 16.338s` / `OK (skipped=1)`.
- 구현 전에 [기능 비교표](RUSTVIEW2_COMPARE_2026-11-26.md)를 작성했다. 주 창 내용 영역은 추가하지 않았다. 사이드바가 여는 기존 메뉴별 실행 파일을 확장했다.

## 최종 기능 대조

| 대상 | 구현 결과 | 확인 범위 |
|---|---|---|
| 설정 기본 화면 | 기존 분류·검색·입력 종류·저장·되돌리기·revision 충돌 보호 유지 | 기존 Rust 32시험, 관련 Python, 실제 창 |
| 설정 관리 | 관리 단추 → 글자 상용구·본문 상용구·가상 폴더·앱 정보·창/검색 5개 관리 화면 | 실제 합성 IDO 저장, 합성 HWPX 조회, 앱 정보·창/검색 조회 |
| 상용구 | 기존 IdiomService 사용. HWPX가 원본이면 직접 열기·읽기 전용, IDO는 추가·편집·삭제·내장 항목 가져오기, 본문 HWP 등록 연결 | IDO 저장·원본 HWPX 보존 확인. 실제 한/글 반영은 미시험 |
| 설정 전용 처리 2키 | `always_on_top`, `mongttang_search`를 관리 창에서 편집. 열린 Rust 창 표시 상태 반영, 검색 프로그램 처리 성공 후 설정 저장, 다른 창 저장 충돌 거부 | 서비스는 모의 객체로 성공·실패·경합 시험. 실제 서비스/자동 실행 변경 안 함 |
| 글꼴 | 기존 Windows 글꼴에 기존 Python 한/글 글꼴 카탈로그를 비동기로 합침 | 읽기 전용 요청·기존 컨트롤 시험 |
| 변환 출력 | HWPX·PDF·PDF 병합·PNG·JPG·TXT·MD + PDF 압축·이미지 PDF·DOCX·EPUB·HTML, 총 12종 | 형식/클릭 영역 시험, 실제 합성 PDF 병합 |
| 변환 입력·결과 | 파일 선택·드롭·제거·비우기·휠 스크롤·병합 순서(Alt+↑/↓), 저장 위치·압축 용량/방식, 실제 결과 경로·실패/건너뜀·결과/폴더 열기 | 실제 합성 PDF 2개 순서 보존, 원본 바이트 보존 |
| 변환 중단·닫기 | 협력 취소, 현재 작업 정리 후 닫기. 작업 중 수신한 파일은 대기시킨 뒤 목록에 추가 | Python 합성 취소 시험. 실제 COM 중단은 미시험 |
| 파일명 규칙 | 앱의 23종 규칙별 기본값·체크·선택지·숫자·여러 줄 입력, 순서·켜기/끄기, 최근 치환 | 실제 23종 폼 생성 확인, 기존 규칙 회귀 시험 |
| 파일명 목록 | 파일/폴더 펼치기·폴더 자체·재귀·숨김 포함, 선택 제거·비우기·정렬 5종 양방향·파일 순서 변경, 직접 이름·제외/복원 | 기존 서비스 공유, 합성 목록/계획 시험 |
| 파일명 적용·보관 | 충돌 차단/번호, 미리보기 확인 후 적용, 중단·실패/제외 요약, 앱과 같은 이력·선택 이력 되돌리기, 규칙 세트 저장/삭제/가져오기/내보내기 | 실제 이름 변경→이력→되돌리기, 세트 왕복 시험 |

파일명 규칙 23종: 문자열 치환, 앞/뒤 추가, UUID 이름 치환, 날짜, EXIF, 번호 붙이기, 번호 증감, 숫자 자리 맞춤, 위치 삭제, 괄호 구간 삭제, 숫자만 남기기, 숫자 삭제, 이름 지우기, 결과 접미사 정리, 문서 제목, 이름 틀, 공백/전각 정리, 대소문자, 이름 목록, 확장자 대소문자/교체/추가/삭제.

## 작업 통신·상태

- 새 `native_tasks` Rust 공통 크레이트와 `docfit_app/native_tasks.py`. Rust 창이 화면 없는 Python 자식 프로세스에 제한된 JSON 명령을 상속된 stdio로 보낸다. 임의 셸 실행 API는 없다. 잘못된 JSON·과대 요청·미등록 명령을 거부한다.
- RUSTSIDE2의 기존 창 식별자·변환 열기 파이프·파일 전달을 유지한다. 전달받기만으로 실행하지 않는다. 실제 변환·이름 변경은 기존 서비스와 JobManager가 맡는다. Python 부트스트랩은 Tk·WebView·상주 작업 전에 분기한다. windowed EXE의 `sys.stdin/stdout=None`도 상속된 표준 핸들로 복원한다.
- 창 상태는 `Idle / Requesting / Running / Stopping / Closing`. 진행 중 편집·중복 요청을 막고, 중단 요청은 상태 조회 중에도 보존한다. 닫기 취소 시 초안을 유지한다. 파일명 드롭은 요청 중에도 대기시킨다.
- `DOCFIT_NATIVE_TRACE`를 명시한 개발 실행에서만 JSONL 관측 기록을 남긴다. 기본 실행에서는 쓰지 않는다. 합성 시험은 이 기록으로 23종 폼의 종류·필드 수를 대조했다.
- 변환 성공을 종료 코드로 추측하지 않는다. Python이 반환한 실제 결과 경로를 사용하며, PDF 병합 결과는 모든 입력 행에 같은 실제 출력으로 표시한다. 부분 취소에서도 완료한 결과를 보존한다.
- PDF 압축 실행기가 명시된 `pdf_target_bytes`를 무시하던 기존 오류를 수정했다. 합성 모의 압축기에서 3 MiB 전달을 확인했다.
- 부트스트랩 버전은 기존 v2.2.107 한 곳으로 통일했다. 버전 올림은 하지 않았다.

## 최종 시험·실제 출력 줄

MinGW와 카드에서 지정한 `CARGO_TARGET_DIR` 환경변수를 사용했다. 저장소 설정에 빌드 경로를 넣지 않았다. 기존 시험은 수정하지 않았다.

| 명령 | 최종 결과 |
|---|---:|
| `cargo test --manifest-path native_tasks/Cargo.toml` | 5 통과 |
| `cargo test --manifest-path convert_win/Cargo.toml` | 18 통과 |
| `cargo test --manifest-path settings_win/Cargo.toml` | 17 + 15 = 32 통과 |
| `cargo test --manifest-path rename_win/Cargo.toml` | 39 통과 |
| 관련 Python 28모듈 | 271건 중 269 통과·2 조건부 제외 |
| 실제 Win32 합성 시험 | 아래 3개 PASS |

변경 Rust 크레이트 **4개 전체 94시험 통과**, 문서 시험 실패 0. 실제 출력:

```text
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s
Ran 271 tests in 43.159s
OK (skipped=2)
rename: 23 native rule forms + drop/preview/apply/history/undo + dirty close protection PASS
settings: isolated config unchanged + synthetic IDO save + HWPX readonly management/about/runtime settings read PASS
convert: real native 12 targets + ordered PDF merge + sources unchanged PASS
per_monitor_system_dpi=120
bootstrap_crlf= 3565 lf_only= 0
```

설정 17/15건의 시간은 직전 실행의 실제 출력이다. 마지막 재실행에서도 같은 건수와 실패 0을 확인했다.

Python 대상은 `test_native_tasks`(신규 13건), `test_conversion_stop`, `test_conversion_file_actions`, `test_convert_pipe`, `test_conversion_window_service`, `test_settings_window_service`, `test_settings_compare`, `test_settings_list_keys`, `test_idiom_file_save`, `test_rename_window_service`, `test_file_rename_service`, `test_file_rename_recovery`, `test_file_rename_simple_rules`, `test_file_rename_date_format`, `test_file_rename_number_shift`, `test_file_rename_plus`, `test_file_rename_name_limits`, `test_ipc_job_file_rename`, `test_jobs`, `test_sidebar_launch`, `test_file_conversion`, `test_pdf_compress`, `test_cli`, `test_file_rename_core`, `test_file_rename_rust_preview`, `test_hwp_idioms`, `test_hwpx_idioms_roundtrip`, `test_text_conversion_embedded_runtime`. 제외 2건은 `test_settings_compare`의 독립 Rust 설정 CLI 미배치에 따른 기존 조건이다.

합성 시험 재현: `.venv`의 Python으로 `scripts/smoke_rustview2.py --bin-dir <cargo출력/debug> --out <작업폴더안시험경로>` 실행. APPDATA·LOCALAPPDATA·문서 설정 원본 위치를 시험 폴더로 격리한다. 합성 TXT, 빈 쪽 PDF, 합성 IDO·HWPX만 사용한다. PDF 두 쪽 너비 123→234의 순서, 입력 바이트, 시험 설정 JSON 보존을 확인한다.

초기의 컴파일 누락(Shell Common·EnableWindow 가져오기), 신규 시험의 규칙 세트 첫 항목 고정 가정, Win32 자동화의 원격 EDIT 쓰기 방식·폼 ID 개수 오류는 수정하고 재시험했다. 한국어 소스는 `apply_patch`로 편집해 PowerShell 경유로 일시적으로 생긴 `?` 문자열도 복구했다. 기존 시험 변경·판정 완화는 없다. 합성 불량 PDF 시험의 `incorrect startxref pointer(3)` 출력과 옛 Win32 시험용 함수의 미사용 경고는 남아 있다.

## 실제 화면

8장을 저장했다. 120 DPI(125%)의 실제 Win32 컨트롤을 `PrintWindow`로 캡처하고 설정·관리·파일명·변환의 대표 이미지를 직접 확인했다. 목업이 아니다.

- [설정](RUSTVIEW2_2026-11-26/settings.png)
- [합성 IDO 편집](RUSTVIEW2_2026-11-26/settings-idiom-edit.png) / [HWPX 읽기 전용](RUSTVIEW2_2026-11-26/settings-idioms.png)
- [앱 정보](RUSTVIEW2_2026-11-26/settings-about.png) / [창·검색 설정](RUSTVIEW2_2026-11-26/settings-runtime.png)
- [파일명 미리보기](RUSTVIEW2_2026-11-26/rename-preview.png)
- [변환 전](RUSTVIEW2_2026-11-26/convert-ready.png) / [실제 PDF 병합 결과](RUSTVIEW2_2026-11-26/convert-complete.png)

## 남은 확인·제약

1. **이 PC의 설치본·서비스·자동 실행 등록·사용자 설정을 바꾸지 않았다.** 가상 폴더/검색의 실제 켜기·끄기, 자동 실행 변경, 열린 여러 Rust 창의 최상위 표시 변경은 실행하지 않았다. 서비스는 모의 객체로, 창·검색 화면은 조회만 시험했다.
2. 실제 한/글 COM 처리, 본문 HWP의 한/글 펼치기, 12종 형식의 모든 입출력 조합, 설치형 windowed EXE의 IPC는 미확인이다. 변환 중 COM 호출이 돌아오지 않으면 협력 취소가 대기할 수 있다. COM 시험을 하지 않아 COM 잠금도 잡지 않았다.
3. 물리 IME·키보드/포인터 전체 조작, 다중 모니터 DPI 이동, 화면 읽기, 장시간/대량 자료는 PM 확인이 필요하다. 새 관리·고급 파일명 창은 표준 Win32 색을 사용해 기존 설정 창의 다크 그리기와 외형이 다르다. Flutter의 그림 예시나 날짜 간편 입력 줄은 그대로 복제하지 않고 설명·규칙 폼으로 제공한다.
4. 전체 Python·Flutter·CI/MSVC·설치/업데이트/제거·기본 전환은 카드대로 PM 릴리스 단계에 남긴다. 별도 프로세스 창의 설정 변경 감시를 모든 화면에 추가하지는 않았다.
5. 속도는 측정하지 않았다. 전체 시험 잠금이 없음을 확인했지만 시작 속도·메모리 개선 수치를 주장하지 않는다.

TODO와 PM 인계에 상태를 기록했다. GitLab 접근·푸시·태그·main 병합·공개는 하지 않는다.
