use kfind_names::query::{Sort, parse_groups};
use kfind_names::{Builder, Options, Searcher};

#[test]
fn complete_snapshot_has_every_hit_and_the_same_prefix_for_all_sorts() {
    let mut builder = Builder::new();
    let volume = builder.add_volume("Z:\\synthetic", 5);
    for id in 0..1507 {
        builder.push_meta(
            volume,
            id + 10,
            5,
            &format!("예산집행-{:04}.txt", 1507 - id),
            0,
            id * 17,
            id as i64 * 19,
        );
    }
    let names = builder.finish();
    let view = names.view();
    for sort in [
        Sort::Relevance,
        Sort::Name,
        Sort::Path,
        Sort::Size,
        Sort::Date,
    ] {
        let full = Searcher::new()
            .search_groups(
                &view,
                names.generation(),
                &parse_groups("예산; 집행 file:"),
                &Options {
                    limit: usize::MAX,
                    sort,
                    ..Options::default()
                },
            )
            .unwrap();
        assert_eq!(full.total, 1507);
        assert_eq!(full.top.len(), 1507);
        for limit in [1, 20, 200, 1000] {
            let bounded = Searcher::new()
                .search_groups(
                    &view,
                    names.generation(),
                    &parse_groups("예산; 집행 file:"),
                    &Options {
                        limit,
                        sort,
                        ..Options::default()
                    },
                )
                .unwrap();
            assert_eq!(bounded.top, full.top[..limit]);
        }
    }
}
