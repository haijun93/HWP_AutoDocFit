# 몽땅클립보드 빠른 목록: 그림을 "붙임사진 편집"으로 바로 보내기 (2026-11-14)

## 1. 개요 및 목적
단축키(Ctrl+Shift+V) 몽땅클립보드 빠른 목록에서 마우스 hover 시 나타나는 액션 단추 중 그림 항목(`kind == "image"` 또는 그림 파일이 포함된 항목)에 한해 **[붙임사진]** 버튼을 [핀][X]의 왼쪽에 추가한다.
이 버튼을 누르면:
1. 해당 그림 항목을 클립보드에 올린다(기존 복원 경로).
2. 앱(Python 백엔드)에 "붙임사진 편집 열고 클립보드 그림 붙여넣기" 명령을 보낸다.
3. 앱이 켜져 있지 않으면 앱 실행 파일(`app_path` 또는 설치/상주 경로)을 `--photo-paste` 인자로 실행한다.
4. 빠른 목록 팝업은 즉시 닫는다.
5. 앱 및 Flutter 화면은 명령/인자를 받아 '붙임사진 편집'(탭 5)으로 전환하고, 클립보드에 올라간 그림을 새 사진(배경 이미지)으로 붙여넣는다.

---

## 2. 변경된 파일 목록 및 상세 내용

### 1) K에이전트 (Rust)
- **`kagent/src/store.rs`**:
  - `Settings` 구조체에 `pub app_path: String` 필드 추가.
  - 기본값 `""`, JSON 직렬화/역직렬화(`apply_file`, `to_json`) 반영.
- **`kagent/src/popup.rs`**:
  - 상수 `ROW_PHOTO_W: i32 = 28` 정의.
  - `enum Action { Photo, Pin, Delete }` 정의.
  - `row_action(x, width, photo_w, pin_w, delete_w) -> Option<Action>`로 일반화.
    - 그림 아닌 항목(`photo_w = 0`): `[핀][X]` 2개 영역 (폭 56px).
    - 그림 항목(`photo_w = 28`): `[붙임사진][핀][X]` 3개 영역 (폭 84px).
  - `is_image_item(item: &Summary) -> bool` 판별 함수 추가.
  - `draw_photo_edit_icon(hdc, r, color)`: 선 그림 방식(작은 사진 액자 사각형 + 산 모양 선 + 우측 하단 대각선 연필 선) 아이콘을 GDI Pen으로 정교하게 렌더링. 테마 및 선택 상태 색상(`p.row_text(sel, p.icon)`) 반영.
  - `State`에 `hover_action: Option<Action>` 필드 추가 및 `WM_MOUSEMOVE`, `WM_MOUSELEAVE`에서 갱신.
  - 상단 메뉴바 힌트 영역에 `Action::Photo` 마우스 올림 시 `"붙임사진 편집으로 보내기"` 안내 문구(툴팁/접근성 안내) 표시.
  - 클릭 처리(`WM_LBUTTONUP`):
    - **Mutex 데드락 방지**: `st.items` 조회를 블록 스코프로 묶어 즉시 잠금 해제한 후 액션 처리.
    - `Action::Photo` 클릭 시 `crate::restore(id, false, std::ptr::null_mut())`로 클립보드 복원 후 `send_to_photo_edit()` 호출, `hide()`로 창 닫기.
  - `send_to_photo_edit()` & `launch_app_for_photo_paste()`:
    - 앱 서비스 Named Pipe(`\\.\pipe\HwpAutoDocFit.appsvc.<SID>`)로 `{"cmd":"open_photo_edit"}` 전송(타임아웃 800ms).
    - 응답 실패/파이프 미존재 시 `settings.app_path` 또는 현재 실행 디렉토리 주변의 `hwp-auto-docfit.exe`를 찾아 `--photo-paste` 인자로 백그라운드 프로세스 실행.
  - 단위 시험 수정 및 추가:
    - `hover_actions_reserve_right_side_only`: 버튼 2개(그림 아님)와 버튼 3개(그림 항목)의 경계값 영역 검증.
    - `image_item_detection`: kind="image", kind="files" 및 Category::Image 포함 여부 판정 검증.

### 2) Python 백엔드 (docfit_app)
- **`docfit_app/kagent_app_service.py`**:
  - `set_photo_edit_handler(callback)` 함수 추가.
  - `handle(request)`: `cmd in ("open_photo_edit", "open_photo_edit_paste")` 수신 시 등록된 핸들러 콜백 실행 및 `{"ok": True}` 응답.
- **`docfit_app/instance_files.py`**:
  - `split_photo_paste_arguments(arguments)`: 커맨드라인 인자에서 `--photo-paste` 분리.
  - `start(gui)` 및 `poll()`: 앱 미실행 인자 및 IPC 파일 큐 요청에서 `gui._photo_edit_startup = True` 플래그 설정.
- **`desktop_web_ui.py`**:
  - `start_background()` 시 `kagent_app_service.set_photo_edit_handler(bridge.request_photo_edit)` 등록.
  - `request_photo_edit()` 및 `_drain_photo_edit_request()`:
    - `_photo_edit_open` 플래그 및 `gui._photo_edit_startup` 플래그 확인 후 클리어.
    - WebView2에 `window.dispatchEvent(new CustomEvent('docfit-open-photo-edit', {detail: {paste: true}}))` 전송.
  - `get_state()`: 상태 반환 딕셔너리에 `"photo_edit_open": photo_edit_open` 포함.

### 3) Flutter 화면 (flutter_ui)
- **`flutter_ui/lib/features/workspace/tab_navigation_shell.dart`**:
  - `const kTabPaint = 5;`, `const kTabPhotoEdit = 5;` 상수 추가 (탭 5: '붙임사진 편집').
- **`flutter_ui/lib/features/workspace/workspace_adapters.dart`**:
  - `PaintAdapter` 및 `IntegratedWorkspace`에 `photoPasteTrigger: ValueListenable<int>?` 추가 및 `FlowEditorScreen`에 전달.
- **`flutter_ui/lib/main.dart`**:
  - `Workspace` 위젯에 `final VoidCallback? onPhotoEditOpen;` 콜백 추가.
  - `_WorkspaceState.apply`: 상태 폴링에서 `next['photo_edit_open'] == true` 시 `widget.onPhotoEditOpen?.call()` 호출.
  - `_IntegratedHomeState`:
    - `photoPasteTrigger = ValueNotifier<int>(0);` 선언 및 전달.
    - `onPhotoEditOpen` 발생 시 `tabRequest.value = kTabPaint; photoPasteTrigger.value++;` 실행.
    - `docfit-open-photo-edit` 윈도우 이벤트 수신 시 동일하게 탭 전환 및 붙여넣기 트리거.
- **`flutter_ui/lib/features/flows/flow_editor_screen_native.dart`**:
  - `FlowEditorScreen`에서 `DiagramEditorScreen`으로 `initialMode` 및 `photoPasteTrigger` 전달.
  - `DiagramEditorScreen`:
    - `initialMode`, `photoPasteTrigger` 전달받음.
    - 웹뷰 ready 메시지 수신 시 `post({'type': 'set_mode', 'mode': widget.initialMode})` 전송.
    - `pasteFromClipboard()` 함수 구현: `widget.api('table_sketch', ['clipboard_image'])` 호출 후 웹뷰로 `set_background` 메시지 전송.
    - `photoPasteTrigger` 및 `docfit-open-photo-edit` 사건 수신 시 자동으로 `pasteFromClipboard()` 실행 (웹뷰 준비 전이면 플래그 보관 후 ready 시 자동 실행).
    - 앱바 도구 모음에 `[클립보드 사진 붙여넣기]` 아이콘 버튼(`Icons.content_paste_go`) 추가.
- **`flutter_ui/lib/features/flows/flow_editor_screen_web.dart`**:
  - 웹 환경에서도 `pasteFromClipboard()` 구현 및 사건 수신, 앱바 도구 버튼 추가.

### 4) 시험 코드
- **`tests/test_instance_files.py`**:
  - `--photo-paste` 명령줄 인자 분리 및 startup 플래그 설정 단위 시험 추가.
- **`tests/test_kagent_app_service.py`**:
  - `open_photo_edit` 명령 수신 시 핸들러 콜백 실행 단위 시험 추가.
- **`flutter_ui/test/photo_edit_paste_test.dart`**:
  - `tabRequest`를 통한 5번('붙임사진 편집') 탭 전환 및 `photoPasteTrigger` 리스너 통지 위젯 테스트 추가.

---

## 3. 확인 못 한 부분 및 안내 사항
- **명령 실행 없음 규칙 준수**: 사용자 요청에 따라 `run_command`를 통한 컴파일(`cargo check/test`, `flutter test`, `python -m unittest`)을 실행하지 않았습니다. 정적 분석 및 문법/타입/스코프 검토를 완료하였습니다.
- **실제 환경 동작 확인 필요**:
  1. 실제 Windows 환경에서 kagent 팝업을 띄우고 그림 항목 마우스 hover 시 `[붙임사진][📌][✕]` 3개 단추가 보이는지, 상단 힌트에 "붙임사진 편집으로 보내기"가 표시되는지 확인.
  2. 단추 클릭 시 클립보드에 그림이 복원되고 앱의 '붙임사진 편집' 탭으로 전환되며 그림이 캔버스에 들어가는지 확인.
  3. 앱이 종료된 상태에서 클릭 시 앱이 `--photo-paste` 인자로 기동되며 '붙임사진 편집' 화면으로 진입하는지 확인.

---

## 4. G-OPEN: 글·파일·폴더·웹·그림 연결 프로그램으로 바로 열기 버튼 (2026-11-14)

### 1) 개요 및 목적
단축키(Ctrl+Shift+V) 몽땅클립보드 빠른 목록에서 마우스 hover 시 나타나는 액션 단추 중 [핀][X] 단추의 왼쪽에, 글·파일·폴더·웹·그림 항목을 연결 프로그램으로 직접 열 수 있는 **[열기]** 버튼을 추가한다.
G-PHOTO에서 구현된 `row_action` 자리 체계를 확장하여 그림 항목과 일반 항목의 버튼 배치를 통일하고 확장하였다.

### 2) 버튼 순서 및 UI/UX
- **버튼 순서 (오른쪽 정렬)**:
  - **그림 항목 (4개 버튼)**: `[열기][붙임사진][핀][X]` (폭 112px)
  - **일반 항목 (3개 버튼)**: `[열기][핀][X]` (폭 84px)
- **아이콘 디자인 (`draw_open_icon`)**:
  - 핀(📌)·X(✕)와 어울리는 선 그림(GDI Pen 1px) 방식.
  - 작은 창/상자 모양 + 우상단 바깥으로 나가는 대각선 화살표와 화살표 촉을 정교하게 렌더링.
- **UIA 및 힌트**:
  - UIA/접근성 이름 및 상단 메뉴바 힌트 영역에 마우스 hover 시 `"연결 프로그램으로 열기"` 문구 표시.
- **닫기 동작**:
  - 버튼 클릭 또는 단축키(Ctrl+O)로 열기 시 빠른 목록 즉시 닫음(설정의 '고르면 바로 붙여넣기'와 무관하게 닫기).

### 3) 항목 종류별 열기 동작
1. **글(text)**:
   - 임시 파일 이름 규칙: `%TEMP%\kclip-open-<id>.txt` (`TEMP_PREFIX = "kclip-open-"`).
   - UTF-8 BOM (`0xEF, 0xBB, 0xBF`)을 선두에 기록하여 메모장 등 Windows 기본 연결 프로그램에서 한글 깨짐 방지.
   - 30일 정리 규칙: 파일 생성 및 정리 시 `%TEMP%` 내의 `kclip-open-*` 파일 중 최종 수정 시간이 30일(`30 * 24 * 3600초`) 이상 지난 파일을 자동으로 정리(`cleanup_old_temp_files`).
   - 기본 텍스트 뷰어로 ShellExecute `"open"` 실행.
2. **파일(files)**:
   - 첫 파일(여러 개면 최대 10개까지 순서대로)을 ShellExecute `"open"`으로 실행.
   - 경로 미존재 시 `"경로가 없습니다:\n{path}"` 안내.
   - **보안**: 명령 실행 파일(`.exe`, `.bat`, `.cmd`, `.ps1`, `.vbs`, `.js`, `.lnk` 등 `FORBIDDEN` 목록)은 열기 전에 확인창 없이 **열지 않음**으로 차단하고 `"보안상 실행 파일은 직접 열 수 없습니다:\n{path}"` 안내 표시.
3. **폴더**:
   - `is_dir()` 경로 판정 시 Windows 탐색기(`ShellExecute` `"explore"` / `"open"`)로 열기.
4. **웹(URL)**:
   - `safe_web_url` 규칙에 따라 `http://` 및 `https://` 스킴만 허용하고, 그 외 위험 스킴(`file:`, `javascript:`, `ftp:`, `data:`, `ms-settings:` 등)은 차단.
   - 정상 웹 URL은 기본 브라우저(`ShellExecute` `"open"`)로 열기.
5. **그림(image)**:
   - `%TEMP%\kclip-open-<id>.png`로 저장 후 기본 이미지 뷰어로 열기.
   - 기존의 `[붙임사진]` 버튼(앱의 붙임사진 편집 탭으로 보내기)과 독립적으로 동작.

### 4) 기존 열기 경로 및 단축키 연계
- **키보드 단축키 (Ctrl+O)**:
  - 빠른 목록에서 키보드 탐색 중 `Ctrl+O` 입력 시 현재 선택된 항목에 대해 [열기] 동작 수행 후 창 닫음.
- **우클릭 메뉴 연계**:
  - 우클릭 팝업 메뉴의 `열기`(command 1) 및 `연결 프로그램 선택…`(command 2)도 갱신된 `open_item::open(hwnd, id, ...)`을 호출하도록 통합.

### 5) Mutex 재잠금(데드락) 방지
- 빠른 목록 팝업의 `STATE.lock()` 상태에서 `open_item_by_id`, `app().store.lock()`, `hide()`를 호출하지 않도록,
  1. `item_info` 추출 후 블록 스코프 종료로 `st` 락 즉시 해제.
  2. `is_hovered` 확인 후 블록 스코프 종료로 `STATE` 락 즉시 해제.
  3. `Action::Open` 처리 시 `open_item_by_id` 내부에서만 `store.payload`를 조회하고 즉시 해제.
  4. 이후 `hide()`를 단독 호출하여 자기 자신과의 데드락을 완전히 방지.

### 6) 변경된 파일 목록
- **`kagent/src/open_item.rs`**:
  - `temp_file_name(id, ext)`, `temp_file_path(id, ext)` 함수 추가.
  - `cleanup_old_temp_files(dir, max_age_secs)` 30일 정리 규칙 구현.
  - `open(hwnd, id, kind, data, extra, choose_app)` 시그니처 갱신 및 항목 종류별(글, 파일, 폴더, 웹, 그림) 열기 로직 구현.
  - 임시 파일 명명 규칙, UTF-8 BOM, 30일 정리 규칙, URL 스킴 필터링, 실행 파일 보안 차단 단위 테스트 추가.
- **`kagent/src/popup.rs`**:
  - `ROW_OPEN_W: i32 = 28` 추가.
  - `enum Action`에 `Open` 추가.
  - `row_action(x, width, open_w, photo_w, pin_w, delete_w)` 시그니처 확장 및 영역 판정.
  - `draw_open_icon(hdc, r, color)` 바깥으로 나가는 화살표 선 그림 아이콘 렌더링 구현.
  - `open_item_by_id(hwnd, id)` 헬퍼 함수 추가.
  - 상단 메뉴바 힌트 영역에 `"연결 프로그램으로 열기"` 반영.
  - `WM_MOUSEMOVE`, `WM_LBUTTONUP`의 버튼 너비 계산 및 `Action::Open` 클릭 처리(창 닫기 포함).
  - `WM_KEYDOWN`에 `Ctrl+O` 단축키 핸들러 추가.
  - `WM_RBUTTONUP` 우클릭 메뉴의 `open_item::open` 호출부 연계.
  - 단위 시험 `hover_actions_reserve_right_side_only`를 3개/4개 버튼 경계값으로 갱신.

### 7) 확인 못 한 부분 및 안내 사항
- **명령 실행 없음 규칙 준수**: 사용자 요청에 따라 `run_command`(`cargo check/test`)를 실행하지 않았습니다. 정적 분석 및 문법/타입/스코프 검토를 꼼꼼히 완료하였습니다.
- **실제 환경 동작 확인 권장**:
  1. 마우스 hover 시 그림 항목에서 `[열기][붙임사진][📌][✕]` 4개 버튼, 일반 항목에서 `[열기][📌][✕]` 3개 버튼이 올바른 순서로 나타나는지 확인.
  2. [열기] 버튼에 마우스 호버 시 상단 힌트에 "연결 프로그램으로 열기"가 표시되는지 확인.
  3. 글 항목 클릭 시 `%TEMP%\kclip-open-<id>.txt`가 UTF-8 BOM으로 생성되어 텍스트 편집기로 열리고 팝업이 닫히는지 확인.
  4. 파일/폴더/웹/그림 항목 클릭 시 각각 연결 프로그램(탐색기/브라우저/뷰어)으로 정상 실행되는지 확인.
  5. `.exe`, `.bat` 등 실행 파일 클릭 시 실행되지 않고 차단 안내가 뜨는지 확인.
  6. 빠른 목록에서 `Ctrl+O` 입력 시 선택된 항목이 열리는지 확인.
