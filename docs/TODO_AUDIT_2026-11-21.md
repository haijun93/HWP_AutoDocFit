# TODO.md 오래된 미완료 항목 구현 여부 점검 (2026-11-21)

- 기준: 브랜치 `feature/TODOAUDIT`(= `github/main`, HEAD `f88f52f4`), v2.2.85까지의 릴리스 노트(`releases/*.md`).
- 방법: 코드·문서·릴리스 노트·`git log --grep`을 읽기만 했다. 코드·`TODO.md`는 고치지 않았고 시험·앱 실행·한/글 COM은 쓰지 않았다(실행 중 앱·서비스 미접촉).
- 이전 감사 `docs/TODO_AUDIT_2026-11-18.md`는 참고만 했고, 그 뒤 3일간 커밋(225건)과 현재 코드로 다시 판정했다.
- "행"은 현재 `TODO.md`의 줄 번호다. 판정 기준: **완료**(적힌 과업 범위가 코드·커밋으로 확인됨) / **일부 완료** / **미착수** / **판정 불가**(실기 필요 등 이유 명시).
- 이 환경에서는 Windows 실기·Everything·한/글 COM을 확인할 수 없으므로, 코드와 커밋으로 확인되는 것만 '완료'로 적었다. "실기 필요"는 구현과 별개로 사용자 화면 확인이 남았다는 뜻이다.

## 0. 점검 대상과 제외

대상(미완료 `- [ ]` 29건, 2026-11-18 이전 추가분): 행 6~13, 35, 38, 41~43, 46~47, 51, 54~55, 58~59, 62(+72), 75~76, 90~91, 93, 95, 97.
- 행 97(Everything 벤치마크)은 2026-11-19 추가분이지만 요청서가 대상으로 지정해 포함했다. 행 62는 하위 세부(단계별)로 나눠 판정했다.
- 제외: 2026-11-19 이후 추가 항목(행 82·83·86·87·99~105), `[상태 2026-11-21: …]` 메모가 붙은 항목(행 22·24·32·86·101), 이미 `[x]`인 항목(행 16·18·20·26·28·102), 2026-11-20 추가 행 30.

## 1. 항목별 판정표

| 항목(행·제목) | 판정 | 근거 | 남은 일 | 사용자 확인·실기 |
|---|---|---|---|---|
| 행6 몽땅검색 단축키 창 UI 전면 개편(큰 검색칸·거르기 줄·2줄 결과·키보드 우선, 16ms·5MB) | 일부 완료 | 화면 개편 `01c7e303`, 2줄 카드 v2.2.76, 줄 배치 통합 `67f8c842`·`3a8472f5`. 측정 `docs/MG2_KTM_2026-11-14.md`: 입력→결과 8~14ms, 쉴 때 2.3~2.6MB(5MB 이내). 단 표시 **직후** 바로 입력하면 첫 결과 377~586ms | 표시 직후 입력 지연 원인 확정·수정, 드라이브 전체 파일명 색인 측정 | 실기 필요 |
| 행7 업무 연보(A1 엔진~A5) | 일부 완료 | 엔진·CSV·SVG·JSON: `kfind/annual/src/{cluster,stats,history,output}.rs`, 커밋 `53345631`·`d1fac63e`·`79473c21`·`b79cfdc2`·`9a8aa4ba`·`b20d6649`, v2.2.70·74·76·77 노트(주차 열지도·타임라인 SVG·인수인계 보고서용 JSON) | 실적·인수인계 **HWPX** 보고서, 엑셀(현재 Excel 호환 CSV만), 앱 화면 연결(노트가 "화면 연결은 다음 버전") | 화면 연결 후 확인 |
| 행8 몽땅검색 통합 검색(파일명>위계>본문, 같은 문서 1회, Everything보다 빠르게) | 일부 완료 | 1단계(ktm): `ktm/src/tm_state.rs:273` `combine_results`(우선순위 0·1·2, 문서 키 중복 제거), `3a8472f5`(탭 줄 제거·통합), `9b0f5f6c`·`10271b5f` 시험, v2.2.81 노트. 파일명 이름 질의 Everything 대비 23/23 빠름 기록 `0d6204e3` | 2단계: 앱 '몽땅검색' 화면(`timemachine_screen.dart`)은 여전히 파일명·본문·위계 3종 분리 검색(통합 코드 없음). 같은 PC 통합 질의 비교 측정 | 실기 필요 |
| 행9 연관 검색어 `;` | 일부 완료 | 엔진: `kfind/core/src/query.rs` `parse_groups`, `index.rs` `search_groups`·`count_groups`(`21509ddb`·`831ed598`·`e38038ee`). ktm 칩: `ktm/src/window.rs:2220`(`related_chip_rects`), `3d422287`, v2.2.76 노트. `docs/QMSSEMI_GAP_2026-11-18.md` | 파일명 경로의 문서 교집합, 서비스 파이프(legacy) 연관 그룹 필드, 묶음별 결과 강조(문서상 미구현) | 실기 필요 |
| 행10 v2.2.65 실사용 확인(첫 실행 자료 이동·서비스 자동 시작·단축키 등록; 첫 입력 지연·관리자 서비스 설치 버튼·비교 도구 구문 누락) | 판정 불가 | 사유: 첫 실행·설치·단축키 등록은 Windows 실기 항목. 세부: 첫 입력 지연은 `MG2` 남은 문제 1과 동일, 미해결. 서비스 설치 함수 `docfit_app/kfind_process.py:241 install_service`는 있으나 이를 부르는 화면(flutter_ui·desktop_web_ui)은 찾지 못함 → 미착수. 비교 도구 `tools/app1_equivalence.py:56,148`은 `phrases`를 이미 다룸(구문 누락은 해소된 것으로 보이나 수정 커밋은 특정 못 함) | 실기 확인, 설치 단추 연결 | 실기 필요 |
| 행11 단축키 앱(MG3, ktm) 고도화·실기 확인 | 일부 완료 | `ktm/` 독립 exe·트레이·단일 인스턴스·설치(`09874bf3`·`e388531c`·`e59632f3`·`d03e207d`), 서비스 파이프 시간 제한 `ed10a7f2`, 캡처 점검 모드 `3a8472f5` | 첫 입력 지연, 실기 확인 | 실기 필요 |
| 행12 전면 Rust 전환(UI0~UI10·R0~R9) | 일부 완료 | UI0 `ui_theme/src`, UI1 kagent 관리창, 추가로 ktm·kvf·dock 상주 창, kagent 클립보드 설정 창 `7f3684b5`·`1f4c3223`, Rust 설정 저장 `5a1b43e4`. 그러나 `docs/RUST_MIGRATION_INVENTORY_2026-11-19.md`: 메뉴 0~15 본체는 모두 Flutter 웹 위, Flutter 57,091줄·Python 89,291줄 | UI2~UI10, R0~R9 대부분 | 단계마다 실기 |
| 행13 한/글 COM 대체(COMX1 완료·COMX2) | 일부 완료 | COMX1 연구만: `01b17317`(Rust 프로토타입, COM 대조 49/49). 항목이 가리키는 `docs/COMX1_LINESEG_2026-11-13.md`는 현재 트리에 **없음**(커밋 `01b17317`에만 있음). COMX2 커밋·코드 없음 | COMX2 실기 검증·COM 호출 축소, 참조 문서 복원 | 실기·한/글 필요 |
| 행35 퀵몽땅검색 고도화 — Docufinder AI 제외 전 기능 | 일부 완료 | 대조표 `docs/QMS_DOCUFINDER_PARITY.md`, 구현 `acc9a89a`(구문·제외어·ext·path·기간), 와일드카드·정규식 `kfind/core/src/query.rs:106-109`, 위치 배지·미리보기·정렬·최근/즐겨찾기·CSV 내보내기(문서 표) | 포맷별 위치 완전성(PDF 쪽·시트/행·암호 문서 표시), 16ms/2MB 실측, 대조표 미확인 칸 | 실기 필요 |
| 행38 퀵몽땅검색 UI 고도화(Anything 14장) ①사이드바 ②옵션 바 ③결과 ④통계 모달 ⑤설정 모달 | 일부 완료 | `docs/QMS_PARITY_GAP_2026-11-18.md`: ②·③ 대부분 '구현'(정렬 5종·결과 내 검색·목록/격자·복사/CSV·접이식 그룹). ④ `ktm/src/window.rs:2490`(연도별 문서)·`:2570`(총 검색 횟수), ⑤ `ktm/src/settings.rs:78` `version_grouping` 등 | ①의 폴더 추가·문서 수 연결, 행별 수정 시각, 하단 총 문서·폴더 수, 기간 옵션 구성 차이. 첨부 원본과 픽셀 대조는 미실시 | 화면 대조 필요 |
| 행41 kordoc 의존 줄이기 2·3단계 | 일부 완료 | 2단계 일부: HWP 본문 변환 Rust(klib) 우선 `96955c93`, 색인 `2a691b35`. 단 `4f80b9a3`로 범위 축소(마크다운 중간 결과는 kordoc 유지). 3단계 첫 조각: `b19a8f30`(pypdfium2 추출기), 비교 `0fae6f89`·`docs/KORDOC3B_2026-11-18.md`(표 유형 96.97% < 99%라 **연결 안 함**) | 마크다운→보고서 서식 HWPX 생성기, DOCX·XLS·XLSX·PPTX 이전, '고급 변환 엔진' 별도 꾸러미 분리, 변환 준비 상태 판단 수정 | 실문서 묶음 시험(Windows) |
| 행42 앱 런칭 속도 높이기 | 미착수 | `flutter_ui/web/index.html:12` 로딩 문구 그대로. 단계별 시간 로그·지연 로드·골격 화면을 넣은 커밋 없음(`git log --grep` 런칭·startup: 항목 추가 커밋 `9975ac29`뿐). 최신 CI 기록 `release-assets/smart-launcher-startup.json`: 처음 6,082ms, 재실행 1,741·1,719ms로 `warm_ui_*_within_target:false` 유지. (이전부터 있던 캐시 런처·지연 import `docs/START2_*`는 이 항목의 후속 개선이 아님) | 단계별 측정, 초기화 후순위화, 골격 화면, 개선 전후 수치 기록 | 실기 필요 |
| 행43 v2.2.65 설치 오류 2건(VCRUNTIME140·DeleteFile 코드 5) | 일부 완료 | 수정 반영: `.cargo/config.toml`(`+crt-static`), `installer/setup.iss:21`(`CloseApplications=force`)·`:90`(`PrepareToInstall`에서 kagent 등 종료), `.github/workflows/build-windows-exe.yml:290-299`(VCRUNTIME140·MSVCP140·api-ms-win-crt 가져오기 검사) | VC++ 없는 PC 새 설치, 실행 중 덮어쓰기 설치, 설치 뒤 자동 재시작 확인 → 확인 뒤 배포 | 실기 필요(설치) |
| 행46 kfind-core 남은 약점(색인 크기·시간·메모리·개수만 질의) | 일부 완료 | 세부: **색인 시간** 합성 0.546→0.215초 `68ef5755`·`docs/KFIND_SIZE_2026-11-18.md`(실제 말뭉치 비교 아님). **메모리** 합성 peak 17.7→16.8MiB. **개수만 질의** 단일 `2688da55`, 그룹 `831ed598`·`e38038ee`·`docs/KFCOUNT_GAP_2026-11-18.md` → 이 세부는 완료. **색인 크기** 합성 0%(변화 없음), 포맷 변경은 미적용 | 크기(Tantivy 8.2MB 대 20.2MB) 격차, 실제 말뭉치 재측정(BENCH2 기준) | 실측은 실기 |
| 행47 앱 통합 뒤 Everything·Docufinder·UFFS와 화면 기준 비교 | 일부 완료 | Everything 비교 도구 구현: `qms_bench/`(Rust), `docfit_app/qms_bench_service.py`, `scripts/qms_bench_analyze.py`(시험 22건 `docs/BENCH_ANALYSIS_2026-11-19.md`), 개발자 모드 메뉴 `c40d0bbb`. 결과 파일은 저장소에 없음 | Docufinder·UFFS 어댑터 없음(미착수), 실제 측정·보고서 | 실기·Everything 설치 필요 |
| 행51 COM 호출 줄이기(간격 3단계 이후·기본 켜기) | 일부 완료 | 1단계·2단계 `d83360e3`·`77557e7a`(경계 기억 확대, `DOCFIT_SPACING_MEMO=reuse`), 설계 `docs/COM_CALLS_DESIGN_2026-11-18.md`. `docs/SPD2_SPACING_MEMO_2026-11-13.md:17` "기본 꺼짐 유지" | Run·SetPos·GetPos·InitScan 묶기, `batch_format.py` XML 일괄 확장, 기본 켜기 | 한/글 COM 실기(잠금 필수) |
| 행54 HWP·PDF 위계 추출, hwpx 본문 안 제N장 판정 | 일부 완료 | 세부: **hwpx 제N장 판정 완료**(`docs/HIERPDF_GAP_2026-11-18.md`, 시험 `test_hwpx_chapter_and_section_markers_map_to_distinct_levels`). **PDF 부분**: 줄 단위 추출+번호 체계 판정 `5a7a3561`(레이아웃·OCR 위계 미구현). **HWP5 일부**: klib 추출 위계 `2ca407b8`, 실 HWP 표본 검증 없음 | PDF 좌표·글꼴 기반 위계, 스캔 PDF, 실 HWP 표본 검증 | 실문서 필요 |
| 행55 위계검색 vs 일반검색 실문서 평가 | 미착수 | `scripts/eval_hierarchy_search.py`와 합성 Markdown 180건 기준선 `docs/benchmarks/위계검색_품질_기준선_2026-11-12.md`만 있음. `docs/HRUST1_RUST_PRIMARY_2026-11-14.md`(실문서 342건)는 Python↔Rust 판정 **일치**(차이 0) 비교이지 위계 대 일반 검색 품질 비교가 아님 | 실제 문서 평가 세트·질의 세트 작성 후 측정 | 사용자 문서 사본 필요 |
| 행58 숨은 Tk 백엔드·Tk 폴백 정리 | 미착수 | 제거 작업 커밋 없음. 조사 문서만: `docs/TK_INVENTORY_2026-11-18.md`(`26eb38b1`). 현재도 `docfit_ui/legacy.py`, `hwp-auto-docfit.py`·`docfit_core/abbreviation_dialog.py`가 tkinter 사용. 선행 조건(UI6 이식 후 정식 릴리스 1회)도 미충족 | UI6 이식 뒤 폐기 | — |
| 행59 Flutter·웹 번들 단계적 제거 | 일부 완료 | Win32 네이티브 화면 일부만(ktm·kagent 관리창·설정 창·kvf·dock). `flutter_ui/lib/features` 24개 폴더가 모든 메뉴 화면을 유지(인벤토리 문서) | UI2~UI10 이식 후 Flutter 삭제 | 단계마다 실기 |
| 행62 사진뷰(1단계 MVP) | 일부 완료 | 코드: `docfit_app/photo_album_service.py`(폴더 등록·증분 스캔·EXIF Orientation 0x0112 보정·연월 격자·즐겨찾기·태그·검색), `photo_album_screen.dart`(크게 보기 좌우·Esc), 태그 `eebb7ea3`·klib `ff9e0c5b`. 새 탭·'사진첩2' 개명 완료(`external_app_launcher.dart:16`). 다만 엔진은 계획(Rust klib `photo_*` 스캔·EXIF·썸네일)과 달리 대부분 Python(klib `photo.rs`는 104줄, 태그만) | 엔진의 Rust 이전 | 화면 실기 |
| 행62 사진뷰 2단계 ① 사진대지 ② 공문 첨부용 일괄 ③ 몽땅검색에서 사진 찾기 | 일부 완료 | ①`build_photo_sheet_hwpx`(2×2·2×3, `photo_album_service.py:1206`) ②`batch_export`(`:1129`)·화면 UI(`photo_album_screen.dart:23`). ③ 몽땅검색(ktm·kfind)에서 사진을 함께 찾는 코드 없음 → 미착수 | ③ | 한/글 연동 실기 |
| 행62 사진뷰 3단계(자르기·보정, 장소명 묶기, 슬라이드쇼) | 일부 완료 | 슬라이드쇼·위치별 묶음·사본 편집(`edit_copy`)은 코드에 있음(`photo_album_screen.dart:20-23`, 커밋 `02ae2e3d` 등). 자르기·간단 보정 전용 기능은 확인 못 함 | 자르기·보정 확인/보완 | 화면 실기 |
| 행72 MCP(AI 답변 → 서식 적용 HWPX) | 일부 완료 | PoC만: `17d47856`, `docfit_core/mcp_service.py`(`create_hwpx_from_ai_answer`, 파일 5행 "format profiles and Hancom COM finalization are deliberately separate phases"), `scripts/mcp_server.py` | 사용자 서식 프로필, 내어쓰기·최종검수, 장시간 작업 취소, 실기 연결·보안 시험 | 실기 |
| 행75 S1 업데이트 신뢰 근원 분리 | 일부 완료 | 서명 검증 구현·기본 꺼짐: `65b892d8`, `docfit_core/update_signature.py`, `docfit_app/update_service.py:221`. 활성화 점검 목록 `baab5c68`(체크 항목이 전부 미체크, 공개키 상수 비어 있음) | 공개키·절차 결정 후 켜기, Authenticode 코드서명, 릴리스 계정 2단계 인증(코드서명·계정은 코드에 흔적 없음) | 사용자 결정 필요 |
| 행76 로그 민감 정보 마스킹 확대 · Python hash lock 범위 | 일부 완료 | 마스킹: `7e9968a7`(doc_grid 로그 한정)이 `feature/LOGMASK`에만 있고 현재 main에 **미병합**(HEAD에 조상 아님, 해당 함수 코드 없음). hash lock: Windows EXE CI만 `--require-hashes`(`.github/workflows/build-windows-exe.yml:113`, `docs/HASHLOCK_REVIEW_2026-11-18.md`) | LOGMASK 병합·적용 범위 확대(브랜치는 main과 이력이 갈라져 있어 커밋 단위 선별 필요), 로컬 빌드·native prototype·Linux CI의 hash lock | 병합 결정 |
| 행90 PDF 편집 탭 UI 전면 재디자인(PDFCraft식 + 드래그앤드롭) | 일부 완료 | v2.2.76 노트(도구 막대·끌어 놓기 영역·파일 카드·쪽 썸네일 격자·작업 패널), v2.2.78(선택·회전·삭제·드래그/방향키 순서·되돌리기), v2.2.83(끌어 놓기 오류 수정). `pdf_edit_workspace.dart`의 `DragTarget`·`ReorderableListView`·`_ToolChip`·썸네일 `83533478`·`d567aa28` | 요구 문구의 '점선' 테두리(현재 실선 `Border.all`)·'호버 회전·삭제' 방식은 코드로 확정 못 함(회전·삭제는 카드 단추로 존재). 사용자 화면 확인 | 화면 확인 필요 |
| 행91 아웃라이너(Logseq 벤치마킹, Rust) 개발 후 사이드메뉴 추가 | 완료 | 1단계: `outliner_core`(블록 트리·`[[링크]]`·`((블록참조))` `api.rs:81 resolve_block_ref`·역링크·일지 `journal_*`·속성·검색, 시험 58건), `165db501`·`docs/OUTLINER_DESIGN_2026-11-18.md`. 2단계: 서비스·Flutter 탭·사이드메뉴 `flutter_ui/lib/features/outliner/`, `tab_navigation_shell.dart`, v2.2.76 노트, 입력 오류 수정 v2.2.84 | 아웃라이너 Win32 전환·2단계 결정 5건은 별도 항목(행103·105) | 화면 확인 권장 |
| 행93 문서뷰·사진뷰 폴더 기본값 '바탕화면 포함 모든 드라이브'(DEFAULTALL) | 완료 | `7e1b943c`·`708e10c5`(제외 폴더 판정 보정), `docfit_app/library_scope.py:23 default_roots`, 제외 폴더 UI `doc_grid_screen.dart:155-171`·`photo_album_screen.dart:247-274`, kfind 이름 색인 재사용·저장본 표시 `4bcf16db`·v2.2.85 노트, 격차표 `docs/DEFAULTALL_GAP_2026-11-18.md` | (후속 속도 과제는 행101에서 별도 관리) | 화면 확인 권장(이 PC 전체 드라이브) |
| 행95 하단 독(Dock) 메뉴바 DOCK1~ | 일부 완료 | DOCK1 설계 `docs/DOCK_DESIGN_2026-11-18.md`, DOCK2 골격 `dock/`(lib.rs 457줄·native.rs 443줄: 단일 인스턴스 뮤텍스·모니터·DPI), 클릭 라우팅·토글·hidden_tabs `708a8001`, '메뉴 표시 방식'(사이드바/Dock)·폴백 `be095bc3`, GDI 아이콘·툴팁·키보드 `9e16af8f`, v2.2.78·79 노트 | 메뉴별 단독 창 이식(3단계 이후, 현재는 앱 탭 열기 신호), 자원 측정(`scripts/dock_resource_probe.py`는 작성만, 미실행), 사이드바와 같은 아이콘(행24 별도), 실기 | 실기 필요 |
| 행97 Everything 대 몽땅검색 검색 속도 벤치마크(개발자 모드, 속도·리소스 두 기준) | 완료 | BENCH1 `docs/BENCH_EVERYTHING_DESIGN_2026-11-19.md`(`d9107f1b`), BENCH2 `qms_bench/`(`c907bb17`·`6955c4eb`·`eedfafe9`: p50·p95, 작업 집합·CPU·읽기 바이트, IPC·HTTP·es.exe 어댑터), BENCH3 개발자 모드 15번 메뉴 `c40d0bbb`·브리지 `139fb57a`, BENCH4 `scripts/qms_bench_analyze.py`·`tests/test_qms_bench_analyze.py` | 이 PC에서 실제 측정 실행과 결과 분석은 사용자 몫(저장소에 결과 파일 없음) | 실기·Everything 필요 |

> 한계: '완료' 3건은 모두 코드·커밋·릴리스 노트 근거를 확인한 구현 완료이며, 사용자 화면·실기에서 동작을 본 것은 아니다.

## 2. 세부 판정(여러 갈래 항목)

| 항목 | 세부 | 판정 | 근거 |
|---|---|---|---|
| 행10 | 첫 실행 자료 옮기기·서비스 자동 시작·단축키 등록 | 판정 불가 | Windows 설치·실행 필요 |
| 행10 | 단축키 창 첫 입력 지연 | 미착수(원인 미확정) | `docs/MG2_KTM_2026-11-14.md` 남은 문제 1 |
| 행10 | 관리자 서비스 설치 버튼 | 미착수 | 함수만 있음(`kfind_process.py:241`), 호출 화면 없음 |
| 행10 | 비교 도구 구문 검색 누락 | 일부 완료(해소로 보임) | `tools/app1_equivalence.py:56,148`이 `phrases` 처리 |
| 행41 | 2단계 HWP 본문 Rust 우선 | 완료 | `96955c93`·`2a691b35` |
| 행41 | 2단계 마크다운→HWPX 생성기·DOCX/XLS/PPTX 이전 | 미착수 | `4f80b9a3`로 범위 축소, 생성기 코드 없음 |
| 행41 | 3단계 PDF 추출기 | 일부 완료 | `b19a8f30` 추가, 표 96.97%라 연결 보류 |
| 행41 | 3단계 고급 변환 엔진 별도 꾸러미 | 미착수 | `kordoc_pack/` 그대로, 분리 커밋 없음 |
| 행46 | 색인 크기 | 미착수(합성 변화 0%) | `docs/KFIND_SIZE_2026-11-18.md` |
| 행46 | 색인 시간 | 일부 완료 | 합성 0.546→0.215초 |
| 행46 | 색인 메모리 | 일부 완료 | 합성 17.7→16.8MiB |
| 행46 | 개수만 질의 | 완료 | `2688da55`·`831ed598`·`e38038ee` |
| 행54 | hwpx 제N장 판정 | 완료 | `docs/HIERPDF_GAP_2026-11-18.md` |
| 행54 | PDF 위계 | 일부 완료 | `5a7a3561` |
| 행54 | HWP 위계 | 일부 완료 | `2ca407b8`, 실 HWP 표본 검증 없음 |
| 행62 | 1단계 MVP / 새 탭 / '사진첩2' 개명 / 문서뷰와 공용 스캔 | 완료(엔진은 Python 중심) | 위 표 + `rust_scan.py`·`library_scope.py`를 문서뷰와 공유 |
| 행62 | 2단계 ①사진대지 ②첨부용 일괄 | 완료 | `build_photo_sheet_hwpx`·`batch_export` |
| 행62 | 2단계 ③몽땅검색에서 사진 찾기 | 미착수 | ktm·kfind에 사진 경로 없음 |
| 행62 | 3단계 슬라이드쇼·장소명(위치 묶음) | 완료 | `photo_album_screen.dart` 헤더 주석·`02ae2e3d` |
| 행62 | 3단계 자르기·간단 보정 | 판정 불가 | 사본 편집(`edit_copy`)의 세부 기능 목록을 코드만으로 확정 못 함 |
| 행75 | 서명 검증 구현 | 완료(기본 꺼짐) | `65b892d8` |
| 행75 | 공개키·절차 결정 후 켜기 | 미착수 | 점검 목록 전부 미체크, 공개키 상수 비어 있음 |
| 행75 | Authenticode·계정 2단계 인증 | 미착수 | 코드·설정 흔적 없음 |
| 행76 | 마스킹 doc_grid 로그 한정 | 일부(미병합) | `7e9968a7` `feature/LOGMASK`에만 |
| 행76 | 마스킹 범위 확대 | 미착수 | — |
| 행76 | hash lock: Windows EXE CI | 완료 | `build-windows-exe.yml:113` |
| 행76 | hash lock: 로컬 빌드·native prototype·Linux CI | 미착수 | `docs/HASHLOCK_REVIEW_2026-11-18.md` |
| 행95 | 설계(DOCK1)·골격(DOCK2) | 완료 | 설계 문서·`dock/` |
| 행95 | 클릭 라우팅·설정 전환·폴백·hidden_tabs | 완료(코드) | `708a8001`·`be095bc3` |
| 행95 | 단일 인스턴스·모니터·DPI | 완료(코드) | `dock/src/native.rs:36,99,224`·DPI 시험 `lib.rs:322` |
| 행95 | 메뉴별 단독 창 이식 | 미착수 | 현재는 Flutter 탭 열기 신호 |
| 행95 | 자원 측정 | 미착수 | 스크립트에 "not run" 표기 |

## 3. 요약

- 점검 항목 **29건**(행 62는 하위 세부 3묶음을 §2에서 따로 판정했으나 항목 수는 1건으로 셈).
- 판정별 개수: **완료 3건 / 일부 완료 22건 / 미착수 3건 / 판정 불가 1건**.
  - 완료: 행91, 행93, 행97.
  - 미착수: 행42(앱 런칭 속도), 행55(위계검색 평가), 행58(Tk 정리).
  - 판정 불가: 행10(v2.2.65 실사용 확인 — 사유: Windows 설치·실행 실기 필요. 세부 일부는 §2에서 판정).
  - 일부 완료 22건: 행6·7·8·9·11·12·13·35·38·41·43·46·47·51·54·59·62·72·75·76·90·95.
- 실기·사용자 확인이 필요한 항목: 행6·8·9·10·11·13·35·38·42·43·46·47·51·62·90·95·97 등(거의 전부). 순수하게 결정이 필요한 것은 행75(서명 활성화)·행76(LOGMASK 병합).

### 완료로 체크해도 되는 항목(코드·커밋 근거 확인분)

1. **행91 아웃라이너(Logseq 벤치마킹)** — 1단계 `outliner_core`(`api.rs:81 resolve_block_ref`, 일지·역링크·검색, 시험 58건, `165db501`), 2단계 Flutter 탭·사이드메뉴(`flutter_ui/lib/features/outliner/outliner_workspace.dart`, `tab_navigation_shell.dart:97,371`), v2.2.76 노트, 입력 오류 수정 v2.2.84. 이후 Win32 전환·결정 5건은 행103·105가 따로 관리하므로 이 항목은 닫아도 된다. 단 설치본에서 입력→Enter→저장 흐름은 사용자가 한 번 보는 것이 좋다.
2. **행93 문서뷰·사진뷰 기본 범위 '바탕화면 포함 모든 드라이브'** — `7e1b943c`·`708e10c5`, `docfit_app/library_scope.py:23`, 제외 폴더 UI(`doc_grid_screen.dart:155`·`photo_album_screen.dart:247`), v2.2.77·85 노트. 후속 속도 개선은 행101에서 이어지므로 닫아도 된다.
3. **행97 Everything 대 몽땅검색 벤치마크** — BENCH1~4 전부 트리에 있음(`d9107f1b`·`c907bb17`·`6955c4eb`·`eedfafe9`·`c40d0bbb`·`139fb57a`·`scripts/qms_bench_analyze.py`, 분석기 시험 22건). 이 PC에서 실제 측정 결과를 만드는 일은 사용 단계이므로 항목은 닫고, 측정은 행47에서 추적한다.

### 남은 일 우선순위 5개

| 순위 | 일 | 효과 | 위험 | 작업량 | 비고 |
|---|---|---|---|---|---|
| 1 | 설치·실사용 실기 묶음 확인(행43·행10·행6·행11): VC++ 없는 PC 설치, 덮어쓰기 설치, 첫 실행, 단축키, 표시 직후 입력 지연 | 높음(배포 가능 여부 결정) | 낮음(읽기·확인) | S(사용자 실기 1회) | 확인되면 '남은 확인' 문구가 사라져 항목 여러 개를 닫을 수 있음 |
| 2 | 앱 런칭 속도(행42): 단계별 시간 로그 → 가장 큰 구간 제거, 첫 화면 골격 | 높음(매 실행 체감) | 중(WebView2·Flutter 로딩 경로 수정) | M | 최신 기록이 아직 목표 미달(재실행 1.7초) |
| 3 | 통합 검색 2단계(행8)+`;` 파일명 경로·묶음별 강조(행9): 앱 '몽땅검색' 화면도 통합 | 높음(핵심 사용자 동선) | 중 | L | ktm은 이미 통합이라 엔진·병합 코드 재사용 가능 |
| 4 | COM 호출 줄이기 3단계(행51): Run/SetPos/GetPos 묶기·XML 일괄·기본 켜기 | 높음(처리 시간의 83%가 COM) | 높음(한/글 조판 회귀) | L | 사본·COM 시험 잠금 필수 |
| 5 | LOGMASK 병합·적용 확대(행76) | 중(보안·개인정보) | 낮음 | S | `feature/LOGMASK`가 main과 이력이 갈라져 있어 `7e9968a7` 한 커밋만 선별 적용 |

참고: 행75(업데이트 서명 활성화)는 효과·위험 모두 크지만 공개키·절차 결정이 선행되어야 해서 순위에서 뺐다. 행55(위계검색 실문서 평가)는 효과 중간·위험 낮음·M으로 6순위다.
