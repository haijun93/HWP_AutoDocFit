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
