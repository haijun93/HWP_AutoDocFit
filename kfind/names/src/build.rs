//! 레코드(볼륨·레코드 번호·부모 레코드 번호·이름·속성)를 모아 DFS 순서 기본판 `Names`를 만든다.
use std::collections::HashMap;

use rayon::prelude::*;

use crate::index::{ExtTable, NONE, Names, SIZE_UNKNOWN, TIME_UNKNOWN, Volume, ext_fold, flag, flags_of, make_key};
use crate::source::rec_of;

/// 색인 만들기. 레코드는 아무 순서로 넣어도 된다(MFT 열거 순서 그대로).
#[derive(Debug, Default)]
pub struct Builder {
    vols: Vec<(String, u64)>,
    vol: Vec<u16>,
    rec: Vec<u64>,
    parent: Vec<u64>,
    attrs: Vec<u32>,
    name: Vec<u8>,
    name_off: Vec<u32>,
    /// 크기(바이트)·고친 시각(FILETIME). 모르면 `SIZE_UNKNOWN`·`TIME_UNKNOWN`.
    size: Vec<u64>,
    mtime: Vec<i64>,
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            name_off: vec![0],
            ..Default::default()
        }
    }

    pub fn with_capacity(entries: usize, name_bytes: usize) -> Self {
        let mut b = Builder::new();
        b.vol.reserve(entries);
        b.rec.reserve(entries);
        b.parent.reserve(entries);
        b.attrs.reserve(entries);
        b.size.reserve(entries);
        b.mtime.reserve(entries);
        b.name.reserve(name_bytes);
        b.name_off.reserve(entries + 1);
        b
    }

    /// 볼륨을 더하고 번호를 돌려준다. `root_name`은 경로 첫 마디(`C:`), `root_rec`은 뿌리 폴더 레코드 번호.
    pub fn add_volume(&mut self, root_name: &str, root_rec: u64) -> u16 {
        self.vols.push((root_name.to_string(), rec_of(root_rec)));
        (self.vols.len() - 1) as u16
    }

    pub fn len(&self) -> usize {
        self.rec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rec.is_empty()
    }

    /// 항목 하나. 뿌리 폴더 자체(`rec == root_rec`)는 넣지 않아도 된다(`add_volume`이 만든다).
    pub fn push(&mut self, vol: u16, rec: u64, parent_rec: u64, name: &str, attrs: u32) {
        self.push_meta(vol, rec, parent_rec, name, attrs, SIZE_UNKNOWN, TIME_UNKNOWN);
    }

    /// `push`에 크기·고친 시각을 더한 것(Q-F2 크기·날짜 정렬).
    #[allow(clippy::too_many_arguments)]
    pub fn push_meta(&mut self, vol: u16, rec: u64, parent_rec: u64, name: &str, attrs: u32, size: u64, mtime: i64) {
        if name.is_empty() || vol as usize >= self.vols.len() {
            return;
        }
        let rec = rec_of(rec);
        if rec == self.vols[vol as usize].1 {
            return;
        }
        self.vol.push(vol);
        self.rec.push(rec);
        self.parent.push(rec_of(parent_rec));
        self.attrs.push(attrs);
        self.size.push(size);
        self.mtime.push(mtime);
        self.name.extend_from_slice(name.as_bytes());
        self.name_off.push(self.name.len() as u32);
    }

    /// 다른 만들기(같은 볼륨 번호 체계)의 레코드를 이어 붙인다(병렬 순회 결과 합치기).
    pub fn append_records(&mut self, other: Builder) {
        let base = self.name.len() as u32;
        self.vol.extend_from_slice(&other.vol);
        self.rec.extend_from_slice(&other.rec);
        self.parent.extend_from_slice(&other.parent);
        self.attrs.extend_from_slice(&other.attrs);
        self.size.extend_from_slice(&other.size);
        self.mtime.extend_from_slice(&other.mtime);
        self.name.extend_from_slice(&other.name);
        self.name_off
            .extend(other.name_off[1..].iter().map(|o| o + base));
    }

    fn name_at(&self, i: usize) -> &str {
        std::str::from_utf8(&self.name[self.name_off[i] as usize..self.name_off[i + 1] as usize])
            .unwrap_or("")
    }

    pub fn finish(self) -> Names {
        let nv = self.vols.len();
        let nr = self.rec.len();
        // 원래 항목 번호: 0..nv = 볼륨 뿌리, nv.. = 레코드.
        let total = nv + nr;
        // 볼륨별 레코드 번호 → 원래 항목 번호.
        let mut recmaps: Vec<Vec<u32>> = Vec::with_capacity(nv);
        for (vi, (_, root_rec)) in self.vols.iter().enumerate() {
            let maxr = self
                .rec
                .iter()
                .zip(&self.vol)
                .filter(|(_, v)| **v as usize == vi)
                .map(|(r, _)| *r)
                .max()
                .unwrap_or(0)
                .max(*root_rec);
            let mut m = vec![NONE; maxr as usize + 1];
            m[*root_rec as usize] = vi as u32;
            recmaps.push(m);
        }
        for i in 0..nr {
            recmaps[self.vol[i] as usize][self.rec[i] as usize] = (nv + i) as u32;
        }
        // 원래 부모(원래 번호). 없는 부모는 그 볼륨 뿌리로.
        let mut oparent = vec![NONE; total];
        for i in 0..nr {
            let v = self.vol[i] as usize;
            let p = self.parent[i] as usize;
            let pi = recmaps[v].get(p).copied().unwrap_or(NONE);
            oparent[nv + i] = if pi == NONE || pi as usize == nv + i {
                v as u32
            } else {
                pi
            };
        }
        // 비교형(병렬).
        let names: Vec<&str> = (0..total)
            .map(|i| {
                if i < nv {
                    self.vols[i].0.as_str()
                } else {
                    self.name_at(i - nv)
                }
            })
            .collect();
        let folds: Vec<String> = names.par_iter().map(|s| kfind_core::fold(s)).collect();
        // 자식 목록(CSR).
        let mut cnt = vec![0u32; total + 1];
        for &p in &oparent {
            if p != NONE {
                cnt[p as usize + 1] += 1;
            }
        }
        for i in 0..total {
            cnt[i + 1] += cnt[i];
        }
        let mut fill = cnt.clone();
        let mut kids = vec![0u32; cnt[total] as usize];
        for (i, &p) in oparent.iter().enumerate() {
            if p != NONE {
                kids[fill[p as usize] as usize] = i as u32;
                fill[p as usize] += 1;
            }
        }
        // 형제는 비교형 → 원래 이름 → 번호 순(전순서라 결정적).
        {
            let mut slices: Vec<&mut [u32]> = Vec::with_capacity(total);
            let mut rest: &mut [u32] = &mut kids;
            for i in 0..total {
                let n = (cnt[i + 1] - cnt[i]) as usize;
                let (a, b) = rest.split_at_mut(n);
                slices.push(a);
                rest = b;
            }
            slices.par_iter_mut().filter(|s| s.len() > 1).for_each(|s| {
                s.sort_unstable_by(|&a, &b| {
                    folds[a as usize]
                        .cmp(&folds[b as usize])
                        .then_with(|| names[a as usize].cmp(names[b as usize]))
                        .then(a.cmp(&b))
                });
            });
        }
        // 전위 순회. 뿌리 = 볼륨 순서, 이어서 (고리 때문에) 닿지 못한 항목.
        let mut order: Vec<u32> = Vec::with_capacity(total);
        let mut seen = vec![false; total];
        let mut stack: Vec<u32> = Vec::new();
        let mut visit = |r: u32, order: &mut Vec<u32>, seen: &mut Vec<bool>| {
            stack.push(r);
            while let Some(x) = stack.pop() {
                if seen[x as usize] {
                    continue;
                }
                seen[x as usize] = true;
                order.push(x);
                let ks = &kids[cnt[x as usize] as usize..cnt[x as usize + 1] as usize];
                for &k in ks.iter().rev() {
                    if !seen[k as usize] {
                        stack.push(k);
                    }
                }
            }
        };
        for r in 0..nv as u32 {
            visit(r, &mut order, &mut seen);
        }
        for r in 0..total as u32 {
            if !seen[r as usize] {
                visit(r, &mut order, &mut seen);
            }
        }
        let mut newid = vec![NONE; total];
        for (n, &o) in order.iter().enumerate() {
            newid[o as usize] = n as u32;
        }
        // 새 순서로 배열 채우기.
        let mut parent = Vec::with_capacity(total);
        let mut depth = vec![0u32; total];
        for &o in &order {
            let p = oparent[o as usize];
            // 고리로 닿은 항목은 부모가 뒤 번호일 수 있다 → 뿌리로 끊는다.
            let np = if p == NONE || newid[p as usize] >= newid[o as usize] {
                NONE
            } else {
                newid[p as usize]
            };
            let d = if np == NONE {
                0
            } else {
                depth[np as usize] + 1
            };
            depth[newid[o as usize] as usize] = d;
            parent.push(np);
        }
        let mut size = vec![1u32; total];
        for i in (0..total).rev() {
            let p = parent[i];
            if p != NONE {
                size[p as usize] += size[i];
            }
        }
        let end: Vec<u32> = (0..total).map(|i| i as u32 + size[i]).collect();
        let fold_bytes: usize = folds.iter().map(|f| f.len() + 1).sum::<usize>() + 1;
        let mut fold = Vec::with_capacity(fold_bytes);
        let mut fold_off = Vec::with_capacity(total + 1);
        let mut name = Vec::with_capacity(names.iter().map(|s| s.len()).sum());
        let mut name_off = Vec::with_capacity(total + 1);
        let mut key = Vec::with_capacity(total);
        let mut ext = Vec::with_capacity(total);
        let mut flags = Vec::with_capacity(total);
        let mut sizes = Vec::with_capacity(total);
        let mut mtimes = Vec::with_capacity(total);
        let mut exts = ExtTable::new();
        fold.push(0);
        name_off.push(0);
        for (n, &o) in order.iter().enumerate() {
            let o = o as usize;
            fold_off.push(fold.len() as u32);
            fold.extend_from_slice(folds[o].as_bytes());
            fold.push(0);
            name.extend_from_slice(names[o].as_bytes());
            name_off.push(name.len() as u32);
            let fl = if o < nv {
                flag::DIR
            } else {
                flags_of(self.attrs[o - nv])
            };
            flags.push(fl);
            if o < nv {
                sizes.push(SIZE_UNKNOWN);
                mtimes.push(TIME_UNKNOWN);
            } else {
                sizes.push(self.size[o - nv]);
                mtimes.push(self.mtime[o - nv]);
            }
            ext.push(exts.intern(ext_fold(names[o], fl & flag::DIR != 0)));
            key.push(make_key(depth[n], folds[o].as_bytes()));
        }
        fold_off.push(fold.len() as u32);
        // 레코드 표를 새 번호로.
        let vols = self
            .vols
            .iter()
            .zip(recmaps)
            .map(|((nm, rr), m)| Volume {
                name: nm.clone(),
                root_rec: *rr,
                recmap: m
                    .into_iter()
                    .map(|o| if o == NONE { NONE } else { newid[o as usize] })
                    .collect(),
                journal_id: 0,
                next_usn: 0,
            })
            .collect();
        Names {
            parent,
            fold_off,
            name_off,
            key,
            ext,
            flags,
            size: sizes,
            mtime: mtimes,
            end,
            fold,
            name,
            exts,
            base_len: total as u32,
            redirect: HashMap::new(),
            dirty: false,
            vols,
            generation: 1,
            deleted: 0,
        }
    }
}
