# 메모 편집기 Rust 코어(editor_core) 설계 — 1~3단계

작성: 2026-11-07, 브랜치 `feature/edcore`. 근거: TODO.md '노트북(개발자 모드) 개선 3건 (3) +메모 추가'의 구현 방침
(외부 서식 편집 패키지 없이 Rust 편집 코어 + Flutter UI, Windows 메모장 벤치마킹, 독립 모듈).
1단계(버퍼·실행취소·찾기·Markdown·저장·JSON API) 뒤 2·3단계(인코딩 CP949, 상태 표시줄 값, 찾기·바꾸기 고급, 100만 글자 성능)를 9절에 추가했다.
이 문서의 범위는 **Rust 코어만**이다. Flutter UI·klib 연결은 하지 않았다.

## 1. 구성

`editor_core/` 크레이트(Cargo 의존성 0개, std만 — GitLab 보안 검사 때문). 시험: `cd editor_core && cargo test`.

| 모듈 | 역할 |
|---|---|
| `buffer.rs` `TextBuffer` | 갭 버퍼(바이트) + 줄 시작 색인 + 글자 수. 위치는 UTF-8 **바이트 오프셋**, 글자 경계만 허용. 내부 줄바꿈은 항상 `\n` |
| `doc.rs` `Document` | 버퍼 + 실행취소/다시실행(입력 묶음) + 줄·열 + 찾기·바꾸기 + 저장/자동 저장 + 변경 여부(dirty) |
| `search.rs` | 찾기(대소문자 옵션·온전한 낱말 옵션), 모두 바꾸기 문자열 생성 |
| `markdown.rs` | 블록·인라인 해석, 서식 적용/해제 연산 |
| `fileio.rs` | 인코딩(UTF-8/UTF-8 BOM/UTF-16 LE·BE/CP949) 감지·변환, 줄바꿈(CRLF/LF/CR) 감지·변환, 임시 파일 후 교체 저장 |
| `cp949.rs` + `cp949_table.rs` | CP949(EUC-KR 계열) 변환. 표는 `tools/gen_cp949.py`로 생성(17,048자), 9.1절 |
| `json.rs` | 직접 구현한 JSON 값·파서·직렬화(대리 쌍·깊이 64 제한) |
| `api.rs` `Engine` | JSON 한 줄 명령 → JSON 한 줄 응답. 여러 문서를 번호로 관리 |

### 선택한 구조와 이유
- **갭 버퍼**(rope 대신): 메모 편집은 커서 근처에 편집이 몰려 갭 버퍼가 단순하고 빠르다. 7 MB 글에서 같은 위치 이어 입력 글자당 12 µs.
  병목은 갭이 아니라 줄 색인 갱신(뒤 줄 오프셋 이동, 줄 수에 비례)이다. 수십만 줄 이상이 필요해지면 줄 색인을 청크/Fenwick 트리로 바꾼다(공개 API 불변).
- **내부는 LF 통일**: 열 때 줄바꿈 형식을 감지해 기억하고, 저장할 때 원래 형식으로 되돌린다. 붙여넣은 `\r\n`·`\r`도 LF가 된다. 새 문서는 메모장처럼 CRLF.
- **바이트 오프셋을 API 기준으로**: Flutter(`TextEditingValue`)는 UTF-16 코드 유닛이므로 `edit_to_utf16`/`edit_from_utf16` 변환 명령을 둔다
  (이모지 같은 대리 쌍 한가운데는 오류). 글자 경계가 아닌 위치는 항상 오류로 거절해 문자열이 깨지지 않는다.

## 2. 실행취소 규칙(입력 묶음)
- 이어 쓰기는 한 묶음: 직전에 넣은 글의 안쪽·끝에서 일어나는 교체는 합친다. **한글 조합(ㅎ→하→한→…)이 한 번의 실행취소**가 되는 이유.
- 공백·줄바꿈이 새 낱말을 시작하면 묶음을 나눈다(낱말 단위 실행취소). 줄바꿈부터는 새 묶음.
- Backspace/Delete 연속은 한 묶음(`abcdef`에서 뒤 3자를 지우고 실행취소 1번이면 복원).
- `begin_group`/`end_group`(중첩 가능)으로 여러 편집을 명시적으로 한 묶음으로. 서식 연산·모두 바꾸기는 내부에서 이를 쓴다.
- `commit`: 커서 이동·붙여넣기·포커스 이동 같은 경계에서 호출해 묶음을 마감.
- 단계는 최대 5000묶음(넘으면 가장 오래된 것 폐기). 새 편집은 다시실행 기록을 지운다.
- `dirty`: 저장 시점의 실행취소 위치와 비교하므로 입력 후 전부 취소하면 다시 "저장됨"으로 돌아간다.

## 3. Markdown 해석
지원: 제목(`#`~`######`), 굵게(`**`,`__`), 기울임(`*`,`_`), 인라인 코드(`` ` ``), 코드 블록(``` ``` ```), 링크(`[글](주소)`),
글머리(`- * +`, 들여쓰기 2칸=1단계)·번호(`1.`) 목록, 인용(`>`), 구분선(`---`,`***`,`___`).
의도적 단순화: 표·이미지·HTML·setext 제목·참조 링크·중첩 인용 미지원. 닫히지 않은 표지는 글자 그대로 둔다. 단어 안쪽 `_`(snake_case)는 기울임이 아니다.

`edit_md_parse`는 `[from_line,to_line)`만 해석한다(코드 블록 안/밖 상태는 문서 처음부터 울타리 줄만 훑어 계산). Flutter는 **화면에 보이는 줄만** 요청한다.

서식 연산(모두 한 번의 실행취소, 새 선택 범위를 돌려줌):
`edit_md_inline`(굵게·기울임·코드 토글: 표지 포함 선택·바깥을 감싼 경우 모두 해제, `***`에서 한쪽만 해제 가능),
`edit_md_block`(보통·제목1~6·글머리·번호·인용; 모든 줄이 이미 그 형식이면 보통으로 토글, 코드 블록 안은 건드리지 않음),
`edit_md_link`, `edit_md_rule`, `edit_md_code_block`(토글), `edit_md_clear`(서식 지우기: 줄 머리 표지·인라인 표지·코드 울타리 제거, 코드 블록 안 내용은 보존).

## 4. 공개 API (JSON)
요청 `{"cmd":"edit_xxx","doc":N,...}` → 응답 `{"ok":true,...}` / `{"ok":false,"message":"..."}`.
`cmd`·`ok`·`message` 규약은 klib 파이프와 동일. 위치 인자는 모두 바이트 오프셋.

| 명령 | 인자 | 응답 핵심 |
|---|---|---|
| `edit_ping` | – | `message`,`version` |
| `edit_open` | `text` 또는 `path`, 선택 `line_ending`, `encoding`(지정해 다시 열기) | `doc`, 줄/글자/바이트/낱말 수, `line_ending`,`mixed_line_endings`,`encoding`,`dirty` |
| `edit_close` / `edit_info` | `doc` | 정보 |
| `edit_text` / `edit_lines` | `start,end` / `from,to` | `text` / `lines[{line,start,text}]` |
| `edit_insert` / `edit_delete` / `edit_replace` | `pos,text` / `start,end` / `start,end,text` | `cursor`,`len`,`chars`,`lines`,`dirty`,`can_undo`,`can_redo` |
| `edit_begin_group` / `edit_end_group` / `edit_commit` | – | 묶음 제어 |
| `edit_undo` / `edit_redo` | – | 위와 같음 + `changed` |
| `edit_line_col` / `edit_offset` | `pos` / `line,col` | `line,col`(1부터, 글자 단위) / `pos` |
| `edit_to_utf16` / `edit_from_utf16` | `pos` / `unit` | 변환값 |
| `edit_find` / `edit_find_all` | `query`,`case_sensitive`,`whole_word`,`from`,`wrap`/`limit`,선택 `start`+`end`(범위) | `found,start,end` / `count,matches[]` |
| `edit_replace_next` / `edit_replace_all` | `query,with`+옵션, 모두 바꾸기는 선택 `start`+`end`(범위) | `replaced,start` / `count`,`sel_start`,`sel_end`(바뀐 구간) |
| `edit_md_parse` | `from_line,to_line` | `blocks[{kind,line,start,end,content_start,spans[]}]` |
| `edit_md_inline` | `start,end,kind=bold\|italic\|code` | `sel_start,sel_end` |
| `edit_md_block` | `start,end,kind=paragraph\|heading\|bullet\|number\|quote`,`level` | `sel_start,sel_end` |
| `edit_md_link` / `edit_md_rule` / `edit_md_code_block` / `edit_md_clear` | `start,end[,url]` / `pos` | 선택 범위 / `cursor` |
| `edit_set_line_ending` / `edit_set_encoding` | `line_ending` / `encoding`(별칭 허용, CP949로 못 바꾸는 글자가 있으면 거절) | 정보 |
| `edit_status` | `pos`, 선택 시 `sel_start`,`sel_end` | 상태 표시줄 값(9.2절) |
| `edit_save` / `edit_autosave` | `path`(저장은 생략 가능) | 정보 / `saved,dirty` |

Rust에서는 `editor_core::api::Engine::new().handle(&str) -> String` 한 함수가 전부다. 직접 쓸 때는 `Document`·`markdown::*`를 바로 호출할 수 있다.

### 저장·자동 저장
`save`: 같은 폴더에 `.이름.PID.tmp`를 쓰고 `sync_all` 후 `rename`으로 교체(Windows에서도 기존 파일 덮어쓰기). 실패하면 임시 파일을 지우고 원본은 그대로.
`autosave`: 같은 방식으로 **복구 사본**을 따로 쓰며 원본·dirty 상태는 건드리지 않는다. 호출 시점(입력 후 N초 유휴, 포커스 상실)은 UI가 정한다.
복구 사본 위치·수명 정책(예: `%LOCALAPPDATA%\HWP_AUTODOCFIT\memo-recovery\<메모id>.autosave`, 저장 성공 시 삭제)은 UI 단계에서 확정.
인코딩은 UTF-8/BOM/UTF-16/CP949(9.1절). 새 문서·저장 기본은 UTF-8.

## 5. klib 통합 방안 비교

| | A. klib에 포함 (`klib/Cargo.toml`에 `editor_core = { path = "../editor_core" }`) | B. 새 exe (`editor_core` 단독 서버) |
|---|---|---|
| 호출 경로 | 기존 파이프·Dart 브리지 그대로. `pipe.rs`의 `match cmd`에 `cmd.starts_with("edit_")` 한 줄을 `Engine`에 위임 | 새 파이프 이름·기동/종료/유휴 종료·사용자 SID 보안 등을 다시 구현 |
| 배포·서명 | 파일 수 불변, 서명 대상 불변. klib.exe 크기 증가는 작음(코어 의존성 0, 순수 로직) | exe 1개 추가: 서명·설치기·백신 예외·업데이트 항목 추가 |
| 수명 | klib 프로세스와 같음(유휴 종료 시 열린 편집 문서 상태가 사라짐 → **문서 상태를 UI가 항상 복구 가능하게** 보관 필요) | 편집 전용 수명 가능 |
| 결함 격리 | `panic = "abort"`(klib release 설정)라 코어 버그가 klib(자료함·검색 색인)까지 죽임 | 편집기 문제가 자료함에 번지지 않음 |
| 잠금 | 편집은 문서별 상태라 `Mutex<Engine>` 하나로 충분(명령이 µs 단위) | 해당 없음 |

**권고: A(klib 포함).** 근거: 명령이 모두 µs~수십 ms 순수 계산이고 의존성이 없어 크기·보안 부담이 거의 없으며, 파이프·서명·기동 로직 재사용이 크다.
단, 위험 두 가지를 같이 처리한다: ① `panic = "abort"` 때문에 코어는 `unwrap`/인덱싱 패닉이 없어야 한다 → 이번 시험에 3000회 무작위 모델 시험을 넣었고, UI 연결 전 퍼징(무작위 명령열) 확대를 권한다.
② klib가 유휴로 종료돼도 메모가 사라지지 않게, Flutter가 편집 중 내용을 자동 저장(`edit_autosave`)하고 재연결 시 `edit_open`으로 복원한다.
이후 크기·안정성 문제가 확인되면 같은 `Engine`을 `src/bin/editor_server.rs`로 빼서 B로 바꿀 수 있다(코어 변경 없음).
이 결정은 사용자 확인 사항이다(klib.exe는 서명 대상 배포물이므로).

연결 시 `pipe.rs` 변경 예(다음 카드):
```rust
static EDITOR: std::sync::OnceLock<Mutex<editor_core::api::Engine>> = std::sync::OnceLock::new();
// handle_request 맨 앞(Value로 바꾸기 전)에서:
if val.str("cmd").is_some_and(|c| c.starts_with("edit_")) {
    return EDITOR.get_or_init(|| Mutex::new(editor_core::api::Engine::new())).lock().unwrap().handle(req_str);
}
```
(`handle`은 이미 완성된 JSON 문자열을 돌려주므로 `Value`로 다시 만들지 말고 그대로 응답에 쓴다.)

## 6. IME 경계 (Flutter가 맡는 것 / Rust가 맡는 것)
- **IME 조합(한글 입력)은 Flutter `EditableText`/`TextField`가 그대로 처리**한다. 조합 중 글자("ㅎ→하→한")는 Flutter 쪽 `TextEditingValue.composing`에 있다.
- Rust는 **확정된 변경의 기록·버퍼·이력·저장·서식**만 맡는다. 조합 중에는 Rust에 보내지 않거나, 보내더라도 `replace_range`로 조합 구간을 교체하면 같은 실행취소 묶음에 합쳐진다(시험 `hangul_composition_steps_become_one_undo`).
- 권장 흐름: 조합 확정(또는 조합 종료/공백) 때 변경분 `(start,end,text)`를 UTF-16→바이트로 변환해 `edit_replace`; 서식 단추·실행취소·찾기·저장만 Rust 호출. Ctrl+Z는 UI가 가로채 `edit_undo` 결과 텍스트를 반영(조합 중이면 먼저 확정).
- 큰 글에서 모든 글자를 파이프로 왕복하지 않도록, UI는 `TextEditingController`를 소스로 두고 Rust는 "이력/저장/검색/서식 계산"의 미러로 쓰는 안을 기본으로 한다. 두 쪽 내용이 어긋날 위험(드리프트)은 `edit_info`의 `len`/`chars`로 주기 검증하고 어긋나면 `edit_open`으로 다시 맞춘다.
- 직접 그리는 편집기(자체 커서·선택·IME)는 IME·접근성 위험 때문에 후속 검토 — TODO.md 방침과 동일.
- **한글 조합 실기 확인은 Windows에서만 가능**(이 카드는 macOS에서 코어만 시험).

## 7. 시험·성능 기록 (macOS arm64, `cargo test` 21개 + 성능 1개)
입력: 100,000줄, 6.99 MB, 4,468,890 글자(한글·영문·제목·목록·인라인 코드 혼합). `cargo test --release --test perf -- --nocapture`.

| 항목 | 시간 |
|---|---|
| 불러오기(LF 통일+줄 색인) | 11.5 ms |
| 무작위 위치 입력 4000회 | 244.6 ms (회당 61 µs) |
| 같은 위치 이어 입력 20000자 | 241.2 ms (회당 12 µs) |
| 줄·열 계산 1회 | 0.024 ms |
| 통계(줄·글자·낱말) | 12.3 ms |
| 찾기 모두 – 대소문자 구분 (80000건) | 11.3 ms |
| 찾기 모두 – 대소문자 무시 (80000건) | 31.8 ms |
| 모두 바꾸기 (80000건, 한 번의 실행취소) / 실행취소 | 33.3 ms / 7.5 ms |
| Markdown 전체 해석(100001블록) / 화면 60줄 | 38.1 ms / 0.3 ms |
| 저장(임시 파일 후 교체) / 파일 열기 | 21.1 ms / 14.1 ms |
| 4001묶음 전체 실행취소 | 236.6 ms |
| JSON 명령 1000회(삽입) | 1.2 ms (회당 1.2 µs, 파이프 제외) |

디버그 빌드(`cargo test`)에서도 성능 시험이 통과(약 7초). 파이프 왕복 지연은 포함하지 않았다(Windows 실기 측정 필요).

## 8. 남은 일 / 후속
1. klib 연결(`pipe.rs`에 `edit_*` 위임) + Dart 브리지 클라이언트 — 위 5절 결정 후.
2. `flutter_ui/lib/features/editor/` UI(도구줄, 상태 표시줄 줄·열·글자 수·줄바꿈, 자동 저장 표시, 찾기·바꾸기 막대) — UTF-16↔바이트 변환 포함.
3. 줄 색인 청크화(수십만 줄 이상 필요 시), 줄 바꿈(word wrap) 계산은 UI 쪽.
4. 확대·축소는 UI 단. (CP949는 9.1절에서 완료. 그 밖의 레거시 인코딩·BOM 없는 UTF-16 자동 감지는 하지 않음 — 필요하면 `encoding` 지정으로 열기.)
5. 무작위 명령열 퍼징 확대(패닉 0 보장; klib `panic = "abort"`).

## 9. klib 연결과 메모 화면 2단계 (EDLINK, 2026-11-07, 브랜치 `feature/edlink`)

5절 A안으로 연결했다(새 exe 없음).
- **klib**: `Cargo.toml`에 `editor_core = { path = "../editor_core" }`(경로 의존성만). `pipe.rs`의 `handle_request`가 `cmd`가 `edit_`로 시작하면
  `OnceLock<Mutex<Engine>>`에 위임하고 코어의 응답 문자열을 그대로 돌려준다(자료함 DB 잠금과 무관). 시험 `test_edit_commands_delegate_to_editor_core`.
- **Python**: `library_bridge.ALLOWED_COMMANDS`에 4절의 `edit_*` 33개, `EDITOR_COMMANDS` 추가. 화면 통로는 새 브리지 메서드 대신
  `desktop_web_ui.notebook()`이 `edit_*`를 klib에 그대로 넘긴다(개발자 모드 전용). **`path` 인자는 거절**: 메모는 노트북 출처로 저장하므로
  화면에서 임의 파일을 열거나 덮어쓰는 통로를 만들지 않는다(`edit_save`·`edit_autosave`는 허용 목록에는 있으나 지금 화면 경로로는 쓸 수 없음).
- **Flutter**: `EditorCore` 경계를 넓혀(명령은 `Future`, 찾기·바꾸기·개수·줄바꿈·인코딩·엔진 이름·`changes`·`dispose`)
  `KlibEditorCore`(`klib_editor_core.dart`)를 추가. 6절 권장대로 `TextEditingController`가 원본이고 Rust는 거울이다:
  입력은 바뀐 구간만 UTF-16→UTF-8 바이트로 바꿔 `edit_replace`(요청 줄 세우기로 순서 보장), 실행취소·서식·찾기·바꾸기는 Rust에 묻고 글을 다시 받는다
  (`edit_text`가 응답 64 KiB를 넘으면 `edit_lines`로 400줄씩). 30초마다 `edit_info`로 klib를 깨워 두고 글자 수로 드리프트를 검사한다.
  klib가 문서를 잃으면(유휴 종료 후 재기동) 지금 글로 다시 열고(이전 실행취소 기록은 사라짐, 세대 번호로 중복 반영 방지),
  다시 열 수도 없으면 `DartEditorCore`로 바꿔 끼운다(klib 없음·요청 64 KiB 초과 포함). 노트북 '+메모 추가'는 `NotebookApi.memoEditorBackend`가 있으면 Rust 코어를 쓴다.
- **메모 화면 ②**: 찾기·바꾸기 막대(Ctrl+F/Ctrl+H, F3/Shift+F3, 대/소문자·온전한 낱말, 바꾸기=메모장처럼 첫 번째는 선택만, 모두 바꾸기, 일치 수,
  포커스가 없어도 찾은 구간 칠하기), 확대·축소(Ctrl+휠, Ctrl+=/−/0, 50~500%, Ctrl 누른 동안 본문 휠 스크롤 막음),
  상태 표시줄(줄·열, 글자 수, 확대 비율, 줄바꿈 `Windows (CRLF)`/`Unix (LF)`/`Macintosh (CR)`, 인코딩, 엔진 표시 도움말).
  명령이 파이프를 오가는 동안 들어온 입력은 버린다(Rust와 글이 어긋나지 않게).

### editor_core에 있으면 좋을 공개 API (이번 카드에서는 코어를 고치지 않음)
1. 거꾸로 찾기(`edit_find`에 `backward`) — 지금은 `edit_find_all`(최대 10000개)에서 고른다.
2. 개수만 세기(`edit_count`) — 지금은 일치 구간 목록 전체를 받는다. `edit_find_all`에 상한 초과 여부(`truncated`)도 필요.
3. 실행취소·다시실행·서식·모두 바꾸기 응답에 **바뀐 구간**(`start,end,text`) — 지금은 명령마다 글 전체를 다시 받는다(큰 메모에서 느림).
4. 이력 없이 글을 이어 넣는 열기(`edit_open` 나눠 보내기 또는 `edit_reset_history`) — 요청 64 KiB(Python 브리지 한도)를 넘는 메모는 지금 Dart 폴백.
5. 열린 문서 수 조회(`Engine::doc_count`) — klib 유휴 종료 감시가 편집 중 문서가 있으면 종료하지 않게(지금은 30초 깨우기로 대신).
6. 패닉 격리: klib release는 `panic = "abort"`라 코어 패닉이 자료함까지 끝낸다 — 무작위 명령열 퍼징 확대 필요(8절 5).

## 10. 자료함 '+메모'·글 자료 고치기 (LIBMEMO, 2026-11-07, 브랜치 `feature/libmemo`)

결정 단계 ④(다른 화면에서도 재사용). 노트북 동작은 그대로다.
- **Flutter**: 노트북의 메모 화면(`_MemoPage`)을 `editor/memo_editor_page.dart`의 `MemoEditorPage<T>`로 옮겨 두 화면이 같이 쓴다
  (저장 단추 이름·안내 문구·저장 동작만 다르고 키 `memo-save`·`memo-hint`는 같음).
  자료함 도구줄 `+메모`(`libraryAddMemo`): 편집기를 열고 '자료함에 저장'하면 핵심자료에 `text` 자료로 넣는다. 저장하지 않고 닫으면 초안이 남고(`+메모(초안)`), 휴지통은 초안을 버린다.
  글 자료(`text`·`note`) 더블클릭·메뉴 '열기'·검색 결과 선택은 같은 편집기로 열고 '저장'하면 그 자료의 내용과 이름을 고친다(닫기만 하면 바뀌지 않음, 휴지통 없음).
  파일·폴더·링크·서식 글(`richText`)은 이전처럼 연결 프로그램·보기 창. 새 인터페이스 `LibraryMemoApi`(`readItemText`·`updateItemText`·`memoEditorBackend`)를
  `BridgeLibraryApi`·`InMemoryLibraryApi`가 구현하고, 구현하지 않는 API면 글 자료도 이전 보기 창으로 연다.
- **klib**: `item_text_get {id}` → `content`·`display_name`·`kind`, `item_text_set {id, content, display_name?}`(이름이 비면 그대로). 경로 인자는 없고,
  `text`·`note` 종류이면서 사본이 자료함 데이터 폴더 `boxes/<box_id>/items` 바로 아래 `.txt`일 때만 읽고 쓴다(사용자 원본 보호). 쓰기는 임시 파일 후 바꿔치기,
  저장 뒤 본문 검색 색인을 다시 만든다(실패는 `index_warning`). 시험 `test_item_text_get_set_for_memo_editor`.
- **Python**: `ALLOWED_COMMANDS`에 `item_text_get`·`item_text_set`. `desktop_web_ui.library()`도 `edit_*`를 노트북과 같은 `_memo_editor_request`로 보내
  `path` 인자와 모르는 편집 명령을 거절한다(이전에는 자료함 통로로 `edit_save {path}`가 그대로 klib에 갈 수 있었음).
- **한계**: 요청 64 KiB(Python 브리지 한도)를 넘는 메모는 `item_add`·`item_text_set`이 실패한다(편집기는 Dart 폴백으로 열리지만 저장 불가 — 오류 표시).
  고치기 화면은 자동 저장 초안이 없어 저장하지 않고 닫으면 고친 내용이 사라진다.

## 9. 2·3단계 (2026-11-07): 인코딩·상태 표시줄·찾기 고급·100만 글자 성능

### 9.1 인코딩
- **열 때 자동 감지 순서**: BOM(UTF-8 BOM, UTF-16 LE/BE) → BOM 없는 엄격한 UTF-8(순수 ASCII 포함) → CP949 → 모두 아니면 오류(UTF-8·CP949 오류 위치를 함께 알림).
  UTF-8로 해석되는 글은 항상 UTF-8로 본다(CP949 글이 우연히 유효한 UTF-8일 확률은 한글 글에서 사실상 0). **BOM 없는 UTF-16은 감지하지 않는다**(`encoding` 지정으로 열기).
- **지정해 열기**: `edit_open`의 `encoding`(자동 감지가 틀렸을 때). 이름은 `UTF-8`·`UTF-8 BOM`·`UTF-16 LE`·`UTF-16 BE`·`CP949`와 별칭(`EUC-KR`, `MS949`, `UHC`, `utf-8-sig` 등, 대소문자·공백·하이픈 무시).
- **저장**: 문서가 기억한 인코딩 그대로(새 문서·`text`로 연 문서는 UTF-8). `edit_set_encoding`으로 바꿔 저장 가능.
  **CP949로 표현할 수 없는 글자(이모지, 표에 없는 한자 등)는 `?`로 바꾸지 않고 거절**한다: `set_encoding`은 처음 해당 글자의 줄·열을 알리며 거절하고,
  이미 CP949인 문서에서 그런 글자를 입력한 뒤 `save`/`autosave`하면 오류를 돌려주며 **원본 파일은 그대로**(임시 파일 후 교체 이전 단계에서 실패).
- **CP949 변환표**: `cp949_table.rs`(자동 생성 134 KB, 코드에 포함). 앞바이트 0x81~0xFE × 뒤바이트 0x41~0xFE 격자에 유니코드 값을 적은 표로 **17,048자**(완성형 한글 11,172자 전부 + 한자 4,888자 + 기호·자모·로마자 등 988자). 0x80·0xFF·짝 없는 앞바이트·미할당 코드는 오류.
  역표(유니코드→CP949)는 처음 쓸 때 한 번 만든다(BMP 65,536칸, 128 KB).
  - **출처·라이선스**: 표 값은 "코드 ↔ 유니코드 코드 포인트" 대응이라는 공개 표준(KS X 1001 + 마이크로소프트 UHC 확장, 유니코드 컨소시엄도 같은 대응표를 공개)의 사실 나열이며, 제3자 소스 코드나 크레이트를 복사하지 않았다.
    생성기 `editor_core/tools/gen_cp949.py`가 파이썬 표준 코덱 `cp949`로 대응을 읽어 표를 쓴다(스크립트만 공개, 표는 재생성 가능). 법적 판단이 필요한 배포 전에는 사용자가 한 번 확인할 것(저작권이 성립하지 않는 사실 정보로 보지만 변호사 검토는 아님).
  - **검증**: ① 전체 표를 (앞,뒤,유니코드) FNV-1a 해시로 파이썬 기준값과 비교(`cp949_table_matches_reference`), ② 17,048자 전부 왕복, ③ 대표 글자·실제 문장 바이트열 비교(`한`=C7D1, `똠`=8C63, `①`=A8E7 등), ④ 잘못된 바이트 거절.
  - **알려진 한계**: 윈도 `bestfit949`의 "비슷한 글자로 대체"(예: 일부 특수 따옴표→`"`)는 하지 않는다 — 엄격 변환만. 유니코드 정규화(NFD 한글)는 하지 않는다.
- 줄바꿈은 기존대로 감지·보존(CRLF/LF/CR, 섞이면 `mixed_line_endings`), 새 문서는 CRLF.

### 9.2 상태 표시줄 값 (`edit_status`)
요청 `{"cmd":"edit_status","doc":N,"pos":커서,"sel_start":a,"sel_end":b}` (선택은 생략 가능, a>b여도 됨). 응답:

| 필드 | 뜻 |
|---|---|
| `line`,`col` | 1부터. **열은 글자(유니코드 스칼라) 경계 기준**: 이모지·한글 1개 = 1열, 줄 끝은 `줄 글자 수 + 1`. 위치가 글자 경계가 아니면 오류 |
| `lines` | 줄 수 |
| `chars` | 전체 글자 수(줄바꿈 1개 = 1글자), `chars_no_newline`은 줄바꿈 제외, 선택이 있으면 `selected_chars` |
| `bytes` | 내부(LF) 기준 UTF-8 바이트 수 |
| `line_ending`,`line_ending_label`,`mixed_line_endings` | `CRLF`/`LF`/`CR`, `Windows (CRLF)`/`Unix (LF)`/`Mac (CR)`, 섞임 여부 |
| `encoding` | `UTF-8`·`UTF-8 BOM`·`UTF-16 LE`·`UTF-16 BE`·`CP949` |
| `summary` | 바로 표시할 수 있는 문자열. 예: `줄 2, 열 1  |  5자 (선택 1자)  |  Windows (CRLF)  |  CP949` |

"글자"는 유니코드 스칼라 값이다(결합 문자·NFD 자모·이모지 조합은 여러 글자로 센다). 자소(grapheme) 단위는 표가 필요해 하지 않았다.
비용: 글자 수·줄 수는 O(1), 줄·열은 줄 길이에 비례(한 줄 100만 글자 가운데에서 1.1 ms), 선택 글자 수는 선택 길이에 비례(50만 글자 1.5 ms).

### 9.3 찾기·바꾸기 고급 (정규식은 하지 않음 — 범위 밖)
- **모두 바꾸기는 항상 실행취소 1단위**: 일치가 몇 건이어도 편집 기록은 하나(첫 일치~끝 일치 구간 교체)라 실행취소/다시실행 한 번에 모두 오가고, `dirty`도 처음 상태로 돌아온다(시험 `replace_all_is_single_undo_unit_with_dirty_tracking`). 사용자가 `begin_group` 안에서 부르면 그 묶음에 합쳐진다.
- **범위 지정**: `edit_find_all`·`edit_replace_all`에 `start`+`end`를 주면 선택 영역 안에서만 찾고 바꾼다. 범위에 완전히 들어가는 일치만 대상이고, "온전한 낱말"은 범위 밖 글자도 보고 판정한다(`concat`의 `cat`을 선택 일부로 오인하지 않음).
  응답에 바뀐 구간 `sel_start`,`sel_end`(바꾼 뒤 위치)를 줘 UI가 그대로 선택 표시할 수 있다.
- **줄바꿈 정합**: 찾는 말·바꿀 말의 `\r\n`·`\r`도 LF로 맞춘다(입력 칸이 CRLF로 줘도 LF 글과 일치).
- **빈 변경 없음**: 일치가 없거나 바꾼 결과가 같은 글이면 편집 기록·dirty를 만들지 않는다(개수는 알려 줌). 빈 검색어는 0건. `a`→`aa`처럼 바꿀 말에 찾는 말이 들어 있어도 한 번만 바꾼다.
- 한계: 선택 영역 밖 일치 건수 미리 보기(`edit_find_all`의 `count`로 대체 가능), 정규식·역참조·대소문자 보존 바꾸기는 없다.

### 9.4 큰 글(100만 글자) 성능 — 전/후
측정: `cd editor_core && cargo test --release --test perf_1m -- --nocapture --test-threads=1` (macOS arm64, release). 입력은 한글·영문·숫자 혼합 정확히 1,000,000글자(약 1.95 MB), 줄 배치 3종.
"전"은 이 단계 변경 전 코드(1단계 완료본), "후"는 3회 실행 중앙값. 단위는 칸에 표시(대부분 ms, "글자당/회당"은 µs). 디버그 빌드(`cargo test`)는 같은 시험을 10만 글자로 줄여 통과만 확인한다.


**여러 줄(줄당 40자)**

| 항목 | 전 | 후(3회 중앙값) | 비고 |
|---|---:|---:|---|
| N글자 한 번에 삽입 | 2.83 ms | 0.9 ms | 25001줄 **3배 빠름** |
| └ 실행취소 | 3.85 ms | 1.56 ms | **2배 빠름** |
| └ 다시실행 | 1.77 ms | 0.5 ms | **4배 빠름** |
| N글자 전체 삭제 | 1.89 ms | 1.38 ms | |
| └ 실행취소 | 0.58 ms | 0.46 ms | |
| └ 다시실행 | 2.03 ms | 1.48 ms | |
| 가운데 10% 삭제 | 0.18 ms | 0.16 ms | |
| └ 실행취소 | 0.09 ms | 0.06 ms | |
| 이어 입력 100000자(글자당) | 6.66 µs | 0.06 µs | 합계 7.3 ms **111배 빠름** |
| └ 실행취소 1회(묶음) | 0.24 ms | 0.25 ms | |
| Backspace 100000회(회당) | 4.47 µs | 0.14 µs | 합계 14.0 ms **32배 빠름** |
| └ 실행취소 1회(묶음) | 0.06 ms | 1.18 ms | |
| 무작위 위치 입력(회당) | 0.015 ms | 0.014 ms | 2000회 합계 28.3 ms |
| └ 2000단계 실행취소 | 29.58 ms | 28.41 ms | |
| 줄·열(문서 끝) 1회 | 0 ms | 0 ms | 25001줄 1열 |
| 모두 바꾸기 | 9.22 ms | 6.99 ms | 28571건 |
| └ 실행취소(1단위) | 1.81 ms | 2.09 ms | |

**한 줄**

| 항목 | 전 | 후(3회 중앙값) | 비고 |
|---|---:|---:|---|
| N글자 한 번에 삽입 | 0.52 ms | 0.49 ms | 1줄 |
| └ 실행취소 | 1.42 ms | 1.34 ms | |
| └ 다시실행 | 0.23 ms | 0.22 ms | |
| N글자 전체 삭제 | 1.34 ms | 1.34 ms | |
| └ 실행취소 | 0.24 ms | 0.21 ms | |
| └ 다시실행 | 1.38 ms | 1.41 ms | |
| 가운데 10% 삭제 | 0.15 ms | 0.15 ms | |
| └ 실행취소 | 0.02 ms | 0.02 ms | |
| 이어 입력 100000자(글자당) | 5.18 µs | 0.05 µs | 합계 4.5 ms **104배 빠름** |
| └ 실행취소 1회(묶음) | 0.21 ms | 0.2 ms | |
| Backspace 100000회(회당) | 4.14 µs | 0.05 µs | 합계 4.5 ms **83배 빠름** |
| └ 실행취소 1회(묶음) | 0.02 ms | 1.07 ms | |
| 무작위 위치 입력(회당) | 0 ms | 0 ms | 2000회 합계 0.2 ms |
| └ 2000단계 실행취소 | 0.06 ms | 0.06 ms | |
| 줄·열(문서 끝) 1회 | 1.22 ms | 1.28 ms | 1줄 1000001열 |
| 모두 바꾸기 | 8.5 ms | 6.75 ms | 28571건 |
| └ 실행취소(1단위) | 1.72 ms | 1.73 ms | |

**짧은 줄(줄당 4자)**

| 항목 | 전 | 후(3회 중앙값) | 비고 |
|---|---:|---:|---|
| N글자 한 번에 삽입 | 2.8 ms | 2.73 ms | 250001줄 |
| └ 실행취소 | 1.2 ms | 1.19 ms | |
| └ 다시실행 | 2.59 ms | 2.59 ms | |
| N글자 전체 삭제 | 1.15 ms | 1.16 ms | |
| └ 실행취소 | 2.48 ms | 2.7 ms | |
| └ 다시실행 | 1.29 ms | 1.14 ms | |
| 가운데 10% 삭제 | 0.18 ms | 0.14 ms | |
| └ 실행취소 | 0.31 ms | 0.3 ms | |
| 이어 입력 100000자(글자당) | 33.31 µs | 0.06 µs | 합계 6.4 ms **555배 빠름** |
| └ 실행취소 1회(묶음) | 0.23 ms | 0.21 ms | |
| Backspace 100000회(회당) | 26.34 µs | 4.84 µs | 합계 498.2 ms **5배 빠름** |
| └ 실행취소 1회(묶음) | 0.33 ms | 1.43 ms | |
| 무작위 위치 입력(회당) | 0.037 ms | 0.027 ms | 2000회 합계 54.0 ms |
| └ 2000단계 실행취소 | 73.48 ms | 53.73 ms | |
| 줄·열(문서 끝) 1회 | 0 ms | 0 ms | 250001줄 1열 |
| 모두 바꾸기 | 7.93 ms | 7.19 ms | 7143건 |
| └ 실행취소(1단위) | 4.09 ms | 3.9 ms | |

**읽는 법과 변경 내용**
- 가장 큰 병목 두 가지를 고쳤다.
  1. **이어 입력·Backspace 병합**이 매 글자 묶음 전체를 `format!`으로 다시 복사했다 → 글자 수에 비례해 느려져(10만 글자 묶음에서 글자당 5~7 µs) 100만 글자를 계속 치면 사실상 멈춘다. 이제 끝에 덧붙이기(`push_str`)·제자리 교체로 **글자당 0.05~0.06 µs**, Backspace는 조각을 모았다가 실행취소 때 한 번에 합친다(그래서 Backspace 10만 회 묶음의 실행취소가 0.06→1.1 ms로 늘었다. 병합 비용을 뒤로 미룬 것이며 체감 한계 이하).
  2. **줄 색인**을 갭 버퍼와 같은 물리 좌표로 저장해(`buffer.rs` 머리말), 갭 자리 입력·삭제에서 뒤쪽 줄 전부를 +n 하던 O(줄 수) 갱신을 없앴다. 줄당 4자(25만 줄)에서 이어 입력이 글자당 33 µs → 0.06 µs. 갭을 옮길 때만 옮겨지는 구간의 줄을 고친다. 무작위 위치 편집·실행취소가 약 30~35% 빨라졌다.
- 정확성 확인: 무작위 삽입·삭제·갭 이동 6000단계를 문자열 모델과 비교하며 **매 단계 줄 색인·글자 수 전체 대조**(`buffer::tests`), 기존 시험 전부 통과.
- **바뀌지 않은(잡음 수준) 항목**: 한 번에 삽입/삭제/실행취소/다시실행은 이미 1~4 ms 이내(메모리 복사 한계). 무작위 위치 입력은 갭 이동(memmove)이 지배적이라 줄 배치와 무관하게 위치 이동 거리에 비례한다(2000회 28~54 ms).
- **남은 병목**: 줄당 4자(25만 줄)에서 **줄바꿈을 지우는 Backspace**는 글자당 4.8 µs(줄 색인 `Vec` 중간 삭제 memmove). 25만 줄 이상에서 줄을 연달아 지우는 작업에서만 드러나며 100만 줄이 되면 비례해 커진다 → 필요해지면 줄 색인을 청크/Fenwick 트리로(공개 API 불변).
- 실행취소 기록은 편집당 지운/넣은 글을 문자열로 들고 있어, 100만 글자를 지우면 그만큼 메모리를 쓴다(5000묶음 제한이 있으나 묶음당 크기 제한은 없음). 필요하면 총 바이트 상한을 추가.

변환 속도(같은 100만 글자, 줄당 40자):

| 인코딩 | 저장용 변환 | 열기(자동 감지 포함) | 바이트 수 |
|---|---:|---:|---:|
| UTF-8 | 0.6 ms | 3.6 ms | 1,949,996 |
| UTF-16 LE | 1.5 ms | 2.3 ms | 2,000,002 |
| CP949 | 1.8 ms | 1.7 ms | 1,474,998 |

### 9.5 시험
`cd editor_core && cargo test` — 단위 1(줄 색인 무작위 모델) + 통합: `tests/core.rs` 21, `tests/encoding.rs` 14(CP949 표 해시·17,048자 왕복·대표 값·거절, 감지 순서·별칭, 지정 열기, CP949 파일 열기→편집→저장 왕복, 변환 불가 글자 처리, 상태 값, API, 모두 바꾸기 단위/범위/가장자리), 성능 2 + 100만 글자 2. 의존성 추가 없음(`Cargo.lock`에 외부 패키지 없음).

## 11. 3단계 ③ 큰 글 보기·인코딩 선택 저장 (EDCORE3, 2026-11-07, 브랜치 `feature/edcore`)
klib `edit_*` 명령에 얹을 JSON 규약이다(klib 코드는 Windows에서 연결 — 이 카드에서는 고치지 않음). 위치 인자는 기존대로 **UTF-8 바이트 오프셋**, 새 `char`는 **글자(유니코드 스칼라) 오프셋**(0부터), 줄은 `edit_line_range`만 0부터이고 `line`/`col` 입출력은 1부터(메모장 규약).

### 11.1 큰 글 보기
| 명령 | 요청 | 응답 |
|---|---|---|
| `edit_line_range` | `{"cmd":"edit_line_range","doc":1,"start_line":999000,"count":50}` (`count` 기본 50, 상한 5000) | `{"ok":true,"total_lines":1000001,"start_line":999000,"chars":23888890,"lines":[{"line":999000,"start":…,"start_char":…,"text":"…"},…]}` — 범위를 넘으면 잘려 나오고 `lines`가 `[]`일 수 있음. `text`에 줄바꿈은 없음 |
| `edit_char_to_line_col` | `{"cmd":"edit_char_to_line_col","doc":1,"char":12345}` | `{"ok":true,"line":3,"col":7,"pos":…}` (`pos`=바이트) |
| `edit_line_col_to_char` | `{"cmd":"edit_line_col_to_char","doc":1,"line":3,"col":7}` | `{"ok":true,"char":12345,"pos":…}` (범위 밖은 줄/문서 끝으로 맞춤) |
| `edit_to_char` / `edit_from_char` | `{"pos":…}` / `{"char":…}` | `{"char":…}` / `{"pos":…}` |
| 전체 줄·글자 수 | 기존 `edit_info`/`edit_status`의 `lines`,`chars` | O(1) |

권장 호출: 화면에는 보이는 줄 ± 여유만 `edit_line_range`로 가져오고, 스크롤바 길이는 `total_lines`로 정한다. 편집 응답(`edit_insert` 등)에도 `lines`,`chars`가 이미 들어 있다.

### 11.2 인코딩·줄바꿈 선택 저장, 변환 불가 글자
| 명령 | 요청 | 응답 |
|---|---|---|
| `edit_unencodable` | `{"cmd":"edit_unencodable","doc":1,"encoding":"CP949","limit":100}` (`limit` 기본 100) | `{"ok":true,"encoding":"CP949","ok_to_save":false,"total":4,"distinct_count":2,"distinct":[{"char":"😀","code":"U+1F600","count":3},…(최대 200종, 많은 순)],"first":[{"char":"😀","code":"U+1F600","pos":6,"char_index":4,"line":2,"col":2},…(앞쪽 limit개)]}` |
| `edit_save` | `{"cmd":"edit_save","doc":1,"path":"C:\\…\\a.txt","encoding":"UTF-8 BOM","line_ending":"CRLF"}` — `encoding`(UTF-8·UTF-8 BOM·UTF-16 LE·UTF-16 BE·CP949, 별칭 허용)·`line_ending`(LF·CRLF·CR) **둘 다 생략 가능**(생략하면 문서의 현재 설정) | 성공: `edit_info` 모양(저장 설정이 방금 고른 값으로 바뀌고 `dirty:false`). 실패: `{"ok":false,"message":"CP949로 저장할 수 없는 글자 4개(서로 다른 2종): '😀'(U+1F600) 2줄 2열, …"}` — **파일도 문서 설정도 그대로** |

UI 흐름: '다른 이름으로/인코딩 변경' 대화상자에서 CP949를 고르면 먼저 `edit_unencodable`을 부르고 `ok_to_save:false`면 `distinct`/`first`로 목록·'이동' 버튼(`line`,`col`)을 보여 준 뒤 UTF-8로 되돌리도록 안내한다. 몰래 `?`로 바꾸지 않는다. UTF-8/UTF-16은 `total`이 항상 0.

Rust 직접 호출: `Document::{line_range, char_count, byte_to_char, char_to_byte, char_to_line_col, line_col_to_char, unencodable, to_bytes_with, save_as}`.

### 11.3 구현 요점과 한계
- **줄 범위**: 줄 색인(`line_starts`)을 줄 번호로 직접 접근하므로 시작 줄과 무관, `count`와 읽은 글 크기에만 비례. 줄 번호 ↔ 바이트는 이분 탐색 O(log 줄 수).
- **글자 오프셋 변환**: 줄마다 '줄 시작 앞 글자 수' 캐시(`TextBuffer::cc`)를 두고 질의 때 앞에서부터 **필요한 만큼만** 채운다. 변환 = 캐시 이분 탐색 O(log 줄 수) + 한 줄 안 걷기 O(줄 길이). 한 줄이 아주 긴 글(예: 100만 글자 한 줄)은 줄 안 걷기가 O(줄 길이)다.
- **편집과 캐시**: 캐시된 마지막 줄보다 **뒤**에서의 편집(이어 입력, 끝에 덧붙이기)은 캐시를 건드리지 않는다(입력 경로 비용 변화 없음 — 아래 표). 그보다 **앞**에서 편집하면 그 줄 뒤쪽 캐시만 버리고, 다음 질의가 다시 채운다 → 가운데 편집 직후 첫 '끝 쪽' 질의가 O(뒤쪽 글 크기)(아래 4.7 ms). 즉 완전한 O(log n)은 **캐시가 채워진 읽기 위주 보기**에서 성립한다.
- `edit_save`의 `path`는 그대로 임시 파일 후 교체. CP949 점검은 저장 때 한 번 더 수행(변환 불가가 있으면 쓰기 전에 거절).

### 11.4 측정 (macOS arm64, `cargo test --release --test bigview perf_million_lines_range -- --nocapture`, 100만 줄·글 약 2,389만 자·줄당 약 24자)
| 항목 | 결과 |
|---|---:|
| 100만 줄 글 열기(LF 통일·줄 색인) | 39.8 ms |
| 줄 범위 50줄 조회, 시작 줄 흩뿌려 100회, 중앙값 | 2.5 µs (최대 889 µs = 글자 캐시를 처음 채울 때) |
| 글자 → (줄,열) → 글자 왕복 1000회, 중앙값 | 0.71 µs (최대 250 µs) |
| 가운데(50만 줄) 편집 직후 끝 글자 질의(뒤쪽 캐시 재계산) | 4.7 ms |
| 끝에 입력 직후 끝 글자 질의 | 0.8 µs |
| CP949 + CRLF 저장 바이트 생성(변환 불가 점검 포함) | 103.4 ms (25,888,892 바이트) |
| UTF-16 LE + LF 저장 바이트 생성 | 51.3 ms (37,777,786 바이트) |

입력 성능 회귀 없음(`perf_1m`, 같은 기기): 이어 입력 글자당 0.04~0.09 µs(전 0.06), 무작위 위치 입력 2000회 34~58 ms(전 54).

### 11.5 시험
`cargo test`(editor_core): 단위 1(줄 색인 무작위 모델 + 글자 오프셋 캐시 대조 추가) + `tests/bigview.rs` 6(줄 범위·글자↔줄/열 왕복, 무작위 편집 3000단계 캐시 정합, 인코딩·줄바꿈 선택 저장 바이트 대조, 변환 불가 목록·실패 시 불변, JSON 명령, 100만 줄 성능) + 기존 core 21·encoding 14·perf 3. 의존성 추가 없음.


## 12. 메모 화면 3단계 연결 (MEMO3, 2026-11-08, 브랜치 `feature/memo3`)

11절의 3단계 API를 klib·Python·Flutter 메모 편집기에 연결했다. 코어(editor_core)는 고치지 않았다. Cargo 의존성 추가 없음.

### 12.1 통로
- **klib**: `edit_` 접두 위임(9절) 그대로라 코드 변경 없음. 시험 `test_edit_stage3_large_file_and_encoding_save`(10만 줄 CP949 파일 열기 → 줄 범위 → 줄 편집 → 변환 불가 → CP949 거절·UTF-8 BOM+LF 저장, 120줄 응답이 64 KiB 안인지).
- **Python**: `ALLOWED_COMMANDS`에 `edit_line_range`·`edit_char_to_line_col`·`edit_line_col_to_char`·`edit_to_char`·`edit_from_char`·`edit_unencodable`·`edit_status`.
  화면이 파일 경로를 모르게 **호스트 파일 대화상자**를 거친다(`desktop_web_ui._memo_editor_request`):
  `edit_select_file {mode: open|save}` → 선택 번호(`selection_id`, 경로는 호스트 메모리에만, 5분·1회용) →
  `edit_open {selection_id}`(목적 `memo_text_open`: .txt .md .csv .log) / `edit_save {selection_id, encoding, line_ending}`(목적 `memo_text_save`: .txt .md)에서 호스트가 경로로 바꿔 klib에 넘긴다.
  `path` 인자는 여전히 거절하고, 코어 응답의 `path`는 지우고 파일 이름 `name`만 화면에 돌려준다. 열기용 선택으로 저장하거나 다른 명령에 선택 번호를 쓰면 거절.

### 12.2 화면 (`flutter_ui/lib/features/editor/`)
- `editor_files.dart`: `EditorFileSupport`(인코딩·줄바꿈·파일 이름·`unencodable`·`saveToFile`), `MemoFileGateway`(대화상자·열기), `UnencodableReport`.
- **열기(Ctrl+O, 도구줄 폴더 단추)**: 지금 내용이 있으면 확인 후 대화상자 → `edit_open {selection_id}`(인코딩·줄바꿈 자동 감지). 결과가
  128 KiB·3000줄 이하면 글을 받아(`fetchEditorText`, 응답 한도를 넘으면 줄 수를 반씩 줄임) **기존 입력칸**으로 편집하고, 연 문서를 그대로 이어 쓴다(`KlibEditorCore.adopt`). 그보다 크면 **큰 글 보기**.
- **큰 글 보기**(`large_text_session.dart` + `large_text_view.dart`): Rust 문서가 원본이고 글 전체를 Flutter로 가져오지 않는다.
  고정 높이 줄 목록(`ListView.builder` + `itemExtent`)이 보이는 줄만 만들고, 세션은 `edit_line_range`로 120줄씩 받아 캐시한다(2만 줄 넘으면 먼 줄부터 버림).
  **편집 중인 한 줄(문단)만 `TextField`**라 IME 조합은 기존대로 그 입력칸이 처리하고(조합 중에는 아래 키를 가로채지 않음), 줄을 옮기거나 포커스를 잃으면 그 줄을 `edit_commit`+`edit_replace`로 확정한다.
  Enter=줄 나누기, 줄 맨 앞 Backspace=앞 줄과 합치기, 줄 끝 Delete=다음 줄과 합치기, ↑/↓=위아래 줄, 줄바꿈이 든 붙여넣기(Ctrl+V)는 한 번의 replace.
  편집 뒤 캐시는 같은 규칙으로 고쳐(뒤쪽 줄 번호 +k·시작 바이트 +delta) 다시 받지 않고, 편집 전에 보낸 줄 요청의 응답은 버전 번호로 버린다. 실행취소·다시실행·모두 바꾸기는 캐시를 비우고 보이는 줄만 다시 받는다.
  찾기는 `edit_find`(거꾸로 찾기·개수는 응답 크기 때문에 `edit_find_all` 1000건까지), 바꾸기는 '모두 바꾸기'만. 서식 단추는 끈다(평문). 자동 줄 바꿈 없음(긴 줄은 잘려 보임, 메모장 '자동 줄 바꿈' 끔과 같음).
  응답 한도(64 KiB)를 넘는 한 줄은 '너무 긴 줄'로 표시만 하고 편집하지 않는다. 큰 파일 보기 중에는 메모 저장 단추(노트북·자료함)가 안내만 하고 저장하지 않는다(파일은 '다른 이름으로 저장').
- **다른 이름으로 저장(Ctrl+Shift+S, `memo_save_dialog.dart`)**: 인코딩 UTF-8·UTF-8 BOM·UTF-16 LE·CP949, 줄바꿈 CRLF·LF(기본값 = 지금 형식).
  CP949면 먼저 `edit_unencodable`로 점검해, 바꿀 수 없는 글자가 있으면 '?'로 바꾸지 않고 **글자 종류별 개수와 처음 나오는 자리(줄·열, '이동')** 를 보여 주고 'UTF-8로 저장' 또는 '돌아가기'를 고르게 한다. 통과하면 저장 위치 대화상자 → `edit_save`.
- **상태 표시줄**: 인코딩·줄바꿈은 코어가 알려 준 값(연 파일에서 감지한 형식, 저장 뒤에는 저장한 형식)이다. 큰 글 보기에서는 '큰 파일 보기 · N줄', 줄·열은 편집 줄의 커서, 글자 수는 코어 값. 왼쪽에 열기·저장 결과 안내.

### 12.3 10만 줄 파일 열기 시간 (Windows 11, release)
파일: 100,000줄 한글·영문 혼합 CRLF(CP949 5,688,888 B / UTF-8 7,388,888 B).

| 경로 | 항목 | 결과 |
|---|---|---:|
| klib 프로세스 안(`handle_request`, `cargo test --release`) | 열기(CP949 감지·LF 통일·줄 색인) | 79.8 ms |
| | 첫 화면 120줄 / 끝 화면 120줄 | 0.36 ms / 7.19 ms |
| 실제 파이프(Python `LibraryBridge` → 격리 시험 klib, 5회) | 열기 CP949 중앙값 | 67.6 ms (63.2~123.7) |
| | 열기 UTF-8 중앙값 | 72.9 ms (59.1~82.7) |
| | 첫 화면 120줄 / 끝 화면 | 약 21 ms / 26~28 ms |
| | CP949 변환 불가 점검(10만 줄) | 16.3 ms(CP949 원본) / 38.0 ms(UTF-8 원본) |
| | UTF-8 BOM + LF 저장(7.3 MB) | 46~47 ms |
| Flutter 위젯 시험(가짜 백엔드, 참고값) | 열기 → 첫 화면 그리기 | 약 0.5~0.6 s(가짜가 요청마다 글 전체를 다시 나눠 느림) |

열기 = 파일 선택 뒤 `edit_open`부터 첫 화면 줄 120개가 오기까지 약 **90~100 ms**(파이프 포함). 입력칸에 글 전체를 넣지 않으므로 10만 줄이어도 화면에 만드는 위젯은 보이는 줄 수만큼이다.

**파이프 지연 주의(기존 구조, 이번에 고치지 않음)**: 같은 요청을 반복하면 대부분 0.2~0.5 ms인데 가끔 **20 ms**(서버 대기 주기)·**63 ms**(파이프 인스턴스가 아직 없을 때 Python 클라이언트가 50 ms 쉬고 다시 붙는 재시도 `retry_delay`)가 끼어든다.
줄 범위·`edit_info`처럼 응답이 큰 요청은 거의 매번 약 20 ms다. 큰 글 보기의 줄 확정(commit+replace 2회 왕복)이 0.5~90 ms로 들쭉날쭉한 원인이며, 파이프 서버가 다음 인스턴스를 미리 만들어 두거나(여러 인스턴스 대기) 클라이언트 재시도 간격을 줄이면 개선된다(klib·브리지 공용 경로라 별도 카드 권장).

### 12.4 시험
- cargo: klib `test_edit_stage3_large_file_and_encoding_save` 추가(기존 `test_edit_commands_delegate_to_editor_core` 포함 전체 통과), editor_core 전체.
- Python: `tests/test_library_bridge.py::MemoEditorFileBridgeTests` 4건(3단계 명령 허용·`edit_select_file`은 klib로 가지 않음, 목적별 확장자, 선택 번호로 열기·저장 시 호스트 경로만 전달·응답에서 path 제거·1회용, 잘못된 모드·목적·명령·확장자 거절).
- Flutter: `test/memo_editor_file_test.dart` 6건(작은 CP949/CRLF 파일 열기 → 상태 표시줄·제목 → 입력 반영 → UTF-8 BOM+LF 저장 후 상태 표시줄 갱신, CP949 변환 불가 목록·이동·돌아가기·UTF-8로 저장, 취소·Dart 폴백 시 단추 숨김,
  **10만 줄 큰 글 보기**(줄 범위 요청만·글 전체 명령 없음·끝 스크롤·줄 편집·Enter 나누기·Backspace 합치기·뒤쪽 캐시 이동·실행취소·먼 줄 찾기·CP949 저장), 응답 한도 초과 시 줄 요청 반씩 나누기·너무 긴 줄 편집 거절, 보고서 해석). 가짜 백엔드(`test/fake_editor_backend.dart`)에 3단계 명령 추가.

### 12.5 남은 일
1. Windows 실기에서 큰 글 보기의 한글 IME 조합(편집 줄 TextField)·마우스 끌어 여러 줄 선택(지금은 한 줄 안에서만 선택) 확인.
2. 큰 글 보기의 여러 줄 선택·복사·삭제, 바꾸기(한 건씩), 자동 줄 바꿈, 가로 스크롤 — 지금은 한 줄 편집 모델.
3. 큰 파일의 자동 저장·복구 사본(`edit_autosave`): klib가 유휴 종료(기본 60초, 30초 깨우기로 막음)되면 저장하지 않은 큰 파일 편집은 사라진다(화면에 안내).
4. 위 12.3의 파이프 지연(20/63 ms).
5. 같은 파일 '저장'(덮어쓰기): 지금은 '다른 이름으로 저장'만(대화상자에서 같은 파일을 고르면 덮어쓰기 확인 후 저장).
