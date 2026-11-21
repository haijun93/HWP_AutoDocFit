# THUMBRS2 결과 (2026-11-22)

## 범위와 구현

- `image = 0.25` 의존성을 추가하고 JPEG/PNG/GIF/BMP/WebP/TIFF 기능만 켰다. `klib thumb <입력> --size N [--quality Q] --out <출력.jpg>`를 구현했다.
- EXIF Orientation 1~8 보정, Lanczos3 축소, JPEG 출력, 512 MiB 입력·64 MP 이미지 상한과 오류 반환을 적용했다. 작은 이미지는 확대하지 않는다.
- `scripts/thumb_compare.py`는 Windows `klib.exe` 경로를 처리하고 손상 파일 패닉 여부도 검사한다. 사진뷰 서비스 연결은 하지 않았다.

## 빌드와 테스트

Windows x64, Python 3.14.0, Pillow 13.0.0, `PATH` 앞 MinGW, 환경변수 `CARGO_TARGET_DIR=E:/docfit-tmp/cargo-target-THUMBRS2`에서 수행했다. 저장소 설정 파일에 PC 전용 경로는 추가하지 않았다.

```text
cargo build --release --manifest-path klib/Cargo.toml
Finished `release` profile [optimized] target(s) in 11m 55s

cargo test --manifest-path klib/Cargo.toml
test result: 21 passed; 0 failed; 0 ignored; 0 measured
test result: 22 passed; 0 failed; 0 ignored; 0 measured
test result: 127 passed; 0 failed; 1 ignored; 0 measured
test result: 3 passed; 0 failed; 0 ignored; 0 measured
test result: 0 passed; 0 failed; 0 ignored; 0 measured
```

첫 성공 빌드 이후 `thumb.rs` 변경 빌드는 274.5초 및 373.9초였고, 변경 없는 빌드는 0.68초 및 1.21초였다. 전체 테스트를 새 debug 아티팩트에서 처음 빌드하는 데 4분 22초가 걸렸다. 기존 프로젝트 경고는 남아 있으나 오류는 없었다.

## 의존성 라이선스

`cargo tree -e normal -p image`와 `cargo metadata --format-version 1`로 `Cargo.lock`에 새로 추가된 18개 패키지의 라이선스를 확인했다. Cargo가 `image 0.25.10`을 선택했다.

| 신규 전이 의존성 | 버전 | Cargo metadata 라이선스 |
|---|---:|---|
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| color_quant | 1.1.0 | MIT |
| fax | 0.2.7 | MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| gif | 0.14.2 | MIT OR Apache-2.0 |
| half | 2.7.1 | MIT OR Apache-2.0 |
| image | 0.25.10 | MIT OR Apache-2.0 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| png | 0.18.1 | MIT OR Apache-2.0 |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| quick-error | 2.0.1 | MIT/Apache-2.0 |
| tiff | 0.11.3 | MIT |
| weezl | 0.1.12 | MIT OR Apache-2.0 |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib |

신규 항목은 모두 MIT 또는 Apache-2.0 선택지가 있다. 일부에는 Zlib/Unlicense 또는 BSD 선택지가 함께 있고, `moxcms`와 `pxfm`은 Apache-2.0 선택이 가능하다. `zlib-rs`는 이미지 의존성 트리에도 보이지만 기존 잠금 파일에 이미 있던 패키지이며 신규 추가분은 아니다.

## 바이너리 크기

같은 `github/main` 기준 커밋을 임시 worktree에서 같은 release 설정으로 빌드해 비교했다.

| 기준 | 크기 |
|---|---:|
| 썸네일 추가 전 `klib.exe` | 10,537,472 bytes |
| 썸네일 추가 후 `klib.exe` | 11,569,664 bytes |
| 증가량 | 1,032,192 bytes (+9.8%) |

증가량은 약 1.0 MiB로, 현재 포함한 여섯 포맷 기능을 유지하는 범위에서는 제한적이다.

## Pillow 대조

`python scripts/thumb_compare.py`는 임시 폴더에 생성한 사례만 사용했다. 결과는 19개 사례 모두 출력 크기(가로·세로)가 일치했다. EXIF Orientation 1~8 모두 예상 방향의 출력 치수를 만들고 패턴 방향도 확인했다. 투명 PNG, 그레이 PNG, 팔레트 GIF, 작은 이미지, 극단 종횡비와 손상 JPEG를 포함했다. 손상 JPEG는 exit 1, `panic_free=True`였다.

| 지표 | 결과 |
|---|---:|
| 사례 | 19 |
| 크기 일치 | 19/19 |
| 평균 사례 MAE 평균 | 16.7861/255 |
| 관측 최대 픽셀 차이 | 192/255 |
| 기준 | 3/255 |
| 기준 통과 | 아니오 |

3/255 초과는 모두 16개 사례였다. 축소하지 않는 64×32 사례도 MAE 22.9603/255였고 EXIF 1~4는 약 18.2/255, 수정한 EXIF 5~8은 약 18.2/255였다. 따라서 이 합성 패턴에서 가장 큰 차이는 축소 필터만으로 설명되지 않는다. `image`의 zune JPEG 인코더와 Pillow가 쓰는 libjpeg의 양자화·색차 샘플링 결과 차이가 지배적이며, 고주파 합성 패턴에서는 Lanczos 구현 차이가 추가된다. 현재 허용 기준을 만족한다고 볼 수 없다. 3/255가 필수 제품 기준이면 다음 단계에서 libjpeg 호환 인코더를 선택하거나 인코더 출력을 기준으로 다시 설계해야 한다.

## 4000×3000 속도 비교

스크립트가 만든 합성 JPEG 20장을 각각 한 번 처리했다. 이 한 번 측정에서는 Python이 더 빨랐다.

| 구현 | 총 시간 | 처리량 |
|---|---:|---:|
| Pillow | 3.646초 | 5.49장/초 |
| Rust `klib thumb` | 10.822초 | 1.85장/초 |

Rust 비교는 프로세스 시작을 파일마다 포함한다. 이 결과만으로 최적화 후 성능을 단정할 수는 없으며, 이번 측정에서는 Rust가 속도 이점을 보이지 않았다.

## 연결 지점 제안

다음 THUMBRS3에서 `docfit_app/photo_album_service.py:PhotoAlbumService.get_thumbnail` (현재 1033행)의 썸네일 생성 구간에서 `klib.exe thumb`를 호출하는 어댑터를 둔다. 기존 크기·캐시 경로·base64 반환과 오류 응답 규약은 Python 서비스가 유지하고, Rust는 캐시 미스 때 생성만 담당하도록 연결한 뒤 서비스 시험에서 Pillow 결과와 호환성을 확인한다.

## 미확인

- 3/255 픽셀 오차 허용 기준은 미달이다. 특히 Pillow/libjpeg와 Rust zune 인코더 간 호환성 개선 또는 제품 허용치 결정이 필요하다.
- 속도는 20장 1회 측정뿐이며, 반복 분산 및 다양한 실제 사진 분포는 측정하지 않았다.
- 허용 포맷별 손상 파일은 JPEG 한 사례만 확인했다. EXIF orientation 파서는 JPEG APP1의 TIFF IFD0 경로만 직접 처리한다.
- 사용자 사진이나 사진뷰 서비스의 Rust 연결은 다루지 않았다.
