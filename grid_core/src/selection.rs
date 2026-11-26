//! 경로로 보존하는 다중 선택. 범위는 현재 화면 순서, 기준은 마지막 Shift 없는 클릭이다.
use std::collections::HashSet;

#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub paths: HashSet<String>,
    pub anchor: Option<String>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.paths.clear();
        self.anchor = None;
    }

    pub fn click(&mut self, path: &str, order: &[String], ctrl: bool, shift: bool) {
        if shift {
            let from = self
                .anchor
                .as_ref()
                .and_then(|a| order.iter().position(|p| p == a));
            let to = order.iter().position(|p| p == path);
            if let (Some(a), Some(b)) = (from, to) {
                if !ctrl {
                    self.paths.clear();
                }
                self.paths
                    .extend(order[a.min(b)..=a.max(b)].iter().cloned());
                return;
            }
        }
        if ctrl && !shift {
            if !self.paths.remove(path) {
                self.paths.insert(path.into());
            }
        } else {
            self.paths.clear();
            self.paths.insert(path.into());
        }
        self.anchor = Some(path.into());
    }

    pub fn context(&mut self, path: &str) {
        if !self.paths.contains(path) {
            self.paths.clear();
            self.paths.insert(path.into());
            self.anchor = Some(path.into());
        }
    }

    pub fn all(&mut self, order: impl Iterator<Item = String>) {
        self.paths = order.collect();
    }
}
