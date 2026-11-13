//! 키보드 탐색·포커스(순수 함수): Tab/Shift+Tab 순서, 기본 단추(Enter)·취소(Esc), Alt 접근 키.

/// 포커스를 받을 수 있는 요소 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop {
    pub id: u32,
    pub enabled: bool,
    /// 접근 키(Alt+글자). 이름의 `&X`에서 얻는다.
    pub access: Option<char>,
}

/// 포커스 고리.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusRing {
    pub stops: Vec<Stop>,
    pub current: Option<usize>,
    /// Enter가 누를 단추·Esc가 누를 단추.
    pub default_id: Option<u32>,
    pub cancel_id: Option<u32>,
}

/// 대화상자 키 처리 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    /// 포커스가 이 요소로 옮겨졌다.
    Focus(u32),
    /// 이 요소를 실행(기본 단추·취소 단추·접근 키).
    Activate(u32),
    None,
}

impl FocusRing {
    pub fn new(stops: Vec<Stop>) -> Self {
        let mut r = Self { stops, ..Default::default() };
        r.current = r.stops.iter().position(|s| s.enabled);
        r
    }
    pub fn focused(&self) -> Option<u32> {
        self.current.and_then(|i| self.stops.get(i)).map(|s| s.id)
    }
    pub fn focus_id(&mut self, id: u32) -> bool {
        match self.stops.iter().position(|s| s.id == id && s.enabled) {
            Some(i) => {
                self.current = Some(i);
                true
            }
            None => false,
        }
    }
    /// Tab(back=false)/Shift+Tab(back=true): 사용 안 함은 건너뛰고 돌아간다.
    pub fn next(&mut self, back: bool) -> Nav {
        let n = self.stops.len();
        if n == 0 {
            return Nav::None;
        }
        let start = self.current.unwrap_or(if back { 0 } else { n - 1 });
        for k in 1..=n {
            let i = if back { (start + n * 2 - k) % n } else { (start + k) % n };
            if self.stops[i].enabled {
                self.current = Some(i);
                return Nav::Focus(self.stops[i].id);
            }
        }
        Nav::None
    }
    /// Enter·Esc·Alt+글자.
    pub fn key(&mut self, k: DialogKey) -> Nav {
        match k {
            DialogKey::Tab { back } => self.next(back),
            DialogKey::Enter => self.default_id.map(Nav::Activate).unwrap_or(Nav::None),
            DialogKey::Escape => self.cancel_id.map(Nav::Activate).unwrap_or(Nav::None),
            DialogKey::Access(c) => {
                let c = c.to_ascii_uppercase();
                match self.stops.iter().position(|s| s.enabled && s.access.map(|a| a.to_ascii_uppercase()) == Some(c)) {
                    Some(i) => {
                        self.current = Some(i);
                        Nav::Activate(self.stops[i].id)
                    }
                    None => Nav::None,
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKey {
    Tab { back: bool },
    Enter,
    Escape,
    Access(char),
}

/// 이름의 접근 키(`열기(&O)` → 'O', `&&`는 글자 &).
pub fn access_key(label: &str) -> Option<char> {
    let mut it = label.chars().peekable();
    while let Some(c) = it.next() {
        if c == '&' {
            match it.next() {
                Some('&') => continue,
                Some(k) => return Some(k),
                None => return None,
            }
        }
    }
    None
}

/// 화면에 보일 이름(`&` 표시 없앰, `&&` → `&`).
pub fn strip_access(label: &str) -> String {
    let mut out = String::new();
    let mut it = label.chars().peekable();
    while let Some(c) = it.next() {
        if c == '&' {
            if let Some(&n) = it.peek() {
                it.next();
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(id: u32, enabled: bool, label: &str) -> Stop {
        Stop { id, enabled, access: access_key(label) }
    }

    #[test]
    fn tab_order_skips_disabled_and_wraps() {
        let mut r = FocusRing::new(vec![stop(1, false, "a"), stop(2, true, "&b"), stop(3, false, "c"), stop(4, true, "d")]);
        assert_eq!(r.focused(), Some(2));
        assert_eq!(r.next(false), Nav::Focus(4));
        assert_eq!(r.next(false), Nav::Focus(2));
        assert_eq!(r.next(true), Nav::Focus(4));
        assert!(!r.focus_id(3));
        assert!(r.focus_id(2));
        assert_eq!(FocusRing::new(vec![]).next(false), Nav::None);
        assert_eq!(FocusRing::new(vec![stop(1, false, "")]).next(false), Nav::None);
    }

    #[test]
    fn enter_escape_access_keys() {
        let mut r = FocusRing::new(vec![stop(1, true, "확인(&O)"), stop(2, true, "취소(&C)"), stop(3, false, "&X")]);
        r.default_id = Some(1);
        r.cancel_id = Some(2);
        assert_eq!(r.key(DialogKey::Enter), Nav::Activate(1));
        assert_eq!(r.key(DialogKey::Escape), Nav::Activate(2));
        assert_eq!(r.key(DialogKey::Access('c')), Nav::Activate(2));
        assert_eq!(r.focused(), Some(2));
        assert_eq!(r.key(DialogKey::Access('x')), Nav::None, "사용 안 함");
        assert_eq!(r.key(DialogKey::Tab { back: false }), Nav::Focus(1));
    }

    #[test]
    fn access_key_parsing() {
        assert_eq!(access_key("열기(&O)"), Some('O'));
        assert_eq!(access_key("A && B"), None);
        assert_eq!(access_key("끝&"), None);
        assert_eq!(strip_access("열기(&O)"), "열기(O)");
        assert_eq!(strip_access("A && B"), "A & B");
    }
}
