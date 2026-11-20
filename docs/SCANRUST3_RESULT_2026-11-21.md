# SCANRUST3 결과 (2026-11-21)

## 엔진 선택 순서

기본 문서·사진 범위에서는 `DOCFIT_SCAN_ENGINE=python`이면 Python 스캔을 강제한다. 그 외에는 이름 색인 파이프의 `names_status`가 `state=ready`, `mode=service`인지 확인한 뒤 Rust `names_list`로 기본 루트, 확장자, 예약·사용자 제외 경로를 전달한다. 파이프 없음, 서비스 모드 아님, `empty/building/failed`, 잘못된 응답이면 `klib scan` 사용자 모드로 폴백한다. klib 실행 파일 없음·실행/종료 오류·잘못된 JSON이면 Python 순회로 폴백한다. 선택 엔진은 시작 응답·진행 상태·완료 결과의 `engine` 필드에 기록한다.

## 색인 응답과 메타데이터

현재 이름 색인 응답에는 `name/dir/is_dir/size/mtime` 필드가 있으나 size·mtime은 색인에서 모르는 경우 sentinel일 수 있고 `cloud` 속성은 없다. 새 `names_list` 응답은 경로와 이름만 반환해 서비스 인덱스를 빠르게 전달한다. 문서뷰는 목록 전체를 stat으로 보충하지 않는다. 사진뷰는 새 경로·이름만 SQLite에 최소 등록하고 상세 metadata는 비워 둔다. 제안: 화면에 실제 나타난 카드에서만 경로 stat/RECALL 확인하고, 필요할 때 사진 EXIF를 읽어 기존 경로 캐시를 갱신한다. 이번 구현은 metadata 보충을 위해 klib scan을 실행하지 않는다.

## 즉시 표시와 갱신

마지막 기본 목록을 기존 `kagent_client.data_dir()` 아래 `scan-cache/{documents,photos}.json`에 버전·확장자·제외 폴더·기본 루트 서명과 함께 원자적으로 저장한다. 설정 서명이 달라지면 읽지 않는다. 캐시가 있으면 시작 응답은 `state=cached`와 저장 목록을 즉시 반환하고 같은 요청에서 새 스캔을 백그라운드로 시작한다. 완료 결과가 화면과 저장 캐시를 대체하며, 취소 결과는 저장하지 않는다. 사진뷰는 최신 서비스 경로 집합을 반영해 사라진 항목을 조회에서 제외한다.

## Flutter 상태 호환성

문서뷰는 `running/cancelling`에서 진행 상태를 반영하고 `complete/cancelled`에서 결과를 교체한다. 알 수 없는 상태를 오류로 강제하지 않고 시작 응답의 기존 결과를 유지한다. 사진뷰도 알 수 없는 상태를 예외 처리하지 않고 마지막 DB 조회로 진행한다. 두 화면에서 `cached` 시작 응답을 먼저 렌더링하도록 반영했다.

## 구현 판단과 미확인 사항

원자 파일 저장·JSON 직렬화는 기존 Python 브리지 데이터 디렉터리와 맞물려 현재 Python에 두었다. 장기적으로 파일 목록 판정·확장자/제외 적용은 Rust에 두는 편이 적절하다.

실제 설치 서비스의 named pipe ready 응답과 전체 드라이브 스캔 시간은 확인하지 않았다. 실제 서비스 대신 가짜 pipe 서버로 질의 경로를 시험했다. 이 PC의 앱·서비스는 종료·재시작·설치·등록하지 않았다.

## 검증 결과

지정 관련 Python 시험:

```text
python -m unittest tests.test_doc_grid_service tests.test_photo_album_service tests.test_default_library_scan tests.test_scan_compare tests.test_bridge_methods tests.test_scan_cache_service -v
Ran 48 tests in 18.791s
OK (skipped=3)
```

`klib` 라이브러리 시험은 MinGW DLL 도구 경로를 임시 PATH에 추가해 통과했다.

```text
cargo test --manifest-path klib/Cargo.toml --lib
 test result: ok. 19 passed; 0 failed; 0 ignored
```

`kfind`에는 `--lib` 대상이 없어 구현 패키지를 다음 명령으로 확인했다.

```text
cargo check --manifest-path crates/kfind/Cargo.toml
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.23s
```

Flutter 검증은 `flutter_ui`에서 실행했다.

```text
flutter analyze
No issues found! (ran in 14.3s)

flutter build web --release
Compiling lib\main.dart for the Web... 59.2s
Built build\web
```

