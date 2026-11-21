use kfind_names::query::{Options, Sort, search};
use kfind_names::{Builder, NONE, parse};

fn legacy_key(depth: u8, fold: &[u8]) -> u64 {
    let mut p = [0u8; 8];
    let k = fold.len().min(7);
    p[1..1 + k].copy_from_slice(&fold[..k]);
    ((depth.min(63) as u64) << 56) | u64::from_be_bytes(p)
}

#[test]
fn thousand_random_queries_match_precompact_ranking_reference() {
    let mut b = Builder::with_capacity(4096, 100_000);
    b.add_volume("C:", 5);
    for d in 0..32u64 {
        b.push(0, 6 + d, 5, &format!("자료{d}"), 0x10);
    }
    let words = ["보고서", "계획", "예산", "업무", "memo", "집행"];
    for i in 0..4000u64 {
        let w = words[(i as usize * 17 + i as usize / 11) % words.len()];
        let raw = match i % 4 {
            0 => format!("{w}_결과-{i:05}.hwp"),
            1 => format!("{} 계획 {i:05}.pdf", w.to_uppercase()),
            2 => format!("{w} 자료 {}", "긴이름".repeat(8)),
            _ => format!("임시 {w} {i:05}.txt"),
        };
        b.push_meta(
            0,
            100 + i,
            6 + (i % 32),
            &raw,
            if i % 19 == 0 { 2 } else { 0 },
            i * 31,
            i as i64,
        );
    }
    // Empty names are rejected by Builder, and the hidden boundary is represented in the flags.
    b.push(0, 5000, 6, "", 0);
    let names = b.finish();
    let v = names.view();
    let opts = Options {
        limit: 100,
        sort: Sort::Relevance,
        ..Default::default()
    };
    let mut rng = 0x6d2b_79f5u32;
    for _ in 0..1000 {
        rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let term = words[rng as usize % words.len()];
        let q = parse(term);
        let got = search(&v, &q, &opts).unwrap();

        // Original implementation ranked using a precomputed u64 containing depth + fold prefix.
        let needle = kfind_core::fold(term);
        let mut expected: Vec<(u64, Vec<u8>, u32)> = (0..v.len() as u32)
            .filter(|&i| {
                v.fold_of(i)
                    .windows(needle.len())
                    .any(|x| x == needle.as_bytes())
            })
            .map(|i| {
                let f = v.fold_of(i);
                let grade = if f == needle.as_bytes() {
                    0u64
                } else if f.starts_with(needle.as_bytes()) {
                    1
                } else {
                    2
                };
                let old_base = legacy_key(v.depth[i as usize], f) & ((1u64 << 62) - 1);
                ((grade << 61) | (old_base >> 1), f.to_vec(), i)
            })
            .collect();
        expected.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let expected_ids: Vec<u32> = expected.iter().take(opts.limit).map(|r| r.2).collect();
        assert_eq!(got.total, expected.len(), "query={term:?}");
        assert_eq!(got.top, expected_ids, "query={term:?}");
    }
    assert_ne!(names.view().find_dir("C:"), Some(NONE));
}

#[test]
fn korean_case_long_empty_and_hidden_boundaries() {
    let mut b = Builder::new();
    b.add_volume("C:", 5);
    b.push(0, 6, 5, "한글 자료", 0x10);
    b.push(0, 7, 6, "한글_문서.hwp", 0);
    b.push(0, 8, 6, "ABC-Memo.txt", 0);
    b.push(0, 9, 6, &format!("긴이름{}.pdf", "가나다".repeat(80)), 0);
    b.push(0, 10, 6, "숨김한글.hwp", 2);
    b.push(0, 11, 6, "", 0);
    let names = b.finish();
    let v = names.view();
    assert_eq!(
        search(
            &v,
            &parse("한글"),
            &Options {
                hidden: false,
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        2
    );
    assert_eq!(
        search(&v, &parse("abc"), &Options::default())
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        search(
            &v,
            &parse("숨김한글"),
            &Options {
                hidden: false,
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        0
    );
    assert_eq!(
        search(&v, &parse("숨김한글"), &Options::default())
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        search(&v, &parse("긴이름"), &Options::default())
            .unwrap()
            .total,
        1
    );
}
