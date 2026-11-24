# KORDOCC1 — Markdown → HWPX 생성기 1단계

- 브랜치 `feature/KORDOCC1`, 시작점 `bf624ca5`(작업 시작 시 로컬 `github/main`과 동일, 원격 조회 없음).
- Python 내부 모듈 구현. 사용자 카드의 Python 허용을 적용했으며 추가 의존성·앱 연결·푸시·사용자 문서 사용 없음.
- 판정: **생성기 1단계 구현, kordoc 전면 대체는 보류**. 구조 유효성 11/11, 합성 정답 대비 10/11, kordoc 기준본 대비 5/11. 한/글 2020 열기·사본 저장·저장 전후 구조/텍스트 보존 11/11, 각 1쪽.
- 신규 19개 + 기존 harness 42개 = 단위 시험 61개 통과. 전체 Python 시험은 중간 커밋의 별도 사본에서 실행 후 아래에 기록한다.

## 1. 현재 Markdown → HWPX 호출 조사

| 호출 위치 | 입력·조건 | 호출과 프리셋 | 주요 소비 경로 |
|---|---|---|---|
| `docfit_core/kordoc_bridge.py:617` | MD 파일 + 선택 프리셋 + 선택 그림 디렉터리 | `generate <md> -o <hwpx> --preset <보고서/계획서/기안문> --silent [--image-dir …]` | 공용 생성 진입점 |
| `docfit_core/kordoc_bridge.py:415,452` | HWP/HWPX/HML/PDF/XLS/XLSX/DOCX/PPTX/PNG/JPG/JPEG/WebP → 임시 MD. 이미지·PDF는 조건부 OCR | `parse_document` → `generate_hwpx(markdown, preset, …, image_dir=temp_dir)`; 기본 보고서 | `convert_to_hwpx`; 새 파일만 게시 |
| `docfit_core/file_conversion.py:423,485` | MD/Markdown 직접 입력; HTML은 Pandoc으로 MD를 만든 후 재진입 | 임시 UTF-8 MD → `generate_hwpx(…, "보고서", …)` | 독립 파일변환; PDF·PNG/JPG 출력도 중간 HWPX를 한/글로 렌더링. TXT는 별도 XML 생성 경로 |
| `docfit_core/file_conversion.py:493,547` | GIF/TIFF 포함 이미지 프레임 정규화 → OCR → 프레임 MD 합침 | `generate_hwpx(…, "보고서", …, image_dir=work)` | 이미지 → 편집 가능한 HWPX |
| `docfit_engine/formatter_text.py:909` | 라벨/AI 개조식 직접 처리에 해당하지 않는 MD; 그림이 있는 MD 포함 | 원본 MD → `generate_hwpx(…, "보고서", …, image_dir=source.parent)` | 외부 문서 후처리 전 HWPX 준비; 엔진 불가 시 텍스트 대체 |
| `docfit_engine/formatter_text.py:923,929` | 지원 오피스 입력 또는 DOC/DOCX/PDF의 파싱 MD | `convert_to_hwpx` 또는 `parse_document` → 보고서 생성, PDF는 이후 서식 이식 | 후처리 파이프라인 |
| `docfit_core/mcp_service.py:47,79` | AI 답변 문자열을 검증한 임시 `answer.md` | `generate_hwpx(markdown, preset, staged)` | MCP `create_hwpx_from_ai_answer`; 별도 새 출력 파일 |
| `scripts/compare_conversion_fidelity.py:48,51` | 합성 MD 또는 합성 오피스 입력의 파싱 MD | MD는 직접 보고서 생성, 나머지는 `convert_to_hwpx` | KORDOCB2 기준본 생성 |

호출 지점은 정적 조사다. 실제 사용자별 호출 빈도는 **확인 못 함**. 준비 상태 조회·텍스트 추출만 하는 타임머신/검색은 생성 호출 표에서 제외했다. 앱 소비 경로는 이번 코드와 연결하지 않았다.

## 2. 구현·서식·출력 경계

- `docfit_core/markdown_hwpx.py`: `generate_hwpx(markdown_text, new_target)` → 경로·경고·본문 문단 수·표 수. 문자열 API이므로 입력 파일과 인코딩 판단은 후속 어댑터의 책임이다.
- 상태 순서: 입력 해석 → ZIP/XML 조립 → 임시 파일 구조 검증 → 배타적 새 파일 생성. 기존 파일은 덮어쓰지 않고 검증 실패 시 최종 결과를 만들지 않는다. COM·kordoc·네트워크 호출 없음.
- `kfind/annual/src/hwpx.rs`의 mimetype 우선 무압축 ZIP, header/section 분리, 170mm 네이티브 표·셀 좌표·폭 배분을 Python으로 이식했다. `hwpx_fmt`는 기존 문서의 XML/서식 후처리 crate이므로 실행 의존으로 붙이지 않았다. 이 카드에서 Rust 빌드는 하지 않았다.
- 제목 #~###: 수준별 `제목 1~3`/`Heading 1~3` 스타일, 한컴돋움 16pt 굵게. 본문·표: 휴먼명조 11pt, 표 머리 #E7EEF8·굵게, 줄간격 기본 160%. 결정서 2절은 기본 서식 수치를 다시 적지 않아 같은 결정서 1절의 공공 보고서 수치를 적용했다.
- □·ㅇ·-·※ 및 -/*/+/1./1) 항목: 기호와 본문 사이 일반 스페이스 정확히 1개, 탭 없음. 일반 *·+는 -로 표기, 숫자 시작값은 보존한다. 깊이는 앞 빈칸 2개씩으로 보관하며 문단 내어쓰기·왼쪽 여백은 0 기본값이다. 한/글 측정 후 후속 서식 통일 단계에서 맞춘다. 자동 번호 컨트롤은 쓰지 않는다.
- 문단은 빈 줄로 구분한다. 문단 안 원문 줄바꿈·두 칸 줄바꿈·역슬래시 줄바꿈·`<br>`는 `hp:lineBreak`로 보존한다. 굵게·기울임·동시 강조·중첩 강조는 실제 글자 모양을 사용한다. 파이프 표는 `hp:tbl`, 이스케이프 파이프 및 셀 줄바꿈을 지원한다. 부족한 셀은 빈칸, 초과 열은 오류로 처리한다.
- 그림·4~6단 제목·코드 전용 서식·링크 대상은 미지원 경고. 그림 Markdown 참조와 4~6단 제목 표식은 일반 글자로 남기고, 코드 내용·링크 표시 글자는 보존한다. 미지원 그림을 숨기지 않기 위해 B2 `doc_basic`의 그림을 11종에 그대로 남겼다.

## 3. 비교 방법과 추출기 보완

- `docfit_core/markdown_hwpx_fixtures.py`: B2의 `doc_basic`(그림 포함), `doc_tables`, `doc_text_only`를 재사용하고 제목·공공 기호·일반 항목·번호·3단 중첩·강조·줄바꿈·혼합 문서를 더한 **서로 다른 Markdown 11종**. B2의 11개는 MD·HTML·DOCX·XLSX·PPTX 파일 합계이므로 같은 개수의 MD 11개와 구분한다.
- 독립 블록 명세에서 기대 구조를 만들었다. 자체 생성본을 정답으로 역생성하지 않는다. 입력 SHA-256, 프리셋, 경고, 산출 해시와 실행 결과를 저장한다.
- 실제 설치 kordoc **4.24.9**, 프리셋 **보고서**, 동일 입력 바이트·동일 그림 디렉터리 사용. 설치·다운로드 없이 해당 프로세스의 환경변수로 기존 런타임을 지정했다.
- `conversion_fidelity.compare_sets`·기존 CLI와 동일한 지표/판정 함수 사용. 구조 100%, 재현율 ≥99.5%, 정밀도 ≥99% 그대로 유지. `generate` 종료 코드 1은 실패 fixture가 있다는 뜻이다.
- KORDOCB2가 실제 출력에서 미검증이라고 남긴 추출 규칙을 보완했다: `hp:t` 자식의 tail(줄바꿈 뒤 글자), ※ 등 공공 기호, 문단 왼쪽 여백에 기록된 목록 깊이, 이름이 `__kordoc_hN`인 제목 장식 표. 제목 표에서 자동 생성된 로마 장 번호는 원문 본문이 아닌 장식으로 제외한다. 일반 1칸 표는 데이터 표로 유지한다.
- 왼쪽 여백 깊이는 사용된 기호 문단의 서로 다른 여백을 오름차순으로 정규화한 값이다. 임의 서식 실문서에서 논리 수준과 일치하는지는 **확인 못 함**. 한/글 빈칸 들여쓰기와 native BULLET/NUMBER 수준도 기존 경로로 처리한다.
- B2 기존 시험은 수정하지 않았다. 보완한 추출 규칙을 신규 회귀 시험으로 검증했다. 기준본에서도 3단 제목의 목록 변환·번호 목록 유형 변경·일부 기호별 여백 변경·literal `<br>` 잔존이 있어, 후보만의 결함과 구분한다.

## 4. 비교 결과

| 합성 문서 | 정답 → 자체 | kordoc → 자체 | 자체 재현율/정밀도 | 차이 원인 |
|---|---|---|---|---|
| doc_basic | 불합격 | 불합격 | 100% / 86.90% | 그림 1→0, 참조 글자 추가. kordoc은 3단 제목을 □ 항목으로 만들고 번호 목록을 기호 목록으로 바꿈 |
| doc_tables | 합격 | 합격 | 100% / 100% | 표 2개·행열·셀 글자 보존 |
| doc_text_only | 합격 | 합격 | 100% / 100% | 본문 보존 |
| headings | 합격 | 불합격 | 100% / 100% | kordoc의 ###가 기호 목록인 반면 자체는 3단 제목으로 보존 |
| public_markers | 합격 | 불합격 | 100% / 100% | kordoc의 기호별 문단 여백으로 추출한 깊이 차이(75% 일치) |
| bullets | 합격 | 불합격 | 100% / 100% | kordoc이 * 항목에 다른 여백 적용: 깊이 66.67% 일치 |
| ordered | 합격 | 불합격 | 100% / 100% | kordoc의 1. 목록이 □가 되어 번호/기호 유형 0% 일치 |
| nested | 합격 | 합격 | 100% / 100% | 3단 깊이 보존 |
| emphasis | 합격 | 합격 | 100% / 100% | 실제 bold/italic 및 표 강조 보존 |
| linebreaks | 합격 | 불합격 | 100% / 100% | kordoc에 `<br>` 문자열이 남음. 기준 대비 재현율 84.85%, 정밀도 100% |
| mixed | 합격 | 합격 | 100% / 100% | 한·영·숫자·특수문자·이스케이프 파이프·목록·표 보존 |

`자체 재현율/정밀도`는 독립 정답 기준이다. kordoc → 자체의 전체 지표와 정답 → kordoc의 별도 결과는 동봉 기계 보고서를 참고한다. 문서별 합격은 모든 지표 동시 통과이며 평균으로 상쇄하지 않았다. 정답 → kordoc도 5/11이다. 기준본을 그대로 흉내 내도록 자체 제목/번호를 낮추지는 않았다.

- 3회 반복: 자체 33개 산출물의 동일 입력별 ZIP 바이트 동일, kordoc 33개 산출물의 추출 구조 동일. kordoc ZIP 바이트 동일성은 판정하지 않았다.
- 구조 검증: ZIP/XML·mimetype 첫 항목 무압축·OPF 참조·표 행열/좌표·170mm 폭·탭 없음, 11/11 통과.
- 첫 COM 시험은 11/11 열기 실패였다. 원인은 annual 골격의 OPF 네임스페이스 끝 `/` 및 ZIP 루트 기준 `href` 누락. 합성 빈 문서와 조합 시험으로 분리하여 고쳤다. XML 문법 검사만으로 한/글 열림을 보장하지 않음을 확인했다. 기존 annual 코드는 이번 범위에서 수정하지 않았다.
- 최종 한/글 2020 **9, 6, 1, 10778**: 공용 잠금(`com_test_lock` 기본값) 아래 독립 COM 인스턴스 1개로 순차 실행. 열림·별도 HWPX 저장 11/11, 모두 **1쪽**, 저장본 XML 구조 정상, 저장 전후 동일 harness 모든 해당 지표 100%. 글꼴·크기·굵게/기울임의 저장 후 속성도 확인했다. 이 결과는 시각적 조판 동등성 판정이 아니다.

## 5. 시험·재현

```text
python -m unittest tests.test_markdown_hwpx tests.test_conversion_fidelity
Ran 61 tests — OK

python scripts/benchmark_markdown_hwpx.py generate --out <새 외부 산출폴더> --reference
python scripts/benchmark_markdown_hwpx.py com --out <같은 산출폴더>
python scripts/compare_conversion_fidelity.py compare --reference <산출폴더>/reference.json --candidate <산출폴더>/candidate --manifest <산출폴더>/manifest.json
```

전체 `python -m unittest discover -s tests`: **진행 전 — 중간 커밋의 별도 사본에서 실행 후 기록 예정**.

## 6. 못 한 것·후속 과제·KORDOCD 제안

1. 그림 임베딩 미구현으로 doc_basic 불합격. 내부 `Image` 블록·BinData/manifest·크기/원본 해시를 추가하고 같은 문서를 재검증해야 한다. 전면 대체 합격이라고 판정하지 않는다.
2. 입력에 충실한 제목/번호와 kordoc 보고서 프리셋의 기호화 사이 정책 차이는 후속 라우팅 카드에서 명시한다. 현재 기준에서 예외 허용은 적용하지 않았다. H3·번호 정보를 지워 kordoc과 맞추지 않는다.
3. 병합/중첩 표, 그림·수식·각주·페이지 구분·고급 Markdown·하이퍼링크 대상·사용자 템플릿·긴 다쪽 문서 조판은 **확인 못 함/미구현**. CommonMark 전체 파서가 아니며 HTML 표는 지원하지 않는다. 자동 내어쓰기 실측·시각적 레이아웃 비교도 후속 단계다.
4. KORDOCD 입력 어댑터는 `Document/Paragraph/Run/Table` 공통 블록을 목표로 DOCX(제목·번호·run 강조·표/병합), XLSX(시트·수식 결과·날짜·병합), PPTX(슬라이드 제목·도형 글·표·그림)를 각각 분리한다. 기존 파서가 만든 MD를 문자열로 억지 재해석하기보다 중간 블록과 `warnings/unsupported`를 명시하고 XML 쓰기를 공유하는 방향을 제안한다. 현재 모듈에서 그 블록 API의 외부 공개·직접 입력은 아직 구현하지 않았다.
5. 어댑터별 B2 manifest와 동일 비교기를 유지하고, 이번 MD 11종만으로 오피스 입력 합격을 추정하지 않는다. 라이선스 검토 전 새 의존성 추가 없음. PDF 고급 레이아웃·OCR은 기존 kordoc에 남긴다. E 라우팅/MCP·F 런타임 분할은 별도 카드이며 앱 준비 상태와 번들은 건드리지 않았다.
