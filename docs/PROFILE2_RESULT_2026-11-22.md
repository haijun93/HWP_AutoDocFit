# PROFILE2 결과 — klib 크기 줄이기 + kfind 서비스 오류 로그 (2026-11-22)

근거: `docs/WINRUST_TECHNIQUE_REVIEW_2026-11-20.md` 우선순위 2·3.

## (1) klib 크기
`[profile.release]` 현재값: `opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`.
- **opt-level 유지**(3). **panic = "abort" 유지**: klib 소스에는 `catch_unwind` 경계가 없고(검색 0건) 스레드 패닉을 격리하는 설계가 없다.
  unwind 로 바꾸면 크기만 늘고 얻는 것이 없다(kfind 와 달리 격리 경계 부재). 서비스 격리가 필요하면 경계 도입과 함께 별도 카드로 다룬다.
- **변경**: `tantivy` 기본 기능에서 `stemmer`(frostem)·`stopwords`를 끔. klib 는 자체 토크나이저(KoBigram·Bigram)만 등록하고
  `"default"`(기본 내장 토크나이저)만 쓰므로 영향 없음. 색인 디스크 형식에 닿는 `lz4-compression`·`columnar-zstd-compression`·`mmap`은 **유지**(기존 색인 호환).
  `Cargo.lock`에서 `frostem` 1개 제거.
- 검토했으나 안 한 것: `opt-level = "s"`(지시상 유지), `rusqlite`(bundled 외 기능 없음), `zip`·`flate2`(이미 default-features 끔).

| 항목 | 변경 전 | 변경 후 |
|---|---:|---:|
| klib.exe 크기(바이트, 이 PC GNU 툴체인) | 10,537,472 | 10,228,224 (-2.9%) |
| `klib scan --roots <저장소 작업폴더>` 1회차(워밍업 후) | 165 ms | 159 ms |
| 2회차 | 171 ms | 173 ms |
| 3회차 | 164 ms | 160 ms |

속도 차이는 측정 오차 안(±10 ms). 크기 감소가 작은 것은 tantivy 의 나머지(lz4·zstd·쿼리 파서)·bundled SQLite 가 대부분이기 때문이며,
더 줄이려면 압축 방식 변경(색인 호환 깨짐)이나 `opt-level = "s"` 가 필요해 이번 범위에서 제외했다.

## (2) kfind 서비스 오류 파일 로그
- 새 모듈 `crates/kfind/src/errlog.rs`: `<데이터 폴더>/logs/kfind-service.log`, 256 KiB 도달 시 `.1`로 회전(1세대, 총 약 512 KiB 상한). 줄바꿈은 공백으로 평탄화.
- 로그 폴더는 `--index-root`의 부모(`<데이터>/index` → `<데이터>/logs`)에서 유도. 인자가 없으면 `%ProgramData%\HwpAutoDocFit`. 하드코딩 경로 없음.
- `service_main`: `run()` 오류와 패닉(`catch_unwind`)을 stderr 와 파일에 모두 기록. 상태 보고 로직은 건드리지 않음.
- 단위 시험 3개 추가: 덧붙이기·줄바꿈 평탄화, 회전·용량 상한, 로그 폴더 유도.

## 시험
kfind `cargo test --release --locked`(39 passed, 이전 36 + 3):
- `test result: ok. 39 passed; 0 failed` / `ok. 0 passed`(main) / `ok. 1 passed`(names_pipe) / `ok. 2 passed`(user_mode) / `ok. 0 passed`(doc)

klib: `cargo test --release --locked`는 **변경 전부터** 실패한다(`panic = "abort"` 프로필과 테스트 하니스 충돌, "requires panic strategy abort"). CI 는 릴리스 없는 `cargo test --locked`를 쓴다(`build-windows-exe.yml:316`). 따라서 둘 다 실행:
- CI 방식 `cargo test --locked`: 19 passed / 22 passed / 127 passed, 1 ignored / 3 passed / 0 passed(doc) — 실패 0
- 릴리스 최적화 + 시험용 `CARGO_PROFILE_RELEASE_PANIC=unwind`(설정 파일은 불변): 19 / 22 / 127(1 ignored) / 3 / 0 — 실패 0

## 남은 문제
- klib 크기는 -2.9%로 목표(대폭 감소)에 못 미침. 추가 감소는 호환·속도 위험이 있어 사용자 결정 필요.
- `cargo test --release`가 klib 에서 실패하는 것은 기존 문제(문서화만).
- 로그 실제 서비스 환경(SCM) 기록은 이 PC 에서 서비스 설치 없이 단위 시험으로만 검증.
