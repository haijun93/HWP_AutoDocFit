//! 두 화면 "차이 0" 점검(OUTLINEKEY1).
//!
//! 같은 시작 문서와 같은 편집 입력 순서를 두 경로로 적용해 결과 Markdown(과 화면에 보이는 줄)을 전수 대조한다.
//!  (a) **Flutter**: `outliner_workspace.dart` 가 보내는 엔진 요청 `op='key'` 를 그대로 흉내 낸다
//!      (마크다운·블록 번호·커서·칸의 글·접힘 번호). 엔진(`outliner_core::handle_request`)을 직접 호출한다.
//!  (b) **네이티브**: `outliner_win::edit::Editor`(가짜 EDIT)에 실제 키 이벤트를 넣는다.
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
    /// Alt+Shift+↑/↓
    MoveUp,
    MoveDown,
    /// TODO 순환(Ctrl+Enter).
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
    folded: Vec<usize>,
}

fn engine(op: &str, md: &str, args: Value) -> Result<Value, String> {
    let resp = outliner_core::handle_request(json!({"id": op, "op": op, "document": md, "args": args}));
    if resp["ok"].as_bool() != Some(true) {
        return Err(resp["error"]["message"].as_str().unwrap_or("error").to_string());
    }
    Ok(resp["result"].clone())
}

impl Flutter {
    fn load(md: &str) -> Flutter {
        let doc: Document = serde_json::from_value(engine("parse", md, json!({})).unwrap()).unwrap();
        Flutter { md: md.to_string(), doc, folded: Vec::new() }
    }

    fn markdown(&self) -> String {
        self.md.clone()
    }

    /// `_visibleBlocks` (outliner_workspace.dart) 그대로.
    fn visible(&self) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        let mut folded_at: i64 = -1;
        for (i, b) in self.doc.blocks.iter().enumerate() {
            let indent = b.indent as i64;
            if folded_at >= 0 && indent > folded_at {
                continue;
            }
            folded_at = -1;
            out.push((b.text.clone(), b.indent));
            if self.folded.contains(&i) {
                folded_at = indent;
            }
        }
        out
    }

    fn apply(&mut self, st: &Step) -> Result<(), String> {
        if matches!(st.op, Op::Append) {
            let last = self.doc.blocks.last().ok_or("블록 없음")?.id;
            let r = engine("edit", &self.md, json!({"action": "insert", "block_id": last, "text": ""}))?;
            self.md = r["markdown"].as_str().ok_or("no markdown")?.to_string();
            self.doc = serde_json::from_value(engine("parse", &self.md, json!({}))?).map_err(|e| e.to_string())?;
            return Ok(());
        }
        let block = self.doc.blocks.get(st.block).ok_or("블록 없음")?.clone();
        let value = st.typed.map(str::to_string).unwrap_or_else(|| block.text.clone());
        let end = value.encode_utf16().count();
        let caret: usize = match st.op {
            Op::Enter(Some(c)) => value.chars().take(c).map(char::len_utf16).sum(),
            Op::Backspace => 0,
            _ => end,
        };
        let mut args = json!({"action": action(st.op), "block": st.block, "start": caret, "end": caret,
            "folded": self.folded});
        if !matches!(st.op, Op::Fold | Op::Unfold) {
            args["text"] = Value::String(value);
        }
        let r = engine("key", &self.md, args)?;
        self.md = r["markdown"].as_str().ok_or("no markdown")?.to_string();
        self.doc = serde_json::from_value(r["document"].clone()).map_err(|e| e.to_string())?;
        self.folded = serde_json::from_value(r["folded"].clone()).map_err(|e| e.to_string())?;
        match r["note"].as_str() {
            Some(note) if r["changed"] == false => Err(note.to_string()),
            _ => Ok(()),
        }
    }
}

fn action(op: Op) -> &'static str {
    match op {
        Op::Enter(_) => "enter",
        Op::Tab => "indent",
        Op::ShiftTab => "outdent",
        Op::Backspace => "backspace",
        Op::MoveUp => "move_up",
        Op::MoveDown => "move_down",
        Op::Toggle => "cycle_todo",
        Op::Fold => "collapse",
        Op::Unfold => "expand",
        Op::Append => "insert",
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
                Op::MoveUp => VK_UP,
                Op::MoveDown => VK_DOWN,
                Op::Fold | Op::Unfold | Op::Append => unreachable!(),
            },
            self.t,
        );
        ev.shift = matches!(st.op, Op::ShiftTab | Op::MoveUp | Op::MoveDown);
        ev.ctrl = matches!(st.op, Op::Toggle);
        ev.alt = matches!(st.op, Op::MoveUp | Op::MoveDown);
        self.ed.key(ev);
        let msg = self.ed.message.take();
        self.ed.finish_edit(); // 입력 중 글을 문서에 확정
        // 메시지는 "저장했습니다" 류 안내가 아니라 오류만 본다.
        match msg {
            Some(m) if self.markdown() == before => Err(m),
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------- 시나리오

struct Scenario {
    name: &'static str,
    md: &'static str,
    steps: Vec<Step>,
}

fn scenarios() -> Vec<Scenario> {
    use Op::*;
    let long_ko: &'static str = Box::leak("가나다라마바사아자차카타파하".repeat(40).into_boxed_str()); // 560자
    let long_md: &'static str = Box::leak(format!("- {long_ko}\n- 끝\n").into_boxed_str());
    let tree = "- 가\n - 나\n  - 다\n   - 라\n- 마\n";
    vec![
        // ---- Enter ----
        Scenario { name: "빈 문서(- ) 입력 후 Enter", md: "- \n", steps: vec![st(0, Enter(None), "첫 줄")] },
        Scenario { name: "빈 문서 Enter 만(빈 블록 추가)", md: "- \n", steps: vec![s(0, Enter(None))] },
        Scenario { name: "영문 줄 끝 Enter", md: "- hello\n- world\n", steps: vec![s(0, Enter(None))] },
        Scenario { name: "한글 줄 끝 Enter", md: "- 안녕하세요\n", steps: vec![s(0, Enter(None))] },
        Scenario { name: "긴 줄(560자) 끝 Enter", md: long_md, steps: vec![s(0, Enter(None))] },
        Scenario { name: "긴 줄 수정 후 Enter(입력 중 글 저장)", md: long_md, steps: vec![st(0, Enter(None), "짧게 고침")] },
        Scenario { name: "들여쓴 블록 Enter(자식이 있으면 첫 자식)", md: tree, steps: vec![s(2, Enter(None))] },
        Scenario { name: "TODO 블록 Enter(새 블록은 상태 없음)", md: "- TODO 할 일\n- 다음\n", steps: vec![s(0, Enter(None))] },
        Scenario { name: "DONE 블록 Enter", md: "- DONE 끝낸 일\n", steps: vec![s(0, Enter(None))] },
        Scenario {
            name: "Enter 3연타(끝에서 계속 입력)",
            md: "- 시작\n",
            steps: vec![st(0, Enter(None), "가"), st(1, Enter(None), "나"), st(2, Enter(None), "다")],
        },
        Scenario { name: "링크·태그·속성이 든 글 Enter", md: "- x\n", steps: vec![st(0, Enter(None), "[[회의록]] 참고 #중요 key:: 값")] },
        Scenario { name: "이모지·결합 문자 글 Enter", md: "- a\n", steps: vec![st(0, Enter(None), "😀 한글 e\u{301}")] },
        Scenario { name: "CRLF 문서 Enter", md: "- a\r\n- b\r\n", steps: vec![s(0, Enter(None))] },
        Scenario { name: "끝 줄바꿈 없는 문서 Enter", md: "- a\n- b", steps: vec![s(1, Enter(None))] },
        Scenario { name: "보존 줄(제목) 낀 문서 Enter", md: "# 제목\n- a\n\n- b\n", steps: vec![s(0, Enter(None))] },
        Scenario { name: "접힌 부모 아래에서 마지막 블록 Enter", md: "- 가\n - 나\n- 다\n", steps: vec![s(2, Enter(None))] },
        // ---- Tab / Shift+Tab ----
        Scenario { name: "둘째 블록 Tab(자식이 됨)", md: "- a\n- b\n", steps: vec![s(1, Tab)] },
        Scenario { name: "첫 블록 Tab(오류)", md: "- a\n- b\n", steps: vec![s(0, Tab)] },
        Scenario { name: "최상위 Shift+Tab(오류)", md: "- a\n- b\n", steps: vec![s(1, ShiftTab)] },
        Scenario { name: "Tab 두 번(깊이 2)", md: "- a\n- b\n- c\n", steps: vec![s(1, Tab), s(2, Tab), s(2, Tab)] },
        Scenario { name: "Tab 후 Shift+Tab 되돌리기", md: "- a\n- b\n", steps: vec![s(1, Tab), s(1, ShiftTab)] },
        Scenario { name: "깊이 4 트리 중간 Shift+Tab", md: tree, steps: vec![s(2, ShiftTab)] },
        Scenario { name: "한글 문서 들여쓰기 경계(첫·끝 블록)", md: "- 가\n- 나\n- 다\n", steps: vec![s(0, Tab), s(2, Tab), s(2, ShiftTab)] },
        // ---- Backspace ----
        Scenario { name: "가운데 빈 블록 Backspace(삭제)", md: "- a\n- \n- c\n", steps: vec![s(1, Backspace)] },
        Scenario { name: "첫 빈 블록 Backspace(삭제)", md: "- \n- b\n", steps: vec![s(0, Backspace)] },
        Scenario { name: "앞 블록이 빈 칸일 때 병합(공백 차이 없음)", md: "- \n- 뒤\n", steps: vec![s(1, Backspace)] },
        Scenario { name: "첫 블록 맨 앞 Backspace(아무 일 없음)", md: "- a\n- b\n", steps: vec![s(0, Backspace)] },
        Scenario { name: "빈 블록 두 번 Backspace", md: "- a\n- \n- \n", steps: vec![s(2, Backspace), s(1, Backspace)] },
        // ---- 상태(TODO) ----
        Scenario { name: "TODO 순환(없음→TODO→DONE→없음)", md: "- 일\n", steps: vec![s(0, Toggle), s(0, Toggle), s(0, Toggle)] },
        Scenario { name: "입력 중 글 + 토글", md: "- 일\n- 이\n", steps: vec![st(1, Toggle, "이 수정")] },
        // ---- Alt+Shift+↑/↓ (한 줄짜리 형제만: 두 방식이 같다) ----
        Scenario { name: "Alt+Shift+↓ 평평한 목록", md: "- a\n- b\n- c\n", steps: vec![s(0, MoveDown)] },
        Scenario { name: "Alt+Shift+↑ 평평한 목록", md: "- a\n- b\n- c\n", steps: vec![st(2, MoveUp, "c 수정")] },
        Scenario { name: "첫 블록 Alt+Shift+↑(오류)", md: "- a\n- b\n", steps: vec![s(0, MoveUp)] },
        Scenario { name: "마지막 블록 Alt+Shift+↓(오류)", md: "- a\n- b\n", steps: vec![s(1, MoveDown)] },
        Scenario { name: "제목(보존 줄) 위로 Alt+Shift+↑(오류)", md: "# 제목\n- a\n- b\n", steps: vec![s(0, MoveUp)] },
        // ---- 접기(화면에 보이는 줄까지 대조) ----
        Scenario { name: "접기 → 하위 숨김", md: "- 가\n - 나\n  - 다\n- 라\n", steps: vec![s(0, Fold)] },
        Scenario { name: "접기 → 펼치기", md: "- 가\n - 나\n- 라\n", steps: vec![s(0, Fold), s(0, Unfold)] },
        Scenario { name: "하위 없는 블록 접기(보이는 변화 없음)", md: "- 가\n- 나\n", steps: vec![s(1, Fold)] },
        Scenario { name: "마지막 부모만 접기", md: "- 가\n - 나\n- 다\n - 라\n", steps: vec![s(2, Fold)] },
        // ---- 새 블록 단추(OUTLINERWIN5 추가 기능) ----
        Scenario { name: "새 블록 단추: 단순 문서 끝에 추가", md: "- a\n- b\n", steps: vec![s(0, Append)] },
        Scenario { name: "새 블록 단추: 들여쓴 마지막 블록 뒤(같은 단계)", md: tree, steps: vec![s(0, Append), s(1, Tab), s(2, Append)] },
        Scenario { name: "새 블록 단추: 한글·TODO 문서", md: "- TODO 할 일\n- 안녕\n", steps: vec![s(0, Append), s(0, Append)] },
        // ---- 예전 허용 차이였던 것(이제 같다) ----
        Scenario { name: "한가운데 Enter(커서 분할)", md: "- 안녕하세요\n", steps: vec![s(0, Enter(Some(2)))] },
        Scenario { name: "맨 앞 Enter", md: "- 글\n", steps: vec![s(0, Enter(Some(0)))] },
        Scenario { name: "두 글 Backspace 병합", md: "- 안녕\n- 하세요\n", steps: vec![s(1, Backspace)] },
        Scenario { name: "Tab 할 때 입력 중 글", md: "- a\n- b\n", steps: vec![st(1, Tab, "b 고침")] },
        Scenario { name: "접은 뒤 토글", md: "- 가\n - 나\n- 다\n", steps: vec![s(0, Fold), s(0, Toggle)] },
        Scenario { name: "하위 있는 블록 아래로 이동", md: "- a\n- b\n - b1\n- c\n", steps: vec![s(1, MoveDown)] },
        Scenario { name: "첫 자식 위로 이동", md: "- a\n - a1\n- b\n - b1\n", steps: vec![s(3, MoveUp)] },
        Scenario { name: "마지막 한 블록 Backspace", md: "- \n", steps: vec![s(0, Backspace)] },
        Scenario { name: "접힌 하위 건너 Backspace", md: "- a\n - a1\n- b\n", steps: vec![s(0, Fold), s(2, Backspace)] },
        Scenario { name: "빈 하위 블록 Enter(내어쓰기)", md: "- a\n - \n", steps: vec![s(1, Enter(None))] },
        Scenario { name: "여러 줄 블록 편집·분할", md: "- 첫\n  둘\n  id:: x\n- 끝\n", steps: vec![s(0, Enter(Some(2)))] },
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
fn scenarios_have_zero_differences() {
    let mut bad = Vec::new();
    for sc in scenarios() {
        let t = run(&sc);
        if t.first_divergence.is_some() || t.oks.iter().any(|(a, b)| a != b) {
            bad.push(describe(&sc, &t));
        }
    }
    assert!(bad.is_empty(), "갈라진 시나리오 {}개:\n{}", bad.len(), bad.join("\n"));
}
