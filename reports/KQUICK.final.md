# KQUICK 빠른 목록 빈 항목 문제 — 조사 및 수정 결과

작성일: 2026-11-08
브랜치: `fix/kquick-empty` (`github/main` 기준)

## 원인과 근거

- KAgent 감시 흐름은 `WM_CLIPBOARDUPDATE` → `clip::read()` → 작업 스레드의 SQLite 저장입니다. 텍스트와 파일 경로는 각각 `kind=text`, `kind=files`로 저장되고 본문·미리보기는 DPAPI로 암호화됩니다. 파일 경로만 보관하는 정책이 일반 텍스트를 버리는 코드는 없습니다.
- 제외 조건은 `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`, 설정된 제외 앱, 일시정지, 읽기 실패, 항목 크기 초과입니다. 기본 항목 한도는 10MB, 총용량은 200MB입니다. `policy_rev`가 없는 구설정의 기본 `max_items=50`만 500으로 이전하며, 새 정책 버전의 사용자 지정값 50은 유지합니다.
- Flutter 관리 화면은 KAgent 상태 파일의 named pipe로 SQLite를 다시 조회합니다. 빠른 팝업은 KAgent의 최근 50개 메모리 스냅샷을 주로 사용합니다. 팝업을 열 때 이 캐시를 DB와 동기화하지 않아, 외부에서 DB에 추가한 기록이 빠른 목록에서는 오래된 상태로 남을 수 있었습니다.
- 실제 격리 로그 `kagent/test-results/isolated_clipboard_popup.json`에는 전용 테스트 인스턴스가 고유 `...kagent.test.<해시>.<SID>` pipe를 만들고 기본 핫키를 등록한 뒤 PowerShell 클립보드 설정에서 거부된 기록이 있습니다. 이 세션에서는 캡처/저장 로그를 생성한 별도 실행도 있었고, 텍스트 `bytes=48`, 파일 경로 `bytes=95`, 목록 수 `1`→`2`를 확인했습니다. 사용자 프로세스에는 손대지 않았습니다.

## 수정 및 재발 방지

- `popup::show()`가 표시 전에 `refresh_recent()`를 호출해 SQLite의 최신 기록으로 메모리 스냅샷을 갱신합니다.
- Rust 회귀 시험은 비어 있는 오래된 캐시와 새 SQLite 항목을 놓고 재조회 후 빠른 목록 필터가 항목을 반환하는지 검사합니다.

## 재현 전·후 및 검증

- 전: 팝업 재열기 시 기존 메모리 스냅샷을 필터링하므로 다른 화면/프로세스가 저장한 항목이 누락될 수 있었습니다.
- 후: 각 표시 요청에서 최신 SQLite 스냅샷을 읽습니다.
- `cargo test --manifest-path kagent/Cargo.toml`: 88 passed, 1 ignored.
- `python -m unittest discover -s tests -p 'test_kclip_service.py'`: 22 passed.
- `flutter test test/kclip_dialog_test.dart test/kclip_category_test.dart`: 19 passed.
- 전체 Python 시험은 장시간 출력 없이 대기해 결과를 확보하지 못했습니다. Flutter 전체 시험은 의존성 복구 전 `flutter_test` 누락으로 실패했지만 관련 KClip 시험은 복구 후 통과했습니다.
- 격리 UI에서 실제 화면 렌더링까지 자동 확인하는 단계는 현재 Windows Forms 클립보드 접근 거부로 완료하지 못했습니다. 이를 통과로 보고하지 않았습니다.
- `git diff --check` 통과.
