# KFMEM4 결과 — 이름 색인 메모리 직접 계측

## 요약

`name_index_probe`가 디렉터리 이름·속성만 읽어 1,000,005개 항목을 수집하고, NTFS 서비스 경로와 같은 `Builder::push` 및 `Builder::finish`를 실행했다. 측정 프로세스 안에서 `Names::heap_bytes()`와 Windows `GetProcessMemoryInfo(...).PrivateUsage`를 읽었다. 색인 내용과 파일 이름은 출력·저장하지 않았다.

완성된 색인의 용량 합계는 108.467 B/항목, PrivateUsage는 123.195 B/항목이었다. `shrink_to_fit`은 `heap_bytes`를 전혀 줄이지 않았고 PrivateUsage는 2.83 MB 증가했다. 이는 할당자/프로세스 변동 범위로 보며 절감 효과로 계산하지 않는다. 100개 합성 질의의 결과 전체(각 질의의 건수와 상위 ID 목록)는 전후 일치했다.

이전 서비스 보고는 3,343,333개에서 PrivateUsage 약 605 MB, `heap_bytes` 약 624 MB였다(단위·측정 시점은 명시되지 않음). 이를 187 B/항목으로 환산한 값과 이번 직접 표본의 `heap_bytes` 차이는 78.533 B/항목이다. 이름 길이가 이전 모델보다 큰 점은 9.229 B/항목을 설명한다. 남은 78.533 B/항목은 현재 데이터로 원인별 확정이 불가능하다. 실행 중인 서비스를 조회·중지·변경하지 않았으므로 서비스 수치와 동일 시점의 직접 대조도 하지 않았다.

## 실행과 결과

실행 명령(빌드 캐시는 저장소 밖의 `%TEMP%` 아래):

```powershell
$env:PATH="$env:LOCALAPPDATA\DocFitTools\mingw64\bin;$env:PATH"
$env:CARGO_TARGET_DIR=Join-Path $env:TEMP 'KFMEM4-cargo-target'
cargo run --release --manifest-path kfind/names/Cargo.toml --example name_index_probe -- 1000000
```

예제는 기존 이름 통계 예제와 같이 Windows 시스템·프로그램 파일·사용자 프로필 디렉터리 트리를 읽기 전용으로 걷는다. 75초 또는 1,000,000개에서 멈춘다. 이번 순회는 22.6초, 1,000,000개(디렉터리 170,016, 파일 829,984, 열거 오류 54)에 끝났다. 색인에 더해진 볼륨 루트 5개를 포함해 `Names`에는 1,000,005개가 있었다. 경로·이름 목록은 기록하지 않았다.

이 예제는 수집 중 `Builder::push`를 사용한다. 이는 `kfind/names/src/ntfs.rs::index_volume`과 동일하게 MFT 이름·속성을 Builder에 넣는 호출이며, 이후 볼륨별 레코드 맵·부모 연결·접힘 이름·확장자 표·최종 배열은 같은 `Builder::finish`가 만든다. 다만 입력 원천은 MFT가 아니라 제한된 디렉터리 순회다. 표본 레코드 번호는 볼륨별 조밀 번호이며 실제 MFT 레코드 번호의 희소도를 재현하지 않는다. 서비스는 고정 NTFS 볼륨별로 MFT를 열거하고, 해당 원천에서는 크기·시각을 모름 값으로 둔다. 예제도 MFT와 같이 `Builder::push`를 호출한다. 사용자 모드의 지정 경로 순회, 전체 드라이브 색인, 서비스 후속 USN 적용은 실행하지 않았다.

### 같은 프로세스에서 잰 단계

| 단계 | 항목 수 | 색인 `heap_bytes` | PrivateUsage | 항목당 PrivateUsage |
|---|---:|---:|---:|---:|
| 프로세스 기준(색인 전) | — | — | 614,400 B | — |
| 디렉터리 수집 완료, `Builder` 보유 | 1,000,000 입력 | 해당 없음 | 91,627,520 B | 91.627 B |
| `finish` 직후 | 1,000,005 | 108,467,173 B (108.467 B) | 123,195,392 B (123.195 B) | 123.195 B |
| `Names::shrink_to_fit` 직후 | 1,000,005 | 108,467,173 B (108.467 B) | 126,021,632 B (126.021 B) | 126.021 B |

`Builder`가 보유하는 중간 배열은 `finish(self)`에서 소비되고 반환 전에 Rust 값으로 drop된다. `finish`의 `recmaps`, 원래 부모 배열, 접힘 문자열, CSR 자식 배열, 형제 정렬 조각, DFS 순서·방문 배열·스택도 최종 `Names`를 만든 뒤 함수가 반환하면 drop된다. 이 사실은 수명 분석으로 확인했다. PrivateUsage 스냅샷만으로 drop된 할당의 페이지 회수 여부나 빌드 중 최고 메모리를 분리할 수는 없다. 빌드 후 수치는 완성 색인뿐 아니라 프로세스·Rayon 런타임과 할당자 상태를 함께 포함한다.

### `heap_bytes` 실제 열별 합계

| 열 | 바이트 | B/항목 |
|---|---:|---:|
| 부모 번호 | 4,000,020 | 4.000 |
| 접힘 오프셋 | 4,000,024 | 4.000 |
| 원본 이름 오프셋 | 4,000,024 | 4.000 |
| 깊이 | 1,000,005 | 1.000 |
| 확장자 번호 | 2,000,010 | 2.000 |
| 플래그 | 1,000,005 | 1.000 |
| 크기 | 8,000,040 | 8.000 |
| 수정 시각 | 8,000,040 | 8.000 |
| 하위 트리 끝 | 4,000,020 | 4.000 |
| 접힘 이름 풀 | 33,709,218 | 33.709 |
| 원본 이름 풀 | 35,620,597 | 35.621 |
| 볼륨 레코드 맵 | 3,000,015 | 3.000 |
| 기타(확장자 이름·추정 redirect) | 137,155 | 0.137 |
| **합계** | **108,467,173** | **108.467** |

기본 열은 합계 39 B/항목이며, 이 표본의 원본·접힘 이름 풀은 69.330 B/항목이다. KFMEM3의 평균 이름 30.119 B를 원본·접힘 이름 양쪽에 가정한 99.238 B 모델보다 이번 표본이 9.229 B/항목 크다. 이는 이번 첫 100만 항목 표본의 이름 풀 크기 차이이며 전체 PC 분포로 일반화하지 않는다.

## 88 B 차이 후보별 판정

이전 서비스 보고값 187 B/항목과 이번 표본 108.467 B/항목의 차이는 78.533 B/항목이다. KFMEM3의 99.238 B 모델과 비교해 말하던 약 88 B 차이는 이번 표본의 이름 풀만 반영하면 약 78.5 B까지 좁혀진다.

| 항목 | 바이트 환산 | 근거와 한계 |
|---|---:|---|
| 최종 `Vec` 여유 용량 | 표본에서 0 B/항목 | 열별 용량 합계가 `heap_bytes`에 반영된다. `shrink_to_fit` 전후 차이도 0 B였다. 실제 서비스 색인의 개별 capacity는 읽지 않았으므로 서비스 값에 그대로 적용할 수 없다. |
| 이번 표본의 이름 길이 증가 | 모델 대비 +9.229 B/항목 | 이름 풀 합계가 69.330 B/항목. 서비스 파일명 분포와 같은지는 미확인. |
| 레코드 맵 희소도 | 표본 3.000 B/항목; 서비스 차이 미측정 | `RecMap`은 최대 레코드 번호까지 슬롯을 만든다. 이번 표본 번호는 조밀하다. 남은 78.533 B를 전부 이 열로 설명하려면 실제 표본보다 약 26.18 슬롯/항목이 추가로 필요하다(총 약 29.18 슬롯/항목). 실제 FRN 최댓값·항목 수를 확보하지 않아 이 조건이 성립하는지 모른다. |
| 확장자 `HashMap`·할당자 | `heap_bytes` 기타 0.137 B/항목, 실제 bucket 바이트 미측정 | `heap_bytes`는 확장자 문자열에 `len + 56`을 추정 반영하지만 HashMap bucket/제어 배열 용량은 포함하지 않는다. 따라서 이는 PrivateUsage 차이에 기여할 수 있지만, `heap_bytes`가 187 B가 된 원인으로는 설명할 수 없다. |
| 접힘·원본 이름 풀 | 표본 69.330 B/항목 | 실제 저장 용량 합계로 직접 측정. Unicode 접기 길이는 이번 이름 통계와 별도 집계하지 않았다. |
| 빌드 임시 배열 | 반환된 `Names`에는 남지 않음; 잔류 바이트 분리 불가 | `finish` 반환 시 중간 벡터가 drop되는 것은 코드에서 확인했다. PrivateUsage는 수집 후보다 finish 뒤 31,567,872 B 증가했지만, 이 차이는 최종 배열 할당·스레드 런타임·allocator 상태를 함께 포함하며 임시 자료 잔류량으로 간주하지 않는다. |
| PrivateUsage와 `heap_bytes`의 차 | finish 직후 14,728,219 B (14.728 B/항목) | 프로세스 기준값, allocator, Rayon 및 기타 메모리가 섞인 차이. 소형 할당 헤더·파편화만의 바이트로 분리할 수 없다. |

따라서 **용량 성장 배수·최종 임시 벡터·확장자 표의 추정치만으로 88 B를 설명한다는 가설은 이 표본에서 지지되지 않는다.** KFMEM3의 서비스 보고값은 `heap_bytes` 약 624 MB라고 적혀 있으므로 PrivateUsage/할당자만의 차이로도 설명할 수 없다. 남은 핵심 검증 항목은 서비스와 같은 MFT FRN 희소도, 같은 시점의 원시 `Names::heap_breakdown`, 그리고 187 B 보고값의 단위·분모 일치다.

## 다듬기 및 질의 비교

| 단계 | `heap_bytes` | PrivateUsage | 질의 100개 p50 | 결과 대조 |
|---|---:|---:|---:|---|
| `finish` 직후 | 108,467,173 B | 123,195,392 B | 4.671 ms | 기준 |
| `shrink_to_fit` 직후 | 108,467,173 B | 126,021,632 B | 5.315 ms | 각 질의의 전체 건수와 상위 ID 목록 일치 |
| 변화 | 0 B (0%) | +2,826,240 B | +13.8% | 동일 |

한 번씩 실행한 합성 질의 측정이며 p50 변화는 허용 20% 이내지만, 이 결과는 성능 이득을 뜻하지 않는다. `shrink_to_fit`은 용량을 실제 줄이지 못했다. 따라서 이 메서드를 서비스 빌드에서 호출하지 않는다. 후속 질의 지연은 한 번의 표본이고 측정 잡음 영향도 있다.

예제는 `before_checksum=15791180 after_checksum=15791180 equal=true`를 출력했다. `equal`은 합계만 비교하는 값이 아니라 100개 각각의 `(total, top IDs)` 전체 벡터를 비교한다.

## 서비스 영향과 미확인

- 서비스, 앱, Everything은 실행·중지·질의하지 않았다. 예제는 별도 프로세스에서 파일 내용 없이 디렉터리 이름·메타데이터만 열거했다.
- `shrink_to_fit`을 서비스 경로에서 호출하지 않으므로 서비스 재시작으로 달라지는 점은 없다. 계측 코드를 포함해도 실행 중 프로세스에는 반영되지 않는다.
- 미확인: MFT의 실제 최대 FRN/레코드 밀도와 RecMap bytes; 서비스 색인의 동일 시점 배열별 capacity; 확장자 HashMap bucket·allocator 헤더; `finish` 중 peak PrivateUsage; 서비스 605 MB와 `heap_bytes` 약 624 MB의 단위 및 시점.
- 초기 시범 실행은 `push_meta`를 사용해 MFT와 다르게 크기·시각을 채웠다. 그 결과는 최종 표에 포함하지 않는다. 최종 예제는 `index_volume`과 같은 `Builder::push`를 사용한다.

## 시험

허용된 시험 두 개만 실행했다. 전체 시험이나 다른 Cargo 시험은 실행하지 않았다.

`cargo test --manifest-path kfind/names/Cargo.toml`:

```text
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 29.21s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --manifest-path crates/kfind/Cargo.toml`:

```text
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.66s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.65s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
