//! 설정 폼(선언 → 자동 배치·키보드 이동). S5 서식 관리·S6 설정·S18 몽땅클립보드 관리창이 같은 부품을 쓴다.
//! `settings_menu.rs`의 "순수 모델 → 그리기 분리" 방식: 이 모듈은 값과 배치만, 그리기는 창이 한다.
//! 글·경로 칸은 OS EDIT 자식 컨트롤로 편집한다(Q14) — [`Field::needs_edit`].
//!
//! 키 전이표(포커스 칸 기준):
//! | 키 | 체크 | 선택 | 숫자 | 글·경로 |
//! |---|---|---|---|---|
//! | Space | 켜고 끄기 | 다음 | - | - |
//! | ← → | - | 이전·다음 | -step·+step(범위 안) | (EDIT가 처리) |
//! | Tab / Shift+Tab | 다음·이전 칸(사용 안 함 건너뜀) ||||
//! 값이 바뀌면 `Changed(key)`, 아니면 `Moved`·`None`.

use crate::geom::Rect;
use crate::layout::{Size, row, stack};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Check(bool),
    Choice { options: Vec<String>, selected: usize },
    Number { value: i64, min: i64, max: i64, step: i64 },
    Text(String),
    Path(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub key: &'static str,
    pub label: String,
    pub value: Value,
    pub enabled: bool,
    /// 칸 아래 설명(없으면 빈 글).
    pub help: String,
}

impl Field {
    pub fn new(key: &'static str, label: impl Into<String>, value: Value) -> Self {
        Self { key, label: label.into(), value, enabled: true, help: String::new() }
    }
    /// OS EDIT 자식 컨트롤이 필요한 칸.
    pub fn needs_edit(&self) -> bool {
        matches!(self.value, Value::Text(_) | Value::Path(_))
    }
    /// 저장·표시용 문자열.
    pub fn text(&self) -> String {
        match &self.value {
            Value::Check(b) => b.to_string(),
            Value::Choice { options, selected } => options.get(*selected).cloned().unwrap_or_default(),
            Value::Number { value, .. } => value.to_string(),
            Value::Text(s) | Value::Path(s) => s.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormKey {
    Space,
    Left,
    Right,
    Tab { back: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormEvent {
    Changed(&'static str),
    Moved(&'static str),
    None,
}

/// 칸 하나의 배치(이름 영역, 값 영역).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub label: Rect,
    pub control: Rect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Form {
    pub fields: Vec<Field>,
    pub focus: usize,
}

impl Form {
    pub fn new(fields: Vec<Field>) -> Self {
        let focus = fields.iter().position(|f| f.enabled).unwrap_or(0);
        Self { fields, focus }
    }
    pub fn get(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }
    pub fn set_text(&mut self, key: &str, s: String) -> bool {
        match self.fields.iter_mut().find(|f| f.key == key).map(|f| &mut f.value) {
            Some(Value::Text(v) | Value::Path(v)) if *v != s => {
                *v = s;
                true
            }
            _ => false,
        }
    }
    /// 이름 폭 label_w, 줄 높이 row_h, 줄 간격 gap으로 위에서부터 배치.
    pub fn layout(&self, area: Rect, label_w: i32, row_h: i32, gap: i32) -> Vec<Slot> {
        stack(area, self.fields.len(), row_h, gap)
            .into_iter()
            .map(|r| {
                let cells = row(r, &[Size::Px(label_w), Size::Fill(1)], gap);
                Slot { label: cells[0], control: cells[1] }
            })
            .collect()
    }
    /// 마우스 위치의 칸.
    pub fn hit(&self, slots: &[Slot], x: i32, y: i32) -> Option<usize> {
        slots.iter().position(|s| s.label.contains(x, y) || s.control.contains(x, y))
    }
    /// 칸 클릭: 포커스 옮기고, 체크면 바꾼다.
    pub fn click(&mut self, i: usize) -> FormEvent {
        let Some(f) = self.fields.get(i) else { return FormEvent::None };
        if !f.enabled {
            return FormEvent::None;
        }
        self.focus = i;
        if matches!(f.value, Value::Check(_)) { self.key(FormKey::Space) } else { FormEvent::Moved(f.key) }
    }
    pub fn key(&mut self, k: FormKey) -> FormEvent {
        if let FormKey::Tab { back } = k {
            let n = self.fields.len();
            for step in 1..=n {
                let i = if back { (self.focus + n * 2 - step) % n } else { (self.focus + step) % n };
                if self.fields[i].enabled {
                    self.focus = i;
                    return FormEvent::Moved(self.fields[i].key);
                }
            }
            return FormEvent::None;
        }
        let Some(f) = self.fields.get_mut(self.focus) else { return FormEvent::None };
        if !f.enabled {
            return FormEvent::None;
        }
        let changed = match (&mut f.value, k) {
            (Value::Check(b), FormKey::Space) => {
                *b = !*b;
                true
            }
            (Value::Choice { options, selected }, FormKey::Space | FormKey::Right) => {
                let n = options.len();
                let next = if k == FormKey::Space { (*selected + 1) % n.max(1) } else { (*selected + 1).min(n.saturating_sub(1)) };
                std::mem::replace(selected, next) != next
            }
            (Value::Choice { selected, .. }, FormKey::Left) => {
                let next = selected.saturating_sub(1);
                std::mem::replace(selected, next) != next
            }
            (Value::Number { value, min, max, step }, FormKey::Left | FormKey::Right) => {
                let d = if k == FormKey::Left { -*step } else { *step };
                let next = (*value + d).clamp(*min, *max);
                std::mem::replace(value, next) != next
            }
            _ => false,
        };
        if changed { FormEvent::Changed(f.key) } else { FormEvent::None }
    }
    /// 모든 값(key, 문자열).
    pub fn values(&self) -> Vec<(&'static str, String)> {
        self.fields.iter().map(|f| (f.key, f.text())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        let mut disabled = Field::new("locked", "잠김", Value::Check(false));
        disabled.enabled = false;
        Form::new(vec![
            Field::new("autostart", "자동 실행", Value::Check(true)),
            Field::new("keep", "보관 개수", Value::Choice { options: vec!["100".into(), "500".into(), "1000".into()], selected: 1 }),
            disabled,
            Field::new("pause", "일시정지(분)", Value::Number { value: 10, min: 1, max: 60, step: 5 }),
            Field::new("dir", "저장 폴더", Value::Path("C:\\자료".into())),
        ])
    }

    #[test]
    fn keyboard_changes_values_within_bounds() {
        let mut f = form();
        assert_eq!(f.key(FormKey::Space), FormEvent::Changed("autostart"));
        assert_eq!(f.get("autostart").unwrap().text(), "false");
        assert_eq!(f.key(FormKey::Tab { back: false }), FormEvent::Moved("keep"));
        assert_eq!(f.key(FormKey::Right), FormEvent::Changed("keep"));
        assert_eq!(f.key(FormKey::Right), FormEvent::None, "끝에서 멈춤");
        assert_eq!(f.key(FormKey::Space), FormEvent::Changed("keep"), "Space는 돌아감");
        assert_eq!(f.get("keep").unwrap().text(), "100");
        assert_eq!(f.key(FormKey::Tab { back: false }), FormEvent::Moved("pause"), "사용 안 함 건너뜀");
        assert_eq!(f.key(FormKey::Right), FormEvent::Changed("pause"));
        for _ in 0..20 {
            f.key(FormKey::Right);
        }
        assert_eq!(f.get("pause").unwrap().text(), "60");
        assert_eq!(f.key(FormKey::Tab { back: false }), FormEvent::Moved("dir"));
        assert!(f.get("dir").unwrap().needs_edit());
        assert_eq!(f.key(FormKey::Space), FormEvent::None, "글 칸은 EDIT가 처리");
        assert!(f.set_text("dir", "D:\\".into()));
        assert!(!f.set_text("dir", "D:\\".into()));
        assert_eq!(f.key(FormKey::Tab { back: false }), FormEvent::Moved("autostart"), "돌아감");
        assert_eq!(f.key(FormKey::Tab { back: true }), FormEvent::Moved("dir"));
        assert_eq!(f.values()[4], ("dir", "D:\\".to_string()));
    }

    #[test]
    fn layout_and_click() {
        let mut f = form();
        let slots = f.layout(Rect::new(0, 0, 400, 400), 120, 28, 8);
        assert_eq!(slots[1].label, Rect::new(0, 36, 120, 64));
        assert_eq!(slots[1].control, Rect::new(128, 36, 400, 64));
        assert_eq!(f.hit(&slots, 200, 40), Some(1));
        assert_eq!(f.hit(&slots, 200, 30), None, "간격");
        assert_eq!(f.click(2), FormEvent::None, "사용 안 함");
        assert_eq!(f.click(0), FormEvent::Changed("autostart"));
        assert_eq!(f.click(3), FormEvent::Moved("pause"));
        assert_eq!(f.focus, 3);
    }
}
