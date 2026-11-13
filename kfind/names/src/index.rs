//! 이름 색인 본체(소유판 `Names`)와 질의 코드가 읽는 빌린 보기(`View`).
use std::collections::HashMap;

use crate::build::Builder;
use crate::source::{Change, ChangeKind, attr, rec_of};

/// 없음(부모 없음·레코드 없음).
pub const NONE: u32 = u32::MAX;

/// 항목 표지(u8).
pub mod flag {
    pub const DIR: u8 = 1;
    pub const HIDDEN: u8 = 2;
    pub const SYSTEM: u8 = 4;
    /// 지워졌거나 다른 번호로 옮겨 갔다(질의에서 빼고, 폴더면 `redirect`로 새 번호를 찾는다).
    pub const DELETED: u8 = 8;
    /// 기본판(DFS 순서) 뒤에 덧붙인 항목: 하위 트리 구간이 없어 경로 조건은 부모 사슬로 확인한다.
    pub const APPENDED: u8 = 16;
    pub const REPARSE: u8 = 32;
}

pub(crate) const EXT_OTHER: u16 = u16::MAX;
const MAX_EXT_CHARS: usize = 16;

/// 정렬 키: 깊이(6비트) << 56 | 비교형 앞 7바이트(큰 자리부터). 질의가 맨 위 2비트에 등급을 얹는다.
#[inline]
pub(crate) fn make_key(depth: u32, fold: &[u8]) -> u64 {
    let mut p = [0u8; 8];
    let k = fold.len().min(7);
    p[1..1 + k].copy_from_slice(&fold[..k]);
    ((depth.min(63) as u64) << 56) | u64::from_be_bytes(p)
}

#[inline]
pub(crate) fn key_depth(key: u64) -> u32 {
    ((key >> 56) & 63) as u32
}

pub(crate) fn flags_of(attrs: u32) -> u8 {
    let mut f = 0;
    if attrs & attr::DIRECTORY != 0 {
        f |= flag::DIR;
    }
    if attrs & attr::HIDDEN != 0 {
        f |= flag::HIDDEN;
    }
    if attrs & attr::SYSTEM != 0 {
        f |= flag::SYSTEM;
    }
    if attrs & attr::REPARSE_POINT != 0 {
        f |= flag::REPARSE;
    }
    f
}

pub(crate) fn attrs_of(f: u8) -> u32 {
    let mut a = 0;
    if f & flag::DIR != 0 {
        a |= attr::DIRECTORY;
    }
    if f & flag::HIDDEN != 0 {
        a |= attr::HIDDEN;
    }
    if f & flag::SYSTEM != 0 {
        a |= attr::SYSTEM;
    }
    if f & flag::REPARSE != 0 {
        a |= attr::REPARSE_POINT;
    }
    a
}

/// 원래 이름의 확장자 비교형(폴더는 없음). 마지막 `.` 뒤 1~16글자.
pub(crate) fn ext_fold(name: &str, is_dir: bool) -> Option<String> {
    if is_dir {
        return None;
    }
    let pos = name.rfind('.')?;
    if pos == 0 {
        return None;
    }
    let e = &name[pos + 1..];
    if e.is_empty() || e.chars().count() > MAX_EXT_CHARS {
        return None;
    }
    let f = kfind_core::fold(e);
    if f.is_empty() { None } else { Some(f) }
}

/// 확장자 표(번호 0 = 없음).
#[derive(Debug, Clone, Default)]
pub(crate) struct ExtTable {
    pub names: Vec<Vec<u8>>,
    pub map: HashMap<Vec<u8>, u16>,
}

impl ExtTable {
    pub fn new() -> Self {
        ExtTable { names: vec![Vec::new()], map: HashMap::new() }
    }
    pub fn intern(&mut self, f: Option<String>) -> u16 {
        let Some(f) = f else { return 0 };
        if let Some(&id) = self.map.get(f.as_bytes()) {
            return id;
        }
        if self.names.len() >= EXT_OTHER as usize {
            return EXT_OTHER;
        }
        let id = self.names.len() as u16;
        self.names.push(f.clone().into_bytes());
        self.map.insert(f.into_bytes(), id);
        id
    }
}

/// 확장자 이름 목록 보기(소유판은 Vec, 매핑판은 0으로 구분한 덩어리).
#[derive(Clone, Copy)]
pub enum ExtList<'a> {
    Owned(&'a [Vec<u8>]),
    Blob { blob: &'a [u8], off: &'a [u32] },
}

impl<'a> ExtList<'a> {
    pub fn len(&self) -> usize {
        match self {
            ExtList::Owned(v) => v.len(),
            ExtList::Blob { off, .. } => off.len().saturating_sub(1),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, i: usize) -> &'a [u8] {
        match self {
            ExtList::Owned(v) => &v[i],
            ExtList::Blob { blob, off } => &blob[off[i] as usize..off[i + 1] as usize],
        }
    }
    pub fn find(&self, f: &[u8]) -> Option<u16> {
        (1..self.len()).find(|&i| self.get(i) == f).map(|i| i as u16)
    }
}

/// 질의 코드가 읽는 빌린 보기. 소유판(`Names::view`)과 매핑판(`NamesSeg::view`)이 같은 꼴을 낸다.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub parent: &'a [u32],
    /// 길이 n+1. 항목 i의 비교형 = `fold[fold_off[i] .. fold_off[i+1]-1]`(뒤 1바이트는 구분자 0).
    pub fold_off: &'a [u32],
    /// 길이 n+1. 항목 i의 원래 이름(UTF-8) = `name[name_off[i] .. name_off[i+1]]`.
    pub name_off: &'a [u32],
    pub key: &'a [u64],
    pub ext: &'a [u16],
    pub flags: &'a [u8],
    /// 하위 트리 끝(제외). 기본판 항목만 뜻이 있다.
    pub end: &'a [u32],
    /// 비교형 덩어리: `0, f0, 0, f1, 0, …, f(n-1), 0`.
    pub fold: &'a [u8],
    pub name: &'a [u8],
    pub exts: ExtList<'a>,
    /// 이 번호 미만이 DFS 순서 기본판.
    pub base_len: u32,
    /// 옮겨 간 폴더 옛 번호 → 새 번호(소유판만).
    pub redirect: Option<&'a HashMap<u32, u32>>,
    /// 기본판 안에서 조상이 옮겨 간 구간이 있는가(있으면 경로 조건을 부모 사슬로 확인).
    pub dirty: bool,
}

impl<'a> View<'a> {
    #[inline]
    pub fn len(&self) -> usize {
        self.parent.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }
    #[inline]
    pub fn fold_of(&self, i: u32) -> &'a [u8] {
        let i = i as usize;
        &self.fold[self.fold_off[i] as usize..self.fold_off[i + 1] as usize - 1]
    }
    #[inline]
    pub fn name_of(&self, i: u32) -> &'a str {
        let i = i as usize;
        let b = &self.name[self.name_off[i] as usize..self.name_off[i + 1] as usize];
        // 쓰는 쪽이 늘 &str에서 넣고 매핑판은 열 때 UTF-8을 검사한다.
        std::str::from_utf8(b).unwrap_or("")
    }
    #[inline]
    pub fn is_live(&self, i: u32) -> bool {
        self.flags[i as usize] & flag::DELETED == 0
    }
    /// 옮겨 간 폴더면 새 번호로.
    #[inline]
    pub fn resolve(&self, mut i: u32) -> u32 {
        if let Some(r) = self.redirect {
            let mut guard = 0;
            while let Some(&j) = r.get(&i) {
                i = j;
                guard += 1;
                if guard > 64 {
                    break;
                }
            }
        }
        i
    }
    /// 부모 번호(옮겨 간 폴더는 새 번호로 따라감). 뿌리는 NONE.
    #[inline]
    pub fn parent_of(&self, i: u32) -> u32 {
        let p = self.parent[i as usize];
        if p == NONE { NONE } else { self.resolve(p) }
    }
    /// 조상 폴더 번호를 가까운 것부터.
    pub fn ancestors(&self, i: u32) -> Ancestors<'_, 'a> {
        Ancestors { v: self, cur: self.parent_of(i), steps: 0 }
    }
    /// 항목이 들어 있는 폴더의 경로(`C:\Users\x`). 뿌리 항목이면 빈 문자열.
    pub fn dir_path(&self, i: u32) -> String {
        let mut parts: Vec<&str> = self.ancestors(i).map(|a| self.name_of(a)).collect();
        parts.reverse();
        join_path(&parts)
    }
    /// 전체 경로.
    pub fn full_path(&self, i: u32) -> String {
        let mut parts: Vec<&str> = self.ancestors(i).map(|a| self.name_of(a)).collect();
        parts.reverse();
        parts.push(self.name_of(i));
        join_path(&parts)
    }
    /// 살아 있는 항목 수.
    pub fn live_count(&self) -> usize {
        self.flags.iter().filter(|&&f| f & flag::DELETED == 0).count()
    }
    /// 살아 있는 바로 아래 항목들.
    pub fn children(&self, dir: u32) -> Vec<u32> {
        let hi = if dir < self.base_len && self.flags[dir as usize] & flag::APPENDED == 0 && !self.dirty {
            self.end[dir as usize]
        } else {
            self.base_len
        };
        let lo = if hi == self.base_len && self.dirty { 0 } else { dir + 1 };
        (lo..hi)
            .chain(self.base_len..self.len() as u32)
            .filter(|&j| j != dir && self.is_live(j) && self.parent_of(j) == dir)
            .collect()
    }

    /// 경로(`C:\a\b`, 대소문자 무시)로 살아 있는 폴더를 찾는다.
    pub fn find_dir(&self, path: &str) -> Option<u32> {
        let mut comps = path.split(['\\', '/']).filter(|c| !c.is_empty());
        let first = comps.next()?;
        let mut cur = (0..self.len() as u32).find(|&i| {
            self.parent[i as usize] == NONE && self.is_live(i) && self.name_of(i).eq_ignore_ascii_case(first)
        })?;
        for c in comps {
            let lc = c.to_lowercase();
            cur = self
                .children(cur)
                .into_iter()
                .find(|&j| self.flags[j as usize] & flag::DIR != 0 && self.name_of(j).to_lowercase() == lc)?;
        }
        Some(cur)
    }
}

fn join_path(parts: &[&str]) -> String {
    let mut s = String::new();
    for (k, p) in parts.iter().enumerate() {
        if k > 0 && !s.ends_with('\\') {
            s.push('\\');
        }
        s.push_str(p);
    }
    s
}

pub struct Ancestors<'v, 'a> {
    v: &'v View<'a>,
    cur: u32,
    steps: u32,
}

impl Iterator for Ancestors<'_, '_> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.cur == NONE || self.steps > 4096 {
            return None;
        }
        let c = self.cur;
        self.steps += 1;
        self.cur = self.v.parent_of(c);
        Some(c)
    }
}

/// 볼륨(드라이브) 정보.
#[derive(Debug, Clone)]
pub struct Volume {
    /// 뿌리 항목 이름(`C:`).
    pub name: String,
    /// 뿌리 폴더의 레코드 번호(NTFS는 5).
    pub root_rec: u64,
    /// 레코드 번호 → 항목 번호.
    pub recmap: Vec<u32>,
    /// USN 저널 ID와 다음에 읽을 USN(따라잡기용, 디렉터리 순회 원천은 0).
    pub journal_id: u64,
    pub next_usn: i64,
}

/// 소유판 이름 색인.
#[derive(Debug, Clone)]
pub struct Names {
    pub(crate) parent: Vec<u32>,
    pub(crate) fold_off: Vec<u32>,
    pub(crate) name_off: Vec<u32>,
    pub(crate) key: Vec<u64>,
    pub(crate) ext: Vec<u16>,
    pub(crate) flags: Vec<u8>,
    pub(crate) end: Vec<u32>,
    pub(crate) fold: Vec<u8>,
    pub(crate) name: Vec<u8>,
    pub(crate) exts: ExtTable,
    pub(crate) base_len: u32,
    pub(crate) redirect: HashMap<u32, u32>,
    pub(crate) dirty: bool,
    pub(crate) vols: Vec<Volume>,
    pub(crate) generation: u64,
    pub(crate) deleted: usize,
}

impl Names {
    pub fn view(&self) -> View<'_> {
        View {
            parent: &self.parent,
            fold_off: &self.fold_off,
            name_off: &self.name_off,
            key: &self.key,
            ext: &self.ext,
            flags: &self.flags,
            end: &self.end,
            fold: &self.fold,
            name: &self.name,
            exts: ExtList::Owned(&self.exts.names),
            base_len: self.base_len,
            redirect: if self.redirect.is_empty() { None } else { Some(&self.redirect) },
            dirty: self.dirty,
        }
    }

    /// 바뀔 때마다 1씩 오르는 세대(질의 결과 재사용을 무효로 한다).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    pub fn volumes(&self) -> &[Volume] {
        &self.vols
    }

    /// 덧붙인 항목·지운 항목 수(기본판 다시 만들기 판단용).
    pub fn delta_count(&self) -> usize {
        (self.len() - self.base_len as usize) + self.deleted
    }

    /// 메모리 사용량(바이트, 배열 용량 기준 근사).
    pub fn heap_bytes(&self) -> usize {
        self.parent.capacity() * 4
            + self.fold_off.capacity() * 4
            + self.name_off.capacity() * 4
            + self.key.capacity() * 8
            + self.ext.capacity() * 2
            + self.flags.capacity()
            + self.end.capacity() * 4
            + self.fold.capacity()
            + self.name.capacity()
            + self.vols.iter().map(|v| v.recmap.capacity() * 4).sum::<usize>()
            + self.redirect.len() * 16
            + self.exts.names.iter().map(|n| n.len() + 24 + 32).sum::<usize>()
    }

    fn lookup(&self, vol: u16, rec: u64) -> u32 {
        self.vols
            .get(vol as usize)
            .and_then(|v| v.recmap.get(rec as usize).copied())
            .unwrap_or(NONE)
    }

    fn set_rec(&mut self, vol: u16, rec: u64, idx: u32) {
        let v = &mut self.vols[vol as usize];
        let r = rec as usize;
        if r >= v.recmap.len() {
            v.recmap.resize(r + 1, NONE);
        }
        v.recmap[r] = idx;
    }

    fn mark_deleted(&mut self, idx: u32) {
        let f = &mut self.flags[idx as usize];
        if *f & flag::DELETED == 0 {
            *f |= flag::DELETED;
            self.deleted += 1;
        }
    }

    fn push_entry(&mut self, parent: u32, name: &str, attrs: u32) -> u32 {
        let idx = self.parent.len() as u32;
        let depth = if parent == NONE { 0 } else { key_depth(self.key[parent as usize]) + 1 };
        let f = kfind_core::fold(name);
        self.fold.extend_from_slice(f.as_bytes());
        self.fold.push(0);
        self.fold_off.push(self.fold.len() as u32);
        self.name.extend_from_slice(name.as_bytes());
        self.name_off.push(self.name.len() as u32);
        let fl = flags_of(attrs) | flag::APPENDED;
        let e = self.exts.intern(ext_fold(name, fl & flag::DIR != 0));
        self.parent.push(parent);
        self.key.push(make_key(depth, f.as_bytes()));
        self.ext.push(e);
        self.flags.push(fl);
        self.end.push(idx + 1);
        idx
    }

    /// 만들기·이름 바꾸기·옮기기: 같은 이름·부모면 속성만 맞추고, 아니면 옛 항목을 지우고 새로 덧붙인다.
    /// 바뀌었으면 true.
    pub fn upsert(&mut self, vol: u16, rec: u64, parent_rec: u64, name: &str, attrs: u32) -> bool {
        if vol as usize >= self.vols.len() || name.is_empty() {
            return false;
        }
        let rec = rec_of(rec);
        let parent_rec = rec_of(parent_rec);
        if rec == self.vols[vol as usize].root_rec {
            return false;
        }
        let mut pidx = self.lookup(vol, parent_rec);
        if pidx == NONE || self.flags[pidx as usize] & flag::DELETED != 0 {
            pidx = self.lookup(vol, self.vols[vol as usize].root_rec);
        }
        let old = self.lookup(vol, rec);
        if old != NONE && self.flags[old as usize] & flag::DELETED == 0 {
            let v = self.view();
            if v.parent_of(old) == pidx && v.name_of(old) == name {
                let keep = self.flags[old as usize] & (flag::DELETED | flag::APPENDED);
                let nf = flags_of(attrs) | keep;
                if self.flags[old as usize] == nf {
                    return false;
                }
                self.flags[old as usize] = nf;
                self.generation += 1;
                return true;
            }
            let was_dir = self.flags[old as usize] & flag::DIR != 0;
            self.mark_deleted(old);
            let new = self.push_entry(pidx, name, attrs);
            if was_dir {
                self.redirect.insert(old, new);
                if old < self.base_len {
                    self.dirty = true;
                }
            }
            self.set_rec(vol, rec, new);
        } else {
            let new = self.push_entry(pidx, name, attrs);
            self.set_rec(vol, rec, new);
        }
        self.generation += 1;
        true
    }

    /// 지우기. 바뀌었으면 true.
    pub fn remove(&mut self, vol: u16, rec: u64) -> bool {
        let rec = rec_of(rec);
        let idx = self.lookup(vol, rec);
        if idx == NONE || self.flags[idx as usize] & flag::DELETED != 0 {
            return false;
        }
        if vol as usize >= self.vols.len() || rec == self.vols[vol as usize].root_rec {
            return false;
        }
        self.mark_deleted(idx);
        self.set_rec(vol, rec, NONE);
        self.generation += 1;
        true
    }

    /// 변경 기록 하나를 반영한다.
    pub fn apply(&mut self, c: &Change) -> bool {
        match c.kind {
            ChangeKind::Upsert => self.upsert(c.vol, c.rec, c.parent, &c.name, c.attrs),
            ChangeKind::Remove => self.remove(c.vol, c.rec),
            ChangeKind::Ignore => false,
        }
    }

    /// 살아 있는 항목으로 기본판(DFS 순서)을 다시 만든다. 세대는 이어서 오른다.
    pub fn compact(&self) -> Names {
        let mut b = Builder::new();
        let v = self.view();
        for (vi, vol) in self.vols.iter().enumerate() {
            b.add_volume(&vol.name, vol.root_rec);
            let mut rec_of_idx: HashMap<u32, u64> = HashMap::new();
            for (r, &i) in vol.recmap.iter().enumerate() {
                if i != NONE {
                    rec_of_idx.insert(i, r as u64);
                }
            }
            for (r, &i) in vol.recmap.iter().enumerate() {
                if i == NONE || !v.is_live(i) || r as u64 == vol.root_rec {
                    continue;
                }
                let p = v.parent_of(i);
                let pr = if p == NONE { vol.root_rec } else { rec_of_idx.get(&p).copied().unwrap_or(vol.root_rec) };
                b.push(vi as u16, r as u64, pr, v.name_of(i), attrs_of(self.flags[i as usize]));
            }
        }
        let mut n = b.finish();
        n.generation = self.generation + 1;
        for (dst, src) in n.vols.iter_mut().zip(&self.vols) {
            dst.journal_id = src.journal_id;
            dst.next_usn = src.next_usn;
        }
        n
    }

    /// USN 따라잡기 위치를 적는다.
    pub fn set_journal(&mut self, vol: u16, journal_id: u64, next_usn: i64) {
        if let Some(v) = self.vols.get_mut(vol as usize) {
            v.journal_id = journal_id;
            v.next_usn = next_usn;
        }
    }

    /// 덧붙임·지움이 있는가(있으면 `seg::write`가 먼저 `compact`한다).
    pub fn has_delta(&self) -> bool {
        self.len() != self.base_len as usize || self.deleted > 0 || !self.redirect.is_empty() || self.dirty
    }
}
