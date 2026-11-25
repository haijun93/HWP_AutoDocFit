use ktm::layout::*;

#[test]
fn scale_width_labels_and_visible_states_have_disjoint_hit_regions() {
    let phrases = [
        "",
        "예산",
        "공사 계약 안전관리계획 및 세부 추진 일정 검토 결과",
    ];
    for scale in [100, 125, 150, 200] {
        for width in [300, 520, 700] {
            for phrase in phrases {
                for open in [false, true] {
                    for ac in [false, true] {
                        let related = vec![format!("1. {phrase}"), format!("2. {phrase}")];
                        let folders = vec!["폴더:".into(), "전체".into(), phrase.into()];
                        let measure = |s: &str| {
                            s.chars()
                                .map(|c| if c.is_ascii() { 6 } else { 12 })
                                .sum::<i32>()
                                * scale
                                / 100
                        };
                        let m = measured_quick_search_layout(
                            scale,
                            width * scale / 100,
                            2000 * scale / 100,
                            12 * scale / 100,
                            20 * scale / 100,
                            &related,
                            &folders,
                            open,
                            ac,
                            measure,
                        );
                        let b = m.bands;
                        for pair in
                            [b.related, b.options, b.filters, b.summary, b.autocomplete].windows(2)
                        {
                            assert!(pair[0].bottom <= pair[1].top);
                        }
                        let rects: Vec<_> = m
                            .related
                            .iter()
                            .chain(&m.dates)
                            .chain(&m.extensions)
                            .chain(&m.folders)
                            .collect();
                        for (i, rc) in rects.iter().enumerate() {
                            assert!(
                                rc.left >= 12 * scale / 100
                                    && rc.right <= (width - 12) * scale / 100 + 1
                            );
                            assert!(rc.bottom - rc.top >= 28 * scale / 100);
                            let (x, y) = ((rc.left + rc.right) / 2, (rc.top + rc.bottom) / 2);
                            assert!(hit_quick_band(x, y, **rc));
                            for (j, other) in rects.iter().enumerate() {
                                if i != j {
                                    assert!(!hit_quick_band(x, y, **other));
                                }
                            }
                        }
                        assert!(m.dates.last().unwrap().bottom < m.extensions[0].top);
                        assert!(m.extensions.last().unwrap().bottom < m.folders[0].top);
                        assert!(b.list_top >= b.summary.bottom);
                    }
                }
            }
        }
    }
}

#[test]
fn long_korean_chip_is_bounded_and_next_chip_wraps() {
    let labels = ["아주 긴 한글 문구가 반복되는 폴더 이름", "다음"];
    let r = measured_chip_rects(&labels, 12, 10, 212, 28, 4, 6, |s| {
        s.chars().count() as i32 * 12
    });
    assert_eq!(r[0].right, 212);
    assert_eq!(r[1].top, 42);
    assert_eq!(r[1].left, 12);
}

#[test]
fn tiny_window_never_exposes_hits_below_its_bottom() {
    for height in [0, 100, 190, 300] {
        let m =
            measured_quick_search_layout(200, 1040, height, 24, 40, &[], &[], true, true, |s| {
                s.len() as i32
            });
        let b = m.bands;
        for rc in [b.related, b.options, b.filters, b.summary, b.autocomplete] {
            assert!(rc.top <= rc.bottom && rc.bottom <= height);
        }
        for rc in m.dates.iter().chain(&m.extensions) {
            assert!(rc.top <= rc.bottom && rc.bottom <= height);
        }
    }
}

#[test]
fn preview_never_covers_summary_or_the_first_result_row() {
    for scale in [100, 125, 150, 200] {
        let row = 70 * scale / 100;
        let desired = 64 * scale / 100;
        for available in [
            0,
            row - 1,
            row,
            row + desired - 1,
            row + desired,
            500 * scale / 100,
        ] {
            let preview = fitting_preview_height(available, row, desired, true);
            assert!(preview == 0 || available - preview >= row);
            assert_eq!(fitting_preview_height(available, row, desired, false), 0);
        }
    }
}

#[test]
fn many_folders_keep_a_single_row_and_the_selected_long_label_is_bounded() {
    let mut folders = vec![
        "폴더:".into(),
        "전체".into(),
        "긴 선택 폴더 이름 ".repeat(100),
    ];
    folders.extend((0..1000).map(|i| format!("후보 폴더 {i}")));
    let m = measured_quick_search_layout(100, 520, 400, 12, 16, &[], &folders, false, false, |s| {
        s.chars().count() as i32 * 12
    });
    assert_eq!(m.folders.len(), 3);
    assert_eq!(m.folders[2].right, 508);
    assert!(m.folders.iter().all(|r| r.top == m.folders[0].top));
    assert!(m.bands.summary.bottom < 400);
}
