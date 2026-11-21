# PDF 압축 이미지 PDF 방식 결과 (2026-11-21)

## 변경

- `docfit_core/pdf_compress.py`: 기존 글자 보존 압축 뒤 목표 미달일 때 기존 `convert_to_raster_pdf` 엔진으로 이미지 PDF 단계(150/120/100/85/72dpi, JPEG 품질 82/72/62/52/42)를 시도한다. 사용자는 자동 전환·글자 보존·이미지 PDF 사용을 고를 수 있다.
- 결과 상태에 `method`와 `text_preserved`를 넣었다. 자동 전환 결과 안내에 글자 선택·검색이 불가능하다고 표시한다. 쪽 수·실효 쪽 크기를 확인하고, 원본은 읽기만 한다. 취소는 쪽 렌더링 사이와 품질 단계 사이에서 확인한다.
- 보안 PDF는 이미지 PDF 선택/전환을 거부한다. 해당 방식으로 바꾸면 원본 권한·암호를 유지할 수 없기 때문이다. 전자서명 PDF도 기존 거부 규칙을 유지한다.
- ② JPG 등 여러 이미지의 총 크기 압축은 이 카드에서 구현하지 않았다. 후속 설계 제안: 파일별 해상도·JPEG 품질·최대 변 길이 단계와 전체 목표 크기 조정, 투명도 보존 형식 선택, 원본 옆 저장, 미리보기 비교, EXIF 위치 제거 옵션.

## 시험과 측정

시험 PDF는 `tests/test_pdf_compress.py`가 임시 폴더에 만든 1쪽 벡터 경로 PDF이며 사용자 PDF를 사용하지 않았다.

| 방식 | 파일 크기 전 | 파일 크기 후 | 목표 | 상태 |
|---|---:|---:|---:|---|
| 글자 보존 그림 압축 | 54,043 B | 20,380 B | 1 B | `method=preserve_text`, `text_preserved=true`, `reached_target=false` |
| 자동 이미지 PDF 전환 | 54,043 B | 9,306 B | 10 KiB | `method=image_pdf`, `text_preserved=false`, `reached_target=true`, 72dpi·품질 42 |
| 자동 이미지 PDF 전환 | 54,043 B | 9,306 B | 1 B | `method=image_pdf`, `text_preserved=false`, `reached_target=false` |

실행한 시험 명령 및 결과:

- `python -m unittest discover -s tests -p 'test_pdf_compress.py'` — **13개 통과**.
- `python -m unittest discover -s tests -p 'test_bridge_methods.py'` — **8개 통과**.
- `flutter analyze` (작업 폴더 `flutter_ui`) — **No issues found!**
- `flutter build web --release` (작업 폴더 `flutter_ui`) — **Built build\web** (140.5초).
- 계측 스크립트 출력: `원본=54043 bytes`; `그림 재압축=20380 bytes; method=preserve_text; text_preserved=True; reached_target=False`; `자동 전환(10KB)=9306 bytes; method=image_pdf; text_preserved=False; reached_target=True`; `이미지 PDF 최저 단계(1B)=9306 bytes; method=image_pdf; text_preserved=False; reached_target=False`.

`.venv\Scripts\python.exe`는 저장소에 없어 관련 시험은 시스템 Python 3.14.0으로 실행했다. 한/글 COM 실기, Windows PDF 뷰어별 확인은 하지 않았다. 보안 PDF는 이미지 PDF 전환 거부만 단위 시험으로 확인했다. JPG 다건 크기 압축은 미구현이다.
