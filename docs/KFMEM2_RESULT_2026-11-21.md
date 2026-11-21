# KFMEM2 — kfind 메모리 구조 조사 (2026-11-21)

## 결과 요약

`names_status.heap_bytes`는 프로세스 메모리가 아니라 `Names`의 주요 배열 `Vec` 용량을 더한 근사치다. 확장자 표와 이동 redirect는 일부 추정치만 더하며, `HashMap`의 버킷·할당자 메타데이터, 현재 질의기 캐시, 빌드 도중의 자료, 파이프/스레드 메모리는 포함하지 않는다. 따라서 둘을 직접 비교해 실제 힙 크기로 해석할 수 없다. 기존 결과 보고는 프로세스 전용 메모리 605MB, 색인 힙 약 624MB, 3,343,333개 항목으로 기록했으며 각각의 단위/측정 시점이 같다는 보장은 없다. 합성 `Vec` 추정 83B/항목에 비해 187B/항목은 약 2.25배지만, 이번 표본은 그 차이를 설명하지 못했다.

이 PC에서 서비스는 관찰만 했고 실행·중지·변경하지 않았다. 이름 크레이트 예제로 저장소 폴더 1개를 읽기 전용 순회했으나 2,542개 항목에 그쳐, 수십만~수백만 항목의 원인 재현은 완료하지 못했다. 소표본의 고정 비용이 큰 `PrivateUsage / heap_bytes = 10.319`는 실제 서비스 배수로 일반화할 수 없다. 원인 확정이나 메모리 최적화는 하지 않았다.

## 상주·임시 자료 목록

| 구조·경로 | 수명과 증가 정책 | `heap_bytes` 반영 | 판단 |
|---|---|---|---|
| `Names` 본체 배열: `parent`, `fold_off`, `name_off`, `depth`, `ext`, `flags`, `size`, `mtime`, `end`, `fold`, `name` | 색인 수명 동안 상주. 빌드 시 `Vec::with_capacity(total)` 등으로 채우며, 이후 USN delta는 `push`/`extend`로 기본 `Vec` 증가 정책을 따른다. `shrink_to_fit` 호출 없음. | 각 배열의 `capacity × 원소 크기`를 합산 | 핵심 영구 색인 용량은 반영한다. 용량 초과분도 `capacity`에 포함된다. |
| 볼륨 `RecMap` | MFT 레코드 번호를 인덱스로 써 최대 레코드 번호까지 3바이트 packed map을 할당한다. 24비트 초과 값은 `u32` map으로 전환한다. USN 변경으로 더 큰 번호가 오면 `resize`한다. | packed/wide의 실제 `Vec.capacity`를 합산 | 희소한 레코드 번호 분포가 공간에 영향을 준다. |
| 확장자 `ExtTable`의 `names: Vec<Vec<u8>>`, `map: HashMap<Vec<u8>, u16>` | 고유 확장자마다 문자열을 `names`와 HashMap 키에 중복 보관. 확장자 가짓수에 비례. | 이름 문자열은 `len + 56` 추정만 포함하고 HashMap bucket/제어 바이트·키 할당자 오버헤드는 빠짐 | 수십 종의 확장자라면 전체 항목 수에 비례하는 100B/항목 설명은 되지 않는다. 정확한 바이트는 미측정. |
| `redirect: HashMap<u32,u32>` | 이름 변경/이동 때 디렉터리 redirect를 추가. 압축 때 새 기본판에 다시 구성. | 항목당 16B로 추정. 실제 버킷 용량은 미포함. | 보통 delta 크기와 연동. |
| `Builder` 입력 열: 볼륨·레코드·부모·속성·크기·시각·원본 이름 및 오프셋 | MFT 열거/디렉터리 순회에서 누적. 기본 `Vec` 성장, 사전 `with_capacity`는 서비스 경로에서 사용하지 않는다. `finish(self)` 도중 출력과 동시에 살아 있다가 반환 시 drop. | 제외 | 빌드 중 peak 메모리. drop 후 allocator가 OS에 페이지를 돌려주는지는 allocator 동작에 달림. |
| `walk::index_tree`의 `Ctx.out: Vec<Vec<Rec>>`, 디렉터리별 `batch`, 하위 경로 UTF-16 `Vec`, Rayon 작업 | 파일 시스템 순회 중 임시. 각 디렉터리에서 묶음을 만들고 Builder로 복사한 뒤 drop. Rayon worker 풀은 프로세스 수명 동안 남을 수 있음. `shrink_to_fit` 없음. | 제외 | user 지정 경로 등 walk 원천의 빌드 peak에만 해당. |
| `Builder::finish`의 `recmaps`, `oparent`, 이름 참조, `folds: Vec<String>`, CSR `cnt/fill/kids`, sibling slice, DFS `order/seen/stack`, 최종 배열 | 결과 `Names`를 만드는 동안 중첩 생존. 함수 반환 시 Builder/중간 배열은 drop. 각 `Vec`가 필요한 예상 길이만큼 capacity를 예약하지만 문자열별 fold 할당, `clone` 등 별도 할당도 있음. `shrink_to_fit` 없음. | 최종 `Names`에 남은 것만 포함 | peak 및 allocator 잔류를 검증할 대규모 계측이 필요하다. |
| USN 읽기 buffer와 변경 목록 | 볼륨 follower 스레드별 `Vec<u8>`는 64KiB로 resize되고 재사용. 변경 목록 `Vec<Change>`는 읽기마다 생성. `apply`는 색인 배열·recmap·redirect를 늘릴 수 있음. | 현재 배열 증가는 최종 `heap_bytes`에 잡히지만 USN buffer/일시 변경 목록, HashMap 여유 용량은 제외 | 정상 유휴 buffer는 follower 수에 따라 작다. 변경 폭주 시 임시 peak는 미측정. |
| compaction 및 교체 | delta가 60,000개 또는 현재 항목의 5%를 넘으면 `compact()`로 새 색인을 빌드하고 `*g = Some(names)`로 교체. 잠금을 기다리던 이전 읽기가 끝날 때까지 구 색인이 남을 수 있음. | 응답 시점의 현재 `Names` 하나만 반영 | 교체 순간 두 색인이 공존할 수 있다. 대량 delta가 없는 정규 초기 구축에는 적용되지 않는다. |
| SID별 `Searcher` 캐시 `ss` | 질의가 시작되면 SID별 Searcher를 저장. 65번째 신규 SID 진입 때 기존 맵을 비운다. `Searcher.max_keep = 1 << 24`; 마지막 결과 ID `Vec`를 이 한도까지 유지하고 세대 변경은 다음 질의에서만 무효화한다. | 제외 | 넓은 검색 결과와 여러 SID가 있으면 항목 수에 비례해 추가 상주 메모리가 될 수 있다. 현재 PC에서 실제 캐시 크기는 미확인. |
| SID별 deny 캐시, 파이프/문서 Store·Index 맵, 직렬화 응답·경로 조립 | deny는 SID별 `Vec<u32>`; 파이프에는 문서 인덱스/Store/활성 요청 맵이 별도 존재. `View::dir_path`/`full_path`는 질의 결과 경로를 만들며 영구 경로 캐시는 없다. | 제외 | 해당 서비스 프로세스 메모리에는 포함되지만 `Names.heap_bytes`에는 포함되지 않는다. |

`Vec`의 정확한 성장 배수는 Rust API 계약으로 고정되어 있지 않아 특정 배수로 단정하지 않았다. 확인한 코드에는 이 구조들을 대상으로 하는 `shrink_to_fit` 호출이 없다.

## 실측 1회

예제 `kfind/names/examples/tree_mem_probe.rs`는 서비스/파이프와 무관하게 `walk::index_tree`와 `Builder::finish`를 직접 호출한다. 실행 인자는 임시 폴더가 아닌 읽기 전용 저장소 경로였고, 서비스·앱·Everything에는 접근하지 않았다.

실행 명령(실제 경로는 저장소에 기록하지 않음):

```powershell
$env:PATH="$env:LOCALAPPDATA\DocFitTools\mingw64\bin;$env:PATH"
$env:CARGO_TARGET_DIR='<저장소 밖의 빌드 캐시 경로>'
cargo run --release --manifest-path kfind/names/Cargo.toml --example tree_mem_probe -- <읽기 전용 디렉터리>
```

실행 결과 원문:

```text
baseline_private_bytes=659456
walk entries=2542 dirs=194 files=2347 errors=0 private_bytes=2314240 elapsed_ms=16.9
built entries=2542 heap_bytes=191726 private_bytes=1978368 ratio_private_to_heap=10.319 finish_ms=1.8
heap_breakdown=HeapBreakdown { parent: 10168, fold_offsets: 10172, name_offsets: 10172, sort_keys: 2542, extensions: 5084, flags: 2542, size: 20336, mtime: 20336, subtree_ends: 10168, folded_names: 43032, names: 46766, volume_record_maps: 7626, other: 2782 }
queries=100 p50_ms=0.004 checksum=0 post_query_private_bytes=1978368
```

측정 1회는 완료했지만 항목 규모가 목표에 미달한다. 1.9MB 프로세스 private bytes와 192KB 추정 배열은 프로세스/런타임 고정비 영향을 크게 받는다. checksum 0인 합성 질의라 p50도 실제 유효 결과가 있는 서비스 질의와 비교할 수 없다. 이 측정으로 서비스의 187B/항목과 83~91B/항목 차이를 원인별로 나누거나 allocator 파편화·임시 메모리 반환 여부를 확정할 수 없다.

## 수정·전후 비교

원인 후보(빌드 임시 자료의 allocator 잔류, 확장자 HashMap 추정 누락, SID별 Searcher 결과 캐시, 교체 순간의 구·신 색인 공존)는 코드상 존재하지만 대규모 동일 프로세스 계측으로 크기를 확인하지 못했다. 잘못된 원인에 `shrink_to_fit`/캐시 제한을 적용하면 빌드·질의 성능이나 동작에 영향을 줄 수 있어 코드 변경은 하지 않았다.

| 상태 | PrivateUsage | `heap_bytes` | 질의 100개 p50 | 항목 |
|---|---:|---:|---:|---:|
| 변경 전 소표본 | 1,978,368 B | 191,726 B | 0.004 ms (checksum 0) | 2,542 |
| 변경 후 | 해당 없음 | 해당 없음 | 해당 없음 | 코드 변경 없음 |

기존 검색 결과와의 변경 전후 대조 및 속도 악화율은 코드 수정이 없어 해당하지 않는다. 사용자 서비스 색인에서 반복한 결과 검증은 하지 않았다.

## 서비스 적용·미확인

코드 수정이 없으므로 재시작으로 달라지는 점은 없다. 향후 메모리 감소 패치를 적용하면 실행 중 서비스에는 반영되지 않으므로 사용자가 서비스를 재시작해야 한다.

- 미확인: 서비스의 `heap_bytes` 원시값과 PrivateUsage를 같은 순간에 다시 읽은 대규모 비교, 기존 보고의 두 수치 차이 원인.
- 미확인: 실제 SID별 Searcher 개수·보유 ID 용량, 확장자 HashMap의 bucket capacity, delta·경로 출력 시점의 peak.
- 미확인: `Builder::finish` 뒤 allocator가 페이지를 반환하는지와 초기 MFT 열거 임시 자료의 peak.
- 이번에 실행한 범위: `kfind/names`·`crates/kfind` 테스트만. 전체 시험은 실행하지 않았다.

## 실행한 시험

```text
cargo test --manifest-path kfind/names/Cargo.toml
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.82s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.70s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

cargo test --manifest-path crates/kfind/Cargo.toml
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.95s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.81s
```
