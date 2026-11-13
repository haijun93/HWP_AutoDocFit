//! 디렉터리 순회 원천(NTFS가 아닌 볼륨·관리자 권한이 없을 때). `FindFirstFileExW`(기본 정보·큰 묶음)를
//! 폴더마다 rayon 작업으로 나눠 병렬로 돈다. 재분석 지점(접합점·심볼 링크) 폴더는 이름만 넣고 들어가지 않는다.
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{
    FIND_FIRST_EX_LARGE_FETCH, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW,
    FindNextFileW, WIN32_FIND_DATAW,
};

use crate::build::Builder;
use crate::source::attr;

#[derive(Debug, Default, Clone, Copy)]
pub struct WalkStats {
    pub dirs: usize,
    pub files: usize,
    /// 열지 못한 폴더 수(권한 없음 등).
    pub errors: usize,
}

struct Rec {
    rec: u64,
    parent: u64,
    attrs: u32,
    name: String,
}

struct Ctx {
    next: AtomicU64,
    errors: AtomicUsize,
    out: Mutex<Vec<Vec<Rec>>>,
}

fn list_dir(ctx: &Ctx, path: &[u16], rec: u64, sub: &mut Vec<(Vec<u16>, u64)>) {
    let mut pat = path.to_vec();
    pat.extend("\\*".encode_utf16());
    pat.push(0);
    let mut d: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let h = unsafe {
        FindFirstFileExW(
            pat.as_ptr(),
            FindExInfoBasic,
            &mut d as *mut _ as *mut _,
            FindExSearchNameMatch,
            std::ptr::null(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    if h == INVALID_HANDLE_VALUE {
        ctx.errors.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let mut batch = Vec::new();
    loop {
        let l = d
            .cFileName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(d.cFileName.len());
        let nm = &d.cFileName[..l];
        let dot = nm == [b'.' as u16] || nm == [b'.' as u16, b'.' as u16];
        if !dot && l > 0 {
            let r = ctx.next.fetch_add(1, Ordering::Relaxed);
            let a = d.dwFileAttributes;
            if a & attr::DIRECTORY != 0 && a & attr::REPARSE_POINT == 0 {
                let mut p = path.to_vec();
                p.push(b'\\' as u16);
                p.extend_from_slice(nm);
                sub.push((p, r));
            }
            batch.push(Rec {
                rec: r,
                parent: rec,
                attrs: a,
                name: String::from_utf16_lossy(nm),
            });
        }
        if unsafe { FindNextFileW(h, &mut d) } == 0 {
            break;
        }
    }
    unsafe { FindClose(h) };
    if !batch.is_empty() {
        ctx.out.lock().unwrap().push(batch);
    }
}

fn walk<'s>(s: &rayon::Scope<'s>, ctx: &'s Ctx, path: Vec<u16>, rec: u64) {
    // 깊은 트리에서 작업이 너무 잘게 나뉘지 않게 한 작업이 하위 폴더 몇 개는 직접 이어서 돈다.
    let mut stack = vec![(path, rec)];
    let mut sub = Vec::new();
    let mut done = 0;
    while let Some((p, r)) = stack.pop() {
        list_dir(ctx, &p, r, &mut sub);
        done += 1;
        if done < 8 {
            stack.append(&mut sub);
        } else {
            for (sp, sr) in sub.drain(..) {
                s.spawn(move |s| walk(s, ctx, sp, sr));
            }
        }
    }
}

/// `root`(예: `C:\` 또는 `D:\자료`) 아래 전체를 만들기에 넣는다. 볼륨 이름은 경로 첫 마디로 쓰인다.
pub fn index_tree(b: &mut Builder, root: &str) -> WalkStats {
    let display = root.trim_end_matches(['\\', '/']).to_string();
    let vol = b.add_volume(&display, 0);
    // `\\?\C:` 뒤에 `\*`를 붙이면 뿌리 목록이 된다(긴 경로 허용).
    let start: Vec<u16> = if display.starts_with("\\\\") {
        display.encode_utf16().collect()
    } else {
        format!("\\\\?\\{display}").encode_utf16().collect()
    };
    let ctx = Ctx {
        next: AtomicU64::new(1),
        errors: AtomicUsize::new(0),
        out: Mutex::new(Vec::new()),
    };
    rayon::scope(|s| walk(s, &ctx, start, 0));
    let mut st = WalkStats {
        errors: ctx.errors.load(Ordering::Relaxed),
        ..Default::default()
    };
    for batch in ctx.out.into_inner().unwrap() {
        for r in batch {
            if r.attrs & attr::DIRECTORY != 0 {
                st.dirs += 1;
            } else {
                st.files += 1;
            }
            b.push(vol, r.rec, r.parent, &r.name, r.attrs);
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{Options, parse, search};

    #[test]
    fn walks_temp_tree() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        std::fs::create_dir_all(r.join("문서").join("2026")).unwrap();
        std::fs::write(r.join("문서").join("2026").join("예산_집행계획.hwp"), b"").unwrap();
        std::fs::write(r.join("문서").join("메모.txt"), b"").unwrap();
        std::fs::write(r.join("top.pdf"), b"").unwrap();
        let mut b = Builder::new();
        let st = index_tree(&mut b, &r.to_string_lossy());
        assert_eq!((st.dirs, st.files, st.errors), (2, 3, 0));
        let n = b.finish();
        let v = n.view();
        let res = search(&v, &parse("예산집행"), &Options::default()).unwrap();
        assert_eq!(res.total, 1);
        let p = v.full_path(res.top[0]);
        assert!(p.ends_with("문서\\2026\\예산_집행계획.hwp"), "{p}");
        assert!(
            p.starts_with(r.to_string_lossy().trim_end_matches('\\')),
            "{p}"
        );
        let res = search(&v, &parse("path:문서 ext:txt"), &Options::default()).unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(v.name_of(res.top[0]), "메모.txt");
    }
}
