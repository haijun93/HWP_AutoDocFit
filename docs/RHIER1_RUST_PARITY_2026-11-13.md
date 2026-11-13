# RHIER1 Rust 검색 위계 규칙 맞춤

작업 브랜치: `fix/rust-hierarchy-parity` (`3a69b3e4` 기준). main 병합·푸시와 GitLab 접근은 하지 않는다.

## 범위와 계약

Python `content_search.classify_paragraph_levels` 규칙 판 5를 기준으로 한다.
Markdown 1~4단계, 번호의 연속성·등장 순서·본문 목록, 제N장, 제목 스타일·outline,
제목 표와 제목1: 준말, □·ㅇ·-, Markdown 표, 범정부 라벨과 개요:를 합성 입력으로 대조한다.
사용자 문서와 한/글 COM은 사용하지 않는다. 실제 문서 전체에 대한 동등성은 이번 합격 범위가 아니다.

서식 분석의 `role_sequence`와 검색의 위계는 서로 다른 계약이다.
기존 Rust 응답은 `role_sequence`만 돌려주는데 검색 호출은 정수 `levels`를 읽어 문단을 잃었다.
검색 문단 입력에 `source_path`·스타일·outline·표 식별자를 전달하면 Rust는
기존 서식 분석 응답에 이름 배열 `search_levels`를 추가한다. CLI·DLL은 같은 함수를 사용한다.
검색 호출은 배열 길이·이름을 검증하고, 구형 정수 `levels` 계약도 유지한다.
구형 역할 응답만 있거나 배열이 잘못되면 Python 규칙으로 돌아간다.

`hierarchy_engine=False` 기본값은 그대로 유지한다. 설정 변경·릴리스·main 병합은 하지 않는다.

## 함수별 수정

- 검색 응답 검증·변환: 문단 누락 방지, 표 제목·메타데이터 보존, 구형 정수 배열 호환.
- 기호 어휘 판정: Python과 같은 Unicode So/Sm/Po 범주, ASCII - 공식 기호, 두 자리 이상 번호를 한 계열로 정규화.
- `hierarchy::search_levels`: Python 검색 규칙의 문맥·우선순위와 제목 보완을 적용.
- 기호 통합 함수는 hierarchy로 옮겨 klib가 재사용한다. klib의 최종 분류 단계도 검색 공용 함수를 사용한다.
- `scripts/rhier1_diff.py`: Python 강제 경로, Rust CLI·DLL·klib, 엔진을 켠 검색 경로를 문단별 대조한다.

## 재현

빌드 도구 경로는 실행 환경의 PATH에만 넣는다. 시험 코드에는 PC 절대 경로가 없다.

```text
cargo test --manifest-path hierarchy/Cargo.toml
cargo build --manifest-path hierarchy/Cargo.toml
cargo test --manifest-path klib/Cargo.toml
cargo build --manifest-path klib/Cargo.toml
python scripts/rhier1_diff.py --exe hierarchy/target/debug/hierarchy.exe --dll hierarchy/target/debug/hierarchy.dll --klib klib/target/debug/klib.exe --out reports/RHIER1-parity.json
python -m unittest tests.test_rhier1_parity tests.test_hierarchy_bridge tests.test_hierarchy_path tests.test_hierarchy_page_fit_wire tests.test_hu2_search_hierarchy tests.test_timemachine_content_search tests.test_eval_hierarchy_search tests.test_rlev_rust_index_levels
```

수정 전 바이너리는 기준 커밋에서 빌드한 CLI로 같은 합성 묶음을 대조한다.
기준선의 차이는 검색 위계 필드 누락을 포함하며, 모든 문단이 잘못된 역할로 분류됐다는 뜻은 아니다.

## 검증 결과

- Python 관련 선택 시험 56개: OK, 제외 5개(Windows 전용 조건/기본 탐색 경로의 엔진 실행 파일 없음). 본문검색 전체 모듈이 아니라 위계·제목·번호·표·브리지 관련 시험을 골라 실행했다.
- 대량 파일 생성 시험까지 포함한 넓은 Python 실행은 파일 쓰기 대기로 중단했다. 전체 Python 통과 수치로 사용하지 않는다.
- 초기 DLL ABI 추가 시험의 Rust raw 문자열 구분자 오류는 JSON 매크로로 수정했다. 중간 iterator 참조 타입 오류도 최종 빌드 전에 수정했다.
- 최초 공용 분류 대조에서는 `・`와 전각 `－`의 Unicode 범주 차이로 8문단 불일치를 찾았다. 어휘 판정 함수를 고치고 회귀시험을 추가했다.
- 실제 브리지 시험에서 추가로 1문단 차이를 찾았다. Markdown 표가 아닌 단독 `|`가 네 번째 기호 계열이 되면 Python은 canonical 기호의 3단계 상한을 적용하지만 Rust는 일찍 body를 반환했다. 1~3단계만 직접 매핑하고 더 깊은 계열은 canonical 판정으로 넘기도록 수정했다. `|`·`||`·줄바꿈 포함 표 경계도 시험했다.
- 디버그 빌드 중 E: 여유 공간이 약 244KB가 되어 Rust가 `os error 112`로 실패했다. 두 crate의 이번 빌드 산출물만 `cargo clean`으로 정리했다. `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `-j 2`로 재실행했다. 시험·분류 기준은 낮추지 않았다.
- 공용 target에서 서로 다른 의존성 feature 산출물이 섞여 serde 타입 불일치가 발생했다. 최종 빌드·시험은 **crate별 target 폴더를 분리**했다. 독립 Cargo.toml 두 개에 같은 `CARGO_TARGET_DIR`를 사용하지 않는다.
- 빌드 산출물에만 NTFS 압축을 적용해 약 377MB를 확보했다. 검증한 실행 파일은 `E:/docfit-tmp/rhier1-verified-binaries`에 SHA-256 일치 확인 후 보존했다. 소스·사용자 문서 원본은 압축·정리 대상이 아니다.

최종 결과:

| 경로 | 문서 | 문단 | 차이 |
|---|---:|---:|---:|
| 기준 커밋 CLI | 82 | 229 | 229 (검색 위계 필드 누락) |
| 수정 CLI | 82 | 229 | 0 |
| 수정 DLL | 82 | 229 | 0 |
| 수정 klib classify-levels 7 | 82 | 229 | 0 |

세 Rust 경로의 687개 판정 모두 일치했고, 엔진 결과를 사용하는 검색 분류 경로도 82문서 모두 일치했다.
원시 결과는 `reports/RHIER1-parity.json`이다.

- hierarchy 최종 분리 빌드 `cargo test`: 56개 통과, 실패·제외 0.
- klib 최종 `cargo test`: 175개 통과, 제외 2(기존 제외), 실패 0. 두 crate의 최종 `cargo build`도 완료.
- Python 관련 선택 실행: 56개 중 51개 통과·5개 제외. 최종 계약·실제 브리지·합성 대조 실행: 7개 모두 통과(앞 실행의 계약 시험 5개와 중복). 고유 시험 58개 중 53개 통과·5개 제외.
- `hierarchy_engine=False` 유지 시험 통과. 한/글 COM·실제 사용자 문서 평가·전체 Python 회귀·설치/배포는 이번 검증 범위에 포함하지 않는다.
