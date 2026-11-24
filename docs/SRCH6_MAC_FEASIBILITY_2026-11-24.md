# SRCH6 — macOS search-content 빌드 가능성 점검

범위: 로컬 소스 정적 점검과 Windows 빌드. 실제 macOS 컴파일·실행은 하지 않았으며 맥 워커 후속 카드로 분리한다. Windows 서비스와 GUI의 이식은 범위 밖이다.

확인한 직접 Win32 호출에는 이미 cfg 가드가 있다. 아래 목록의 서비스 모듈 그래프가 크다는 사실만으로 macOS 컴파일 실패를 확정하지 않는다. 맥 후속 카드에서는 이번 target 의존성 분리 상태 그대로 먼저 빌드하고, 실제 오류가 드러난 부분만 최소 분리하는 순서를 권한다.

## 의존성과 차단점

| 파일 | 현재 Windows 의존·대체 경로 | search-content 전용 최소안 |
|---|---|---|
| Cargo.toml | windows-sys가 무조건 dependencies에 있음 | windows-sys 블록만 target.'cfg(windows)'.dependencies로 이동. 공용 의존성은 유지 |
| main.rs | 서비스 단일 인스턴스 mutex, Named Pipe 전송, watchdog·serve가 cfg(windows). 비Windows daemon은 명시 오류. DB·서비스 모듈 선언 자체는 공용 | Windows 서비스 모듈 선언과 서비스 main을 cfg(windows), 공용 CLI main에서 search-content만 허용. windows_subsystem은 cfg_attr(windows, ...)로 한정 |
| pipe.rs | ACL/SID, overlapped Named Pipe, Win32 IO는 이미 cfg(windows). 비Windows serve·사용자 식별 대체 함수 있음. handle_request가 DB·notebook·scan 등 전체 서비스 그래프를 끌어옴 | 모듈 전체를 Windows 전용으로 두고 JSONL은 search_cli의 std::io 경로 유지 |
| scan.rs | reparse 속성 조회만 cfg(windows), 나머지는 std·SQLite. db와 서로 연결 | 검색 CLI에서 모듈 제외. 필요 시 별도 기능으로 재도입 |
| scan_cli.rs | FindFirstFileExW/FindNextFileW, UTF-16 변환은 cfg(windows), 비Windows scan 명시 거부 | search-content 빌드에서 제외. macOS 디렉터리 스캔은 별도 카드 |
| scan_hwp.rs | MetadataExt·GetDriveTypeW로 드라이브/속성 검사. 대부분 cfg 분리됨 | search-content가 직접 쓰지 않으므로 제외 가능. extract가 쓰는 공용 구조는 보존 |
| tm_scan.rs | Windows file_attributes, 비Windows attrs=0 | CLI에서 제외 가능 |
| util.rs | UTF-16 wide, LOCALAPPDATA와 역슬래시 data_dir, reparse 검사. MetadataExt만 cfg(windows) | 데이터 경로를 PathBuf::join으로 만들고 비Windows는 명시 index 경로 또는 사용자 Library/Application Support 경로. wide는 Windows 전용 |
| db.rs | 경로 소문자화·드라이브/UNC·끝 점/공백 별칭의 Windows 보정이 cfg(windows). 비Windows PathBuf 경로 있음. library_root는 util::data_dir에 의존 | JSONL은 DB를 사용하지 않으므로 서비스 그래프와 함께 제외 |
| nb_store.rs | share_mode(0) 이전 잠금은 cfg(windows). 비Windows는 파일 열기만 해서 상호 배제 보장 없음 | search-content에서 제외. 이식 시 File::try_lock 등 실제 프로세스 잠금을 구현·경합 시험 |
| search.rs | 재색인 잠금 share_mode(0), 비Windows는 파일 열기만 함. search_notebook/replace_notebook_source가 crate::notebook 자료형 참조 | Windows 잠금은 유지. 비Windows 재색인에 실제 파일 잠금 필요. notebook용 메서드만 cfg(windows) 또는 공용 자료형 모듈로 분리 |
| search_cli.rs | std::io JSONL 자체는 공용. 기본 index_dir가 data_dir + 역슬래시 | PathBuf::join 사용. KLIB_TANTIVY_DIR 명시 경로 우선. 경로/roots 비교의 대소문자 규칙을 플랫폼별로 분리 |
| search_content_policy.rs | 이번에 추가한 std·Tantivy 기반 순위/캐시. 직접 Win32 없음 | search.rs의 하위 모듈로 함께 보존. roots의 Windows 대소문자 규칙은 search_cli와 같이 후속 점검 |
| content.rs | 직접 Win32 없음. ZIP/XML·추출 모듈 사용. 검색 블록·지문은 공용 | extract·hwp5·classify_levels·unify_marker·legacy_records와 필요한 JSON 모듈을 함께 포함 |

## 권장 최소 후속 변경

1. 이번 카드에서는 Cargo.toml의 Windows 의존성 조건 분리만 적용했다. Windows release 빌드는 성공했으며 cargo 시험은 최종 결과 문서에 기록한다. 다른 cfg나 서비스 코드는 변경하지 않았다.
2. 맥 카드에서 우선 현 상태를 빌드한다. 검색 전용 진입점 분리가 필요하면 `search`, `search_cli`, `content`, 공용 추출 모듈은 유지하고 `db`, `pipe`, `notebook`, `nb_store`, `reindex`, `scan*`, 서비스 전역은 필요한 곳만 cfg(windows)로 감싼다. search.rs의 notebook API 참조를 함께 분리해야 단순 모듈 숨김의 unresolved import를 피할 수 있다.
3. 환경변수 없이 쓰는 기본 경로, 역슬래시, 파일 대소문자 비교를 먼저 시험한다. macOS에서 roots를 무조건 소문자화하면 대소문자를 구분하는 볼륨에서 범위가 잘못 합쳐질 수 있다.
4. 검색 인덱스 재생성 잠금의 비Windows 분기를 실제 잠금으로 바꾼 뒤 두 프로세스 경합·중단 복구를 시험한다. 기존 fallback은 파일 열기에 성공한 두 프로세스를 배제하지 못한다.
5. 맥에서 `cargo build --release --bin klib`, 공용 단위 시험, 합성 Markdown·TXT·HWPX JSONL 색인/검색/삭제/재시작을 실행한다. macOS 빌드 가능 여부는 이 단계 통과 전까지 **확인 못 함**이다.

의존성 target 분리 성공만으로 맥 빌드 성공이나 완전한 이식을 주장하지 않는다. macOS ARM64 링커, bundled SQLite·zstd C 컴파일, 파일 잠금·경로 의미, Python client의 실행 파일 탐색은 맥 실기기 검증 항목이다.

Windows 최종 검증: release 성공, Python 전체 3,605개(20제외) OK. cargo는 시험 환경 panic=unwind로 169통과·4실패·1제외였으며 실패 4개 모두 변경 전 기준에서도 재현됐다. 기존 실패를 의존성 target 분리 성공의 근거로 숨기지 않는다. 상세는 `SRCH6_RESULT_2026-11-24.md`와 `SRCH6_VALIDATION_2026-11-24.json`을 본다.
