# STAB1 — 시험 안정화 (2026-11-27)

제품 코드는 바꾸지 않았다. 시험 파일과 문서만 바꿨다. 시험의 삭제·건너뛰기·기한 연장·단언 완화는 없다.

환경: Windows 11, Python 3.14.0(공유 개발 가상환경). klib 은 작업 사본 소스로 저장소 밖에 **디버그** 빌드(`x86_64-pc-windows-gnu`)했다. 설치된 색인 서비스는 켜져 있었고, 서비스·설치본·사용자 설정은 바꾸지 않았다. 한/글은 실행하지 않았다.

## 재현 횟수 전후

| 항목 | 수정 전 | 수정 후 |
|---|---|---|
| 1. `test_hierarchy_bridge` 스레드 호출 횟수, 기본 설정 500회 | 실패 0 | 실패 0 |
| 1. 같은 시험, 스레드 전환 간격 1µs 로 500회 (Python 3.14) | **실패 500** (`286~297 != 302` 등) | 실패 0 |
| 1. 모듈 전체 | — | 20회 중 실패 0 |
| 2. `python -m unittest tests.test_style_profile` | **10회 중 실패 10** (`No module named 'test_table_style'`) | 10회 중 실패 0 |
| 2. `test_final_review_guards`·`test_v2_t04_t19`·`test_v2_text_input` 단독 | 각 1회 실행, 모두 가져오기 오류 | 모듈·`discover -p` 각 1회 모두 통과 |
| 3. `discover -s tests -p <두 모듈>` 로 읽었을 때 격리 표시 | **없음(`None`)** | `1` (회귀 시험 추가: 수정 전 2 subtest 실패, 후 통과) |
| 3. 두 모듈, 처음 실행하는 klib 사본 | **5회 중 실패 5** | 5회 중 실패 0 |
| 3. 두 모듈, 같은 klib 반복 | 빌드 직후 처음 5회 중 실패 1, 이어 20회 중 실패 0 | 10회 중 실패 0 |
| 3. 두 모듈, klib 지정 안 함(`-m unittest`) | 20회 중 실패 0 | 10회 중 실패 0 |
| 4. `test_semi2_backend_integration`, SEARCHDBG1 이전 klib 디버그 | **20회 중 실패 19** | — |
| 4. 같은 시험, 현재 소스 klib 디버그 | — | 20회 중 실패 0 |

## 원인과 조치

### 1. 위계 브리지 간헐 실패 — 시험 결함
`Mock.call_count` 는 증가 연산이 잠기지 않아 8개 스레드가 동시에 부르면 횟수를 잃는다(ROLLBACK2 에서 본 `301 != 302`). 제품 `run_library` 는 `finally` 에서 결과마다 해제한다. 해제 호출을 `list.append` 로 직접 세고, 횟수 302 와 함께 해제한 주소까지 확인하도록 바꿨다(전보다 강한 단언).

### 2. 가져오기 오류 — 시험 결함
시험 모듈을 `from test_x import` 로 가져와 `tests` 폴더가 경로에 있을 때만 동작했다. `from tests.test_x import` 로 바꿨다. 같은 원인의 모듈 3개도 함께 고쳤다.

### 3. 기본 범위 검색 시험 — 원인 두 가지
- **설치 서비스 조회**: 제품 쪽 차단(`DOCFIT_FORBID_REAL_DATA`)은 `dbaedbcf`(v2.2.106)에 이미 있다. RUSTSIDE2 의 실패 4건은 그 수정이 없는 갈래(`65977809`)에서 돌린 것이다. 느린 가짜 서비스를 끼우면 같은 시험 4건이 실패한다(1회 확인, 실제 서비스는 조회하지 않음).
- 남아 있던 틈: `discover -s tests -p <모듈>` 은 `tests/__init__.py` 를 읽지 않아, 두 모듈만 고르면 격리가 꺼진 채 실제 서비스·사용자 설정을 읽을 수 있었다. 두 모듈이 `import tests` 를 하도록 했다. 개인 자료를 읽게 되므로 격리를 끈 상태의 시험 실행은 하지 않았다.
- **새 klib 의 첫 실행 지연**: 새로 만든 klib(약 266MB 디버그, 서명 없음)의 첫 실행이 5.44초, 이후 0.10초였다. 시험 기한 5초를 넘긴다. 실행 파일 검사(백신) 때문으로 **추정**한다. `tests/klib_support.py` 의 `warm_up_klib()` 를 두 모듈 `setUpClass` 에서 불러, 기한을 재기 전에 빈 폴더를 한 번 검색한다. 기한과 단언은 그대로다. 앱에서 설치·갱신 직후 첫 검색이 느린지는 확인하지 않았다.

### 4. 디버그 klib `search-content` 종료 — 이미 수정됨, Windows 재시험만 함
`6770af3a`(SEARCHDBG1, 현재 갈래에 포함)에서 고쳐졌다. 맥에서만 확인된 상태여서 Windows 에서 전후를 다시 확인했다. 이전 소스는 `search_content_policy.rs` 만 `6770af3a^` 로 되돌린 저장소 밖 사본으로 빌드했다. 따로 stderr 를 모아 `tantivy-0.27.0 segment_postings.rs:173`, `assertion failed: self.doc() <= target` 를 확인했다. 코드는 바꾸지 않았다.

## 최종 시험

관련 19모듈(`python -m unittest ...`):

```text
test_hierarchy_bridge test_style_profile test_table_style test_default_library_scan
test_library_exclusion_settings test_names_service_isolation test_data_path_guard
test_final_review_guards test_fidelity test_offline_runtime test_v2_t04_t19 test_v2_text_input
test_desktop_browser_api test_doc_grid_service test_doc_grid_bridge test_photo_album_service
test_scan_stream test_svccheck1 test_semi2_backend_integration
```

- klib 지정 안 함: `Ran 197 tests in 38.961s` / `OK (skipped=13)`
- `KLIB_EXE`=현재 소스 디버그 klib: `Ran 197 tests in 40.277s` / `OK (skipped=5)`
- 전체 모듈 읽기만(`discover`, 실행 안 함): 4119건, 읽기 오류 0
- `cargo test --lib search`: `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out`
- `cargo test --bin klib occurrences`: `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 100 filtered out`

## 하지 않은 것·남은 문제

- 전체 Python·전체 cargo 시험은 실행하지 않았다.
- release klib 은 이번에 빌드·시험하지 않았다.
- 시험 모듈 446개 중 파일 맨 앞 가져오기로 `tests` 를 읽는 것은 55개다. 나머지를 `discover -p` 로 골라 돌리면 격리가 켜진다는 보장이 없다(전체 `discover` 는 다른 모듈이 먼저 읽어 켜진다). 골라 돌릴 때는 `python -m unittest tests.<모듈>` 을 쓴다. 일괄 정리는 이번 범위 밖이다.
