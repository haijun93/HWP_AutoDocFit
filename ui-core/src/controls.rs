//! 단추·체크·라디오·탭·드롭다운(순수 상태 기계) + GDI 그리기 도움.
//!
//! 단추 누름 규칙(Windows 캡션 단추와 같음, popup·kvf에서 옮김): 누른 단추 위에서 뗄 때만 동작.
//! | 사건 | 결과 |
//! |---|---|
//! | 누름(b) | pressed = b |
//! | 뗌(b2) | pressed == b2면 Some(b2)(실행), 아니면 None. pressed는 비움 |
//! | 벗어남 | hover·pressed 비움 |

use windows_sys::Win32::Graphics::Gdi::{DT_CENTER, DT_END_ELLIPSIS, DT_LEFT};

use crate::geom::Rect;
use crate::paint::Painter;
use crate::theme::AppColors;

/// 마우스 올림·누름 추적(단추 id 종류는 무엇이든).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press<T: Copy + PartialEq> {
    pub hover: Option<T>,
    pub pressed: Option<T>,
}

impl<T: Copy + PartialEq> Default for Press<T> {
    fn default() -> Self {
        Self { hover: None, pressed: None }
    }
}

impl<T: Copy + PartialEq> Press<T> {
    /// 마우스 이동: 바뀌었으면 true(다시 그리기).
    pub fn hover(&mut self, at: Option<T>) -> bool {
        let changed = self.hover != at;
        self.hover = at;
        changed
    }
    pub fn down(&mut self, at: Option<T>) {
        self.pressed = at;
    }
    /// 뗌: 누른 것과 같은 단추면 실행할 단추.
    pub fn up(&mut self, at: Option<T>) -> Option<T> {
        let p = self.pressed.take();
        at.filter(|b| Some(*b) == p)
    }
    pub fn leave(&mut self) {
        self.hover = None;
        self.pressed = None;
    }
}

/// 단추 그리기 상태.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Visual {
    pub hover: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
    /// 켜짐(토글 단추·선택된 탭).
    pub on: bool,
}

/// 모양 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// 기본 단추(강조 바탕).
    Primary,
    /// 보조 단추(테두리).
    Secondary,
    /// 글자만(머리글 단추 등).
    Flat,
}

/// 단추 색 결정(순수 함수): (바탕, 글자, 테두리). 바탕 None = 칠하지 않음.
pub fn button_colors(kind: ButtonKind, v: Visual, c: &AppColors) -> (Option<u32>, u32, Option<u32>) {
    if v.disabled {
        return (Some(c.surface_subtle.bgr()), c.faint.bgr(), Some(c.line.bgr()));
    }
    let border_focus = if v.focused { Some(c.accent.bgr()) } else { None };
    match kind {
        ButtonKind::Primary => {
            let bg = if v.pressed { c.accent_deep } else { c.accent_fill };
            (Some(bg.bgr()), c.on_accent.bgr(), border_focus.or(Some(bg.bgr())))
        }
        ButtonKind::Secondary => {
            let bg = if v.pressed { c.line } else if v.hover { c.surface_subtle } else { c.surface };
            (Some(bg.bgr()), c.text.bgr(), border_focus.or(Some(c.line_strong.bgr())))
        }
        ButtonKind::Flat => {
            let bg = if v.pressed { Some(c.line.bgr()) } else if v.hover || v.on { Some(c.surface_subtle.bgr()) } else { None };
            (bg, if v.on { c.accent.bgr() } else { c.text_body.bgr() }, border_focus)
        }
    }
}

/// 단추 하나 그리기.
pub fn draw_button(p: &Painter, r: Rect, label: &str, kind: ButtonKind, v: Visual, c: &AppColors) {
    let (bg, fg, border) = button_colors(kind, v, c);
    if let Some(bg) = bg {
        p.fill_rect(r, bg);
    }
    if let Some(b) = border {
        p.frame(r.win(), b);
    }
    p.text_color(fg);
    p.text_rect(label, r, DT_CENTER | DT_END_ELLIPSIS);
}

/// 체크 상자(□/☑) + 글자.
pub fn draw_check(p: &Painter, r: Rect, label: &str, checked: bool, v: Visual, c: &AppColors, box_px: i32) {
    let y = r.top + (r.height() - box_px) / 2;
    let b = Rect::xywh(r.left, y, box_px, box_px);
    p.fill_rect(b, if checked { c.accent_fill.bgr() } else { c.surface.bgr() });
    p.frame(b.win(), if v.focused { c.accent.bgr() } else if checked { c.accent_fill.bgr() } else { c.line_strong.bgr() });
    if checked {
        p.text_color(c.on_accent.bgr());
        p.text_rect("✓", b, DT_CENTER);
    }
    p.text_color(if v.disabled { c.faint.bgr() } else { c.text.bgr() });
    p.text_rect(label, Rect::new(b.right + box_px / 2, r.top, r.right, r.bottom), DT_LEFT | DT_END_ELLIPSIS);
}

// ---- 탭(분류 메뉴바, popup에서 옮김) ----

/// 탭 배치(순수 함수). long·short: 각 탭 글자 폭(긴 이름·짧은 이름), avail: 쓸 수 있는 폭.
/// 돌려주는 값: 각 탭의 (왼쪽, 오른쪽)(0부터), 짧은 이름을 쓰는지.
/// 순서: ① 기본 여백 ② 최소 여백 ③ 짧은 이름+최소 여백 ④ 같은 폭으로 나눔(글자는 말줄임).
pub fn tab_layout(long: &[i32], short: &[i32], avail: i32, pad: i32, min_pad: i32) -> (Vec<(i32, i32)>, bool) {
    let lay = |widths: &[i32], p: i32| -> Option<Vec<(i32, i32)>> {
        let total: i32 = widths.iter().map(|w| w + 2 * p).sum();
        if total > avail {
            return None;
        }
        let mut x = 0;
        Some(widths.iter().map(|w| { let l = x; x += w + 2 * p; (l, x) }).collect())
    };
    if let Some(v) = lay(long, pad) { return (v, false); }
    if let Some(v) = lay(long, min_pad) { return (v, false); }
    if let Some(v) = lay(short, min_pad) { return (v, true); }
    let n = long.len().max(1) as i32;
    let avail = avail.max(0);
    let spans = (0..n).map(|i| (avail * i / n, avail * (i + 1) / n)).collect();
    (spans, true)
}

/// x가 어느 탭 위인지.
pub fn tab_at(x: i32, spans: &[(i32, i32)]) -> Option<usize> {
    spans.iter().position(|&(l, r)| x >= l && x < r)
}

/// delta만큼 옮긴 위치(양 끝에서 돌아간다).
pub fn wrap_index(cur: usize, delta: isize, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    (cur as isize + delta).rem_euclid(n as isize) as usize
}

// ---- 라디오·드롭다운 ----

/// 하나만 고르는 묶음(라디오·드롭다운 공통). 사용 안 함 항목은 건너뛴다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub options: Vec<String>,
    pub enabled: Vec<bool>,
    pub selected: usize,
}

impl Choice {
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        let n = options.len();
        Self { options, enabled: vec![true; n], selected: selected.min(n.saturating_sub(1)) }
    }
    /// 다음·이전 사용 가능 항목으로(끝에서 멈춤). 바뀌면 true.
    pub fn step(&mut self, delta: isize) -> bool {
        let n = self.options.len() as isize;
        let mut i = self.selected as isize;
        loop {
            i += delta.signum();
            if i < 0 || i >= n {
                return false;
            }
            if self.enabled[i as usize] {
                self.selected = i as usize;
                return true;
            }
        }
    }
    pub fn select(&mut self, i: usize) -> bool {
        if i < self.options.len() && self.enabled[i] && i != self.selected {
            self.selected = i;
            return true;
        }
        false
    }
    pub fn label(&self) -> &str {
        self.options.get(self.selected).map(String::as_str).unwrap_or("")
    }
}

/// 드롭다운 상태: 닫힘/열림(강조 위치).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dropdown {
    pub choice: Choice,
    pub open: Option<usize>,
}

/// 드롭다운 사건.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropKey {
    Toggle,
    Up,
    Down,
    Enter,
    Escape,
    Click(usize),
}

impl Dropdown {
    pub fn new(choice: Choice) -> Self {
        Self { choice, open: None }
    }
    /// 바뀌어 확정되면 true(값 변경 알림).
    pub fn key(&mut self, k: DropKey) -> bool {
        match (self.open, k) {
            (None, DropKey::Toggle | DropKey::Enter) => {
                self.open = Some(self.choice.selected);
                false
            }
            (None, DropKey::Up) => self.choice.step(-1),
            (None, DropKey::Down) => self.choice.step(1),
            (None, _) => false,
            (Some(h), DropKey::Up | DropKey::Down) => {
                let mut probe = Choice { selected: h, ..self.choice.clone() };
                probe.step(if k == DropKey::Up { -1 } else { 1 });
                self.open = Some(probe.selected);
                false
            }
            (Some(h), DropKey::Enter) => {
                self.open = None;
                self.choice.select(h)
            }
            (Some(_), DropKey::Click(i)) => {
                self.open = None;
                self.choice.select(i)
            }
            (Some(_), DropKey::Escape | DropKey::Toggle) => {
                self.open = None;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_runs_only_when_released_on_same_button() {
        let mut p: Press<u8> = Press::default();
        assert!(p.hover(Some(1)));
        assert!(!p.hover(Some(1)));
        p.down(Some(1));
        assert_eq!(p.up(Some(1)), Some(1));
        p.down(Some(1));
        assert_eq!(p.up(Some(2)), None);
        assert_eq!(p.pressed, None);
        p.down(None);
        assert_eq!(p.up(Some(2)), None);
        p.down(Some(3));
        p.leave();
        assert_eq!(p.up(Some(3)), None);
    }

    #[test]
    fn tab_layout_shrinks_padding_then_short_labels_then_equal_share() {
        let long = [24, 24, 24, 24, 24, 12, 48, 60];
        let short = [24, 24, 24, 24, 24, 12, 24, 24];
        let (s, short_used) = tab_layout(&long, &short, 600, 9, 4);
        assert!(!short_used);
        assert_eq!(s[0], (0, 42));
        assert_eq!(s[7].1, 240 + 8 * 18);
        let (s, short_used) = tab_layout(&long, &short, 320, 9, 4);
        assert!(!short_used);
        assert_eq!(s[7].1, 304);
        let (s, short_used) = tab_layout(&long, &short, 260, 9, 4);
        assert!(short_used);
        assert_eq!(s[7].1, 244);
        let (s, short_used) = tab_layout(&long, &short, 200, 9, 4);
        assert!(short_used);
        assert_eq!((s[0], s[7].1), ((0, 25), 200));
        let (s, _) = tab_layout(&long, &short, -5, 9, 4);
        assert!(s.iter().all(|(l, r)| *l == 0 && *r == 0));
        assert_eq!(tab_at(48, &[(6, 48), (48, 90)]), Some(1));
        assert_eq!(tab_at(5, &[(6, 48)]), None);
        assert_eq!(wrap_index(0, -1, 8), 7);
        assert_eq!(wrap_index(7, 1, 8), 0);
        assert_eq!(wrap_index(3, 1, 0), 0);
    }

    #[test]
    fn choice_skips_disabled_and_stops_at_ends() {
        let mut c = Choice::new(vec!["가".into(), "나".into(), "다".into()], 0);
        c.enabled[1] = false;
        assert!(c.step(1));
        assert_eq!(c.selected, 2);
        assert!(!c.step(1));
        assert!(c.step(-1));
        assert_eq!(c.label(), "가");
        assert!(!c.select(1));
        assert!(!c.select(9));
    }

    #[test]
    fn dropdown_open_navigate_commit_or_cancel() {
        let mut d = Dropdown::new(Choice::new(vec!["5".into(), "10".into(), "20".into()], 0));
        assert!(!d.key(DropKey::Toggle));
        assert_eq!(d.open, Some(0));
        d.key(DropKey::Down);
        d.key(DropKey::Down);
        assert_eq!(d.open, Some(2));
        assert!(!d.key(DropKey::Escape));
        assert_eq!(d.choice.selected, 0, "Esc는 바꾸지 않음");
        d.key(DropKey::Enter);
        d.key(DropKey::Down);
        assert!(d.key(DropKey::Enter));
        assert_eq!(d.choice.selected, 1);
        d.key(DropKey::Toggle);
        assert!(d.key(DropKey::Click(2)));
        assert_eq!(d.open, None);
        assert!(d.key(DropKey::Up), "닫힌 채 ↑는 바로 바꿈");
        assert_eq!(d.choice.label(), "10");
    }

    #[test]
    fn button_colors_follow_state() {
        let c = AppColors::LIGHT;
        let (bg, fg, _) = button_colors(ButtonKind::Primary, Visual::default(), &c);
        assert_eq!((bg, fg), (Some(c.accent_fill.bgr()), c.on_accent.bgr()));
        let (bg, _, _) = button_colors(ButtonKind::Primary, Visual { pressed: true, ..Default::default() }, &c);
        assert_eq!(bg, Some(c.accent_deep.bgr()));
        let (_, fg, _) = button_colors(ButtonKind::Secondary, Visual { disabled: true, ..Default::default() }, &c);
        assert_eq!(fg, c.faint.bgr());
        let (_, _, border) = button_colors(ButtonKind::Flat, Visual { focused: true, ..Default::default() }, &c);
        assert_eq!(border, Some(c.accent.bgr()));
        assert_eq!(button_colors(ButtonKind::Flat, Visual::default(), &c).0, None);
    }

    #[test]
    fn draw_button_and_check_paint_pixels() {
        let s = crate::paint::Offscreen::new(120, 30).unwrap();
        let p = s.painter();
        p.transparent();
        let c = AppColors::DARK;
        p.fill_rect(Rect::xywh(0, 0, 120, 30), c.canvas.bgr());
        draw_button(&p, Rect::xywh(0, 0, 60, 30), "확인", ButtonKind::Primary, Visual::default(), &c);
        assert_eq!(s.pixel(1, 1), c.accent_fill.bgr());
        draw_check(&p, Rect::xywh(60, 0, 60, 30), "켜기", true, Visual::default(), &c, 14);
        assert_eq!(s.pixel(62, 10), c.accent_fill.bgr());
    }
}
