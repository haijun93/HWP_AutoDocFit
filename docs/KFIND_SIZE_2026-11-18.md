# KFIND 크기·색인·count 벤치마크 (2026-11-18)

## 변경

`kfind/core/src/segment.rs`에서 낱말 posting 임시 맵의 키를 `Box<[u8]>`에서 `Vec<u8>`로 바꿨다. 기존 코드는 `&[u8]`를 맵에 넣을 때 Box 할당/복사를 했고, 이후 posting 작성 시 다시 `Vec<u8>`로 복사했다. 변경은 posting 직렬화 전에 한 번만 키를 소유하게 하며, 디스크 형식과 검색 평가 코드는 그대로다.

- `FORMAT_VERSION=4`, Python `EXPECTED_VERSIONS` 변경 없음 (포맷 바뀌지 않음)
- 결과 비교: 3개 쿼리 × off/dict/bigram 총 9조건에서 문장 수, 문서 수, 상위 20개 결과가 HEAD와 동일
- 색인 크기: 파일 구성 및 전체 바이트 동일

## 측정 방법

Windows x64, Rust release, `kc1_bench` 사용. 재현 가능한 합성 입력은 300문서 × 12문장(3,600문장), 세 가지 고정 문장 조합을 순환시켜 만들었다. 인덱스는 1 segment로 작성했고, 각 인덱스 작성은 새 프로세스에서 측정했다. 쿼리 p50은 7회 반복, count/search는 31회 반복의 중앙값이다. HEAD 기준 소스는 임시 디렉터리에서 빌드했다. 사용자 문서나 PC 전용 corpus 경로는 사용하지 않았다.

| 항목 | HEAD | 변경 | 차이 |
|---|---:|---:|---:|
| 색인 크기 | 2,064,053 B | 2,064,053 B | 0% |
| 색인 시간 | 0.546 s | 0.215 s | −60.6% |
| 빌드 peak working set | 17.7 MiB | 16.8 MiB | −5.1% |
| 빌드 peak private bytes | 15.2 MiB | 14.4 MiB | −5.3% |

세 쿼리의 count/search p50 (ms):

| Query | HEAD count | 변경 count | HEAD search | 변경 search |
|---|---:|---:|---:|---:|
| 안전관리계획 | 0.3004 | 0.2712 | 0.3191 | 0.2945 |
| 검색엔진색인 (0 hit) | 0.0198 | 0.0177 | 0.0199 | 0.0175 |
| 업무기록회의 | 0.5113 | 0.2789 | 0.5609 | 0.3001 |

쿼리별 결과 집합과 상위 순위는 모두 동일했다. count API는 기존에도 ranking/hit materialization을 생략하는 별도 경로이며, 이번 변경은 색인 키 할당을 줄여 count와 search 모두에 간접 효과를 낸다. count 전용 알고리즘 자체는 바꾸지 않았다.

## 판단 및 제한

색인 크기/정수 코덱 변경은 저장 포맷을 바꾸고 사용자 요청에서 버전 및 README/TODO 변경을 금지했으므로 적용하지 않았다. 같은 포맷 안에서 크기 변화는 없었다. 색인 peak 메모리는 working set/private bytes를 기록했다. 작은 합성 corpus 한 점의 결과이므로 실제 대형 corpus의 성능을 보장하지 않는다.

검증:

- `cargo test --manifest-path kfind/core/Cargo.toml` 통과 (전체)
- `python -m unittest tests.test_kfind_versions_match` 통과
- `crates/kfind` 시험 실행은 이 환경의 GNU target에 `dlltool.exe`가 없고, 설치된 MSVC target도 Visual Studio linker 대신 Git `link.exe`가 선택되어 링크 실패

커밋/푸시하지 않았다.