# SEMI1 — 몽땅검색 연관 검색어

기준 `github/main` (`bf624ca5`), 작업 브랜치 `feature/SEMI1`. 로컬 커밋만 수행하며 푸시·배포·main 병합은 하지 않는다.

## 구현

- `related_groups=True`를 켜면 SQLite·Rust 모두 묶음 안의 모든 낱말이 같은 문장에 있는지 확인한 뒤 문서 집합의 교집합을 구한다. 문장 경계는 마침표·물음표·느낌표(한글 문장 부호 포함)와 줄바꿈이다. 파일명은 이름 전체를 한 검색 단위로 취급한다.
- 순위는 최고 위계 → 묶음별 일치 문단 수 합 → 경로다. 상위 개수·offset은 교집합 뒤 적용한다. `2026; 공사 계약`은 `2026`과 `공사 계약`이 서로 다른 문단에 있어도 일치하지만 `공사`와 `계약`만 서로 다른 문단에 있으면 제외한다.
- 순위는 최고 위계 → 묶음별 일치 문단 수 합 → 정규화 경로 오름차순이다. 상위 개수와 offset은 교집합 뒤 적용한다. `2026; 공사 계약`은 두 묶음이 각각 다른 문장·문단에 있어도 문서가 두 묶음 모두에 일치하면 결과에 남는다. `공사`와 `계약`만 서로 다른 문장에 있으면 해당 묶음은 일치하지 않는다.
- 순수 파서는 따옴표·이스케이프·빈 묶음·NFC·공백을 처리한다. `&`는 일반 문자다. 새 옵션에서 필터 모양·`~N`도 문자 그대로이며 근접 검색은 추가하지 않았다.
- 기존 기본 검색 경로·응답 필드는 보존한다. 옵션 응답에는 `group_matches`를 추가해 묶음별 질의·낱말·일치 수·문단 번호·위계·발췌·강조 좌표를 제공한다.
- ktm은 `settings.json`의 `related_queries=true`에서 새 경로를 사용한다(기본 false). 기존 worker·취소·레이아웃을 유지하며 칩과 묶음 색상·근거 배지를 연결한다. 칩·색상 표시는 기존처럼 앞 8묶음까지이며 전체 근거는 결과에 보존한다.
- 파일명 서비스는 `names_related`라는 별도 명령이다. 설치 서비스에는 자동 반영하지 않았으므로 새 ktm 옵션으로 파일명 검색을 할 때에는 같은 소스로 빌드한 서비스가 필요하다.
- 기준선(main)에는 SRCH6 공통 `SearchBackend`와 `search-content` 경로가 없다. SEMI1 브랜치는 독립 어댑터·CLI 명령을 추가했다. `search_synthetic.py` 생성 함수는 기준 시드 `20261124`를 사용한다. 아래 설계에 따라 후속 SRCH6 통합 때 공통 backend 계약과 JSONL 명령을 합친다.

## 검증

검증은 Windows 작업 PC에서 직접 실행했다. 저장소 `.venv`가 없어 시스템 Python 3.14.0을 사용했다. Cargo 출력은 저장소 밖 `CARGO_TARGET_DIR`에 두었고, Rust 시험 때 MinGW 경로를 `PATH`에 추가했다.

| 항목 | 결과 | 비고 |
|---|---|---|
| 집중 Python 회귀시험 | `Ran 30 tests`, `OK (skipped=1)` | 파서·빈 묶음·따옴표·문장 AND·교집합·순위·근거·기본 응답·파일명 |
| kfind-core | `103 passed, 0 failed` | `cargo test --manifest-path kfind/core/Cargo.toml` |
| kfind-names | `32 passed, 0 failed` | 전체 단위·통합 시험 |
| klib library | `19 passed, 0 failed` | `cargo test --manifest-path klib/Cargo.toml --lib` |
| klib 연관 검색 | `1 passed, 0 failed` | 바이너리 테스트를 로컬에서만 임시 활성화해 실행 후 `Cargo.toml` 원복. 후보 상한·교집합·순위·근거·문장 분리 검증 |
| ktm | `260 passed, 0 failed, 1 ignored` | 문장 분리 기대값 수정 후 재실행 |
| klib 바이너리 빌드 | 성공 | `cargo build --manifest-path klib/Cargo.toml --bin klib`; 기존 경고만 출력 |
| SQLite/Rust 합성 대조 | 24/24 조합 일치 | 시드 20261124, 합성 문서 1,000개 + 제어 문서 5개, 본문·파일명 각각. 집합·정렬·묶음 근거 차이 0 |
| 전체 Python unittest | `Ran 3620 tests in 1005.198s`, `OK (skipped=26)` | Rust 실행 파일 경로를 지정해 parity 시험 포함. 요약 후 임시 폴더 정리에서 잠긴 SQLite 파일의 `PermissionError`가 출력됐으나 종료 코드는 0 |

### 추가 및 변경된 파일 목록
- `tests/test_related_query.py`: 세미콜론(;) 분리, 공백·따옴표·빈 묶음, 특수문자(&, ~N), NFC, 200자 제한, 문단 matches 순수 단위시험 작성
- `tests/test_related_search.py`: SQLite 기반 연관 검색(문단 단위 AND, 문서 단위 교집합, 위계 우선 순위, 기존 호환 응답 + group_matches, 파일명 검색) 단위시험 작성
- `tests/test_related_synthetic.py`: 합성 코퍼스(시드 20261124) 생성기 연동, 문서 교집합, 위계 및 일치수 순위, Backend 프로토콜 호환 검증 시험 작성
- `docs/SEMI1_PROGRESS.md`: 진행 경과 기록
- `docs/SEMI1_RESULT_2026-11-24.md`: 결과 및 인계 문서 갱신


문장 단위 규칙과 다르던 기존 ktm 기대값 1건을 같은 문단의 서로 다른 문장 사례로 고쳐 검색 결과에서 제외하도록 했다. 기준을 낮추지 않았고, 전체 Python 시험의 WebView 재시작 경고와 임시 SQLite 잠금 정리 오류는 기능 시험 실패가 아닌 종료 후 환경 경고로 기록한다.

최종 검토에서 파일명 확장자의 점을 문장 경계로 잘못 볼 수 있어 파일명은 전체 이름을 단일 검색 단위로 처리하도록 보완했다. 이 보정 후 집중 Python 18개, kfind-core 파서 2개, kfind-names 실제 그룹 검색 1개, klib 연관 검색 1개가 통과했다. 위 전체 Python 및 1,000문서 parity 수치는 이 마지막 파일명 경계 보정 전 결과이며, 보정 후 재실행이 남아 있다.

## SRCH6 backend 통합 제안(후속 카드, 이번 카드에서 구현하지 않음)

현재 로컬 `feature/SRCH6`에는 `SearchBackend.search(query, *, roots, hierarchy, limit, offset, sort, start, end)` 공통 계약과 `search-content` JSONL 서비스가 있다. SEMI1 기준선에는 이 공통 backend가 없어서 독립 옵션으로 개발했다. 후속 병합 시 다음처럼 합친다.

1. 공통 `search()` 계약에 `related_groups=False`와 `filenames=False` 키워드 인자를 기본값으로 추가한다. 기본값이 false일 때 SQLite와 Rust 모두 기존 메서드·기존 응답 그대로 사용한다.
2. SQLite 어댑터는 현재 `search_related()`를 호출하고, Rust 어댑터는 별도 장기 실행 프로세스나 색인을 더 만들지 않고 기존 `search-content`의 `op="search"` 요청에 `related_groups: true`를 전달한다. Rust CLI가 공용 파서와 문장 매처로 묶음별 후보를 모아 교집합을 만든 뒤 limit/offset을 적용한다.
3. 파일명 검색은 SRCH6의 기존 kfind-names 서비스 경로를 재사용하고 동일 파서·교집합·문자열 매처를 적용한다. 본문/이름 명령이 분리돼 있어도 `group_matches` 응답 구조와 순위 규칙은 같게 한다.
4. 기존 응답 키와 타입을 유지하고 `group_matches`만 추가한다. 기본 질의, 취소, 범위 필터, backend 상태·재색인 흐름은 현재 동작을 유지한다.
5. 통합 회귀시험에서 backend 양쪽의 결과 집합·순서·묶음 근거가 같고, 옵션 미지정 호출의 기존 응답 비교가 동일할 때만 통합한다.

## legacy 화면 연결 설계(구현하지 않음)

연결 가능하다. `docfit_app/timemachine_service.py`의 `content_search` → `_search_rows`, `file_search` → `search_files`, 질의형 `related_search` → `_related_query_search`에 공통 `related_groups` 설정을 전달하면 된다. 기본값은 false로 두고, 옵션이 켜진 경우에만 `SqliteBackend.search(..., related_groups=True)` 또는 통합된 Rust backend로 위임한다. 조사 폴더·기간·limit/offset은 기존 값 그대로 넘긴다.

`items`, `name`, `file_name`, `snippet`, `highlights`, `locations`, `position`, `breadcrumb`의 이름과 타입을 유지하고 `group_matches`를 추가한다. 기존 화면은 모르는 필드를 무시하므로 1차로 엔진만 연결할 수 있다. 2차로 칩과 묶음 색상·문단 이동을 추가하되, 새 `group_matches[].matches[]`를 근거로 써서 발췌문에 없는 묶음도 일치 사실을 잃지 않게 한다. Markdown 내보내기도 같은 옵션을 전달해야 화면과 결과 문서의 집합이 같아진다.

문서를 기준으로 유사한 문서를 찾는 기존 `related_search(path=...)`는 별도 기능이므로 그대로 둔다. 쿼리 문법과 문서 유사도 검색을 같은 플래그로 묶지 않는다. 이번 카드에서는 legacy 서비스·Flutter 화면을 수정하지 않았다.

## 확인하지 않은 범위

- 설치본·실사용 문서·한/글 조판/COM 기능은 시험하지 않는다. 합성 파일만 사용한다.
- 기본 backend 승격·설치 서비스 교체·SRCH 통합 병합·배포는 수행하지 않는다.
- 대규모 코퍼스의 메모리·검색 속도 및 첫 그림 16ms 일반 보장은 별도 실측 항목이다.
