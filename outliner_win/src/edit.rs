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
use crate::view::{View, build_rows};
use outliner_core::Document;
use std::collections::VecDeque;

pub const VK_BACK: u16 = 0x08;
pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0D;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_UP: u16 = 0x26;
pub const VK_DOWN: u16 = 0x28;
pub const VK_S: u16 = 0x53;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Enter,
    Indent,
    Outdent,
    Up,
    Down,
    Backspace,
    /// 다른 블록(문서 안 순서 번호)으로 편집 이동.
    Goto(usize),
    Save,
    Cancel,
    /// TODO 토글(Ctrl+Enter).
    Toggle,
    /// 블록(하위 포함)을 같은 단계의 위/아래 형제 위로 옮긴다(Alt+↑/↓).
    MoveUp,
    MoveDown,
}

/// `[[` 자동완성 팝업 상태(편집 중에만).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Popup {
    pub ctx: LinkCtx,
    pub items: Vec<String>,
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
    /// 열려 있는 `[[` 자동완성 팝업.
    pub popup: Option<Popup>,
    /// Esc 로 닫은 `[[` 의 위치 — 같은 `[[` 에서는 다시 열지 않는다.
    popup_off: Option<usize>,
}

/// 한 줄 규칙: 줄바꿈·탭은 공백으로.
pub fn sanitize(s: &str) -> String {
    s.chars().map(|c| if matches!(c, '\r' | '\n' | '\t') { ' ' } else { c }).collect()
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
        self.doc = docio::edit_doc(&self.doc, action, id, text)?;
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
    fn delete(&mut self, idx: usize) -> Result<(), String> {
        self.act("delete", idx, None)?;
        self.folded.remove(idx);
        Ok(())
    }
    /// `idx` 블록에 바로 다음 블록을 합친다.
    fn merge_next(&mut self, idx: usize) -> Result<(), String> {
        self.act("merge", idx, None)?;
        self.folded.remove(idx + 1);
        Ok(())
    }
    fn subtree_end(&self, idx: usize) -> usize {
        let ind = self.doc.blocks[idx].indent;
        let mut e = idx + 1;
        while e < self.doc.blocks.len() && self.doc.blocks[e].indent > ind {
            e += 1;
        }
        e
    }
    /// 블록 `i` 와 바로 위 블록을 맞바꾼다(엔진 `up`).
    fn swap_up(&mut self, i: usize) -> Result<(), String> {
        self.act("up", i, None)?;
        self.folded.swap(i, i - 1);
        Ok(())
    }
    /// 구간 `[start, end)` 를 위치 `to`(< start) 로 끌어올린다(인접 교환 반복).
    fn move_range_up(&mut self, start: usize, end: usize, to: usize) -> Result<(), String> {
        if (end - start) * (start - to) > 2000 {
            return Err("한 번에 옮기기에는 블록이 너무 많습니다".into());
        }
        for k in 0..(end - start) {
            let cur = start + k;
            for step in 0..(start - to) {
                self.swap_up(cur - step)?;
            }
        }
        Ok(())
    }
    /// `idx` 블록과 그 하위를 같은 단계의 위/아래 형제 건너편으로 옮기고 옮긴 블록의 새 번호를 돌려준다.
    fn move_subtree(&mut self, idx: usize, up: bool) -> Result<usize, String> {
        let len = self.doc.blocks.len();
        if idx >= len {
            return Err("블록이 없습니다".into());
        }
        let ind = self.doc.blocks[idx].indent;
        let end = self.subtree_end(idx);
        if up {
            let mut p = idx;
            loop {
                if p == 0 {
                    return Err("위로 이동할 블록이 없습니다".into());
                }
                p -= 1;
                if self.doc.blocks[p].indent <= ind {
                    break;
                }
            }
            if self.doc.blocks[p].indent < ind {
                return Err("같은 단계의 위쪽 블록이 없습니다".into());
            }
            self.move_range_up(idx, end, p)?;
            Ok(p)
        } else {
            if end >= len || self.doc.blocks[end].indent != ind {
                return Err("같은 단계의 아래쪽 블록이 없습니다".into());
            }
            let nend = self.subtree_end(end);
            self.move_range_up(end, nend, idx)?;
            Ok(idx + (nend - end))
        }
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
            popup: None,
            popup_off: None,
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

    fn cmd_of(ev: &KeyEvent) -> Option<(Cmd, fn(&ImePolicy) -> ComposingKey)> {
        if ev.ctrl || ev.alt {
            return None;
        }
        match ev.vk {
            VK_RETURN if !ev.shift => Some((Cmd::Enter, |p| p.enter)),
            VK_TAB => Some((if ev.shift { Cmd::Outdent } else { Cmd::Indent }, |p| p.tab)),
            VK_UP if !ev.shift => Some((Cmd::Up, |p| p.arrows)),
            VK_DOWN if !ev.shift => Some((Cmd::Down, |p| p.arrows)),
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
        // 자동완성 팝업이 열려 있으면 ↑↓·Enter·Tab·Esc 는 팝업이 먼저 받는다(조합 중이면 확정만 하고 명령은 버림).
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
                        return if ev.processkey { KeyResult::Pass } else { KeyResult::Swallow };
                    }
                    if ev.processkey {
                        return KeyResult::Pass;
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
        if ev.vk == VK_RETURN && ev.ctrl && !ev.shift && !ev.alt {
            self.submit(Cmd::Toggle, ev.now_ms, ev.processkey, composing);
            return KeyResult::Swallow;
        }
        if ev.alt && !ev.ctrl && !ev.shift && (ev.vk == VK_UP || ev.vk == VK_DOWN) {
            if candidate {
                return KeyResult::Pass;
            }
            self.submit(if ev.vk == VK_UP { Cmd::MoveUp } else { Cmd::MoveDown }, ev.now_ms, ev.processkey, composing);
            return KeyResult::Swallow;
        }
        if ev.vk == VK_RETURN && (ev.shift || ev.ctrl) && !ev.alt {
            return KeyResult::Swallow; // 한 줄 규칙: 줄바꿈 입력 없음
        }
        match ev.vk {
            VK_ESCAPE if !ev.alt && !ev.ctrl && !ev.shift => {
                if composing || candidate {
                    return KeyResult::Pass; // 조합 취소는 IME 소유
                }
                self.submit(Cmd::Cancel, ev.now_ms, ev.processkey, false);
                return KeyResult::Swallow;
            }
            VK_BACK if !ev.alt && !ev.ctrl => {
                if composing || candidate || ev.processkey {
                    return KeyResult::Pass;
                }
                let (s, e) = self.host.selection();
                let empty = self.host.read_text().is_empty();
                if s == e && (empty || s == 0) {
                    self.submit(Cmd::Backspace, ev.now_ms, false, false);
                    return KeyResult::Swallow;
                }
                return KeyResult::Pass;
            }
            _ => {}
        }
        let Some((cmd, pick)) = Self::cmd_of(&ev) else { return KeyResult::Pass };
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

    // ---------- `[[` 자동완성 ----------

    /// 글·캐럿이 바뀐 뒤 부른다: 캐럿이 `[[` 뒤면 팝업을 열거나 후보를 갱신하고, 아니면 닫는다.
    pub fn refresh_popup(&mut self) {
        if self.session.is_none() {
            self.popup = None;
            return;
        }
        let text = self.host.read_text();
        let (s, e) = self.host.selection();
        let ctx = if s == e { links::link_context(&text, s) } else { None };
        let Some(ctx) = ctx else {
            self.popup = None;
            self.popup_off = None;
            return;
        };
        if self.popup_off == Some(ctx.start16) {
            self.popup = None;
            return;
        }
        self.popup_off = None;
        let items = links::candidates(&self.page_names, &self.recent_names, &ctx.query, 8);
        if items.is_empty() {
            self.popup = None;
            return;
        }
        let selected = match &self.popup {
            Some(p) if p.ctx.query == ctx.query && p.ctx.start16 == ctx.start16 => p.selected.min(items.len() - 1),
            _ => 0,
        };
        self.popup = Some(Popup { ctx, items, selected });
    }

    pub fn move_popup(&mut self, down: bool) {
        if let Some(p) = self.popup.as_mut() {
            let n = p.items.len();
            p.selected = if down { (p.selected + 1) % n } else { (p.selected + n - 1) % n };
        }
    }

    pub fn dismiss_popup(&mut self) {
        if let Some(p) = self.popup.take() {
            self.popup_off = Some(p.ctx.start16);
        }
    }

    /// 후보(번호 없으면 선택된 것)로 `[[질의` 를 `[[이름]]` 으로 바꾸고 캐럿을 `]]` 뒤에 둔다.
    pub fn accept_popup(&mut self, index: Option<usize>) -> bool {
        let Some(p) = self.popup.take() else { return false };
        let Some(name) = p.items.get(index.unwrap_or(p.selected)).cloned() else { return false };
        let text = self.host.read_text();
        // 팝업이 열린 뒤 글이 바뀌었을 수 있어 문맥을 다시 구한다.
        let (s, _) = self.host.selection();
        let ctx = links::link_context(&text, s).unwrap_or(p.ctx);
        let (new, caret) = links::apply_completion(&text, &ctx, &name);
        self.host.set_text(&new, caret);
        self.popup_off = None;
        self.refresh_popup();
        true
    }

    // ---------- 명령 실행 ----------

    fn typed_text(&mut self) -> String {
        sanitize(&self.host.read_text())
    }

    fn install(&mut self, doc: Document, target_block: usize) {
        self.view.doc = doc;
        self.view.rows = build_rows(&self.view.doc);
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

    fn execute(&mut self, cmd: Cmd) {
        self.message = None;
        if let Cmd::Cancel = cmd {
            self.popup = None;
            self.session = None; // 문서는 마지막 확정 상태 그대로 — 확정되지 않은 입력만 버린다
            return;
        }
        let editing = self.session.is_some();
        let (sel_start, sel_end) = if editing { self.host.selection() } else { (0, 0) };
        // 분할·삭제·합치기는 글 반영을 자기 트랜잭션 안에서 한다. 나머지는 먼저 글을 반영한다.
        if !matches!(cmd, Cmd::Enter | Cmd::Backspace) && editing && !self.commit_text() {
            return;
        }
        match cmd {
            Cmd::Cancel => {}
            Cmd::Toggle => {
                let Some(block) = self.target_block() else { return };
                let mut tx = Tx::new(&self.view.doc);
                match tx.act("toggle_status", block, None).and_then(|_| tx.finish()) {
                    Ok(doc) => self.install(doc, block),
                    Err(e) => self.fail(e),
                }
            }
            Cmd::MoveUp | Cmd::MoveDown => {
                let Some(block) = self.target_block() else { return };
                let mut tx = Tx::new(&self.view.doc);
                let moved = tx.move_subtree(block, cmd == Cmd::MoveUp).and_then(|nb| tx.finish().map(|d| (d, nb)));
                match moved {
                    Ok((doc, nb)) => {
                        self.install(doc, nb);
                        if editing {
                            self.begin(nb, Some(sel_start));
                        }
                    }
                    Err(e) => self.fail(e),
                }
            }
            Cmd::Save => {
                self.save();
            }
            Cmd::Goto(block) => {
                if block < self.view.doc.blocks.len() {
                    self.begin(block, None);
                }
            }
            Cmd::Up | Cmd::Down => {
                let cur = self.editing_block();
                if let Some(pos) = self.view.rows.iter().position(|r| Some(r.block) == cur) {
                    let to = if cmd == Cmd::Up { pos.checked_sub(1) } else { Some(pos + 1) };
                    if let Some(row) = to.and_then(|t| self.view.rows.get(t)) {
                        let b = row.block;
                        self.begin(b, None);
                    }
                }
            }
            Cmd::Indent | Cmd::Outdent => {
                let Some(block) = self.target_block() else { return };
                let mut tx = Tx::new(&self.view.doc);
                let r = tx.act(if cmd == Cmd::Indent { "indent" } else { "outdent" }, block, None).and_then(|_| tx.finish());
                match r {
                    Ok(doc) => self.install(doc, block),
                    Err(e) => self.fail(e),
                }
            }
            Cmd::Enter => {
                let Some(block) = self.session.as_ref().map(|s| s.block) else { return };
                let text = self.typed_text();
                let (left, right) = split_utf16(&text, sel_start, sel_end);
                // 엔진에 "커서 위치 분할"이 없어 set_text(앞) + insert(뒤) 로 구성한다.
                let mut tx = Tx::new(&self.view.doc);
                let r = tx.set_text(block, &left).and_then(|_| tx.insert_after(block, &right)).and_then(|_| tx.finish());
                match r {
                    Ok(doc) => {
                        self.install(doc, block + 1);
                        self.begin(block + 1, Some(0));
                    }
                    Err(e) => self.fail(format!("분할하지 못했습니다(입력은 그대로 둠): {e}")),
                }
            }
            Cmd::Backspace => {
                let Some(block) = self.session.as_ref().map(|s| s.block) else { return };
                let text = self.typed_text();
                let pos = self.view.rows.iter().position(|r| r.block == block).unwrap_or(0);
                if text.is_empty() {
                    if self.view.doc.blocks.len() <= 1 {
                        return; // 마지막 한 블록은 지우지 않는다
                    }
                    let target = if pos > 0 { self.view.rows[pos - 1].block } else { block };
                    let mut tx = Tx::new(&self.view.doc);
                    match tx.delete(block).and_then(|_| tx.finish()) {
                        Ok(doc) => {
                            self.install(doc, target);
                            self.begin(target, None);
                        }
                        Err(e) => self.fail(e),
                    }
                } else {
                    // 맨 앞 Backspace: 바로 앞 블록과 합치기(엔진 merge: 두 글이 있으면 사이에 공백 1개).
                    let prev = match pos.checked_sub(1).map(|p| self.view.rows[p].block) {
                        Some(p) if p + 1 == block => p,
                        _ => {
                            self.fail("바로 앞 블록과 붙어 있지 않아 합칠 수 없습니다(접힌 하위 또는 보존 줄)".into());
                            return;
                        }
                    };
                    let prev_len = utf16_len(&self.view.doc.blocks[prev].text);
                    let gap = if prev_len > 0 { 1 } else { 0 };
                    let mut tx = Tx::new(&self.view.doc);
                    let r = tx.set_text(block, &text).and_then(|_| tx.merge_next(prev)).and_then(|_| tx.finish());
                    match r {
                        Ok(doc) => {
                            self.install(doc, prev);
                            self.begin(prev, Some(prev_len + gap));
                        }
                        Err(e) => self.fail(format!("합치지 못했습니다(입력은 그대로 둠): {e}")),
                    }
                }
            }
        }
    }

    fn target_block(&self) -> Option<usize> {
        self.editing_block().or_else(|| self.view.rows.get(self.view.selected).map(|r| r.block))
    }

    /// 편집 중 글을 확정하고 편집을 끝낸다(접기 클릭 등 줄이 사라질 수 있는 동작 전에). 반영 실패면 false.
    pub fn finish_edit(&mut self) -> bool {
        if self.session.is_none() {
            return true;
        }
        if self.host.composing() {
            self.host.request_complete();
        }
        if !self.commit_text() {
            return false;
        }
        self.session = None;
        self.popup = None;
        true
    }

    /// 편집 중인 줄과 상관없이 `block` 의 TODO 상태를 토글한다(체크 칸 클릭). 편집 중이면 글을 먼저 확정하고 편집은 유지.
    pub fn toggle_block(&mut self, block: usize) {
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

    /// 편집 중이 아닐 때의 목록 명령(들여쓰기·내어쓰기)도 같은 경로로 처리한다.
    pub fn nav_command(&mut self, cmd: Cmd) {
        self.submit(cmd, 0, false, false);
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
        if self.session.is_none() {
            return true;
        }
        if self.host.composing() {
            self.host.request_complete();
        }
        self.commit_text()
    }
}
