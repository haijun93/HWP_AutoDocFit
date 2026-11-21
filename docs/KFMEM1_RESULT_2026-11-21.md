# KFMEM1 결과 — kfind 이름 색인 메모리

## 결과 요약

`kfind-names`의 항목별 정렬 키를 저장된 `u64`에서 깊이 `u8`로 바꾸고, 정렬 시 비교형 이름 앞부분으로 기존 키를 재구성했다. 볼륨 레코드→항목 번호 표는 3바이트 packed map으로 바꿨다(항목 번호가 24비트 범위를 넘으면 `u32` 표를 쓴다). 검색용 접기 문자열과 원래 이름은 그대로 유지했다. `names.seg` 형식 버전을 3으로 올렸다.

| 측정 | 변경 전 | 변경 후 | 차이 |
|---|---:|---:|---:|
| 색인 힙 용량 합계 | 304,132,300 B | 277,385,623 B | −26,746,677 B (−8.8%) |
| 항목당 | 90.967 B | 82.967 B | −8.000 B (−8.8%) |
| 질의 p50 | 3.752 ms | 3.967 ms | +5.7% |
| 질의 p95 | 12.803 ms | 13.458 ms | +5.1% |

질의 p50·p95 악화는 허용 한도 20% 이내다. 질의 100개를 각 한 번 실행한 단일 측정이다.

## 항목당 힙 구성

재현 예제: `cargo run --release --example mem_probe -- [entry-count]`. 기본 항목 수 3,343,333개에 볼륨 루트 1개를 더했다. 원래 이름·폴더 깊이·한국어/ASCII 이름을 섞고, 각 이름마다 질의용 fold도 저장했다. MFT 레코드 번호는 연속으로 만들었다.

| 필드 | 변경 전 B/항목 | 변경 후 B/항목 | 변경 |
|---|---:|---:|---:|
| 부모 번호 | 4.000 | 4.000 | — |
| 접기 문자열 오프셋 | 4.000 | 4.000 | — |
| 원래 이름 오프셋 | 4.000 | 4.000 | — |
| 정렬 보조(기존 `u64` 키 → 깊이 `u8`) | 8.000 | 1.000 | −7.000 |
| 확장자 번호 | 2.000 | 2.000 | — |
| 속성 플래그 | 1.000 | 1.000 | — |
| 크기 | 8.000 | 8.000 | — |
| 수정 시각 | 8.000 | 8.000 | — |
| 하위 트리 끝 번호 | 4.000 | 4.000 | — |
| 접기된 검색 이름 바이트 | 21.250 | 21.250 | — |
| 원래 UTF-8 이름 바이트 | 22.717 | 22.717 | — |
| 볼륨 레코드 표(기존 `u32` → packed 24비트) | 4.000 | 3.000 | −1.000 |
| 기타 확장자/리다이렉트 추정 | 0.000 | 0.000 | — |
| **합계** | **90.967** | **82.967** | **−8.000** |

값은 `Vec` 용량을 합산한 힙 추정치로, 프로세스 할당자 메타데이터는 포함하지 않는다. 확장자 `HashMap` 버킷 등 기타 항목도 정확한 할당자 실측은 아니다.

## 결과 동일성

새 `result_order` 시험에서 고정 seed로 만든 질의 1,000개의 건수와 전체 상위 결과 ID 순서를 변경 전 저장 정렬키 공식(`depth + fold prefix`)으로 계산한 참조 결과와 대조했다. 모두 일치했다. 한글, ASCII 대소문자, 긴 이름, 빈 이름(색인 거부), 숨김 경계도 시험했다. 기존 oracle 시험의 모든 정렬 모드·연속 질의·USN 변경·세그먼트 왕복 시험도 통과했다.

## 변경 파일

- `kfind/names/src/index.rs`: 24비트 `RecMap`, 1바이트 깊이 필드, 힙 열별 보고, 맵 참조 시험.
- `kfind/names/src/build.rs`, `query.rs`: 깊이 저장 및 이전 정렬키 계산 방식 재현.
- `kfind/names/src/seg.rs`: 정렬키 저장 열 변경, 세그먼트 버전 3, 레코드 표 로드/저장 적응.
- `kfind/names/src/lib.rs`: 진단 타입 공개.
- `kfind/names/examples/mem_probe.rs`: 대형 합성 힙·질의 측정.
- `kfind/names/tests/result_order.rs`: 무작위 1,000 질의 참조 비교와 경계 사례.
- `kfind/names/examples/fn1_bench.rs`: Windows 전용 벤치 예제를 Windows cfg 안에 둬 맥에서 예제 타깃이 컴파일되도록 함.

## 시험 결과

| 명령 | 결과 |
|---|---|
| `cargo test --manifest-path crates/kfind/Cargo.toml` (오프라인 확인) | 미실행: 잠금된 `tokio 1.54.1`이 캐시에 없고 캐시에는 1.53.1까지만 있음. Windows 전용 서비스 의존성도 이 맥에서 검증 불가. |
| `cargo test --manifest-path kfind/names/Cargo.toml` | 통과. 아래 `test result` 줄 참조. 네트워크 차단 때문에 캐시된 `errno 0.3.14`로 잠금파일을 임시 조정해 시험한 뒤 저장소 `Cargo.lock`은 원상 복구했다. |
| `cargo test --manifest-path kfind/core/Cargo.toml` | 통과. 아래 `test result` 줄 참조. |

```text
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.91s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.42s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.66s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.78s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.59s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.88s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.77s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.90s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 미확인·한계

- 이 맥에는 Windows/MFT가 없어 실제 PC 색인에서 나온 605 MB 전용 메모리(힙 약 624 MB)를 재측정하지 못했다. 합성 결과 82.967 B/항목은 연속 MFT 레코드 기준이므로, PC의 187 B/항목과 직접 비교할 수 없다. 실제 번호 희소도·분포를 넣은 재측정이 필요하다.
- 변경 후 세그먼트 버전은 3이다. 이전 버전 2의 `names.seg`를 버전 호환으로 읽는 동작은 제공하지 않으며, Windows 서비스에서 재색인/갱신 확인이 필요하다.
- `crates/kfind` Windows 서비스 크레이트와 Windows 전용 시험은 맥에서 검증하지 못했다. Windows 담당자가 서비스 재시작·이전 세그먼트 재생성·USN 갱신을 확인해야 한다.
- 전체 색인 힙 목표는 합성 데이터에서는 100 B 미만이지만, 실제 Windows의 100 B/항목 목표 달성은 아직 확인되지 않았다.
