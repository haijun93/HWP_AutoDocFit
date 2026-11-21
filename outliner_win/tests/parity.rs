//! Flutter 화면 대비 "차이 0" 점검 하네스(OUTLINERWIN5).
//!
//! 같은 시작 문서와 같은 편집 입력 순서를 두 경로로 적용해 결과 Markdown(과 화면에 보이는 줄)을 전수 대조한다.
//!  (a) **Flutter 모델**: `outliner_workspace.dart` 가 키마다 보내는 엔진 요청 순서를 그대로 흉내 낸다.
//!      요청 형태는 `outliner_service.py` 규칙(`op='edit'` + `args.action`)이고, 엔진(`outliner_core::handle_request`)을
//!      직접 호출한다. Flutter 는 요청마다 `_markdown` 을 다시 파싱하므로(블록 번호 재부여·접힘 상태 소실) 같은 방식으로 흉내 낸다.
//!  (b) **네이티브**: `outliner_win::edit::Editor`(가짜 EDIT)에 실제 키 이벤트를 넣는다.
//!
//! 결과가 같아야 하는 시나리오(`Expect::Same`)와, 알려진 **허용 차이**(`Expect::Diff`)를 분리한다. 허용 차이는
//! 양쪽의 정확한 결과를 고정해 두므로, 차이가 사라지거나 다른 모양으로 바뀌면 시험이 실패한다(허용 목록이 낡지 않게).
//! 허용 차이 사유는 `docs/OUTLINERWIN5_RESULT_2026-11-21.md` 의 표와 `ALLOWED` 의 번호가 같다.
mod common;
use common::Fake;
use outliner_core::Document;
use outliner_win::edit::*;
use outliner_win::view::View;
use serde_json::{Value, json};

// ---------------------------------------------------------------- 입력 순서

#[derive(Clone, Copy, Debug)]
enum Op {
    /// Enter. `caret`: 글자(char) 단위 캐럿 위치(None = 글 끝).
    Enter(Option<usize>),
    Tab,
    ShiftTab,
    /// 캐럿이 맨 앞(또는 빈 칸)일 때 Backspace.
    Backspace,
    AltUp,
    AltDown,
    /// 상태 전환(Flutter ☑ 단추 / 네이티브 Ctrl+Enter).
    Toggle,
    Fold,
    Unfold,
    /// 머리글 `＋ 새 블록` 단추(마지막 블록 뒤에 빈 블록). `block` 은 쓰지 않는다.
    Append,
}

#[derive(Clone, Copy, Debug)]
struct Step {
    /// 문서 안 블록 순서(0부터). 시험 설계상 항상 화면에 보이는 블록만 가리킨다.
    block: usize,
    op: Op,
    /// 명령 순간 입력 칸에 들어 있는 글(None = 문서 글 그대로).
    typed: Option<&'static str>,
}

fn s(block: usize, op: Op) -> Step {
    Step { block, op, typed: None }
}
fn st(block: usize, op: Op, typed: &'static str) -> Step {
    Step { block, op, typed: Some(typed) }
}

// ---------------------------------------------------------------- (a) Flutter 모델

struct Flutter {
    md: String,
    doc: Document,
}

/// `outliner_service.py` 의 `edit` 변환 그대로: `{'op':'edit','document':md,'args':{'action':…,'block_id':…,'text':…}}`.
fn engine_edit(md: &str, action: &str, block_id: u64, text: Option<&str>) -> Result<Document, String> {
    let mut args = json!({"action": action, "block_id": block_id});
    if let Some(t) = text {
        args["text"] = Value::String(t.into());
    }
    let resp = outliner_core::handle_request(json!({"id": "edit", "op": "edit", "document": md, "args": args}));
    if resp["ok"].as_bool() != Some(true) {
        return Err(resp["error"]["message"].as_str().unwrap_or("error").to_string());
    }
    let md = resp["result"]["markdown"].as_str().ok_or("no markdown")?.to_string();
    let mut doc: Document = serde_json::from_value(resp["result"]["document"].clone()).map_err(|e| e.to_string())?;
    // 응답 document 는 이 편집 직후 상태(접기 표시 포함). markdown 은 호출 쪽이 따로 들고 있는다.
    doc.original = md;
    Ok(doc)
}

impl Flutter {
    fn load(md: &str) -> Flutter {
        let resp = outliner_core::handle_request(json!({"id": "parse", "op": "parse", "document": md, "args": {}}));
        let doc: Document = serde_json::from_value(resp["result"].clone()).unwrap();
        Flutter { md: md.to_string(), doc }
    }

    fn markdown(&self) -> String {
        self.md.clone()
    }

    /// `_visibleBlocks` (outliner_workspace.dart:62-73) 그대로.
    fn visible(&self) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        let mut folded_at: i64 = -1;
        for b in &self.doc.blocks {
            let indent = b.indent as i64;
            if folded_at >= 0 && indent > folded_at {
                continue;
            }
            folded_at = -1;
            out.push((b.text.clone(), b.indent));
            if b.folded {
                folded_at = indent;
            }
        }
        out
    }

    fn adopt(&mut self, doc: Document) {
        self.md = doc.original.clone();
        self.doc = doc;
    }

    /// 한 키 입력 = Flutter 의 한 핸들러. 오류면 상태는 그대로(`_run` 이 `_markdown` 갱신 전에 예외를 잡는다).
    fn apply(&mut self, st: &Step) -> Result<(), String> {
        if matches!(st.op, Op::Append) {
            // `_edit(_blocks.last, 'insert', '')`: 입력 칸 글은 보내지 않는다.
            let last = self.doc.blocks.last().ok_or("블록 없음")?.id;
            let d = engine_edit(&self.md, "insert", last, Some(""))?;
            self.adopt(d);
            return Ok(());
        }
        let block = self.doc.blocks.get(st.block).ok_or("블록 없음")?.clone();
        let id = block.id;
        let value = st.typed.map(str::to_string).unwrap_or_else(|| block.text.clone());
        match st.op {
            // _commitAndInsert: set_text(전체 글) → insert('') (커서 위치와 무관)
            Op::Enter(_) => {
                let d = engine_edit(&self.md, "set_text", id, Some(&value))?;
                let d = engine_edit(&d.original, "insert", id, Some(""))?;
                self.adopt(d);
            }
            // _edit(block, 'indent'|'outdent'): 입력 칸 글은 보내지 않는다.
            Op::Tab => {
                let d = engine_edit(&self.md, "indent", id, None)?;
                self.adopt(d);
            }
            Op::ShiftTab => {
                let d = engine_edit(&self.md, "outdent", id, None)?;
                self.adopt(d);
            }
            Op::Append => unreachable!("위에서 처리"),
            Op::Fold | Op::Unfold => {
                let d = engine_edit(&self.md, if matches!(st.op, Op::Fold) { "fold" } else { "unfold" }, id, None)?;
                self.adopt(d);
            }
            // Backspace: 빈 칸 → _setTextAndEdit(delete) / 맨 앞 → 앞 블록에 글을 이어붙이고 지움(공백 없음)
            Op::Backspace => {
                if value.is_empty() {
                    let d = engine_edit(&self.md, "set_text", id, Some(&value))?;
                    let d = engine_edit(&d.original, "delete", id, None)?;
                    self.adopt(d);
                } else if st.block > 0 {
                    let prev = self.doc.blocks[st.block - 1].clone();
                    let joined = format!("{}{}", prev.text, value);
                    let d = engine_edit(&self.md, "set_text", prev.id, Some(&joined))?;
                    let d = engine_edit(&d.original, "delete", id, None)?;
                    self.adopt(d);
                }
            }
            // _setTextAndEdit(up|down|toggle_status, 입력 칸 글)
            Op::AltUp | Op::AltDown | Op::Toggle => {
                let action = match st.op {
                    Op::AltUp => "up",
                    Op::AltDown => "down",
                    _ => "toggle_status",
                };
                let d = engine_edit(&self.md, "set_text", id, Some(&value))?;
                let d = engine_edit(&d.original, action, id, None)?;
                self.adopt(d);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- (b) 네이티브

struct Native {
    ed: Editor<Fake>,
    t: u64,
}

impl Native {
    fn load(md: &str) -> Native {
        let view = View::from_markdown(md).unwrap();
        let base = outliner_core::serialize(&view.doc);
        Native { ed: Editor::new(view, Fake::default(), None, base), t: 1000 }
    }

    fn markdown(&self) -> String {
        self.ed.markdown()
    }

    fn visible(&self) -> Vec<(String, usize)> {
        self.ed.view.rows.iter().map(|r| (self.ed.view.doc.blocks[r.block].text.clone(), self.ed.view.doc.blocks[r.block].indent)).collect()
    }

    fn apply(&mut self, st: &Step) -> Result<(), String> {
        self.t += 1000; // 중복 제거 창(50 ms) 밖
        if matches!(st.op, Op::Append) {
            let before = self.markdown();
            let ok = self.ed.add_block_at_end(10);
            self.ed.finish_edit();
            return if ok || self.markdown() != before { Ok(()) } else { Err("추가 실패".into()) };
        }
        let row = self.ed.view.row_of_block(st.block).ok_or("블록이 화면에 없음")?;
        let before = self.markdown();
        self.ed.message = None;
        if matches!(st.op, Op::Fold | Op::Unfold) {
            self.ed.view.set_selected(row, 10);
            // 하위가 없는 블록의 접기는 네이티브에서 아무 일도 하지 않는다(Flutter 는 표시만 켜지고 보이는 변화 없음).
            self.ed.view.set_fold(matches!(st.op, Op::Fold), 10);
            return Ok(());
        }
        self.ed.view.set_selected(row, 10);
        assert!(self.ed.start_edit_selected());
        let text = st.typed.map(str::to_string).unwrap_or_else(|| self.ed.view.doc.blocks[st.block].text.clone());
        self.ed.host.type_text(&text);
        let caret16 = match st.op {
            Op::Enter(Some(c)) => text.chars().take(c).map(|ch| ch.len_utf16()).sum(),
            Op::Backspace => 0,
            _ => text.encode_utf16().count(),
        };
        self.ed.host.sel = (caret16, caret16);
        let mut ev = KeyEvent::new(
            match st.op {
                Op::Enter(_) | Op::Toggle => VK_RETURN,
                Op::Tab | Op::ShiftTab => VK_TAB,
                Op::Backspace => VK_BACK,
                Op::AltUp => VK_UP,
                Op::AltDown => VK_DOWN,
                Op::Fold | Op::Unfold | Op::Append => unreachable!(),
            },
            self.t,
        );
        ev.shift = matches!(st.op, Op::ShiftTab);
        ev.ctrl = matches!(st.op, Op::Toggle);
        ev.alt = matches!(st.op, Op::AltUp | Op::AltDown);
        self.ed.key(ev);
        let msg = self.ed.message.take();
        self.ed.finish_edit(); // 입력 중 글을 문서에 확정(Flutter 는 다음 명령 때 보냄 — 허용 차이 D3)
        // 메시지는 "저장했습니다" 류 안내가 아니라 오류만 본다.
        match msg {
            Some(m) if self.markdown() == before => Err(m),
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------- 시나리오

enum Expect {
    /// 모든 단계에서 결과 Markdown·보이는 줄이 같아야 한다.
    Same,
    /// 허용 차이: 번호(ALLOWED)와, 양쪽 최종 결과를 고정한다.
    Diff { id: &'static str, flutter: &'static str, native: &'static str },
}

struct Scenario {
    name: &'static str,
    md: &'static str,
    steps: Vec<Step>,
    expect: Expect,
}

/// 허용 차이 등록부(번호 = 결과 문서의 표). 사유·승인 근거 포함.
const ALLOWED: &[(&str, &str)] = &[
    ("D1", "Enter 커서 분할: Flutter 는 항상 전체 글을 저장하고 빈 블록을 아래에 만든다. 네이티브는 커서 위치에서 쪼갠다(설계 4절 6, 사용자 확정 기능 확장)."),
    ("D2", "Backspace 병합 공백: Flutter 는 앞 글+현재 글을 공백 없이 잇는다. 네이티브는 엔진 merge 규칙으로 두 글이 모두 있으면 공백 1칸(설계 4절 7)."),
    ("D3", "Tab·Shift+Tab 시 입력 중 글: Flutter 는 글을 보내지 않아 파일에 반영되지 않는다. 네이티브는 글을 먼저 확정한다(설계 3.4)."),
    ("D4", "접힘 유지: Flutter 는 다음 편집 요청에서 접힘 표시가 사라진다(요청마다 _markdown 재파싱). 네이티브는 세션 동안 유지한다."),
    ("D5", "Alt+↑/↓: Flutter(엔진 up/down)는 한 줄만 교환해 부모·자식 관계가 어긋난다. 네이티브는 하위 블록을 함께 같은 단계 형제 건너편으로 옮기고, 형제가 없으면 거절한다(OUTLINERWIN4 사용자 확정)."),
    ("D6", "마지막 한 블록의 Backspace 삭제: Flutter 는 블록 0개 상태가 되어 새 블록을 못 만든다(설계 1.4-6). 네이티브는 거절한다."),
    ("D7", "`collapsed:: true` 속성: 네이티브는 접힌 상태로 시작한다(Logseq 관례). Flutter 는 속성을 무시한다."),
    ("D8", "접힌 하위 건너 Backspace 병합: Flutter 는 숨은 마지막 하위 블록에 이어붙인다(결함). 네이티브는 거절한다."),
    ("D9", "입력 칸 안의 탭 문자: 네이티브는 한 줄 규칙(sanitize)으로 공백으로 바꾼다. Flutter 는 그대로 보낸다."),
];

fn scenarios() -> Vec<Scenario> {
    use Expect::*;
    use Op::*;
    let long_ko: &'static str = Box::leak("가나다라마바사아자차카타파하".repeat(40).into_boxed_str()); // 560자
    let long_md: &'static str = Box::leak(format!("- {long_ko}\n- 끝\n").into_boxed_str());
    let tree = "- 가\n - 나\n  - 다\n   - 라\n- 마\n";
    vec![
        // ---- Enter ----
        Scenario { name: "빈 문서(- ) 입력 후 Enter", md: "- \n", steps: vec![st(0, Enter(None), "첫 줄")], expect: Same },
        Scenario { name: "빈 문서 Enter 만(빈 블록 추가)", md: "- \n", steps: vec![s(0, Enter(None))], expect: Same },
        Scenario { name: "영문 줄 끝 Enter", md: "- hello\n- world\n", steps: vec![s(0, Enter(None))], expect: Same },
        Scenario { name: "한글 줄 끝 Enter", md: "- 안녕하세요\n", steps: vec![s(0, Enter(None))], expect: Same },
        Scenario { name: "긴 줄(560자) 끝 Enter", md: long_md, steps: vec![s(0, Enter(None))], expect: Same },
        Scenario { name: "긴 줄 수정 후 Enter(입력 중 글 저장)", md: long_md, steps: vec![st(0, Enter(None), "짧게 고침")], expect: Same },
        Scenario { name: "들여쓴 블록 Enter(같은 단계 유지)", md: tree, steps: vec![s(2, Enter(None))], expect: Same },
        Scenario { name: "TODO 블록 Enter(새 블록은 상태 없음)", md: "- TODO 할 일\n- 다음\n", steps: vec![s(0, Enter(None))], expect: Same },
        Scenario { name: "DONE 블록 Enter", md: "- DONE 끝낸 일\n", steps: vec![s(0, Enter(None))], expect: Same },
        Scenario {
            name: "Enter 3연타(끝에서 계속 입력)",
            md: "- 시작\n",
            steps: vec![st(0, Enter(None), "가"), st(1, Enter(None), "나"), st(2, Enter(None), "다")],
            expect: Same,
        },
        Scenario { name: "링크·태그·속성이 든 글 Enter", md: "- x\n", steps: vec![st(0, Enter(None), "[[회의록]] 참고 #중요 key:: 값")], expect: Same },
        Scenario { name: "이모지·결합 문자 글 Enter", md: "- a\n", steps: vec![st(0, Enter(None), "😀 한글 e\u{301}")], expect: Same },
        Scenario { name: "CRLF 문서 Enter", md: "- a\r\n- b\r\n", steps: vec![s(0, Enter(None))], expect: Same },
        Scenario { name: "끝 줄바꿈 없는 문서 Enter", md: "- a\n- b", steps: vec![s(1, Enter(None))], expect: Same },
        Scenario { name: "보존 줄(제목) 낀 문서 Enter", md: "# 제목\n- a\n\n- b\n", steps: vec![s(0, Enter(None))], expect: Same },
        Scenario { name: "접힌 부모 아래에서 마지막 블록 Enter", md: "- 가\n - 나\n- 다\n", steps: vec![s(2, Enter(None))], expect: Same },
        // ---- Tab / Shift+Tab ----
        Scenario { name: "둘째 블록 Tab(자식이 됨)", md: "- a\n- b\n", steps: vec![s(1, Tab)], expect: Same },
        Scenario { name: "첫 블록 Tab(오류)", md: "- a\n- b\n", steps: vec![s(0, Tab)], expect: Same },
        Scenario { name: "최상위 Shift+Tab(오류)", md: "- a\n- b\n", steps: vec![s(1, ShiftTab)], expect: Same },
        Scenario { name: "Tab 두 번(깊이 2)", md: "- a\n- b\n- c\n", steps: vec![s(1, Tab), s(2, Tab), s(2, Tab)], expect: Same },
        Scenario { name: "Tab 후 Shift+Tab 되돌리기", md: "- a\n- b\n", steps: vec![s(1, Tab), s(1, ShiftTab)], expect: Same },
        Scenario { name: "깊이 4 트리 중간 Shift+Tab", md: tree, steps: vec![s(2, ShiftTab)], expect: Same },
        Scenario { name: "한글 문서 들여쓰기 경계(첫·끝 블록)", md: "- 가\n- 나\n- 다\n", steps: vec![s(0, Tab), s(2, Tab), s(2, ShiftTab)], expect: Same },
        // ---- Backspace ----
        Scenario { name: "가운데 빈 블록 Backspace(삭제)", md: "- a\n- \n- c\n", steps: vec![s(1, Backspace)], expect: Same },
        Scenario { name: "첫 빈 블록 Backspace(삭제)", md: "- \n- b\n", steps: vec![s(0, Backspace)], expect: Same },
        Scenario { name: "앞 블록이 빈 칸일 때 병합(공백 차이 없음)", md: "- \n- 뒤\n", steps: vec![s(1, Backspace)], expect: Same },
        Scenario { name: "첫 블록 맨 앞 Backspace(아무 일 없음)", md: "- a\n- b\n", steps: vec![s(0, Backspace)], expect: Same },
        Scenario { name: "빈 블록 두 번 Backspace", md: "- a\n- \n- \n", steps: vec![s(2, Backspace), s(1, Backspace)], expect: Same },
        // ---- 상태(TODO) ----
        Scenario { name: "TODO 순환(없음→TODO→DONE→TODO)", md: "- 일\n", steps: vec![s(0, Toggle), s(0, Toggle), s(0, Toggle)], expect: Same },
        Scenario { name: "입력 중 글 + 토글", md: "- 일\n- 이\n", steps: vec![st(1, Toggle, "이 수정")], expect: Same },
        // ---- Alt+↑/↓ (한 줄짜리 형제만: 두 방식이 같다) ----
        Scenario { name: "Alt+↓ 평평한 목록", md: "- a\n- b\n- c\n", steps: vec![s(0, AltDown)], expect: Same },
        Scenario { name: "Alt+↑ 평평한 목록", md: "- a\n- b\n- c\n", steps: vec![st(2, AltUp, "c 수정")], expect: Same },
        Scenario { name: "첫 블록 Alt+↑(오류)", md: "- a\n- b\n", steps: vec![s(0, AltUp)], expect: Same },
        Scenario { name: "마지막 블록 Alt+↓(오류)", md: "- a\n- b\n", steps: vec![s(1, AltDown)], expect: Same },
        Scenario { name: "제목(보존 줄) 위로 Alt+↑(오류)", md: "# 제목\n- a\n- b\n", steps: vec![s(0, AltUp)], expect: Same },
        // ---- 접기(화면에 보이는 줄까지 대조) ----
        Scenario { name: "접기 → 하위 숨김", md: "- 가\n - 나\n  - 다\n- 라\n", steps: vec![s(0, Fold)], expect: Same },
        Scenario { name: "접기 → 펼치기", md: "- 가\n - 나\n- 라\n", steps: vec![s(0, Fold), s(0, Unfold)], expect: Same },
        Scenario { name: "하위 없는 블록 접기(보이는 변화 없음)", md: "- 가\n- 나\n", steps: vec![s(1, Fold)], expect: Same },
        Scenario { name: "마지막 부모만 접기", md: "- 가\n - 나\n- 다\n - 라\n", steps: vec![s(2, Fold)], expect: Same },
        // ---- 새 블록 단추(OUTLINERWIN5 추가 기능) ----
        Scenario { name: "새 블록 단추: 단순 문서 끝에 추가", md: "- a\n- b\n", steps: vec![s(0, Append)], expect: Same },
        Scenario { name: "새 블록 단추: 들여쓴 마지막 블록 뒤(같은 단계)", md: tree, steps: vec![s(0, Append), s(1, Tab), s(2, Append)], expect: Same },
        Scenario { name: "새 블록 단추: 한글·TODO 문서", md: "- TODO 할 일\n- 안녕\n", steps: vec![s(0, Append), s(0, Append)], expect: Same },
        // ---- 허용 차이 ----
        Scenario {
            name: "[D1] 한가운데 Enter(커서 분할)",
            md: "- 안녕하세요\n",
            steps: vec![s(0, Enter(Some(2)))],
            expect: Diff { id: "D1", flutter: "- 안녕하세요\n- \n", native: "- 안녕\n- 하세요\n" },
        },
        Scenario {
            name: "[D1] 맨 앞 Enter",
            md: "- 글\n",
            steps: vec![s(0, Enter(Some(0)))],
            expect: Diff { id: "D1", flutter: "- 글\n- \n", native: "- \n- 글\n" },
        },
        Scenario {
            name: "[D2] 두 글이 있는 블록 Backspace 병합",
            md: "- 안녕\n- 하세요\n",
            steps: vec![s(1, Backspace)],
            expect: Diff { id: "D2", flutter: "- 안녕하세요\n", native: "- 안녕 하세요\n" },
        },
        Scenario {
            name: "[D3] Tab 할 때 입력 중 글",
            md: "- a\n- b\n",
            steps: vec![st(1, Tab, "b 고침")],
            expect: Diff { id: "D3", flutter: "- a\n - b\n", native: "- a\n - b 고침\n" },
        },
        Scenario {
            name: "[D3] Shift+Tab 할 때 입력 중 글",
            md: "- a\n - b\n",
            steps: vec![st(1, ShiftTab, "b 고침")],
            expect: Diff { id: "D3", flutter: "- a\n- b\n", native: "- a\n- b 고침\n" },
        },
        Scenario {
            name: "[D4] 접은 뒤 토글하면 Flutter 는 접힘을 잃음",
            md: "- 가\n - 나\n- 다\n",
            steps: vec![s(0, Fold), s(0, Toggle)],
            // 마크다운은 같다. 차이는 보이는 줄(아래 `visible_note`)에서 잡는다.
            expect: Diff { id: "D4", flutter: "- TODO 가\n - 나\n- 다\n", native: "- TODO 가\n - 나\n- 다\n" },
        },
        Scenario {
            name: "[D5] 하위가 있는 블록 Alt+↓",
            md: "- a\n- b\n - b1\n- c\n",
            steps: vec![s(1, AltDown)],
            expect: Diff { id: "D5", flutter: "- a\n - b1\n- b\n- c\n", native: "- a\n- c\n- b\n - b1\n" },
        },
        Scenario {
            name: "[D5] 첫 자식 Alt+↑(부모 밖으로 새어 나감)",
            md: "- a\n - a1\n- b\n - b1\n",
            steps: vec![s(3, AltUp)],
            expect: Diff { id: "D5", flutter: "- a\n - a1\n - b1\n- b\n", native: "- a\n - a1\n- b\n - b1\n" },
        },
        Scenario {
            name: "[D6] 마지막 한 블록(빈 칸) Backspace",
            md: "- \n",
            steps: vec![s(0, Backspace)],
            expect: Diff { id: "D6", flutter: "", native: "- \n" },
        },
        Scenario {
            name: "[D7] collapsed:: true 로 시작",
            md: "- 가 collapsed:: true\n - 나\n- 다\n",
            steps: vec![],
            expect: Diff { id: "D7", flutter: "- 가 collapsed:: true\n - 나\n- 다\n", native: "- 가 collapsed:: true\n - 나\n- 다\n" },
        },
        Scenario {
            name: "[D8] 접힌 하위 건너 Backspace 병합",
            md: "- a\n - a1\n- b\n",
            steps: vec![s(0, Fold), s(2, Backspace)],
            expect: Diff { id: "D8", flutter: "- a\n - a1b\n", native: "- a\n - a1\n- b\n" },
        },
        Scenario {
            name: "[D9] 입력 칸의 탭 문자 Enter",
            md: "- a\n",
            steps: vec![st(0, Enter(None), "가\t나")],
            expect: Diff { id: "D9", flutter: "- 가\t나\n- \n", native: "- 가 나\n- \n" },
        },
    ]
}

// ---------------------------------------------------------------- 실행·대조

struct Trace {
    flutter_md: String,
    native_md: String,
    flutter_vis: Vec<(String, usize)>,
    native_vis: Vec<(String, usize)>,
    /// 단계별 (Flutter 성공?, 네이티브 성공?).
    oks: Vec<(bool, bool)>,
    /// 처음 갈라진 단계 번호(없으면 None).
    first_divergence: Option<usize>,
}

fn run(sc: &Scenario) -> Trace {
    let mut f = Flutter::load(sc.md);
    let mut n = Native::load(sc.md);
    let mut oks = Vec::new();
    let mut first = None;
    if f.markdown() != n.markdown() || f.visible() != n.visible() {
        first = Some(0);
    }
    for (i, step) in sc.steps.iter().enumerate() {
        let fo = f.apply(step).is_ok();
        let no = n.apply(step).is_ok();
        oks.push((fo, no));
        if first.is_none() && (f.markdown() != n.markdown() || f.visible() != n.visible()) {
            first = Some(i + 1);
        }
    }
    Trace { flutter_md: f.markdown(), native_md: n.markdown(), flutter_vis: f.visible(), native_vis: n.visible(), oks, first_divergence: first }
}

fn describe(sc: &Scenario, t: &Trace) -> String {
    format!(
        "[{}]\n 시작: {:?}\n Flutter: {:?} / 보이는 줄 {:?}\n 네이티브: {:?} / 보이는 줄 {:?}\n 단계 성공(F,N): {:?}",
        sc.name, sc.md, t.flutter_md, t.flutter_vis, t.native_md, t.native_vis, t.oks
    )
}

#[test]
fn scenario_count_is_at_least_30_and_names_unique() {
    let all = scenarios();
    assert!(all.len() >= 30, "시나리오 {}개", all.len());
    let mut names: Vec<_> = all.iter().map(|s| s.name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), all.len(), "이름 중복");
}

#[test]
fn same_scenarios_have_zero_differences() {
    let mut bad = Vec::new();
    let mut count = 0;
    for sc in scenarios() {
        if !matches!(sc.expect, Expect::Same) {
            continue;
        }
        count += 1;
        let t = run(&sc);
        if t.first_divergence.is_some() {
            bad.push(describe(&sc, &t));
        }
    }
    assert!(count >= 30, "차이 0 시나리오 {count}개");
    assert!(bad.is_empty(), "차이 0 이어야 하는데 갈라진 시나리오 {}개:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn allowed_differences_are_pinned_and_registered() {
    let mut bad = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    for sc in scenarios() {
        let Expect::Diff { id, flutter, native } = sc.expect else { continue };
        assert!(ALLOWED.iter().any(|(k, _)| *k == id), "{}: 등록 없는 허용 차이 {id}", sc.name);
        used.insert(id);
        let t = run(&sc);
        if t.first_divergence.is_none() {
            bad.push(format!("차이가 사라짐(허용 목록에서 빼야 함): {}", describe(&sc, &t)));
        } else if t.flutter_md != flutter || t.native_md != native {
            bad.push(format!("고정한 결과와 다름: {}\n 고정값 Flutter {flutter:?} / 네이티브 {native:?}", describe(&sc, &t)));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    for (id, _) in ALLOWED {
        assert!(used.contains(id), "허용 차이 {id} 를 쓰는 시나리오가 없다(낡은 항목)");
    }
}

/// D4·D7 은 Markdown 이 같고 "화면에 보이는 줄"이 다르다. 그 모양을 따로 고정한다.
#[test]
fn folding_differences_show_in_visible_rows_only() {
    let all = scenarios();
    let d4 = all.iter().find(|s| s.name.starts_with("[D4]")).unwrap();
    let t = run(d4);
    assert_eq!(t.flutter_md, t.native_md);
    assert_eq!(t.flutter_vis.len(), 3, "Flutter: 토글 뒤 접힘이 풀려 3줄: {:?}", t.flutter_vis);
    assert_eq!(t.native_vis.len(), 2, "네이티브: 접힘 유지 2줄: {:?}", t.native_vis);
    let d7 = all.iter().find(|s| s.name.starts_with("[D7]")).unwrap();
    let t = run(d7);
    assert_eq!(t.flutter_vis.len(), 3);
    assert_eq!(t.native_vis.len(), 2);
}

#[test]
fn allowed_table_has_reasons() {
    for (id, why) in ALLOWED {
        assert!(id.starts_with('D') && why.chars().count() > 20, "{id}");
    }
}
