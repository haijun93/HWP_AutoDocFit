//! 편집 상태 기계(Win32 없이 시험한다). 설계 문서 §3 의 N/E/C/F 상태와 §3.5 의 "확정 후 명령" 순서.
//!
//! 핵심 규칙(어느 순서로 메시지가 와도 같은 결과):
//!  1. 명령(Enter·Tab·↑↓·클릭·저장)은 조합 중이면 먼저 조합을 확정(`request_complete`)하고, 조합이 끝난 뒤에
//!     EDIT 의 글을 읽는다. 조합이 끝나기 전에는 글을 읽지 않는다.
//!  2. 명령 실행 직전에 EDIT 의 현재 글을 항상 읽어 엔진 `set_text` 로 반영한다(글 유실 방지).
//!  3. 엔진 호출이 실패하면 문서도 EDIT 글도 바꾸지 않고 오류만 남긴다(한 명령 = 한 트랜잭션).
//!  4. 조합 중 Backspace·Esc·후보창 조작은 IME 소유 — 가로채지 않는다.
//!  5. IME 가 키를 `VK_PROCESSKEY` 로 한 번, 조합 확정 뒤 진짜 키로 또 한 번 보내도 명령은 한 번만(50 ms 중복 제거).
use crate::docio::{self, FileInfo, SaveError};
use crate::links::{self, LinkCtx};
use crate::view::View;
use outliner_core::{Document, KeyInput, KeyOutcome, UndoHistory, apply_key};
use std::collections::VecDeque;

pub const VK_BACK: u16 = 0x08;
pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0D;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_UP: u16 = 0x26;
pub const VK_DOWN: u16 = 0x28;
pub const VK_LEFT: u16 = 0x25;
pub const VK_RIGHT: u16 = 0x27;
pub const VK_DELETE: u16 = 0x2E;
/// `;` 키(VK_OEM_1) — Ctrl+; 접기 토글.
pub const VK_OEM_1: u16 = 0xBA;
pub const VK_A: u16 = 0x41;
pub const VK_S: u16 = 0x53;
pub const VK_Y: u16 = 0x59;
pub const VK_Z: u16 = 0x5A;

/// EDIT 컨트롤(또는 시험용 가짜)이 해 주는 일.
pub trait EditHost {
    /// EDIT 의 현재 글(조합 중인 글자는 포함되지 않을 수 있다 — 그래서 먼저 확정한다).
    fn read_text(&mut self) -> String;
    /// 선택 범위(UTF-16 오프셋). 캐럿만 있으면 두 값이 같다.
    fn selection(&mut self) -> (usize, usize);
    /// 글을 바꾸고 캐럿을 UTF-16 오프셋 `caret` 에 둔다.
    fn set_text(&mut self, text: &str, caret: usize);
    /// 조합 중인가(`ImmGetCompositionStringW(GCS_COMPSTR)` 길이 > 0 또는 시작~끝 알림 사이).
    fn composing(&mut self) -> bool;
    /// IME 후보창이 열려 있는가.
    fn candidate_open(&mut self) -> bool;
    /// `ImmNotifyIME(NI_COMPOSITIONSTR, CPS_COMPLETE, 0)`.
    fn request_complete(&mut self);
    /// 선택 범위만 바꾼다(괄호 감싸기).
    fn set_selection(&mut self, _start: usize, _end: usize) {}
}

/// 조합 중 키를 받았을 때의 동작. U1·U2(실기 확인)가 끝나면 값만 바꾸면 된다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposingKey {
    /// 먼저 조합을 확정하고, 확정이 끝나면 명령을 실행(설계 문서의 안전한 기본값).
    CompleteThenRun,
    /// 조합만 확정하고 명령은 버린다(두 번째 키부터 명령).
    CompleteOnly,
    /// 아무것도 하지 않고 IME 에 맡긴다.
    Ignore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImePolicy {
    pub enter: ComposingKey,
    pub tab: ComposingKey,
    pub arrows: ComposingKey,
    /// 확정 요청 뒤 조합 종료 알림을 기다리는 최대 시간(F 상태).
    pub complete_timeout_ms: u64,
    /// `VK_PROCESSKEY` 로 처리한 명령 뒤에 같은 진짜 키가 이 시간 안에 오면 중복으로 버린다.
    pub dedupe_ms: u64,
}

impl Default for ImePolicy {
    fn default() -> Self {
        ImePolicy {
            enter: ComposingKey::CompleteThenRun,
            tab: ComposingKey::CompleteThenRun,
            arrows: ComposingKey::CompleteThenRun,
            complete_timeout_ms: 200,
            dedupe_ms: 50,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// 실제 키(`VK_PROCESSKEY` 면 `ImmGetVirtualKey` 로 구한 원래 키).
    pub vk: u16,
    /// 창이 받은 메시지의 wParam 이 `VK_PROCESSKEY`(0xE5)였다.
    pub processkey: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub now_ms: u64,
}

impl KeyEvent {
    pub fn new(vk: u16, now_ms: u64) -> KeyEvent {
        KeyEvent { vk, processkey: false, shift: false, ctrl: false, alt: false, now_ms }
    }
    pub fn process(vk: u16, now_ms: u64) -> KeyEvent {
        KeyEvent { processkey: true, ..KeyEvent::new(vk, now_ms) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyResult {
    /// EDIT·IME 의 기본 처리에 맡긴다.
    Pass,
    /// 우리가 처리했다(기본 처리로 넘기지 않는다).
    Swallow,
}

/// 편집 명령. 규칙 자체는 `outliner_core::apply_key`(docs/OUTLINEKEY1_DESIGN_2026-11-26.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Enter,
    /// Shift+Enter: 블록 안 줄바꿈.
    NewLine,
    Indent,
    Outdent,
    /// 첫 줄의 ↑ / 마지막 줄의 ↓.
    Up,
    Down,
    /// 맨 앞의 ← / 맨 끝의 →.
    Left,
    Right,
    Backspace,
    Delete,
    /// 글을 확정하고 그 블록을 선택(Esc, Alt/Shift+↑↓).
    SelectBlock,
    SelectAllBlocks,
    /// 대기 중인 클립보드 글을 블록 트리로 붙여넣기.
    Paste,
    /// 다른 블록(문서 안 순서 번호)으로 편집 이동.
    Goto(usize),
    Save,
    /// 확정하지 않은 입력을 버리고 편집 종료(창 내부용).
    Cancel,
    /// TODO 순환(Ctrl+Enter): 없음 → TODO → DONE → 없음.
    Toggle,
    /// 블록(하위 포함) 위/아래 이동(Alt+Shift+↑/↓).
    MoveUp,
    MoveDown,
    Collapse,
    Expand,
    ToggleFold,
}

/// `[[` 자동완성 팝업 상태(편집 중에만).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Popup {
    pub ctx: LinkCtx,
    pub items: Vec<String>,
    pub labels: Vec<String>,
    pub selected: usize,
}

struct Session {
    block: usize,
}

#[derive(Clone, Copy)]
struct Queued {
    cmd: Cmd,
    from_processkey: bool,
    at: u64,
}

pub struct Editor<H: EditHost> {
    pub view: View,
    pub host: H,
    pub policy: ImePolicy,
    session: Option<Session>,
    /// 화면 위쪽 줄에 보일 안내(오류·경고). 다음 명령이 시작되면 지워진다.
    pub message: Option<String>,
    pub conflict: bool,
    /// 대화상자로 알려야 할 문제(저장 충돌·저장 실패). 창이 보여 준 뒤 `take()` 한다.
    pub alert: Option<String>,
    file: Option<FileInfo>,
    /// 마지막으로 저장했거나 읽은 마크다운.
    baseline: String,
    queue: VecDeque<Queued>,
    waiting: bool,
    wait_since: u64,
    suppress: Option<(Cmd, u64)>,
    /// 자동완성 후보가 될 페이지 이름·최근 페이지(셸이 채운다).
    pub page_names: Vec<String>,
    pub recent_names: Vec<String>,
    pub tag_names: Vec<String>,
    /// 다른 페이지의 영구 ID·페이지/본문 설명(페이지 전환 때 갱신).
    pub graph_blocks: Vec<(String, String)>,
    pub today: String,
    /// 열려 있는 `[[` 자동완성 팝업.
    pub popup: Option<Popup>,
    /// Esc 로 닫은 `[[` 의 위치 — 같은 `[[` 에서는 다시 열지 않는다.
    popup_off: Option<(usize, links::RefKind)>,
    /// `Cmd::Paste` 가 쓸 클립보드 글.
    pending_paste: Option<String>,
    /// 문서 단위 실행 취소/다시 실행 이력(OUTLINERUNDO1). 편집 명령 하나(=Enter 의 글 확정+분할 등)가 한 단계다.
    pub history: UndoHistory,
}

/// 블록 글 규칙: 줄바꿈은 `\n`, 탭은 공백.
pub fn sanitize(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n").replace('\t', " ")
}

pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// UTF-16 오프셋 → 바이트 위치(서로게이트 쌍 한가운데면 앞 경계로).
fn byte_at(s: &str, off16: usize) -> usize {
    let mut n = 0;
    for (b, c) in s.char_indices() {
        if n >= off16 || n + c.len_utf16() > off16 {
            return b;
        }
        n += c.len_utf16();
    }
    s.len()
}

/// 선택 `[start, end)`(UTF-16)를 지우고 그 자리에서 앞/뒤로 나눈다.
pub fn split_utf16(text: &str, start: usize, end: usize) -> (String, String) {
    let (a, b) = (byte_at(text, start.min(end)), byte_at(text, start.max(end)));
    (text[..a].to_string(), text[b..].to_string())
}

/// 한 명령의 변경을 모아 두었다가 모두 성공했을 때만 반영하는 임시 작업대.
struct Tx {
    doc: Document,
    folded: Vec<bool>,
}

impl Tx {
    fn new(doc: &Document) -> Tx {
        Tx { doc: doc.clone(), folded: doc.blocks.iter().map(|b| b.folded).collect() }
    }
    fn act(&mut self, action: &str, idx: usize, text: Option<&str>) -> Result<(), String> {
        let id = self.doc.blocks.get(idx).ok_or("블록이 없습니다")?.id;
        // 화면 문서는 드래그 후에도 ID를 유지한다. JSON 왕복의 순번 재배정을 피한다.
        outliner_core::edit(&mut self.doc, action, id, text).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn set_text(&mut self, idx: usize, text: &str) -> Result<(), String> {
        if self.doc.blocks.get(idx).ok_or("블록이 없습니다")?.text == text {
            return Ok(());
        }
        self.act("set_text", idx, Some(text))
    }
    fn insert_after(&mut self, idx: usize, text: &str) -> Result<(), String> {
        self.act("insert", idx, Some(text))?;
        self.folded.insert(idx + 1, false);
        Ok(())
    }
    fn finish(mut self) -> Result<Document, String> {
        if self.folded.len() != self.doc.blocks.len() {
            return Err("내부 오류: 접힘 상태 길이 불일치".into());
        }
        for (b, f) in self.doc.blocks.iter_mut().zip(&self.folded) {
            b.folded = *f;
        }
        Ok(self.doc)
    }
}

impl<H: EditHost> Editor<H> {
    pub fn new(view: View, host: H, file: Option<FileInfo>, baseline: String) -> Editor<H> {
        Editor {
            view,
            host,
            policy: ImePolicy::default(),
            session: None,
            message: None,
            conflict: false,
            alert: None,
            file,
            baseline,
            queue: VecDeque::new(),
            waiting: false,
            wait_since: 0,
            suppress: None,
            page_names: Vec::new(),
            recent_names: Vec::new(),
            tag_names: Vec::new(),
            graph_blocks: Vec::new(),
            today: crate::clock::now().date_iso,
            popup: None,
            popup_off: None,
            pending_paste: None,
            history: UndoHistory::default(),
        }
    }

    /// 다른 문서로 바꾼다(페이지 열기·이름 바꾸기 뒤). 편집·대기 명령·팝업은 모두 버린다.
    pub fn replace_document(&mut self, view: View, file: Option<FileInfo>, baseline: String) {
        self.view = view;
        self.file = file;
        self.baseline = baseline;
        self.session = None;
        self.queue.clear();
        self.waiting = false;
        self.suppress = None;
        self.message = None;
        self.conflict = false;
        self.alert = None;
        self.popup = None;
        self.popup_off = None;
        self.history.clear(); // 다른 문서: 이력은 쓸 수 없다
    }

    pub fn editing(&self) -> bool {
        self.session.is_some()
    }

    /// 편집 중인 블록(문서 안 순서 번호).
    pub fn editing_block(&self) -> Option<usize> {
        self.session.as_ref().map(|s| s.block)
    }

    /// 확정 대기(F) 중인가.
    pub fn waiting(&self) -> bool {
        self.waiting
    }

    pub fn file(&self) -> Option<&FileInfo> {
        self.file.as_ref()
    }

    pub fn markdown(&self) -> String {
        outliner_core::serialize(&self.view.doc)
    }

    /// 제목 줄 ●: 저장하지 않은 변경(확정된 문서 변경 + 아직 확정 안 된 EDIT 입력)이 있는가.
    pub fn dirty(&mut self) -> bool {
        if self.markdown() != self.baseline {
            return true;
        }
        match &self.session {
            Some(s) => {
                let block = s.block;
                let typed = sanitize(&self.host.read_text());
                self.view.doc.blocks.get(block).is_some_and(|b| b.text != typed)
            }
            None => false,
        }
    }

    // ---------- 편집 진입 ----------

    /// 선택한 줄의 블록 편집을 시작(Enter·F2·더블클릭). 캐럿은 글 끝.
    pub fn start_edit_selected(&mut self) -> bool {
        match self.view.rows.get(self.view.selected).map(|r| r.block) {
            Some(b) => {
                self.begin(b, None);
                true
            }
            None => false,
        }
    }

    fn begin(&mut self, block: usize, caret: Option<usize>) {
        let text = self.view.doc.blocks[block].text.clone();
        let caret = caret.unwrap_or_else(|| utf16_len(&text));
        self.host.set_text(&text, caret);
        self.popup = None;
        self.popup_off = None;
        self.session = Some(Session { block });
        if let Some(pos) = self.view.rows.iter().position(|r| r.block == block) {
            self.view.selected = pos;
        }
    }

    // ---------- 키 입력 ----------

    /// 캐럿이 글의 첫 줄(`up`) / 마지막 줄에 있는가.
    fn on_edge_line(&mut self, up: bool) -> bool {
        let text = sanitize(&self.host.read_text());
        let (start, end) = self.host.selection();
        let at = if up { start } else { end };
        let (before, after) = split_utf16(&text, at, at);
        if up { !before.contains('\n') } else { !after.contains('\n') }
    }

    fn text_len(&mut self) -> usize {
        utf16_len(&sanitize(&self.host.read_text()))
    }

    /// 키 → (명령, 조합 중 정책). `None` 이면 EDIT 의 기본 처리.
    fn cmd_of(&mut self, ev: &KeyEvent) -> Option<(Cmd, fn(&ImePolicy) -> ComposingKey)> {
        let enter: fn(&ImePolicy) -> ComposingKey = |p| p.enter;
        let tab: fn(&ImePolicy) -> ComposingKey = |p| p.tab;
        let arrows: fn(&ImePolicy) -> ComposingKey = |p| p.arrows;
        let vertical = ev.vk == VK_UP || ev.vk == VK_DOWN;
        let up = ev.vk == VK_UP;
        if ev.alt {
            if ev.ctrl || !vertical {
                return None;
            }
            return Some(match (ev.shift, up) {
                (true, true) => (Cmd::MoveUp, arrows),
                (true, false) => (Cmd::MoveDown, arrows),
                (false, _) => (Cmd::SelectBlock, arrows),
            });
        }
        if ev.ctrl {
            return match ev.vk {
                VK_RETURN if !ev.shift => Some((Cmd::Toggle, enter)),
                VK_UP if !ev.shift => Some((Cmd::Collapse, arrows)),
                VK_DOWN if !ev.shift => Some((Cmd::Expand, arrows)),
                VK_OEM_1 if !ev.shift => Some((Cmd::ToggleFold, arrows)),
                VK_A if ev.shift => Some((Cmd::SelectAllBlocks, arrows)),
                VK_A => {
                    // 글을 이미 모두 선택했으면 블록 선택으로.
                    let length = self.text_len();
                    (self.host.selection() == (0, length)).then_some((Cmd::SelectBlock, arrows))
                }
                _ => None,
            };
        }
        let (start, end) = self.host.selection();
        let caret = start == end;
        match ev.vk {
            VK_RETURN => Some((if ev.shift { Cmd::NewLine } else { Cmd::Enter }, enter)),
            VK_TAB => Some((if ev.shift { Cmd::Outdent } else { Cmd::Indent }, tab)),
            VK_UP | VK_DOWN => {
                if !self.on_edge_line(up) {
                    return None;
                }
                let cmd = if ev.shift { Cmd::SelectBlock } else if up { Cmd::Up } else { Cmd::Down };
                Some((cmd, arrows))
            }
            VK_LEFT if !ev.shift && caret && start == 0 => Some((Cmd::Left, arrows)),
            VK_RIGHT if !ev.shift && caret && start == self.text_len() => Some((Cmd::Right, arrows)),
            _ => None,
        }
    }

    /// 편집 중 EDIT 가 받은 키. `Pass` 면 호출한 쪽이 기본 처리에 넘긴다.
    pub fn key(&mut self, ev: KeyEvent) -> KeyResult {
        if self.session.is_none() {
            return KeyResult::Pass;
        }
        let composing = self.host.composing();
        let candidate = self.host.candidate_open();
        if ev.ctrl && ev.vk == VK_S && !ev.alt {
            self.submit(Cmd::Save, ev.now_ms, false, composing);
            return KeyResult::Swallow;
        }
        // Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z: 확정하지 않은 글이 있거나 조합 중이면 EDIT 자체의 글자 되돌리기에 맡기고(Pass),
        // 없으면 문서 단위 이력을 쓴다.
        if ev.ctrl && !ev.alt && (ev.vk == VK_Z || ev.vk == VK_Y) {
            let redo = ev.vk == VK_Y || ev.shift;
            return if self.history_step(!redo) { KeyResult::Swallow } else { KeyResult::Pass };
        }
        // 자동완성 팝업이 열려 있으면 ↑↓·Enter·Tab·Esc 는 팝업이 먼저 받는다(조합 중이면 확정만 하고 명령은 버림).
        if self.popup.is_some() && ev.ctrl && !ev.alt && !ev.shift && matches!(ev.vk, 0x4E | 0x50) {
            if composing || candidate || ev.processkey { return KeyResult::Pass; }
            self.move_popup(ev.vk == 0x4E);
            return KeyResult::Swallow;
        }
        if self.popup.is_some() && !ev.alt && !ev.ctrl && !candidate {
            match ev.vk {
                VK_UP | VK_DOWN if !ev.shift => {
                    if composing || ev.processkey {
                        return KeyResult::Pass;
                    }
                    self.move_popup(ev.vk == VK_DOWN);
                    return KeyResult::Swallow;
                }
                VK_RETURN | VK_TAB if !ev.shift => {
                    if composing {
                        self.host.request_complete();
                        // VK_PROCESSKEY 뒤 진짜 Enter/Tab이 다시 와도 이번 입력은 확정만 한다.
                        self.suppress = Some((if ev.vk == VK_RETURN { Cmd::Enter } else { Cmd::Indent }, ev.now_ms + self.policy.dedupe_ms));
                        return if ev.processkey { KeyResult::Pass } else { KeyResult::Swallow };
                    }
                    if ev.processkey {
                        return KeyResult::Pass;
                    }
                    let cmd = if ev.vk == VK_RETURN { Cmd::Enter } else { Cmd::Indent };
                    if self.suppress.is_some_and(|(c, until)| c == cmd && ev.now_ms <= until) {
                        return KeyResult::Swallow;
                    }
                    self.accept_popup(None);
                    return KeyResult::Swallow;
                }
                VK_ESCAPE if !ev.shift => {
                    if composing || ev.processkey {
                        return KeyResult::Pass; // 조합 취소는 IME 소유
                    }
                    self.dismiss_popup();
                    return KeyResult::Swallow;
                }
                _ => {}
            }
        }
        let plain = !ev.alt && !ev.ctrl && !ev.shift;
        match ev.vk {
            VK_ESCAPE if plain => {
                if composing || candidate {
                    return KeyResult::Pass; // 조합 취소는 IME 소유
                }
                self.submit(Cmd::SelectBlock, ev.now_ms, ev.processkey, false);
                return KeyResult::Swallow;
            }
            VK_BACK if !ev.alt && !ev.ctrl => {
                if composing || candidate || ev.processkey {
                    return KeyResult::Pass;
                }
                let (s, e) = self.host.selection();
                if s == e && s == 0 {
                    self.submit(Cmd::Backspace, ev.now_ms, false, false);
                    return KeyResult::Swallow;
                }
                if s == e {
                    // 빈 괄호 짝 안: 둘 다 지운다.
                    if let Some((text, caret)) = outliner_core::pair_backspace(&sanitize(&self.host.read_text()), s) {
                        self.host.set_text(&text, caret);
                        return KeyResult::Swallow;
                    }
                }
                return KeyResult::Pass;
            }
            VK_DELETE if plain => {
                if composing || candidate || ev.processkey {
                    return KeyResult::Pass;
                }
                let (s, e) = self.host.selection();
                if s == e && s == self.text_len() {
                    self.submit(Cmd::Delete, ev.now_ms, false, false);
                    return KeyResult::Swallow;
                }
                return KeyResult::Pass;
            }
            _ => {}
        }
        let Some((cmd, pick)) = self.cmd_of(&ev) else { return KeyResult::Pass };
        if candidate {
            return KeyResult::Pass; // 후보창 조작은 IME 소유
        }
        // 직전 명령이 VK_PROCESSKEY 로 처리됐고 같은 진짜 키가 곧바로 또 왔다 → 한 물리 키 = 한 명령.
        if !ev.processkey {
            if let Some((c, until)) = self.suppress {
                if c == cmd && ev.now_ms <= until {
                    self.suppress = None;
                    return KeyResult::Swallow;
                }
            }
            if self.waiting {
                if let Some(last) = self.queue.back() {
                    if last.from_processkey && last.cmd == cmd && ev.now_ms.saturating_sub(last.at) <= self.policy.dedupe_ms {
                        return KeyResult::Swallow;
                    }
                }
            }
        }
        if composing {
            match pick(&self.policy) {
                ComposingKey::Ignore => return KeyResult::Pass,
                ComposingKey::CompleteOnly => {
                    self.host.request_complete();
                    return if ev.processkey { KeyResult::Pass } else { KeyResult::Swallow };
                }
                ComposingKey::CompleteThenRun => {}
            }
        }
        self.submit(cmd, ev.now_ms, ev.processkey, composing);
        if composing && ev.processkey { KeyResult::Pass } else { KeyResult::Swallow }
    }

    /// 편집 중 친 글자의 괄호 짝 규칙. true 면 처리했다(EDIT 에 넘기지 않는다).
    pub fn typed_char(&mut self, ch: char) -> bool {
        if self.session.is_none() || self.host.composing() {
            return false;
        }
        let text = sanitize(&self.host.read_text());
        let (s, e) = self.host.selection();
        let Some((new, start, end)) = outliner_core::type_char(&text, s, e, ch) else { return false };
        self.host.set_text(&new, start);
        if end != start {
            self.host.set_selection(start, end);
        }
        true
    }

    /// 편집 중 붙여넣기. 여러 줄·`- ` 목록은 블록 트리로(true), 한 줄 글은 EDIT 에 맡긴다(false).
    pub fn paste_text(&mut self, clip: &str, now_ms: u64) -> bool {
        let trimmed = clip.trim_end_matches(['\r', '\n']);
        let blocks = trimmed.contains('\n') || trimmed.trim_start().starts_with("- ");
        if self.session.is_none() || !blocks {
            return false;
        }
        self.pending_paste = Some(clip.to_string());
        let composing = self.host.composing();
        self.submit(Cmd::Paste, now_ms, false, composing);
        true
    }

    /// Ctrl+Shift+V: 블록을 나누지 않고 지금 블록 안에 그대로 넣는다.
    pub fn paste_inline(&mut self, clip: &str) {
        if self.session.is_none() {
            return;
        }
        if self.host.composing() {
            self.host.request_complete();
        }
        let text = sanitize(&self.host.read_text());
        let (s, e) = self.host.selection();
        let (left, right) = split_utf16(&text, s, e);
        let value = sanitize(clip);
        let caret = utf16_len(&left) + utf16_len(&value);
        let joined = format!("{left}{value}{right}");
        self.recorded(|ed| {
            ed.host.set_text(&joined, caret);
            ed.run_key("commit", None);
        });
    }

    /// 키·클릭·창 닫기에서 온 명령을 순서대로 처리한다. 조합 중이면 먼저 확정하고 대기(F).
    pub fn submit(&mut self, cmd: Cmd, now_ms: u64, from_processkey: bool, composing_hint: bool) {
        self.queue.push_back(Queued { cmd, from_processkey, at: now_ms });
        let composing = composing_hint || self.host.composing();
        if composing && self.session.is_some() {
            if !self.waiting {
                self.waiting = true;
                self.wait_since = now_ms;
            }
            self.host.request_complete();
        }
        self.run_pending(now_ms);
    }

    /// 대기 중인 명령을 실행한다. 조합이 아직 안 끝났고 제한 시간 전이면 아무것도 하지 않는다.
    /// (조합 종료 알림 → 창이 `PostMessage` 로 부른다 / 타이머가 부른다)
    pub fn run_pending(&mut self, now_ms: u64) {
        if self.queue.is_empty() {
            self.waiting = false;
            return;
        }
        if self.session.is_some() && self.host.composing() && now_ms.saturating_sub(self.wait_since) < self.policy.complete_timeout_ms {
            self.waiting = true;
            return; // F: 확정 대기
        }
        self.waiting = false;
        while let Some(q) = self.queue.pop_front() {
            self.execute(q.cmd);
            if q.from_processkey {
                self.suppress = Some((q.cmd, q.at + self.policy.dedupe_ms));
            }
        }
    }

    // ---------- `[[` 및 `((` 자동완성 ----------

    /// 글·캐럿이 바뀐 뒤 부른다: 캐럿이 `[[` 또는 `((` 뒤면 팝업을 열거나 후보를 갱신하고, 아니면 닫는다.
    pub fn refresh_popup(&mut self) {
        if self.session.is_none() {
            self.popup = None;
            return;
        }
        // 이미 보인 후보는 조합 중 고정한다. 첫 트리거는 확정된 접두만 읽어 문맥을 만들 수 있다.
        // 이 읽기는 표시 전용이며 조합 글을 엔진에 반영하거나 EDIT를 치환하지 않는다.
        if self.host.candidate_open() || (self.host.composing() && self.popup.is_some()) { return; }
        let text = self.host.read_text();
        let (s, e) = self.host.selection();
        let ctx = if s == e { links::any_completion_context(&text, s) } else { None };
        let Some(ctx) = ctx else {
            self.popup = None;
            self.popup_off = None;
            return;
        };
        if self.popup_off == Some((ctx.start16, ctx.kind)) {
            self.popup = None;
            return;
        }
        self.popup_off = None;
        let pairs: Vec<(String, String)> = match ctx.kind {
            links::RefKind::Page => links::candidates(&self.page_names, &self.recent_names, &ctx.query, 8)
                .into_iter().map(|s| (s.clone(), s)).collect(),
            links::RefKind::Tag => {
                let mut tags = self.tag_names.clone();
                tags.extend(self.page_names.clone());
                tags.extend(self.view.doc.blocks.iter().flat_map(|b| b.tags.clone()));
                links::candidates(&tags, &[], &ctx.query, 8).into_iter().map(|s| (s.clone(), format!("#{s}"))).collect()
            }
            links::RefKind::Slash => crate::commands::slash(&ctx.query),
            links::RefKind::Block | links::RefKind::TripleBlock => {
                let q = ctx.query.trim().to_lowercase();
                let mut out = Vec::new();
                for (i, b) in self.view.doc.blocks.iter().enumerate() {
                    if self.editing_block() == Some(i) { continue; }
                    let id_str = b.properties.get("id").cloned().unwrap_or_else(|| b.id.to_string());
                    if q.is_empty() || b.text.to_lowercase().contains(&q) || id_str.to_lowercase().contains(&q) {
                        out.push((id_str.clone(), format!("{} · {id_str}", b.text.replace('\n', " "))));
                        if out.len() >= 8 {
                            break;
                        }
                    }
                }
                for (id, label) in &self.graph_blocks {
                    if out.len() >= 8 { break; }
                    if !out.iter().any(|(v, _)| v == id) && (q.is_empty() || label.to_lowercase().contains(&q) || id.to_lowercase().contains(&q)) {
                        out.push((id.clone(), label.clone()));
                    }
                }
                out
            }
        };
        let (items, labels): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
        if items.is_empty() {
            self.popup = None;
            return;
        }
        let selected = match &self.popup {
            Some(p) if p.ctx.query == ctx.query && p.ctx.start16 == ctx.start16 => p.selected.min(items.len() - 1),
            _ => 0,
        };
        self.popup = Some(Popup { ctx, items, labels, selected });
    }

    pub fn move_popup(&mut self, down: bool) {
        if let Some(p) = self.popup.as_mut() {
            let n = p.items.len();
            p.selected = if down { (p.selected + 1) % n } else { (p.selected + n - 1) % n };
        }
    }

    pub fn dismiss_popup(&mut self) {
        if let Some(p) = self.popup.take() {
            self.popup_off = Some((p.ctx.start16, p.ctx.kind));
        }
    }

    /// 후보(번호 없으면 선택된 것)로 `[[질의` 를 `[[이름]]` (또는 `((id))`) 으로 바꾸고 캐럿을 닫는 괄호 뒤에 둔다.
    pub fn accept_popup(&mut self, index: Option<usize>) -> bool {
        if self.host.composing() || self.host.candidate_open() { return false; }
        let Some(p) = self.popup.clone() else { return false };
        let Some(name) = p.items.get(index.unwrap_or(p.selected)).cloned() else { return false };
        let text = self.host.read_text();
        // 팝업이 열린 뒤 글이 바뀌었을 수 있어 문맥을 다시 구한다.
        let (s, e) = self.host.selection();
        let Some(ctx) = links::any_completion_context(&text, s).filter(|ctx| s == e && *ctx == p.ctx) else {
            self.refresh_popup(); return false;
        };
        let (mut new, mut caret) = if ctx.kind == links::RefKind::Slash {
            crate::commands::insert_slash(&text, &ctx, &name, &self.today)
        } else { links::apply_completion(&text, &ctx, &name) };
        if ctx.kind == links::RefKind::Slash && matches!(name.as_str(), "TODO" | "DONE") {
            let Some(block) = self.editing_block() else { return false; };
            new = new.strip_prefix(&format!("{name} ")).unwrap_or(&new).to_string();
            caret = utf16_len(&new);
            let mut doc = self.view.doc.clone();
            let id = doc.blocks[block].id;
            if let Err(error) = outliner_core::edit(&mut doc, "set_text", id, Some(&new)) {
                self.fail(error.to_string()); return false;
            }
            doc.blocks[block].status = Some(name.clone());
            self.recorded(|ed| {
                ed.install(doc, block);
            });
        }
        // 현재 문서의 임시 숫자 ID는 선택하는 순간 영구 속성으로 만든다. 대상 속성과 참조를 한 단계로 기록.
        if matches!(ctx.kind, links::RefKind::Block | links::RefKind::TripleBlock) {
            let target = self.view.doc.blocks.iter().position(|b| !b.properties.contains_key("id") && b.id.to_string() == name);
            if let (Some(target), Some(block)) = (target, self.editing_block()) {
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
                let hash = outliner_core::content_digest(&format!("{stamp}:{}:{name}:{}", std::process::id(), self.markdown()));
                let uuid = format!("{}-{}-4{}-a{}-{}", &hash[..8], &hash[8..12], &hash[13..16], &hash[17..20], &hash[20..32]);
                (new, caret) = links::apply_completion(&text, &ctx, &uuid);
                let mut doc = self.view.doc.clone();
                let id = doc.blocks[block].id;
                if let Err(error) = outliner_core::edit(&mut doc, "set_text", id, Some(&new)) {
                    self.fail(error.to_string()); return false;
                }
                let after = doc.blocks[target].text.matches('\n').count();
                doc.blocks[target].extra.push(outliner_core::ExtraLine { after, line: format!("id:: {uuid}") });
                doc.blocks[target].properties.insert("id".into(), uuid);
                self.recorded(|ed| ed.install(doc, block));
            }
        }
        self.popup = None;
        self.host.set_text(&new, caret);
        self.popup_off = None;
        // #태그는 닫는 괄호가 없으므로 같은 자리에서 즉시 다시 열리지 않게 한다.
        if ctx.kind == links::RefKind::Tag { self.popup_off = Some((ctx.start16, ctx.kind)); }
        self.refresh_popup();
        true
    }

    // ---------- 명령 실행 ----------

    fn typed_text(&mut self) -> String {
        sanitize(&self.host.read_text())
    }

    fn install(&mut self, doc: Document, target_block: usize) {
        self.view.doc = doc;
        self.view.rebuild();
        if let Some(pos) = self.view.rows.iter().position(|r| r.block == target_block) {
            self.view.selected = pos;
        } else {
            self.view.selected = self.view.selected.min(self.view.rows.len().saturating_sub(1));
        }
    }

    fn fail(&mut self, msg: String) {
        self.message = Some(msg);
    }

    /// 편집 중이면 EDIT 글을 `set_text` 로 반영(변경 없으면 생략)하고 성공 여부를 돌려준다.
    fn commit_text(&mut self) -> bool {
        let Some(block) = self.session.as_ref().map(|s| s.block) else { return true };
        let text = self.typed_text();
        let mut tx = Tx::new(&self.view.doc);
        let result = tx.set_text(block, &text).and_then(|_| tx.finish());
        match result {
            Ok(doc) => {
                self.install(doc, block);
                true
            }
            Err(e) => {
                self.fail(format!("글을 반영하지 못했습니다(입력은 그대로 둠): {e}"));
                false
            }
        }
    }

    /// `f` 가 문서를 바꿨으면 바뀌기 전 마크다운을 되돌리기 한 단계로 기록한다(여러 엔진 호출도 한 단계).
    fn recorded<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let before = outliner_core::serialize(&self.view.doc);
        let result = f(self);
        if outliner_core::serialize(&self.view.doc) != before {
            self.history.record(&before);
        }
        result
    }

    fn execute(&mut self, cmd: Cmd) {
        self.recorded(|ed| ed.execute_inner(cmd));
    }

    fn execute_inner(&mut self, cmd: Cmd) {
        self.message = None;
        let action = match cmd {
            Cmd::Cancel => {
                self.popup = None;
                self.session = None; // 문서는 마지막 확정 상태 그대로 — 확정되지 않은 입력만 버린다
                return;
            }
            Cmd::Save => {
                if self.session.is_some() && !self.commit_text() {
                    return;
                }
                self.save();
                return;
            }
            Cmd::Goto(block) => {
                if self.session.is_some() && !self.commit_text() {
                    return;
                }
                if block < self.view.doc.blocks.len() {
                    self.begin(block, None);
                }
                return;
            }
            Cmd::SelectBlock | Cmd::SelectAllBlocks => {
                if !self.finish_edit_inner() {
                    return;
                }
                let (anchor, end) = if cmd == Cmd::SelectAllBlocks {
                    (0, self.view.rows.len().saturating_sub(1))
                } else {
                    (self.view.selected, self.view.selected)
                };
                self.view.selection_anchor = Some(anchor);
                self.view.selection_end = Some(end);
                self.view.selected = end;
                return;
            }
            Cmd::Enter => "enter",
            Cmd::NewLine => "newline",
            Cmd::Indent => "indent",
            Cmd::Outdent => "outdent",
            Cmd::Up => "up",
            Cmd::Down => "down",
            Cmd::Left => "left",
            Cmd::Right => "right",
            Cmd::Backspace => "backspace",
            Cmd::Delete => "delete",
            Cmd::Paste => "paste",
            Cmd::Toggle => "cycle_todo",
            Cmd::MoveUp => "move_up",
            Cmd::MoveDown => "move_down",
            Cmd::Collapse => "collapse",
            Cmd::Expand => "expand",
            Cmd::ToggleFold => "toggle_collapse",
        };
        let paste = if cmd == Cmd::Paste { self.pending_paste.take() } else { None };
        self.run_key(action, paste);
    }

    /// 편집 중이면 그 블록(칸의 글·커서), 아니면 선택한 줄에 core 규칙을 적용한다.
    fn run_key(&mut self, action: &str, paste: Option<String>) {
        let editing = self.editing_block().is_some();
        let Some(block) = self.target_block() else { return };
        let mut input = KeyInput { action: action.to_string(), block: Some(block), paste, ..KeyInput::default() };
        if editing {
            (input.start, input.end) = self.host.selection();
            input.text = Some(self.typed_text());
        }
        let outcome = match apply_key(&self.view.doc, &input) {
            Ok(outcome) => outcome,
            Err(e) => return self.fail(format!("처리하지 못했습니다(입력은 그대로 둠): {e}")),
        };
        if !outcome.handled {
            return;
        }
        self.message = outcome.note.clone();
        let cursor = outcome.cursor.map(|c| (c.block, c.start));
        let (target, caret) = match cursor {
            Some((b, caret)) => (b, Some(caret)),
            None => (block, None),
        };
        self.adopt(outcome, target);
        if editing {
            let target = target.min(self.view.doc.blocks.len().saturating_sub(1));
            self.view.reveal_block(target, usize::MAX / 4);
            self.view.first = self.view.first.min(self.view.selected);
            self.begin(target, caret);
        }
    }

    /// core 결과 문서를 화면에 반영한다(스크롤 위치는 유지).
    fn adopt(&mut self, outcome: KeyOutcome, target: usize) {
        let first = self.view.first;
        self.install(outcome.document, target);
        self.view.first = first.min(self.view.rows.len().saturating_sub(1));
    }

    fn target_block(&self) -> Option<usize> {
        self.editing_block().or_else(|| self.view.rows.get(self.view.selected).map(|r| r.block))
    }

    /// 편집 중 글을 확정하고 편집을 끝낸다(접기 클릭 등 줄이 사라질 수 있는 동작 전에). 반영 실패면 false.
    pub fn finish_edit(&mut self) -> bool {
        self.recorded(|ed| ed.finish_edit_inner())
    }

    pub fn set_property(&mut self, id: u64, key: &str, value: &str) -> Result<(), String> {
        if !self.finish_edit() { return Err("편집을 확정하지 못했습니다".into()); }
        self.recorded(|ed| {
            outliner_core::references::set_property(&mut ed.view.doc, id, key, value).map_err(|e| e.to_string())?;
            ed.view.rebuild();
            Ok(())
        })
    }

    fn finish_edit_inner(&mut self) -> bool {
        if self.session.is_none() {
            return true;
        }
        if self.host.composing() {
            self.host.request_complete();
            if self.host.composing() {
                self.message = Some("한글 조합을 확정한 뒤 다시 실행해 주세요".into());
                return false;
            }
        }
        if !self.commit_text() {
            return false;
        }
        self.session = None;
        self.popup = None;
        true
    }

    pub fn focus(&mut self, root: Option<u64>) -> bool {
        self.finish_edit() && self.view.focus(root)
    }

    /// 드롭과 확정할 글을 한 이력 단계에 기록한다. 거절되면 입력·문서·이력 유지.
    pub fn drop_subtree(&mut self, source: u64, target: u64, place: outliner_core::DropPlace, page: usize) -> bool {
        self.recorded(|ed| {
            if ed.host.composing() {
                ed.host.request_complete();
                if ed.host.composing() { ed.fail("한글 조합 확정 대기 중입니다".into()); return false; }
            }
            let mut doc = ed.view.doc.clone();
            if let Some(i) = ed.editing_block() {
                let text = ed.typed_text();
                let id = doc.blocks[i].id;
                if let Err(e) = outliner_core::edit(&mut doc, "set_text", id, Some(&text)) { ed.fail(e.to_string()); return false; }
            }
            match outliner_core::move_subtree(&mut doc, source, target, place) {
                Ok(changed) => {
                    if !changed { return false; }
                    let keep = doc.blocks.iter().position(|b| b.id == source).unwrap();
                    ed.session = None;
                    ed.popup = None;
                    ed.install(doc, keep);
                    ed.view.reveal_block(keep, page);
                    ed.message = Some("하위 블록과 함께 옮김 · Ctrl+Z로 실행 취소".into());
                    true
                }
                Err(e) => { ed.fail(e.to_string()); false }
            }
        })
    }

    /// 편집 중인 줄과 상관없이 `block` 의 TODO 상태를 토글한다(체크 칸 클릭). 편집 중이면 글을 먼저 확정하고 편집은 유지.
    pub fn toggle_block(&mut self, block: usize) {
        self.recorded(|ed| ed.toggle_block_inner(block));
    }

    fn toggle_block_inner(&mut self, block: usize) {
        self.message = None;
        if self.session.is_some() && !self.commit_text() {
            return;
        }
        let mut tx = Tx::new(&self.view.doc);
        match tx.act("toggle_status", block, None).and_then(|_| tx.finish()) {
            Ok(doc) => {
                let keep = self.editing_block().unwrap_or(block);
                self.install(doc, keep);
            }
            Err(e) => self.fail(e),
        }
    }

    /// 문서 맨 끝에 같은 단계의 빈 블록을 만들고 편집을 시작한다(Flutter 머리글 + 단추: `insert('')` 를 마지막 블록 뒤에).
    /// 편집 중 글은 먼저 확정한다. 접힌 조상이 있으면 펼쳐 새 줄이 보이게 한다.
    pub fn add_block_at_end(&mut self, page: usize) -> bool {
        self.recorded(|ed| ed.add_block_at_end_inner(page))
    }

    fn add_block_at_end_inner(&mut self, page: usize) -> bool {
        self.message = None;
        if self.session.is_some() && !self.commit_text() {
            return false;
        }
        let Some(last) = self.view.doc.blocks.len().checked_sub(1) else {
            self.fail("블록이 없어 추가할 수 없습니다".into());
            return false;
        };
        let mut tx = Tx::new(&self.view.doc);
        match tx.insert_after(last, "").and_then(|_| tx.finish()) {
            Ok(doc) => {
                self.install(doc, last + 1);
                self.view.reveal_block(last + 1, page);
                self.begin(last + 1, Some(0));
                true
            }
            Err(e) => {
                self.fail(format!("블록을 추가하지 못했습니다: {e}"));
                false
            }
        }
    }

    /// 편집 중이 아닐 때의 목록 명령(들여쓰기·내어쓰기)도 같은 경로로 처리한다.
    pub fn nav_command(&mut self, cmd: Cmd) {
        self.submit(cmd, 0, false, false);
    }

    /// 선택 줄의 블록 번호(문서 순서).
    fn selected_blocks(&self) -> Vec<usize> {
        let ids = self.view.selected_block_ids();
        (0..self.view.doc.blocks.len()).filter(|&i| ids.contains(&self.view.doc.blocks[i].id)).collect()
    }

    /// 선택된 블록(하위 포함)에 한 명령을 적용하고 전체를 하나의 이력 단계로 기록한다.
    /// `action` 은 core 동작 이름이다. 복사·잘라내기면 클립보드 글을 돌려준다.
    pub fn selection_command(&mut self, action: &str) -> Option<String> {
        self.selection_run(action, None)
    }

    pub fn selection_paste(&mut self, markdown: &str) {
        self.selection_run("paste", Some(markdown.to_string()));
    }

    fn selection_run(&mut self, action: &str, paste: Option<String>) -> Option<String> {
        if self.editing() {
            return None;
        }
        let selection = self.selected_blocks();
        if selection.is_empty() {
            return None;
        }
        self.message = None;
        self.recorded(|ed| {
            let had_range = ed.view.selection_anchor.is_some();
            let forward = ed.view.selection_anchor <= ed.view.selection_end;
            let keep = selection[0];
            let input = KeyInput { action: action.to_string(), selection, paste, ..KeyInput::default() };
            let outcome = match apply_key(&ed.view.doc, &input) {
                Ok(outcome) => outcome,
                Err(e) => {
                    ed.fail(e.to_string());
                    return None;
                }
            };
            ed.message = outcome.note.clone();
            let clipboard = outcome.clipboard.clone();
            let chosen = outcome.selection.clone();
            let target = chosen.first().copied().or(outcome.cursor.map(|c| c.block)).unwrap_or(keep);
            ed.adopt(outcome, target);
            let rows: Vec<usize> = chosen.iter().filter_map(|&b| ed.view.row_of_block(b)).collect();
            ed.view.clear_selection();
            if let (true, Some(&lo), Some(&hi)) = (had_range, rows.iter().min(), rows.iter().max()) {
                let (anchor, end) = if forward { (lo, hi) } else { (hi, lo) };
                ed.view.selection_anchor = Some(anchor);
                ed.view.selection_end = Some(end);
                ed.view.selected = end;
            }
            clipboard
        })
    }

    // ---------- 실행 취소 / 다시 실행 ----------

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 한 단계 되돌리거나(`undo`) 다시 실행한다. 처리했으면 true(키를 삼킨다).
    /// 편집 중 확정하지 않은 글이 있거나 한글 조합 중이면 false — 호출한 쪽이 EDIT 자체의 글자 되돌리기에 넘긴다.
    /// 되돌릴 단계가 없어도 true(문서 단위 키이므로 EDIT 의 글자 단위로 새지 않게 한다).
    pub fn history_step(&mut self, undo: bool) -> bool {
        self.message = None;
        let editing = self.editing_block();
        if let Some(block) = editing {
            let committed = self.view.doc.blocks.get(block).map(|b| b.text.clone()).unwrap_or_default();
            if self.host.composing() || self.typed_text() != committed {
                return false;
            }
        }
        let current = outliner_core::serialize(&self.view.doc);
        let target = if undo { self.history.undo(&current) } else { self.history.redo(&current) };
        let Some(markdown) = target else {
            self.message = Some(if undo { "되돌릴 편집이 없습니다" } else { "다시 실행할 편집이 없습니다" }.into());
            return true;
        };
        match docio::parse_doc(&markdown) {
            Ok(doc) => {
                let keep = editing.or_else(|| self.view.rows.get(self.view.selected).map(|r| r.block)).unwrap_or(0);
                // 스냅샷은 Markdown이라 ID가 재배정된다. 다른 블록으로 줌하는 것을 막는다.
                self.view.clear_zoom();
                self.install(doc, keep);
                self.popup = None;
                self.popup_off = None;
                if let Some(block) = editing {
                    self.session = None;
                    // 되돌린 문서에 그 블록이 없으면(분할·추가를 되돌림) 마지막 블록으로 옮겨 편집을 이어 간다.
                    if let Some(last) = self.view.doc.blocks.len().checked_sub(1) {
                        self.begin(block.min(last), None);
                    }
                }
                self.message = Some(if undo { "실행 취소" } else { "다시 실행" }.into());
            }
            Err(e) => {
                self.history.clear();
                self.fail(format!("되돌리지 못했습니다(이력을 비움): {e}"));
            }
        }
        true
    }

    // ---------- 저장 ----------

    /// 열어 둔 파일에 저장(저장 직전 SHA-256 비교). 성공하면 true.
    pub fn save(&mut self) -> bool {
        let md = outliner_core::serialize(&self.view.doc);
        let Some(file) = self.file.as_mut() else {
            self.message = Some("저장할 파일이 없습니다".into());
            return false;
        };
        match docio::save_checked(file, &md) {
            Ok(()) => {
                self.baseline = md;
                self.conflict = false;
                self.message = Some("저장했습니다".into());
                true
            }
            Err(SaveError::Conflict(m)) => {
                self.history.clear(); // 파일이 다른 곳에서 바뀜: 이력의 기준이 어긋났다
                self.conflict = true;
                self.message = Some(format!("⚠ {m}"));
                self.alert = Some(m);
                false
            }
            Err(SaveError::Other(m)) => {
                self.message = Some(format!("저장하지 못했습니다: {m}"));
                self.alert = Some(m);
                false
            }
        }
    }

    /// 창 닫기 전: 편집 중 글을 확정(조합 중이면 먼저 확정)한다. 반영에 실패하면 false.
    pub fn flush_for_close(&mut self) -> bool {
        self.recorded(|ed| ed.flush_for_close_inner())
    }

    fn flush_for_close_inner(&mut self) -> bool {
        if self.session.is_none() {
            return true;
        }
        if self.host.composing() {
            self.host.request_complete();
        }
        self.commit_text()
    }
}
