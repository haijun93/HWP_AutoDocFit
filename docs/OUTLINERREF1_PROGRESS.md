# OUTLINERREF1 진행 기록

- 2026-11-25 00:22 카드 이어받기 시작: Codex 중단 로그($3-codex.log / OUTLINERREF1-codex.log) 확인 결과 사용 한도로 미착수 상태 확인
- 2026-11-25 00:27 (1) outliner_core 엔진: 3중/2중 괄호 블록 참조((( id ))) 및 (( id )) 파싱, 숫자 ID 및 UUID 해결(resolve_block_ref), insert_block_ref, block_backlinks, check_page_references 구현
- 2026-11-25 00:28 (2) docfit_app/outliner_service.py: rename 시 링크 일괄 갱신 및 undo_rename 구현, check_delete 및 delete 휴지통 이동(.trash)과 restore 복원 구현, 블록 참조 연동
- 2026-11-25 00:30 (3) outliner_win: links.rs 블록 참조 문맥(RefKind) 및 자동완성 지원, find.rs block_backlinks_all, pages.rs check_references 및 undo_rename, shell.rs 블록 역참조 병합 및 페이지 관리 반영
- 2026-11-25 00:33 (4) flutter_ui: outliner_workspace.dart 블록 참조 ((, ((( 후보 자동완성 팝업, _BacklinksPanel 블록 역참조 표시, 페이지 이름 변경 시 실행 취소 스낵바, 삭제 시 참조 경고 대화상자 및 복원 스낵바 구현
- 2026-11-25 00:35 (5) 시험 작성: outliner_core 회귀 및 신규 기능 시험(블록 참조/페이지 관리/Enter·Tab·Undo·다중선택 불변), capture_ref_set 하네스 캡처, outliner_service 파이썬 단위 시험 작성
- 2026-11-25 00:36 (6) 결과 문서 작성: docs/OUTLINERREF1_RESULT_2026-11-24.md 작성 (시험 미실행·미커밋 명시, 드래그 이동 및 Win32 되돌리기 단추 설계 메모 포함)
- 2026-11-25 검증 1차: Rust core 85개·Win32 131개·Flutter 아웃라이너 14개 통과, parity 하네스 동등 42 시나리오 차이 0; Flutter analyze 이상 없음. 전체 Python 시험은 진행 중.
- 2026-11-25 서비스 점검: 페이지 참조 확인을 Rust `check_page_references` API로 연결하고, rename/undo 실패 시 페이지 파일을 원위치로 복구하도록 보완. 휴지통 이동 실패는 영구 삭제하지 않고 오류 처리. 서비스 시험 14개 통과.
- 2026-11-25 Win32 블록 참조 자동완성: `((( id )))` / `(( id ))`에서 여백과 닫는 괄호를 함께 치환하는 경계 수정 및 UTF-16/이모지 포함 시험 추가. cargo test outliner_win 전체 재실행 통과(132개); parity 차이 0 포함.
