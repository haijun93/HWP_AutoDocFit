//! cloud 전용 표지는 일반 reparse와 구분하고 갱신·compact·세그먼트 왕복으로 보존한다.
use kfind_names::{Builder, NamesSeg, flag, seg};

fn cloud(view: &kfind_names::View<'_>, name: &str) -> bool {
    let id = (0..view.len() as u32).find(|&i| view.name_of(i) == name).unwrap();
    view.flags[id as usize] & flag::CLOUD_ONLY != 0
}
#[test]
fn cloud_flags_survive_updates_compaction_and_segment_roundtrip() {
    let mut b = Builder::new();
    let vol = b.add_volume("C:", 5);
    for (i, (name, attrs)) in [("local.jpg", 0x400), ("offline.jpg", 0x1000),
        ("recall-open.jpg", 0x40000), ("recall-data.jpg", 0x400000)].iter().enumerate() {
        b.push(vol, 10 + i as u64, 5, name, *attrs);
    }
    let mut n = b.finish();
    assert!(!cloud(&n.view(), "local.jpg"));
    for name in ["offline.jpg", "recall-open.jpg", "recall-data.jpg"] {
        assert!(cloud(&n.view(), name));
    }
    // 수화되어 recall 표지가 없어지면 다시 로컬로 바뀐다.
    assert!(n.upsert(vol, 11, 5, "offline.jpg", 0x400));
    assert!(!cloud(&n.view(), "offline.jpg"));
    // 새 USN 갱신에 recall 표지가 붙으면 cloud 전용이 된다.
    assert!(n.upsert(vol, 10, 5, "local.jpg", 0x400 | 0x1000));
    assert!(cloud(&n.view(), "local.jpg"));
    let n = n.compact();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("names.seg");
    seg::write(&n, &file).unwrap();
    let mapped = NamesSeg::open(&file, true).unwrap();
    for name in ["local.jpg", "recall-open.jpg", "recall-data.jpg"] {
        assert!(cloud(&mapped.view(), name));
    }
    assert!(!cloud(&mapped.view(), "offline.jpg"));
}
