# PROFILE1 결과 — Rust 크레이트 릴리스 프로필 정비 (2026-11-22)

근거: `docs/WINRUST_TECHNIQUE_REVIEW_2026-11-20.md` 우선순위 1. 기능 변경 없음(Cargo.toml 프로필만 변경).

## 변경
- `crates/kfind`, `outliner_core`(outliner.exe), `pdf_edit_core`(pdf-edit.exe), `qms_bench`: `[profile.release]` 신설
  (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "unwind"`).
  kfind 는 `catch_unwind` 격리 경계(`names.rs`, `pipe.rs`)가 있어 `panic = "unwind"`를 명시했다(abort 금지).
- `hierarchy`: 기존 `lto`·`codegen-units = 1`에 `strip = true` 추가(CRLF 유지).

## 크기 (같은 방법: `cargo build --release --locked`, 기본 툴체인, 크레이트별 별도 CARGO_TARGET_DIR)
| 실행 파일 | 변경 전(바이트) | 변경 후(바이트) | 감소 |
|---|---:|---:|---:|
| kfind.exe | 6,131,190 | 2,164,224 | -64.7% |
| outliner.exe | 1,890,300 | 595,968 | -68.5% |
| pdf-edit.exe | 3,587,375 | 724,480 | -79.8% |
| qms_bench.exe | 1,951,448 | 767,488 | -60.7% |
| hierarchy.exe | 2,639,214 | 2,259,456 | -14.4% |
| hierarchy.dll | 3,936,900 | 3,131,392 | -20.5% |

주의: 이 PC 의 기본 툴체인은 `x86_64-pc-windows-gnu`라 설치본(MSVC, 정적 CRT)과 절대 크기가 다르다. 변경 전·후를 같은 방법으로 비교한 값이다.

## 시작 시간 (kfind.exe --version 30회 평균, 4회 반복)
변경 전 20.5~23.0 ms/회, 변경 후 19.2~20.3 ms/회. 나빠지지 않았다(소폭 빨라짐).

## 시험 (`cargo test --release --locked`, 변경 후 프로필)
| 크레이트 | test result 줄 |
|---|---|
| kfind | 36 passed; 0 passed(main); 1 passed(names_pipe); 2 passed(user_mode); 0 passed(doc) — 실패 0 |
| outliner_core | 69 passed; 0 passed(main); 1 passed(cli_smoke); 0 passed(doc) — 실패 0 |
| pdf_edit_core | 3 passed; 0 passed(main); 6 passed(safety); 0 passed(doc) — 실패 0 |
| qms_bench | 49 passed, 1 ignored; 0 passed(main); 0 passed(doc) — 실패 0 |
| hierarchy | 58 passed; 0 passed(bin); 3 passed(bin_cli); 3 passed(c_abi); 2 passed(search_levels_abi); 0 passed(doc) — 실패 0 |

참고: `outliner_win` 은 이미 lto·cg1·strip 이 있어 변경하지 않았다.
