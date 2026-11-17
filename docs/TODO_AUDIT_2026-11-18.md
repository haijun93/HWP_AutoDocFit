# TODO 열린 항목 감사 (2026-11-18)

대상은 `github/main` HEAD `56c11953d5ec1f336a9dcd94cabb8bb75464a05e` (2026-11-18, `Merge remote-tracking branch 'github/main' into feature/pkg-pdfedit`)의 `TODO.md` 열린 체크 42건이다. 근거는 이 커밋 트리의 코드·시험·문서와 해당 이력으로 한정했다. “완료됨”은 적힌 과업 범위가 증거상 완료된 경우이며, 넓은 후속 범위가 남으면 “부분 완료”로 기록했다. 카드 크기는 남은 범위의 대략적 S/M/L이다.

| 항목 첫 40자 | 분류 | 근거 | 다음 카드 크기 |
|---|---|---|---|
| PDF 편집 Rust 1차 전환 | 완료됨 | `a7a0616e` PDFEDIT-WIRE; `pdf_edit_core`, `docfit_app/pdf_edit_service.py`, Flutter PDF 편집 탭; 릴리스 미포함 별도 빌드 패키징은 `1e6f5ae4` PKG-PDFEDIT | S |
| 단축키 몽땅클립보드 줄 버튼 | 완료됨 | `923a6f45`; `kagent/src/popup.rs`; 붙임사진 편집 전달·연결 프로그램 열기 경로 | S |
| 몽땅검색 단축키 창 UI 전면 개편 | 부분 완료 | `01c7e303` QMS-UI 및 `docs/MG2_KTM_2026-11-14.md`는 화면 개편·일부 측정 통과를 기록. 16ms·5MB 목표는 MG2에서 일부 즉시 입력 사례 미달, QMS-PARITY 잔여 기준 측정 필요 | M |
| 업무 연보(문서 작업 통계, 사용자 요청 | 부분 완료 | `51bde0c4`, `53345631`, `d1fac63e`, `79473c21`; `kfind/annual` A1~A3 관련 구현과 `docs/ANNUAL_A1_2026-11-14.md` 설계 근거. A1~A5 명세 전체의 완료 증거는 없어 출력·연결 및 요구사항 대조가 남음 | L |
| 몽땅검색 통합 검색(파일명·위계·본문 | 부분 완료 | `ktm/src/tm_state.rs` 탭별 검색과 `docs/QMS_DOCUFINDER_PARITY.md`는 현재 파일명/본문/위계가 통합 결과가 아님을 명시. 통합 합성·중복 제거 UI와 속도 비교 필요 | L |
| 몽땅검색 연관 검색어 `;`(사용자 요청 | 부분 완료 | `21509ddb`, `kfind/core/src/query.rs`, `Index::doc_set`; 세미콜론 질의의 문서 교집합 구현. legacy 동의어·파일명 경로·ktm 칩/강조 연계는 코드상 확인되지 않음 | M |
| v2.2.65(2026-11-14 게시) 실사용 확인 | 부분 완료 | `docs/PKG1_INTEGRATE_V2265_2026-11-14.md`, `docs/APP1_EQUIV_B_ANALYSIS_2026-11-14.md`; 통합/설치 근거는 있으나 항목이 명시한 실제 PC 첫 실행 자료 이동·관리자 설치·첫 입력 지연 검증은 남음 | M |
| 몽땅검색 단축키 앱(MG3): 독립 검색 단 | 부분 완료 | `09874bf3`, `e388531c`, `e59632f3`; `ktm/`, `docs/MG2_KTM_2026-11-14.md`. 독립 앱·수명 관리·기본 성능 구현, 첫 입력 직후 지연과 실사용 확인이 미완료 | M |
| 전면 Rust 전환(UI0 완료·UI1 완료·UI2 | 부분 완료 | `38868d7d`, `docs/RUSTALL1_PLAN_2026-11-13.md`; UI0·UI1 기반만 구현, UI2~UI10 및 R0~R9 작업 계속 | L |
| 한/글 COM 대체(COMX1 완료·COMX2) | 부분 완료 | TODO가 참조하는 `docs/COMX1_LINESEG_2026-11-13.md`는 현재 `github/main` 트리에 없음. `scripts/perf_baseline_pc.py`, `docfit_engine/`에는 COM 계측/호출 경로가 남아 있고 COMX2 대체·실기 시험 근거 없음 | L |
| 위계 판정 Rust 기준 전환(HRUST1) | 완료됨 | `c8f02894`; `docs/HRUST1_RUST_PRIMARY_2026-11-14.md`의 실문서 342개·32,662문단 비교 차이 0, Rust 기본 경로/설정 폴백 및 관련 시험 기록 | S |
| 퀵몽땅검색 고도화 — Docufinder의 모든 기능 | 부분 완료 | `docs/QMS_DOCUFINDER_PARITY.md`, `acc9a89a`, `158ca7f4`, `1821c199`; 정확 구문·제외어·범위·내보내기 등 일부 구현. 정규식/와일드카드·포맷별 위치 완전성·16ms/2MB 실측 등 잔여가 문서에 명시됨 | L |
| 퀵몽땅검색 창 전면 재디자인 — 퀵몽땅 | 완료됨 | `01c7e303`; `ktm/src/window.rs`; ktm 회귀시험(커밋 기록 71건) 및 실제 화면 확인 기록 | S |
| [오류] 퀵가상폴더(Win+Shift+F)가 호출되지 | 완료됨 | `e947fbde`; 설치 자동 시작·즉시 실행·설정 상태/지금 켜기; 배포·Flutter 분석 및 kvf 시험 기록 | S |
| 단축키 창 이름 변경 (사용자 결정, 2026 | 완료됨 | `6c797881`; `ktm/src`, `kagent/src`, `kvf/src` 및 Flutter 문구; ktm/kagent/kvf/Python·Flutter 시험 기록 | S |
| 몽땅클립보드 켜기 후 화면 전환 (사용자 | 완료됨 | `c0fb8c4d`; `flutter_ui/lib/features/kclip/kclip_dialog.dart`; KCLIPOPEN 소개 창 성공 전환·실패 유지 시험 변경 | S |
| 퀵몽땅검색 UI 고도화 — 첨부 Anything 레 | 부분 완료 | `211f4f01`, `cbfb9725`, `89c4849b`, `865bdf8a`; QMS-ANY1~4 구현 및 `v2.2.72` 노트. TODO 및 `docs/QMS_DOCUFINDER_PARITY.md`가 통합 검색·성능·패리티 후속을 분리 | M |
| 텍스트 입력 화면: 실시간 자동 변환 + | 완료됨 | `611c3162`; `flutter_ui/lib/features/tools/document_tools_workspace.dart`, `docfit_app/document_tools_service.py`; 옵션 파이프라인·자동 갱신 구현, 관련 Python/Flutter 회귀 기록 | S |
| kordoc 의존 줄이기 2·3단계 (사용자 결정 | 부분 완료 | `96955c93`, `klib/src/hwp5.rs`; HWP 본문 추출 Rust 우선 등 2단계 일부. `docs/KORDOC_STEP3.md`는 고급 변환 엔진 분리 등 3단계 계획을 기록 | L |
| [오류·v2.2.68] 단축키 몽땅검색(ktm): 한 | 완료됨 | `bc01537a`; `ktm/src/window.rs` 사용자 입력 시 검색창 재기입 방지; IME 회귀 관련 ktm 시험 반영 | S |
| 앱 런칭 속도를 최대한 높이기 (사용자 요청 | 부분 완료 | `21a8ea0e`, `6236a568`, `docs/START2_LAZY_IMPORT_2026-11-12.md`; 캐시 런처·스플래시·지연 로딩. TODO의 v2.2.65 기준 첫 화면 목표 미달 수치가 최신 main 실측으로 갱신됐다는 근거는 없음 | M |
| [긴급·오류 보고] v2.2.65 설치 오류 2건 — | 부분 완료 | `.cargo/config.toml`, `installer/setup.iss`, `tests/test_distribution_build.py` 및 수정 반영 사항이 TODO에 기록됨. VC 런타임 없는 PC와 실행 중 덮어쓰기 설치 등 실제 Windows 확인은 미완료 | M |
| [오류] 단축키 몽땅클립보드에서 이미지 미리 | 완료됨 | `0f6e98b6`; `kagent/src/preview.rs`, store 잠금 재진입 및 비동기 호출 수정; `docs/RUST_LOCK_REENTRY_AUDIT_2026-11-14.md`, kagent 회귀 시험 | S |
| 몽땅검색: 파일명·본문검색·위계검색 3가지 | 완료됨 | `3da87263`, `f9347a63`; `flutter_ui/lib/features/timemachine/timemachine_screen.dart` 공통 기간 프리셋과 세 검색 전달, Flutter 61건 시험 기록. 릴리스 `v2.2.72.md` | S |
| 몽땅클립보드: '몽땅클립보드 켜기'를 누르면 | 완료됨 | `c0fb8c4d`; KCLIPOPEN 화면 전환 구현 및 `flutter_ui/lib/features/kclip/kclip_dialog.dart` 연결 | S |
| 6. 색인 시 문서당 고정 비용 줄이기 | 완료됨 | `e566317f`, `docs/IDXCOST_2026-11-18.md`; 설정/DLL 재사용·배치 커밋은 선행 코드에 있고 gram 경로 문자열을 doc_id로 교체. 새 회귀 및 합성 측정 포함 | S |
| 7. 파일 이름 검색 캐시 | 완료됨 | `6a158d0f`; 타임머신 파일명 검색 캐시·기준일 재사용 병합, `v2.2.72` 노트 | S |
| 고친 뒤 재측정 | 유효하지 않음 | 이 문구는 인접 성능 개선 과제의 후속 절차로 단독 작업 사양이 아님. 실제 성능 과제는 `docs/IDXCOST_2026-11-18.md`, `docs/KFIND_SIZE_2026-11-18.md` 각각 측정 기록을 보유 | — |
| kfind-core 남은 약점: 색인 크기(Tantivy | 부분 완료 | `68ef5755`, `docs/KFIND_SIZE_2026-11-18.md`; 합성 색인 시간 0.546→0.215초, 결과·크기 동일. 크기/포맷 개선·대형 색인 측정·count 전용 로직은 미해결로 명시 | M |
| 앱 통합 뒤 Everything·Docufinder·UFFS와 | 미착수 | `docs/QMS_DOCUFINDER_PARITY.md`는 화면 왕복 포함 비교가 없음을 밝힘; TODO의 동일 조건 앱 간 화면 기준 비교 수행 기록 없음 | M |
| COM 호출 줄이기: 간격 3단계 이후와 기본 | 부분 완료 | `77557e7a`, `docs/SPD2_SPACING_MEMO_2026-11-13.md`; 경계 기억 구현·사본 검증은 있으나 기본 켜기와 자간 Run/위치 호출 묶음·XML 일괄 적용 확대는 남음 | L |
| 변경 전후를 같은 문서로 비교하는 측정 절 | 미착수 | `docs/RELEASE_TEST_PROCEDURE.md`는 릴리스 검증을 다루지만 이 항목이 요구한 성능 비교 절차를 HANDOVER에 추가했다는 커밋/문서 근거 없음 | S |
| 빈 문단으로 여백을 만든 문서(간격 0, 빈 | 미착수 | TODO에 기술된 회귀 시나리오. `github/main` 시험명·릴리스 노트에서 간격 0, 빈 문단 20개 여백 보존을 검증한 근거 확인 안 됨 | S |
| HWP·PDF 위계 추출(지금은 본문만), hwpx | 부분 완료 | `docfit_core/style_hierarchy.py`, `hierarchy/`; Rust 위계 판정 및 HWPX·Markdown 등 구조 처리는 존재. PDF/HWP 형식 추출·제N장 판정 전체 요구를 완료했다는 시험/문서는 확인되지 않음 | L |
| 위계검색이 일반 검색보다 나은지 실제 문서 | 미착수 | `docs/HIERARCHY_SEARCH_CASE_STUDY_2026-11-12.md`는 사례 분석이며 위계 대 일반 검색의 평가 세트 비교 결과는 찾지 못함 | M |
| 숨은 Tk 백엔드 및 Tk 폴백 정리 | 부분 완료 | `docs/TK_REMOVAL_REVIEW_2026-11-09.md`, `docs/RUSTALL1_PLAN_2026-11-13.md`; Tk 없는 백엔드 기반은 있으나 해당 리뷰는 Tk 제거 단계와 legacy.py 폐기를 향후 작업으로 둠 | L |
| Flutter 및 웹 번들 단계적 제거 | 부분 완료 | `docs/RUSTALL1_PLAN_2026-11-13.md`, `flutter_ui/`; UI0·UI1 네이티브 요소는 있으나 앱 화면은 Flutter/Web 경로를 사용하며 UI2~UI10 계획이 남음 | L |
| 사진뷰(사진 라이브러리 전용) 기능 추가 | 부분 완료 | `02ae2e3d`, `39f06432`, `153f2a0d`, `546dfa53`; 사진뷰 탭·EXIF·중복 검사·사본 편집·묶음·슬라이드쇼 구현. `docs/PHOTO_NEEVIEW.md`는 별도 외부 사진첩의 빌드 기록이며 새 사진뷰 기능 명세의 태그·검색 완료 근거는 없음 | M |
| MCP(AI 답변 → 서식 적용 HWPX) | 부분 완료 | `17d47856`; MCP PoC가 존재. `docs/MCP_INTEGRATION_DESIGN.md`의 사용자 프로필·최종검수·취소·실기/보안 확인은 별도 완료 증거가 없어 남음 | L |
| S1 업데이트 신뢰 근원 분리 | 부분 완료 | `65b892d8`; Ed25519 `latest.json` 검증 구현(기본 꺼짐). `docs/UPDATE_SIGNATURE_DESIGN.md`가 키 활성화 절차·코드 서명·계정 보호 결정/실행을 남은 과제로 다룸 | M |
| 로컬 IPC 접속 슬롯 악용 대응·비동기 분석 | 미착수 | `docfit_app/ipc_server.py`·`docfit_app/api_contract.py`는 IPC 구현 근거지만 슬롯 악용 제한·분석 작업 자원 한도 대응 커밋/시험은 찾지 못함 | M |
| 로그 민감 정보 마스킹, Python hash lock 검토 | 미착수 | 관련 보안 계획 문서는 있으나 이 두 항목을 구현·시험했다는 커밋이나 회귀 시험명은 확인되지 않음 | M |

## 미착수·부분 완료 우선 추천 10

1. 몽땅검색 통합 검색 — 파일명·위계·본문 결과를 한 흐름에서 찾고 중복을 없애는 핵심 사용자 동선이 아직 분리돼 있다.
2. 퀵몽땅검색 Docufinder 패리티 잔여 — 정확 구문·검색 범위 기반 위에 정규식·포맷 위치와 성능 목표를 완성하면 검색 활용 범위가 커진다.
3. 몽땅검색 앱 간 비교 측정 — Everything·Docufinder·UFFS 대비 실제 화면 응답 자료가 있어야 성능 주장을 검증하고 다음 병목을 정할 수 있다.
4. 업무 연보 A4~A5 — A1~A3 분석을 인수인계·실적 보고서(HWPX)와 엑셀 결과로 연결해야 사용자 산출물이 된다.
5. COM 호출 줄이기 — 기준선에서 한/글 COM이 처리 시간의 대부분을 차지하므로 남은 반복 호출 묶기가 대용량 문서 작업 시간을 줄일 여지가 크다.
6. 위계검색 품질 평가 — 실제 문서 평가 세트로 일반 검색과 비교하면 위계 엔진의 유용성과 기본값 판단을 뒷받침한다.
7. S1 서명 활성화·실행파일 서명 — 업데이트 신뢰 검증이 기본 꺼짐이고 실행파일 서명·계정 보호가 남아 공급 경로 신뢰에 직접 영향이 있다.
8. 사진뷰 1단계 잔여 — 이미 탭과 사본 편집이 들어왔으므로 태그·검색 등 라이브러리 기본 동선을 마무리하면 완결성이 높아진다.
9. kordoc 2·3단계 — 대체 변환과 고급 엔진 분리는 기본 설치 크기·의존성과 변환 경로 유지에 영향을 준다.
10. 숨은 Tk 및 Flutter/Web 단계적 제거 — UI 전환 계획의 후속 범위이며 런타임 구조·유지보수 복잡도를 줄이는 큰 작업이다.
