# HIERX 검색 차이 4건 원인

2026-11-10 debug 빌드 비교 결과.

- 질의 목록의 `목적`과 `기준`이 각각 두 번 등장해 query diffs=4로 집계됐다.
- Python은 KIDX13_reference.hwp를 kordoc 런타임 부재로 색인하지 못한다.
- Rust는 별도 설치된 런타임을 발견해 같은 파일의 3개 레코드를 색인한다.
- 이 파일의 표에 `목적으로`, `기준`이 있어 Rust 검색 결과에 파일 하나가 추가된다.
- 사용자가 지정한 해당 파일 1건만 명시적으로 제외하면 41개 파일, 260개 문단의 문서·위계·본문·검색 차이가 모두 0이다.
- timport_too_wide.hwpx는 양쪽 색인 모두 empty 오류이며 문서 결과 차이가 없다.

재현:

```powershell
python tools/kidx_index_regress.py --dump --exclude tests/fixtures/KIDX13_reference.hwp
python tools/kidx_regress.py --dump --exclude tests/fixtures/KIDX13_reference.hwp
```

검색 엔진 또는 기존 형식의 색인 결과는 변경하지 않는다. 비교 도구의 --dump에 검색 결과 서명을 추가했으며, 제외는 사용자가 --exclude로 지정한 파일만 출력 후 적용한다. 기본 실행은 종전처럼 모든 파일을 비교한다.
