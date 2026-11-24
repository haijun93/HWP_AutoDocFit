use kfind_names::query::{hits, search_related};
use kfind_names::{Builder, Options};

#[test]
fn literal_groups_before_limit() {
    let mut b = Builder::new();
    let v = b.add_volume("C:", 5);
    b.push(v, 10, 5, "2026 공사계약 RD.txt", 0);
    b.push(v, 11, 5, "2026 공사 계약 R&D.txt", 0);
    b.push(v, 12, 5, "2026 공사 계약 RD.txt", 0);
    let n = b.finish();
    let v = n.view();
    let result = search_related(
        &v,
        ";2026;; \"공사 계약\"; R&D;",
        &Options {
            limit: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(hits(&v, &result.top)[0].name, "2026 공사 계약 R&D.txt");
    assert_eq!(
        search_related(&v, "; ;", &Options::default())
            .unwrap()
            .total,
        0
    );
}
