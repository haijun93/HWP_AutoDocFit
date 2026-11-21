//! 썸네일 크기 슬라이더 범위(`docfit_app/thumb_size_setting.py` 와 같다).
pub const MIN_THUMB: f64 = 100.0;
pub const MAX_THUMB: f64 = 240.0;

/// `_clamp` 와 같다: NaN 이면 기본값, 아니면 100~240 으로 맞춘다.
pub fn clamp_thumb(value: f64, default: f64) -> f64 {
    if value.is_nan() {
        default
    } else {
        value.clamp(MIN_THUMB, MAX_THUMB)
    }
}

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Byte-bounded least-recently-used cache. Oversized entries are returned to the caller but not kept.
pub struct ByteLru<K, V> {
    max_bytes: usize,
    used_bytes: usize,
    entries: HashMap<K, (V, usize)>,
    order: VecDeque<K>,
}

impl<K: Clone + Eq + Hash, V: Clone> ByteLru<K, V> {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            used_bytes: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }
    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }
    pub fn get(&mut self, key: &K) -> Option<V> {
        let (value, _) = self.entries.get(key)?;
        let value = value.clone();
        self.order.retain(|k| k != key);
        self.order.push_back(key.clone());
        Some(value)
    }
    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        if let Some((_, old)) = self.entries.remove(&key) {
            self.used_bytes = self.used_bytes.saturating_sub(old);
            self.order.retain(|k| k != &key);
        }
        if bytes > self.max_bytes {
            return;
        }
        while self.used_bytes.saturating_add(bytes) > self.max_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((_, n)) = self.entries.remove(&oldest) {
                self.used_bytes -= n;
            }
        }
        self.used_bytes += bytes;
        self.entries.insert(key.clone(), (value, bytes));
        self.order.push_back(key);
    }
}

/// Visible-first request queue. Replacing visibility cancels queued keys that left the viewport.
pub struct ThumbQueue<K> {
    pending: VecDeque<K>,
}
impl<K: Clone + Eq> Default for ThumbQueue<K> {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }
}
impl<K: Clone + Eq> ThumbQueue<K> {
    pub fn set_visible(&mut self, ordered_visible: impl IntoIterator<Item = K>) {
        let next: VecDeque<K> = ordered_visible.into_iter().collect();
        self.pending = next;
    }
    pub fn pop(&mut self) -> Option<K> {
        self.pending.pop_front()
    }
    pub fn cancel(&mut self, key: &K) {
        self.pending.retain(|k| k != key);
    }
    pub fn len(&self) -> usize {
        self.pending.len()
    }
}
