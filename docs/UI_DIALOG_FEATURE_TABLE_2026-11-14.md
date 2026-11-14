# UI 대화상자 기능 대조표 (Flutter UI & Python/Tk 창 전수 대조)

**문서 번호**: `docs/UI_DIALOG_FEATURE_TABLE_2026-11-14.md`  
**작성일**: 2026-11-14  
**목적**: 앱 화면을 Rust Win32 네이티브로 옮길 때(A안, `docs/RUSTALL1_PLAN_2026-11-13.md` 7절) 기존 Flutter UI(약 77곳 안팎의 대화상자 호출부)와 Python desktop_web_ui / Tk 레거시 Toplevel 창 14곳의 기능과 상호작용이 누락되지 않도록 기준 대조표를 정의한다.  
**원칙**: 기존 소스 코드는 일체 수정하지 않으며, 각 대화상자의 열리는 상황, UI 요소, 결과 처리, 키보드 동작(Enter/Esc), 그리고 Rust 대상 화면(UI1~UI10) 매핑을 전수 기록한다.

---

## 1. Rust Win32 UI 화면 체계 요약 (RUSTALL1 7.4절 기준)

| 대상 화면 | 명칭 / 화면 번호 | 해당 모듈 / 주 작업 영역 | 대화상자 수용 형태 |
|---|---|---|---|
| **UI1** | S18 몽땅클립보드 관리창 | `features/kclip/*` | 독립 Win32 창 + 단축키(Win+Shift+V) 팝업 |
| **UI2** | S10 업무자료함 / 가상폴더 | `features/library/*` | 다중 영역 캔버스/탐색기 모달 & 확인 대화상자 |
| **UI3** | S12 타임머신 / 시간순 탐색 | `features/timemachine/*` | 타임라인/달력 검색 + 위계검색/색인 모달 |
| **UI4** | S11 노트북, S13 메모 편집기 | `features/notebook/*`, `features/editor/*` | 3단 패널(출처/검색/노트) + 메모/내보내기 모달 |
| **UI5** | S6 설정, S7 준말, S9 파일이름, S19 도구, S20 외부앱, S25 초기화 | `features/settings/*`, `features/abbreviations/*`, `features/file_rename/*` | 탭형 설정 다이얼로그, 규칙 편집기, 준말 편집기 |
| **UI6** | S1 주 작업, S2 진행, S3 미리보기, S4 변환, S5 서식관리, S8 서식통일 | `features/conversion/*`, `features/formats/*`, `features/style_unify/*`, `main.dart` | 메인 작업 창 자식 다이얼로그 / 검토 확인 모달 |
| **UI7** | S14 표 단위 템플릿 | `features/table_units/*` | Direct2D 기반 표 템플릿 + 칸 글 입력 모달 |
| **UI8** | S15 표그림판 | `features/table_sketch/*` | Direct2D 기반 그리드 캔버스 + 색상/글/가져오기 모달 |
| **UI9** | S16 다이어그램 셸 | `features/diagrams/*`, `features/flows/*` | WebView2 호스팅 컨테이너 + 저장/불러오기 모달 |
| **UI10** | D 삭제 대상 | Flutter 전체, Tk 레거시 전체 | Rust 전환 완료 후 최종 빌드 배포 시 폐기 |

---

## 2. Flutter UI 대화상자 기능 대조표 (showDialog / AlertDialog / Dialog / SimpleDialog)

*컬럼 구조*: `번호 | 파일:줄 | 열리는 상황 | 제목·안내 글 | 입력 칸·버튼·체크 등 요소 | 결과 처리(무엇을 저장·호출) | 키보드(Enter·Esc) | 옮길 Rust 화면 | 비고`

| 번호 | 파일:줄 | 열리는 상황 | 제목·안내 글 | 입력 칸·버튼·체크 등 요소 | 결과 처리(무엇을 저장·호출) | 키보드(Enter·Esc) | 옮길 Rust 화면 | 비고 |
|:---:|---|---|---|---|---|:---:|:---:|---|
| **F-01** | `app/rename_format_dialog.dart:27` | 서식 관리 창에서 서식의 '이름 바꾸기'를 눌렀을 때 | **서식 이름 바꾸기**<br>서식 이름과 기관 이름을 입력하세요. | • 이름 TextField (maxLength: 30)<br>• 기관 TextField (maxLength: 40)<br>• '취소' TextButton<br>• '저장' FilledButton | `(name, org)` 튜플 반환 → `rename_profile` RPC 호출로 서식 메타데이터 업데이트 | Esc: 닫기<br>Enter: 저장 필드 제출 | **UI6** | 서식 관리 자식 모달 |
| **F-02** | `core/close_guards.dart:38` | 저장하지 않은 변경사항이 있는 상태에서 화면 닫기/뒤로가기/종료 시도 시 | **저장하지 않은 변경사항**<br>저장하지 않은 내용이 있습니다. 닫으면 변경사항이 사라집니다. | • '계속 편집' TextButton<br>• '닫기' FilledButton (Danger 색상) | `true`(닫기 진행) 또는 `false`(취소) 반환 | Esc: 계속 편집<br>Enter: 닫기 | **UI5/UI6** | 공통 CloseGuard 모달 |
| **F-03** | `features/abbreviations/abbreviation_editor.dart:63` | 준말 관리 화면에서 [+ 준말 추가] 또는 목록의 [수정]을 눌렀을 때 | **준말 추가** / **준말 수정**<br>자주 쓰는 말의 준말과 본말을 등록하세요. | • 준말 TextField (maxLength: 20)<br>• 본말 TextField (maxLength: 200, multiline)<br>• '취소' TextButton<br>• '저장' FilledButton | `{'key': key, 'value': val}` 반환 → `abbreviation_set` RPC 호출 | Esc: 취소<br>Enter: 저장 | **UI5** | S7 준말 관리 모달 |
| **F-04** | `features/abbreviations/abbreviation_manager.dart:60` | 준말 관리 화면(`AbbreviationManager`) 내부에서 편집 모달을 호출할 때 | (위 `AbbreviationEditor` 호출) | (위 F-03과 동일) | 편집 완료 후 준말 목록 즉시 갱신 | Esc: 닫기 | **UI5** | S7 준말 등록 흐름 |
| **F-05** | `features/abbreviations/abbreviation_manager.dart:75` | 준말 목록에서 [삭제] 버튼을 눌렀을 때 | **준말 삭제**<br>‘{key}’ 준말을 삭제할까요? | • 안내 텍스트<br>• '취소' TextButton<br>• '삭제' FilledButton (Danger 색상) | `true` 반환 시 `abbreviation_delete` RPC 호출 후 목록 갱신 | Esc: 취소<br>Enter: 삭제 | **UI5** | 준말 삭제 확인 |
| **F-06** | `features/conversion/merge_order_dialog.dart:69` | 파일변환 탭에서 'PDF 하나로 합치기'를 눌렀을 때 | **PDF 합칠 순서 정하기**<br>목록의 손잡이를 끌어 순서를 바꾸세요. | • ReorderableListView (문서 목록, 형식 아이콘, 드래그 핸들)<br>• '취소' TextButton<br>• '합치기' FilledButton | 정렬된 파일 목록 반환 → PDF 병합 변환 엔진 호출 | Esc: 취소<br>Enter: 합치기 | **UI6** | S4 변환 보조 다이얼로그 |
| **F-07** | `features/conversion/hwp_bulk_panel.dart:9` | 파일변환 화면에서 'HWP 찾아 HWPX로 바꾸기'를 눌렀을 때 | **HWP 찾아 HWPX로 바꾸기**<br>폴더나 드라이브 전체에서 HWP 문서를 찾아 변환합니다. | • 범위 SegmentedButton (폴더/드라이브 전체)<br>• 폴더 고르기 OutlinedButton<br>• 하위 폴더 재귀 체크박스<br>• 드라이브 선택 FilterChip 목록<br>• 암호 입력 필드 및 암호 추가 버튼 / Chip 목록<br>• '닫기' TextButton<br>• '시작' FilledButton | `hwp_bulk(start, ...)` RPC 호출로 백그라운드 변환 시작 | Esc: 닫기<br>Enter: 암호추가/시작 | **UI6** | S4 일괄 변환 설정 모달 |
| **F-08** | `features/diagrams/diagram_editor_screen_native.dart:357` | 다이어그램/흐름도 편집기에서 수정 후 저장하지 않고 닫기/불러오기 시 | **저장하지 않은 변경사항**<br>현재 다이어그램의 변경사항을 버릴까요? | • '계속 편집' TextButton<br>• '변경사항 버리기' FilledButton | `true`(버리기) / `false`(유지) 반환 | Esc: 계속 편집<br>Enter: 버리기 | **UI9** | WebView2 다이어그램 닫기 가드 |
| **F-09** | `features/diagrams/diagram_editor_screen_native.dart:379` | 다이어그램 편집기에서 [저장] / [다른 이름으로 저장]을 눌렀을 때 | **도형 문서 저장**<br>문서 이름을 입력하세요. | • 이름 TextField (maxLength: 120, autofocus)<br>• '취소' TextButton<br>• '저장' FilledButton | 이름 문자열 반환 → `FlowDraftClient.save` 호출 | Esc: 취소<br>Enter: 저장 | **UI9** | 다이어그램 저장 모달 |
| **F-10** | `features/diagrams/diagram_editor_screen_native.dart:414` | 다이어그램 편집기에서 [저장된 문서] 목록을 열었을 때 | **저장된 다이어그램**<br>불러올 문서를 선택하세요. | • 문서 ListView (이름, 수정일, 삭제 IconButton)<br>• '닫기' TextButton | 선택한 문서 Map 반환 → `storage('load')` 호출로 캔버스 복원 | Esc: 닫기<br>Enter: 항목선택 | **UI9** | 다이어그램 보관함 모달 |
| **F-11** | `features/diagrams/diagram_editor_screen_native.dart:456` | 저장된 다이어그램 보관함에서 특정 문서의 [삭제]를 눌렀을 때 | **흐름도 삭제**<br>{name}을 삭제할까요? | • 안내 Text<br>• '취소' TextButton<br>• '삭제' FilledButton | `true` 반환 시 `storage('delete')` 호출 | Esc: 취소<br>Enter: 삭제 | **UI9** | 다이어그램 삭제 확인 |
| **F-12** | `features/editor/memo_editor.dart:321` | 메모 편집기 툴바에서 [링크(Ctrl+K)]를 눌렀을 때 | **링크**<br>연결할 웹 주소를 입력하세요. | • 주소 TextField (기본값 'https://', autofocus)<br>• '취소' TextButton<br>• '넣기' FilledButton | URL 문자열 반환 → Markdown `[텍스트](url)` 삽입 | Esc: 취소<br>Enter: 넣기 | **UI4** | S13 메모 편집기 링크 모달 |
| **F-13** | `features/editor/memo_editor.dart:370` | 메모 내용이 있는 상태에서 [파일 열기(Ctrl+O)]를 눌렀을 때 | **파일 열기**<br>지금 편집 중인 내용 대신 고른 파일을 엽니다. 계속할까요? | • 안내 Text<br>• '취소' TextButton<br>• '열기' FilledButton | `true` 반환 시 파일 탐색기 선택 및 텍스트 로드 | Esc: 취소<br>Enter: 열기 | **UI4** | 파일 열기 덮어쓰기 경고 |
| **F-14** | `features/editor/memo_editor.dart:435` | 메모 편집기에서 [다른 이름으로 저장(Ctrl+Shift+S)]을 눌렀을 때 | **다른 이름으로 저장** / **다른 이름으로 저장 — {fileName}** | • 인코딩 ChoiceChip (UTF-8, UTF-8 BOM, UTF-16 LE, CP949)<br>• 줄바꿈 ChoiceChip (CRLF, LF)<br>• CP949 불가 문자 검사 리포트 뷰 및 '이동' 버튼<br>• '취소' TextButton<br>• '저장…' / 'UTF-8로 저장' FilledButton | `MemoSaveChoice` 반환 → 지정 인코딩/줄바꿈으로 파일 스트림 기록 | Esc: 취소<br>Enter: 저장/확인 | **UI4/UI5** | 메모장 호환 인코딩 대화상자 |
| **F-15** | `features/editor/memo_editor.dart:600` | 메모 편집기 툴바에서 휴지통 [삭제]를 눌렀을 때 | **메모 삭제**<br>이 메모를 지울까요? 되돌릴 수 없습니다. | • 경고 문구 Text<br>• '취소' TextButton<br>• '삭제' FilledButton | `true` 반환 시 자동저장 타이머 취소 및 `widget.onDelete` 호출 | Esc: 취소<br>Enter: 삭제 | **UI4** | 메모 삭제 확인 |
| **F-16** | `features/file_rename/rule_editor.dart:83` | 파일이름 탭에서 고급 규칙 항목 더블클릭 또는 [규칙 추가] 시 | **규칙 종류별 라벨** (예: 글자 바꾸기, 앞뒤 글자 넣기 등) | • 키별 옵션 입력칸 (TextField, DropdownButton, SwitchListTile)<br>• 최근 입력 히스토리 PopupMenuButton<br>• 에러 안내 Text<br>• '취소' TextButton<br>• '확인' FilledButton | 수정된 규칙 Map 객체 반환 → 규칙 목록 반영 및 실시간 미리보기 갱신 | Esc: 취소<br>Enter: 확인/저장 | **UI5** | S9 파일이름 고급 규칙 편집 모달 |
| **F-17** | `features/file_rename/file_rename_screen.dart:398` | 고급 규칙 메뉴에서 [JSON 내보내기]를 눌렀을 때 | **내보낼 규칙 세트** | • SimpleDialogOption 목록 (규칙 세트 이름)<br>• 취소(영역 밖/Esc) | 선택한 세트 Map 반환 → `ruleset_export` RPC 및 파일 저장 다이얼로그 호출 | Esc: 닫기<br>Enter: 선택 | **UI5** | 규칙 세트 선택 다이얼로그 |
| **F-18** | `features/file_rename/file_rename_screen.dart:420` | 고급 규칙 메뉴에서 [지금 규칙 저장]을 눌렀을 때 | **규칙 세트 저장** | • 이름 TextField (autofocus)<br>• '취소' TextButton<br>• '저장' FilledButton | 세트 이름 반환 → `ruleset_save` RPC 호출 | Esc: 취소<br>Enter: 저장 | **UI5** | 규칙 세트 저장 모달 |
| **F-19** | `features/file_rename/file_rename_screen.dart:444` | 고급 규칙 메뉴에서 [규칙 세트 불러오기]를 눌렀을 때 | **규칙 세트 불러오기** | • SimpleDialogOption 목록 (저장된 규칙 세트 이름)<br>• 취소(영역 밖/Esc) | 선택한 세트 반환 → 규칙 목록(`_rules`)으로 역직렬화 및 미리보기 | Esc: 닫기<br>Enter: 선택 | **UI5** | 규칙 세트 불러오기 |
| **F-20** | `features/file_rename/file_rename_screen.dart:466` | 파일 목록에서 특정 항목의 새 이름을 직접 고치고자 더블클릭할 때 | **새 이름 직접 고치기 · {entry.name}** | • 새 이름 TextField (autofocus)<br>• '규칙 결과로' TextButton (오버라이드 제거 시)<br>• '취소' TextButton<br>• '확인' FilledButton | 새 이름 문자열 반환 → `_overrides` 맵에 반영 및 미리보기 재스케줄 | Esc: 취소<br>Enter: 확인 | **UI5** | 파일별 수동 오버라이드 모달 |
| **F-21** | `features/formats/profile_editor.dart:406` | 서식 관리에서 서식 항목의 [세부 수정]을 눌렀을 때 | **{서식 이름} 서식 세부 편집**<br>예시 문서에서 찾은 서식 요소를 검토·수정합니다. | • 서식 요소 네비게이터 (문단 계층, 표 등)<br>• 글꼴, 크기, 들여쓰기, 정렬, 줄간격 에디터<br>• 미리보기 창<br>• '취소' TextButton<br>• '저장' FilledButton | 서식 JSON 프로파일 저장 → `save_profile` RPC 호출 | Esc: 닫기<br>Enter: 저장 | **UI6** | S5 서식 세부 편집 모달 |
| **F-22** | `features/formats/format_example_edit_dialog.dart:53` | 한/글에서 예시 문서 편집 완료 후 서식 분석을 요청할 때 | **서식 예시 문서 편집**<br>열린 한/글 창에서 예시 문서를 작성하세요. | • 단계 안내 가이드 패널<br>• 에러 안내 Text<br>• '취소' TextButton<br>• '분석 시작' FilledButton | 분석 승인/취소 신호 반환 → `resolve_dialog` RPC 호출로 서식 복제 실행 | Esc: 취소<br>Enter: 분석 시작 | **UI6** | 한/글 COM 연동 서식 분석 모달 |
| **F-23** | `features/kclip/kclip_dialog.dart:22` | Win+Shift+V를 누르거나 메뉴에서 클립보드 관리창을 호출할 때 | **몽땅클립보드**<br>클립보드 이력 및 자주 쓰는 상용구 | • 텍스트/이미지 클립보드 카드 리스트<br>• 검색창, 핀 고정 토글, 삭제 버튼<br>• '닫기' 버튼 | 클립보드 항목 선택 시 Windows 클립보드로 전송 및 포커스 복원 | Esc: 닫기<br>Enter: 항목 선택 | **UI1** | S18 몽땅클립보드 독립 셸 |
| **F-24** | `features/library/library_screen.dart:528` | 자료함 항목에서 '업무자료함에 출처로 추가'를 눌렀을 때 | **업무자료함에 출처로 추가**<br>“{title}”을 넣을 업무자료함을 고르세요. | • 업무자료함 ListView (선택 시 반환)<br>• 새 업무자료함 이름 TextField<br>• '만들어 추가' TextButton<br>• '취소' TextButton | 선택된 `Notebook` 객체 반환 → `notebooks.addSourceFromItem` 호출 | Esc: 취소<br>Enter: 만들어 추가 | **UI2/UI5** | 자료함-업무자료함 연계 모달 |
| **F-25** | `features/library/library_screen.dart:620` | 자료함 상단에서 [가상 폴더 이름 바꾸기]를 눌렀을 때 | **가상 폴더 이름 바꾸기** | • 폴더 이름 TextField (autofocus)<br>• '취소' TextButton<br>• '저장' TextButton | 새 이름 문자열 반환 → `widget.api.renameBox` RPC 호출 | Esc: 취소<br>Enter: 저장 | **UI2** | 가상 폴더 관리 모달 |
| **F-26** | `features/library/library_screen.dart:638` | 자료함 상단에서 [가상 폴더 삭제]를 눌렀을 때 | **가상 폴더 삭제**<br>“{box.name}” 가상 폴더와 등록된 자료를 삭제할까요? | • 안내 Text<br>• '취소' TextButton<br>• '삭제' TextButton (Danger) | `true` 반환 시 `widget.api.deleteBox` 호출 후 목록 재로딩 | Esc: 취소<br>Enter: 삭제 | **UI2** | 가상 폴더 삭제 확인 |
| **F-27** | `features/library/library_screen.dart:666` | 자료함 상단에서 [물리 폴더로 전환]을 눌렀을 때 | **물리 폴더로 전환**<br>선택한 위치에 새 폴더를 만들고 영역별 복사본을 복사합니다. | • 대상 부모 경로 표시<br>• 새 폴더 이름 TextField (금지 문자 검증)<br>• 에러 안내 Text<br>• '취소' TextButton<br>• '복사' TextButton | 폴더 이름 반환 → `widget.api.exportBox` 호출로 실제 디렉터리 생성 및 파일 복사 | Esc: 취소<br>Enter: 복사 | **UI2** | 가상 폴더 물리 추출 모달 |
| **F-28** | `features/library/library_screen.dart:726` | 가상 폴더 내 실행 파일(.exe/.bat 등)을 열려고 시도할 때 | **실행 파일 열기**<br>이 자료는 프로그램(실행 파일)입니다. 믿을 수 있는 파일일 때만 실행하세요. | • 파일 경로 SelectableText<br>• '취소' TextButton (기본 포커스)<br>• '실행' TextButton | `true` 반환 시 OS 실행기로 프로세스 기동 허용 | Esc: 취소<br>Enter: 기본 취소 포커스 | **UI2** | 실행 파일 실행 보안 경고 |
| **F-29** | `features/library/library_screen.dart:752` | 자료 열기 실패 시 (파일 없음, 연결 앱 없음 등) | **자료를 열지 못했습니다** | • 실패 이유 안내 문구<br>• 대상 경로 SelectableText<br>• '경로 복사' TextButton<br>• '확인' TextButton (autofocus) | 경로 클립보드 복사 또는 창 닫기 | Esc: 닫기<br>Enter: 확인 | **UI2** | 자료 열기 에러 대화상자 |
| **F-30** | `features/library/library_screen.dart:786` | 텍스트/메모 자료를 더블클릭했으나 편집기가 연결되지 않았을 때 | **{item.title}** | • 텍스트 내용 SingleChildScrollView / SelectableText<br>• '복사' TextButton<br>• '닫기' TextButton (autofocus) | 본문 클립보드 복사 또는 닫기 | Esc: 닫기<br>Enter: 닫기 | **UI2** | 텍스트 자료 뷰어 대화상자 |
| **F-31** | `features/library/library_screen.dart:999` | 개별 자료 카드 메뉴에서 [이름 바꾸기]를 눌렀을 때 | **자료 이름 바꾸기** | • 이름 TextField (autofocus)<br>• '취소' TextButton<br>• '저장' TextButton | 새 제목 반환 → `widget.api.renameItem` 호출 | Esc: 취소<br>Enter: 저장 | **UI2** | 자료 이름 변경 모달 |
| **F-32** | `features/library/library_screen.dart:1010` | 개별 자료 카드 메뉴에서 [삭제]를 눌렀을 때 | **자료 삭제**<br>“{item.title}” 자료를 가상 폴더에서 삭제할까요? | • 안내 Text (원본 유지 안내)<br>• '취소' TextButton<br>• '삭제' TextButton | `true` 반환 시 `widget.api.deleteItem` 호출 | Esc: 취소<br>Enter: 삭제 | **UI2** | 자료 삭제 확인 모달 |
| **F-33** | `features/library/library_screen.dart:1022` | 자료 간 관계선을 우클릭하여 [관계 이름표]를 눌렀을 때 | **관계 이름표** | • 라벨 TextField (autofocus)<br>• '취소' TextButton<br>• '저장' TextButton | 라벨 문자열 반환 → `widget.api.updateRelationLabel` 호출 | Esc: 취소<br>Enter: 저장 | **UI2** | 관계선 이름표 편집 모달 |
| **F-34** | `features/library/library_screen.dart:1272` | 여러 자료를 선택한 상태에서 선택 표시줄의 [삭제]를 눌렀을 때 | **자료 삭제**<br>고른 자료 {count}개를 가상 폴더에서 삭제할까요? | • 원본 파일 미삭제 안내 Text<br>• '취소' TextButton (autofocus)<br>• '{count}개 삭제' TextButton | `true` 반환 시 일괄 순회 `deleteItem` RPC 호출 | Esc: 취소<br>Enter: 취소 (실수방지) | **UI2** | 다중 자료 일괄 삭제 확인 |
| **F-35** | `features/library/library_screen.dart:1340` | 여러 자료를 일괄 열기 시도했으나 일부/전체 실패했을 때 | **자료 {failures.length}개를 열지 못했습니다** | • 실패한 자료별 상세 사유 목록 ScrollView / SelectableText<br>• '확인' TextButton (autofocus) | 창 닫기 | Esc: 닫기<br>Enter: 확인 | **UI2** | 일괄 열기 실패 결과 안내 |
| **F-36** | `features/library/library_screen.dart:1362` | 여러 자료를 선택 후 [이동·복사…] 단추를 눌렀을 때 | **자료 {count}개 이동·복사** | • 보낼 가상 폴더 DropdownButton<br>• 보낼 영역 DropdownButton (핵심/기초/참고/임시)<br>• 복사본/원본 보존 안내 Text<br>• '취소' TextButton<br>• '복사' TextButton<br>• '이동' TextButton | `_TransferRequest(box, space, copy)` 반환 → 일괄 복사/이동 트랜잭션 실행 | Esc: 취소<br>Enter: 이동/복사 | **UI2** | 자료 다중 전송/복사 모달 |
| **F-37** | `features/notebook/notebook_screen.dart:232` (A) | 업무자료함 상단에서 [+ 새 업무자료함]을 눌렀을 때 | **새 업무자료함** | • 업무자료함 이름 TextField (기본값 '새 업무자료함', autofocus)<br>• '취소' TextButton<br>• '확인' FilledButton | 이름 문자열 반환 → `widget.api.createNotebook` RPC 호출 | Esc: 취소<br>Enter: 확인 | **UI4/UI5** | S11 업무자료함 생성 모달 |
| **F-38** | `features/notebook/notebook_screen.dart:232` (B) | 출처 패널에서 [+ 출처 추가]를 눌렀을 때 | **출처 추가** | • 파일 경로 TextField (지원 형식 안내, autofocus)<br>• '취소' TextButton<br>• '확인' FilledButton | 파일 경로 반환 → `widget.api.addSource` 호출 및 인덱싱 | Esc: 취소<br>Enter: 확인 | **UI4** | 수동 파일 경로 출처 추가 모달 |
| **F-39** | `features/notebook/notebook_screen.dart:346` | 업무자료함에서 [가상 폴더에서 가져오기]를 눌렀을 때 | **가상 폴더에서 가져오기** | • 가상 폴더 선택 DropdownButton<br>• 파일 자료 CheckboxListTile 목록<br>• '취소' TextButton<br>• '가져오기 {picked}개' FilledButton | 선택한 파일 경로 목록 반환 → `_addPaths` 순차 출처 등록 | Esc: 취소<br>Enter: 가져오기 | **UI4** | 자료함 파일 일괄 가져오기 모달 |
| **F-40** | `features/notebook/notebook_screen.dart:604` | 노트 패널에서 [Markdown 내보내기]를 눌렀을 때 | **Markdown 내보내기** (출처 비교 포함 여부 안내) | • Markdown 본문 SelectableText (ScrollView)<br>• '복사' TextButton (클립보드 저장)<br>• '닫기' FilledButton | 클립보드 복사 또는 창 닫기 | Esc: 닫기<br>Enter: 닫기 | **UI4** | 노트 내보내기 결과 대화상자 |
| **F-41** | `features/settings/settings_dialog.dart:718` | 메인 설정 드로어에서 [전체 설정 열기]를 눌렀을 때 | **전체 설정** | • 설정 카테고리 NavigationRail / 리스트<br>• 실시간 검색 필터 TextField<br>• 설정 키별 입력/스위치/옵션 카드 목록<br>• 적용 전/후 예시 이미지 패널<br>• '초기화' TextButton<br>• '취소' KioskSecondaryButton<br>• '변경사항 적용' KioskPrimaryButton | 변경된 설정 Dict 반환 → `save_settings` RPC 호출 | Esc: 닫기(변경확인)<br>Enter: 적용 | **UI5** | S6 전체 환경설정 모달 |
| **F-42** | `features/settings/idiom_panel.dart:151` | 설정의 상용구 탭에서 특정 항목의 [수정]을 눌렀을 때 | **글자 상용구 고치기** / **본문 상용구 고치기** | • 준말 TextField (maxLength: 20)<br>• 본말/이름 TextField<br>• '취소' TextButton<br>• '저장' FilledButton | `true` 반환 시 `save_text` / `save_body` RPC 호출 | Esc: 취소<br>Enter: 저장 | **UI5** | 상용구 수정 대화상자 |
| **F-43** | `features/settings/idiom_panel.dart:207` | 설정의 상용구 탭에서 [삭제]를 눌렀을 때 | **상용구 삭제**<br>‘{key}’ 상용구를 지울까요? | • 안내 Text (백업 폴더 보관 안내)<br>• '취소' TextButton<br>• '삭제' FilledButton (Danger) | `true` 반환 시 목록 제거 후 `save_text`/`save_body` RPC 호출 | Esc: 취소<br>Enter: 삭제 | **UI5** | 상용구 삭제 확인 |
| **F-44** | `features/style_unify/unify_pick_style_dialog.dart:68` | 서식통일 2단계: 조사된 서식 중 대표 서식을 확정할 때 | **대표 서식 확인**<br>{message} | • 조사된 서식 그룹 Wrap/Card 목록<br>• 서식별 글꼴/크기/속성 대표값 폼<br>• '취소' TextButton<br>• '확인' FilledButton | `resolve_dialog` RPC 호출로 선택된 대표 서식 ID 전송 | Esc: 불가(PopScope)<br>Enter: 확인 | **UI6** | S8 서식통일 2단계 대표서식 모달 |
| **F-45** | `features/style_unify/unify_area_dialog.dart:53` | 서식통일 3단계: 대표 서식을 입힐 적용 영역을 확정할 때 | **편집 영역 확인**<br>선택 {selected} / 후보 {total}개 | • 영역별 CheckboxListTile 목록 (문서명, 영역 번호, 마커 등)<br>• '취소' TextButton<br>• '확인' FilledButton | `resolve_dialog` RPC 호출로 체크된 영역 ID 목록 전송 | Esc: 불가(PopScope)<br>Enter: 확인 | **UI6** | S8 서식통일 3단계 편집영역 모달 |
| **F-46** | `features/style_unify/unify_result_dialog.dart:48` | 서식통일 5단계 완료 후: 통일 작업 결과 및 검수 검토 시 | **작업 결과 확인**<br>{status_text} | • 검수 통과/불일치 상태 배너<br>• 검사 문장 수, 불일치 항목 리스트, 미해결 목록<br>• '닫기' FilledButton | `resolve_dialog` RPC 응답으로 완료 처리 | Esc: 불가(PopScope)<br>Enter: 닫기 | **UI6** | S8 서식통일 5단계 결과확인 모달 |
| **F-47** | `features/table_units/table_units_screen.dart:3019` | 표 템플릿 화면에서 표의 특정 칸 글을 직접 수정할 때 | **칸 글 입력 ({row}행 {col}열)** | • 내용 TextField (maxLines: 4, autofocus)<br>• '취소' TextButton<br>• '확인' FilledButton | 입력 문자열 반환 → `_cellTexts` 맵 갱신 및 미리보기 재요청 | Esc: 취소<br>Enter: 확인 | **UI7** | S14 표 단위 칸 글 입력 모달 |
| **F-48** | `features/table_sketch/table_sketch_screen.dart:1971` | 표그림판에서 선 색 또는 배경색 단추를 눌렀을 때 | **색 고르기** | • 26색 팔레트 그리드 (swatch)<br>• 직접 입력 TextField (#RRGGBB)<br>• '없음' TextButton (배경색일 때)<br>• '닫기' TextButton<br>• '적용' FilledButton | 고른 HEX 색상 문자열 반환 → 선/배경 스타일 즉시 적용 | Esc: 닫기<br>Enter: 적용 | **UI8** | S15 표그림판 색상 피커 모달 |
| **F-49** | `features/table_sketch/table_sketch_screen.dart:2925` | 표그림판에서 [한/글 문서에서 표 가져오기] 선택 시 표가 2개 이상일 때 | **가져올 표 고르기 (표 {total}개)** | • 표 요약 ListTile 목록 (번호, 행×열, 첫 칸 글, 에러 사유)<br>• '취소' TextButton | 선택한 표 index 반환 → `importHwpxTable` RPC로 스케치 변환 | Esc: 취소<br>Enter: 선택 | **UI8** | HWPX 표 다중 선택 모달 |
| **F-50** | `features/table_sketch/table_sketch_screen.dart:3929` | 표그림판에서 '글' 도구로 칸을 클릭하여 텍스트를 넣을 때 | **칸 글 입력 ({x}, {y})** | • 내용 TextField (autofocus)<br>• 굵게 Checkbox<br>• 정렬 DropdownButton (가운데/왼쪽/오른쪽)<br>• '취소' TextButton<br>• '확인' FilledButton | `SketchText(x, y, text, bold, align)` 반환 → 문서 객체 추가 및 미리보기 | Esc: 취소<br>Enter: 확인 | **UI8** | 표그림판 칸 텍스트 입력 모달 |
| **F-51** | `features/timemachine/timemachine_screen.dart:798` | 타임머신 상단에서 [확장자] 설정 버튼을 눌렀을 때 | **적용 확장자** | • 확장자 FilterChip Wrap (hwpx, docx, pdf 등)<br>• Rust 본문 색인 SwitchListTile<br>• 쉬는 시간에 색인 SwitchListTile<br>• 쉬는 시간 기준 DropdownButton<br>• '취소' TextButton<br>• '저장' FilledButton | `settings_set` RPC 호출 후 증분 스캔 요청 | Esc: 취소<br>Enter: 저장 | **UI3** | S12 타임머신 색인 옵션 모달 |
| **F-52** | `features/timemachine/related_documents_dialog.dart:264` | 타임머신 목록에서 항목 우클릭 후 [위계검색]을 선택했을 때 | **위계검색**<br>선택 문서의 기준일 및 정기 문서 계열, 연관 문서 목록 | • 정기 문서 계열 그룹 (이전/같은/다음 기간)<br>• 연관 자료 정렬 DropdownButton<br>• 항목별 Card 및 '관련 없음' TextButton<br>• '연관 자료 추가' TextButton<br>• '닫기' TextButton | '관련 없음'(`related_exclude`) 및 '추가'(`related_include`) RPC 호출 | Esc: 닫기<br>Enter: 닫기 | **UI3** | S12 타임머신 위계검색 모달 |
| **F-53** | `features/timemachine/related_documents_dialog.dart:319` | 위계검색 창에서 [연관 자료 추가] 버튼을 눌렀을 때 | **연관 자료 추가** | • 파일명 필터 TextField<br>• 타임머신 후보 문서 ListTile 목록<br>• '취소' TextButton | 선택한 `TimemachineItem` 반환 → `related_include` RPC 호출 | Esc: 취소<br>Enter: 항목선택 | **UI3** | 연관 자료 피커 모달 |
| **F-54** | `features/tools/backend_dialog.dart:64` | 백엔드(Python/Tk)로부터 모달 확인 요청이 수신되었을 때 | **백엔드 요청 종류에 따른 제목** | • `unify_pick_style`: `UnifyPickStyleDialog`<br>• `unify_area`: `UnifyAreaDialog`<br>• `unify_result`: `UnifyResultDialog`<br>• `format_example_edit`: `FormatExampleEditDialog`<br>• 기타 기본 승인/거절 액션 버튼 | 사용자 응답 결과를 패킹하여 `resolve_dialog` RPC로 백엔드에 반환 | Esc: 방어 (PopScope)<br>Enter: 기본승인 | **UI5/UI6** | 백엔드 RPC 연동 다이얼로그 라우터 |
| **F-55** | `main.dart:1049` | 파일변환 탭에서 PDF 병합 순서 조정을 위해 `MergeOrderDialog` 호출 | (위 F-06과 동일) | (위 F-06과 동일) | 병합 순서 확정 및 변환 실행 | Esc: 취소 | **UI6** | S4 변환 메인 진입점 |
| **F-56** | `main.dart:2377` | 메인 서식 선택 카드를 눌러 '서식 키오스크'를 열었을 때 | **서식 고르기**<br>예시 보고서 첫 쪽 그림을 보고 원하는 서식 카드를 누르세요. | • 서식 카드 GridView (스틸컷 이미지, 서식명, 기관명)<br>• 서식별 버튼 (서식예시 확인, 서식예시 폴더, 서식관리)<br>• '닫기' TextButton | 선택 서식 ID 반환 → `set_profile` RPC 호출 | Esc: 닫기<br>Enter: 카드선택 | **UI6** | S5 서식 선택 키오스크 모달 |
| **F-57** | `main.dart:2545` | 백엔드 `pending_dialog` 이벤트 감지 시 `BackendDialog` 표시 | (위 F-54와 동일) | (위 F-54와 동일) | 백엔드 모달 응답 | Esc: 차단 | **UI5/UI6** | 메인 이벤트 루프 감시 |
| **F-58** | `main.dart:2595` | 서식 관리 목록에서 특정 서식의 [세부 수정]을 눌렀을 때 | (위 F-21 `ProfileEditor` 호출) | (위 F-21과 동일) | 프로파일 편집 및 메인 상태 리프레시 | Esc: 닫기 | **UI6** | S5 서식 편집 메인 진입점 |
| **F-59** | `main.dart:2603` | 메인 화면 작업 방식 옆 [세부 작업] 단추를 눌렀을 때 | **세부 작업 선택** | • 단계별 CheckboxListTile 목록 (자간, 줄간격, 글자모양 등)<br>• '전체 선택' TextButton<br>• '전체 해제' TextButton<br>• '기본값 복원' TextButton<br>• '현재 구성을 기본값으로' TextButton<br>• '닫기' FilledButton | `set_stage` 및 `stage_config` RPC 호출로 실행 단계 토글 | Esc: 닫기<br>Enter: 닫기 | **UI6** | S1 주 작업 세부단계 선택 모달 |
| **F-60** | `main.dart:2674` | 메인 화면에서 [서식 관리] 단추를 눌렀을 때 | **서식 관리** | • 탭 SegmentedButton (문단 계층 / 제목표 / 데이터표)<br>• 서식 예시 문서 드롭존 및 분석 버튼<br>• 서식 목록 (사용, 세부수정, 이름바꾸기, 삭제 버튼)<br>• 닫기 IconButton | 서식 추가/수정/삭제 종합 관리 | Esc: 닫기 | **UI6** | S5 서식 종합 관리 모달 |
| **F-61** | `main.dart:2686` | 서식 관리 목록에서 [이름 바꾸기]를 눌렀을 때 | (위 F-01 `RenameFormatDialog` 호출) | (위 F-01과 동일) | 서식 명칭 변경 RPC 호출 | Esc: 취소<br>Enter: 저장 | **UI6** | 서식 이름 변경 흐름 |
| **F-62** | `main.dart:2699` | 서식 관리 목록에서 사용자 정의 서식의 [삭제]를 눌렀을 때 | **서식 삭제**<br>‘{name}’ 서식을 삭제할까요? 삭제하면 되돌릴 수 없어요. | • 경고 문구 Text<br>• '취소' TextButton<br>• '삭제' FilledButton (Danger) | `true` 반환 시 `delete_profile` RPC 호출 | Esc: 취소<br>Enter: 삭제 | **UI6** | 서식 삭제 확인 모달 |
| **F-63** | `main.dart:3804` | 메인 상태바의 [작업 로그] 단추를 눌렀을 때 | **작업 로그** | • 로그 텍스트 SingleChildScrollView / SelectableText (Consolas 12pt)<br>• '복사' TextButton<br>• '새로 고침' TextButton<br>• '지우기' TextButton<br>• '닫기' FilledButton | 로그 복사, 새로고침(`get_log`), 비우기(`clear_log`) RPC 실행 | Esc: 닫기<br>Enter: 닫기 | **UI6** | 처리 기록 조회 모달 |
| **F-64** | `main.dart:4004` | 메인 설정 드로어에서 [전체 설정 열기]를 눌렀을 때 | (위 F-41 `SettingsDialog` 호출) | (위 F-41과 동일) | 전체 환경설정 저장 후 메인 리프레시 | Esc: 닫기 | **UI5** | 전체 설정 호출점 |
| **F-65** | `main.dart:4019` | 메인 설정 드로어에서 [준말 등록·관리]를 눌렀을 때 | **준말 등록·관리** (`AbbreviationManager`) | • 준말 실시간 검색 TextField<br>• 준말-본말 목록 DataTable / 리스트<br>• [+ 준말 추가], [수정], [삭제] 액션<br>• '닫기' 버튼 | 준말 전체 CRUD 관리 | Esc: 닫기 | **UI5** | S7 준말 관리 셸 |
| **F-66** | `main.dart:4101` | 메뉴의 [사용 방법과 단축키] 또는 F1 키를 눌렀을 때 | **사용 방법** | • 사용 순서 가이드 Text (문서선택 → 작업방식 → 정리시작)<br>• 원본 보존 안내 Text<br>• 단축키 목록 (Ctrl+O, Ctrl+Enter, F1)<br>• '확인' FilledButton | 정보 확인 후 닫기 | Esc: 닫기<br>Enter: 확인 | **UI6** | 도움말 및 가이드 모달 |
| **F-67** | `core/instant_search.dart` 계열 | 각 검색창(타임머신, 자료함, 상용구)에서 즉시 검색 동작 | (인라인 바텀 드롭다운 형태) | • 추천 검색어 및 매칭 히트 항목 | 선택 시 검색어 설정 및 화면 갱신 | Esc: 검색어 비우기<br>Enter: 즉시 확정 | **UI2~UI5** | 공통 실시간 인스턴트 검색기 |

---

## 3. Python / Tk `Toplevel` 창 전수 기능 대조표 (14곳)

*컬럼 구조*: `번호 | 파일:줄 | 열리는 상황 | 제목·안내 글 | 입력 칸·버튼·체크 등 요소 | 결과 처리(무엇을 저장·호출) | 키보드(Enter·Esc) | 옮길 Rust 화면 | 비고`

| 번호 | 파일:줄 | 열리는 상황 | 제목·안내 글 | 입력 칸·버튼·체크 등 요소 | 결과 처리(무엇을 저장·호출) | 키보드(Enter·Esc) | 옮길 Rust 화면 | 비고 |
|:---:|---|---|---|---|---|:---:|:---:|---|
| **T-01** | `docfit_ui/legacy.py:747` | 메인 화면에서 [처리 기록 보기] 메뉴/단추를 눌렀을 때 | **한글편집 후처리 기록** | • 처리 로그 Text 위젯 (ScrolledText, 500줄 제한)<br>• '로그 지우기' Button<br>• '기록 복사' Button<br>• '창 닫기' Button | 로그 메모리 버퍼 비우기 및 클립보드 복사 | Esc: 닫기<br>Enter: 닫기 | **UI6/UI5** | Tk 레거시 처리 기록 창 |
| **T-02** | `docfit_ui/legacy.py:1365` | 작업 모드 선택 후 [세부작업 선택]을 눌렀을 때 (`_세부작업_열기`) | **{모드명} 세부 작업 선택**<br>작업에 포함할 항목을 선택하세요. | • 단계별 Checkbutton 목록 (BooleanVar)<br>• '전체 선택' Button<br>• '전체 해제' Button<br>• '기본값 복원' Button<br>• '확인' Button | 선택된 단계 목록을 메인 앱 설정(`self._세부작업_설정`)에 반영 | Esc: 취소/닫기<br>Enter: 확인 | **UI6** | S1 주 작업 세부 토글 모달 |
| **T-03** | `docfit_ui/legacy.py:2476` | 서식 선택 라벨/버튼을 눌렀을 때 (`_서식_키오스크_열기`) | **서식 고르기**<br>원하는 서식 카드를 선택하세요. | • 서식 카드 그리드 (Thumbnail Canvas, Label)<br>• 서식별 라디오/선택 Button<br>• '서식예시 확인' Button<br>• '서식예시 폴더 열기' Button<br>• '서식관리' Button<br>• '닫기' Button | 선택 서식 프로파일 로드 및 메인 앱 서식 변경 | Esc: 닫기<br>Enter: 선택 | **UI6/UI5** | 서식 썸네일 그리드 뷰 |
| **T-04** | `docfit_ui/legacy.py:2799` | 새 서식 예시 문서 작성을 시작할 때 (`_서식_예시편집_시작`) | **서식 예시 문서 편집 안내**<br>열린 한/글 창에서 예시 문서를 작성하세요. | • 단계 안내 Label (항목기호 계층 작성법 등)<br>• '분석 시작' Button<br>• '취소' Button | 한/글 활성 문서 텍스트/표/스타일 분석 후 새 프로파일 생성 | Esc: 취소<br>Enter: 분석 시작 | **UI6** | 한/글 COM 연동 예시 작성 안내 |
| **T-05** | `docfit_ui/legacy.py:2918` | [텍스트로 문서 추가] 메뉴를 눌렀을 때 (`_텍스트로_문서추가_열기`) | **텍스트로 문서 추가**<br>내용을 붙여넣으면 HWPX 문서로 변환해 추가합니다. | • 본문 ScrolledText (autofocus)<br>• 문서 제목 Entry<br>• '변환 및 목록에 추가' Button<br>• '취소' Button | 텍스트를 임시 HWPX로 생성하여 작업 대상 문서 목록에 추가 | Esc: 취소<br>Enter: Ctrl+Enter 변환 | **UI5/UI4** | S19 도구 / 텍스트 HWPX 변환 |
| **T-06** | `docfit_ui/legacy.py:3258` | 서식 복제 후 서식 요소 검토를 원할 때 (`_서식_세부사항_확인`) | **서식 요소 세부사항 확인**<br>분석된 대표값을 검토하고 수정하세요. | • 서식 요소 Treeview (항목기호, 글꼴, 크기, 들여쓰기, 줄간격)<br>• 값 수정을 위한 Combobox / Entry 인라인 에디터<br>• '저장 후 적용' Button<br>• '취소' Button | 수정된 서식 요소 값을 프로파일 JSON에 덮어쓰기 저장 | Esc: 취소<br>Enter: 저장 | **UI6** | 서식 요소 대표값 검토/수정 창 |
| **T-07** | `docfit_ui/legacy.py:3460` | 서식의 계층 스타일 구조를 검토할 때 (`_서식_구조_확인`) | **서식 계층 구조 확인**<br>문단 스타일 계층 트리를 확인하세요. | • 계층형 Treeview (1수준 □, 2수준 ㅇ, 3수준 - 등)<br>• 스타일 속성 상세 뷰<br>• '닫기' Button | 구조 확인 후 프로파일 연결 | Esc: 닫기<br>Enter: 닫기 | **UI6** | 계층형 스타일 트리 검토 창 |
| **T-08** | `docfit_ui/legacy.py:3950` | [환경설정] 메뉴를 눌렀을 때 (`_설정창_생성`/`설정창_열기`) | **환경설정** | • Notebook 탭 (일반, 서식, 자간, 검수, 고급)<br>• 항목별 Checkbutton, Spinbox, Combobox, Entry<br>• '기본값 복원' Button<br>• '취소' Button<br>• '저장' Button | `config.ini` 파일에 전체 환경설정 영구 저장 | Esc: 취소<br>Enter: 저장 | **UI5** | S6 탭형 전체 환경설정 창 |
| **T-09** | `docfit_ui/legacy.py:4955` | 모든 문서 처리가 정상 완료되었을 때 (`_절약_완료창`) | **작업 완료**<br>문서 편집 작업이 완료되었습니다! | • 고양이 일러스트 Canvas / 이미지<br>• 절약된 작업 시간 통계 Label<br>• 처리된 문서 개수 및 성공률<br>• '결과 폴더 열기' Button<br>• '확인' Button | 결과 디렉터리 탐색기 열기 및 완료 화면 닫기 | Esc: 닫기<br>Enter: 확인 | **UI6** | 완료 축하 및 통계 모달 |
| **T-10** | `docfit_ui/legacy.py:5133` | 서식통일 2단계: 대표 서식 확정 창 (`_서식통일_대표서식_검토창`) | **서식 통일 - 2단계: 대표 서식 검토** | • 서식 그룹별 샘플 텍스트 뷰<br>• 대표 서식 선택 Radiobutton 목록<br>• 속성 미세조정 필드<br>• '다음 단계(적용영역 검토)' Button<br>• '취소' Button | 대표 서식 기준 데이터 확정 후 3단계 진행 | Esc: 취소<br>Enter: 다음 단계 | **UI6** | S8 서식통일 2단계 Tk 창 |
| **T-11** | `docfit_ui/legacy.py:5318` | 서식통일 3단계: 적용 영역 선택 창 (`_서식통일_적용영역_검토창`) | **서식 통일 - 3단계: 편집 영역 검토** | • 문서별 발견된 영역 Treeview (체크박스 지원)<br>• '전체 선택' / '전체 해제' Button<br>• '서식통일 실행' Button<br>• '취소' Button | 통일 대상 적용 영역 리스트 확정 후 배치 실행 시작 | Esc: 취소<br>Enter: 실행 | **UI6** | S8 서식통일 3단계 Tk 창 |
| **T-12** | `docfit_ui/legacy.py:5438` | 서식통일 5단계: 최종 결과 검수 확인 창 (`_서식통일_결과_확인창`) | **서식 통일 - 5단계: 결과 검수 확인** | • 불일치 항목 보고 Treeview / ScrolledText<br>• 검수 통과 여부 상태 라벨<br>• '결과 문서 열기' Button<br>• '확인' Button | 한/글에서 결과 문서를 띄우거나 최종 승인 종료 | Esc: 닫기<br>Enter: 확인 | **UI6** | S8 서식통일 5단계 Tk 창 |
| **T-13** | `docfit_core/abbreviation_dialog.py:19` | 메뉴에서 [준말 등록]을 눌렀을 때 (`open_abbreviation_dialog`) | **준말 등록 및 관리** | • 준말 검색 Entry<br>• 준말-본말 Treeview<br>• 준말 입력 Entry, 본말 Entry<br>• '추가/수정' Button, '삭제' Button<br>• '닫기' Button | `abbreviations.json` 파일 영구 저장 | Esc: 닫기<br>Enter: 추가/수정 | **UI5/S7** | 준말 관리 Tk Toplevel 창 |
| **T-14** | `hwp-auto-docfit-reset.py:270` | 초기화 복구 도구를 단독 실행했을 때 (`ResetAppUI`) | **한글편집 후처리 초기화 및 원상복구** | • 복구 옵션 Checkbutton (설정초기화, 캐시삭제, 레지스트리정리)<br>• `messagebox.askyesno` 확인 대화상자<br>• '초기화 실행' Button<br>• '취소' Button | 설정 파일 백업 및 초기 팩토리 상태 복원 | Esc: 취소<br>Enter: 실행 | **UI5/S25** | S25 단독 복구 도구 창 |

---

## 4. Rust Win32 UI1~UI10 화면별 대화상자 구현 매핑 가이드

### 4.1 UI1 (S18 몽땅클립보드 관리창)
- **수용 대화상자**: F-23 (`KClipPanel`)
- **구현 방식**:
  - `WS_POPUPWINDOW | WS_CAPTION | WS_THICKFRAME` 속성의 독립 윈도우로 생성.
  - 전역 핫키 `RegisterHotKey(hWnd, 1, MOD_WIN | MOD_SHIFT, 'V')` 등록.
  - 마우스 포커스 상실 시(`WM_ACTIVATE` / `WA_INACTIVE`) 자동 숨김 처리 가능하도록 구성.
  - 키보드: `VK_ESCAPE`로 숨김, 방향키 탐색, `VK_RETURN`으로 선택 항목 클립보드 복사.

### 4.2 UI2 (S10 업무자료함 / 가상폴더)
- **수용 대화상자**: F-24~F-36 (폴더 생성, 이름변경, 삭제확인, 물리추출, 위험실행경고, 전송/복사, 에러목록 등 13종)
- **구현 방식**:
  - 표준 Win32 모달 대화상자(`DialogBoxParamW` 또는 `CreateDialogParamW`)로 구현.
  - `libraryExport` (물리 폴더 추출): `IFileDialog` (Folder Picker) 호출 후 폴더명 입력 모달 연계.
  - `libraryRiskyOpen` (실행파일 경고): `TaskDialogIndirect`를 활용하여 경고 아이콘(`TD_WARNING_ICON`), 기본 취소 버튼 포커스 설정.
  - 키보드: `VK_ESCAPE`로 즉시 취소, 텍스트 입력칸에서 `VK_RETURN` 시 기본 저장/확인 버튼 구동.

### 4.3 UI3 (S12 타임머신 / 시간순 탐색)
- **수용 대화상자**: F-51 (색인 및 확장자 설정), F-52 (위계검색 결과창), F-53 (연관자료 추가)
- **구현 방식**:
  - `RelatedDocumentsDialog` (위계검색): 620px 너비의 모달 대화상자로, Direct2D를 통해 정기 문서 계열 그래프와 기준일 근거 칩 렌더링.
  - `IncludePicker`: 가상 ListView(`LVS_OWNERDATA`)를 사용하여 100건 이상의 후보 문서를 깜빡임 없이 즉시 필터링.
  - 키보드: 목록에서 방향키, `VK_RETURN`으로 선택, `VK_ESCAPE`로 닫기.

### 4.4 UI4 (S11 노트북, S13 메모 편집기)
- **수용 대화상자**: F-12~F-15 (메모 편집기 링크, 열기경고, 다른이름저장, 삭제), F-37~F-40 (업무자료함 생성, 출처추가, 자료함가져오기, 마크다운 내보내기), T-05
- **구현 방식**:
  - `MemoSaveDialog`: 인코딩/줄바꿈 라디오 그룹 및 Rust `edit_unencodable` 검사 루틴 연동. CP949 불가 글자 목록은 리스트뷰로 표시하고 더블클릭 시 편집기 캐럿 이동(`EM_SETSEL`).
  - `_ExportNoteDialog`: 읽기 전용 다중 라인 Edit 컨트롤(`ES_MULTILINE | ES_READONLY`)에 마크다운을 올리고 클립보드 복사 버튼 제공.

### 4.5 UI5 (설정, 준말, 파일이름 일괄변경)
- **수용 대화상자**: F-03~F-05 (준말 CRUD), F-16~F-20 (파일이름 규칙편집, 세트저장/불러오기, 오버라이드), F-41~F-43 (전체설정, 상용구 수정/삭제), T-08, T-13, T-14
- **구현 방식**:
  - `SettingsDialog`: Win32 PropertySheet 또는 커스텀 탭 컨트롤(`SysTabControl32`)을 사용한 2열 레이아웃(좌측 카테고리/우측 설정 항목 + Direct2D 조판 예시 뷰어).
  - `RuleEditorDialog`: 규칙 종류별 콤보박스 및 파라미터 입력 패널을 동적으로 전환하는 모달 다이얼로그.

### 4.6 UI6 (주 작업 화면, 서식 관리, 서식 통일, 변환)
- **수용 대화상자**: F-01 (서식 이름변경), F-06~F-07 (PDF병합, HWP일괄변환), F-21~F-22 (서식세부편집, 예시작성안내), F-44~F-46 (서식통일 3단계 모달), F-54~F-66 (키오스크, 세부작업, 서식관리, 로그, 사용법), T-01~T-04, T-06~T-07, T-09~T-12
- **구현 방식**:
  - `FormatKioskDialog`: Direct2D 및 WIC(Windows Imaging Component)를 활용한 카드 썸네일 그리드 뷰어. 스크롤 및 마우스 휠 부드러운 스크롤링 지원.
  - `StagesDialog` (세부 작업 선택): `LVS_CHECKBOXES` 스타일의 ListView로 고속 체크 토글 지원.
  - `BackendDialog` (서식통일 3단계): COM 워커 스레드와의 동기화를 위해 Win32 이벤트 객체(`CreateEventW`)를 사용하여 모달 응답 대기.

### 4.7 UI7 & UI8 (표 단위 Direct2D & 표그림판)
- **수용 대화상자**: F-47 (`_CellTextDialog`), F-48 (`_pickColor`), F-49 (`_HwpxTablePicker`), F-50 (`TableSketchTextDialog`)
- **구현 방식**:
  - `_pickColor`: 26색 컬러 매트릭스 버튼 및 커스텀 색상(Hex) 입력 대화상자.
  - `CellTextDialog`: 멀티라인 에디트 컨트롤과 정렬/굵기 체크박스 조합. `VK_RETURN`(Ctrl+Enter)으로 즉시 셀 반영.

### 4.8 UI9 (다이어그램 셸 - WebView2)
- **수용 대화상자**: F-08~F-11 (다이어그램 변경사항 버리기, 저장, 보관함, 삭제)
- **구현 방식**:
  - WebView2 호스팅 부모 윈도우에서 `ICoreWebView2::WebMessageReceived`를 수신하여 네이티브 Win32 다이얼로그로 팝업 띄운 후 결과 JSON을 `PostWebMessageAsJson`으로 반환.

---

## 5. 키보드 접근성 및 메시지 루프 공통 규격

1. **대화상자 메시지 루프 (`IsDialogMessageW`)**:
   - 모달 및 모덜리스 다이얼로그 모두 주 메시지 루프에서 `IsDialogMessageW(hDlg, &msg)`를 호출하여 Tab/Shift+Tab 포커스 이동, 기본 버튼 Enter 처리(`DM_SETDEFID`), Alt+밑줄 단축키를 완벽히 보장한다.
2. **ESC 키 취소 규격**:
   - 모든 대화상자의 `WM_COMMAND`에서 `LOWORD(wParam) == IDCANCEL` 처리 시 작업을 롤백하거나 창을 닫는다.
   - 단, 데이터 손실 위험이 있는 모달(서식통일 2~3단계, 변경사항 미저장 창)은 확인 메시지 창을 한 번 더 띄우거나 `PopScope` 원칙에 따라 ESC 단독 종료를 방어한다.
3. **엔터 키(Enter) 제출 규격**:
   - 단일 라인 텍스트 입력 칸에서 Enter 입력 시 기본 동작 버튼(`IDOK` / `PrimaryButton`)이 트리거된다.
   - 다중 라인 편집기(메모, 텍스트 자료)에서는 Ctrl+Enter를 제출 가속기로 지정한다.
4. **UI Automation (UIA) 지원**:
   - 각 컨트롤에 명확한 UIA Name, Role, HelpText 속성을 부여하여 스크린 리더(내레이터) 환경에서 대화상자 제목과 에러 안내 문구가 자동 낭독되도록 구현한다.
