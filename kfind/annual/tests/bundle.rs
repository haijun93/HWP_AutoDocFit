use kfind_annual::{
    Date, Doc, KST_OFFSET_SECS, Period, analyze, hwpx,
    output::{AnnualInput, write_outputs},
    synthetic::{DEFAULT_SEED, generate_synthetic_annual_artifacts},
};
use std::{path::Path, process::Command};

#[test]
fn native_grids_preserve_idle_weeks_and_year_boundaries() {
    let make = |year, month, day| {
        let ts = Date::new(year, month, day).to_timestamp(KST_OFFSET_SECS);
        Doc::new(format!("합성/{year}_예산_v1.hwpx"), ts, ts, "예산", vec![])
    };
    let docs = vec![make(2025, 12, 31), make(2026, 1, 1), make(2026, 12, 31)];
    let period = Period::new(docs[0].modified, docs[2].modified);
    let input = AnnualInput {
        report: Some(analyze(&docs, Some(period), Some(0.1))),
        docs,
        ..Default::default()
    };
    let weekly = hwpx::weekly_counts(&input);
    assert!(weekly.contains_key(&(2026, 1)));
    assert!(weekly.contains_key(&(2026, 53)));
    assert_eq!(weekly.values().flatten().sum::<u32>(), 3);
    assert!(weekly.values().any(|v| v.iter().sum::<u32>() == 0));
    let timeline = hwpx::topic_timeline_table(&input);
    assert!(timeline.rows.iter().any(|r| r[0].text.contains("2025")));
    assert!(timeline.rows.iter().any(|r| r[0].text.contains("2026")));
    assert!(timeline.rows.iter().any(|r| r[1].text.contains("W53")));
    for table in [hwpx::weekly_heatmap_table(&input), timeline] {
        assert_eq!(table.custom_widths.unwrap().iter().sum::<u32>(), 48_189);
        assert!(table.rows.iter().all(|r| r.len() == table.headers.len()));
    }
}

#[test]
fn synthetic_bundle_passes_both_python_validators() {
    let out = tempfile::tempdir().unwrap();
    let input = generate_synthetic_annual_artifacts(out.path(), DEFAULT_SEED).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (script, args) in [
        (
            "validate_annual_hwpx.py",
            vec!["--charts", "인수인계서.hwpx", "실적보고서.hwpx"],
        ),
        ("validate_annual_xlsx.py", vec!["통계.xlsx"]),
    ] {
        let mut command = Command::new("python");
        command.arg(root.join("scripts").join(script));
        for arg in args {
            if arg.starts_with("--") {
                command.arg(arg);
            } else {
                command.arg(out.path().join(arg));
            }
        }
        assert!(command.status().unwrap().success(), "{script}");
    }
    assert_eq!(input.docs.len(), 124);
    let legacy = out.path().join("legacy");
    write_outputs(&input, &legacy).unwrap();
    assert!(!legacy.join("통계.xlsx").exists());
    assert_eq!(hwpx::render_handover(&input).matches("<hp:tbl ").count(), 1);
    assert_eq!(
        hwpx::render_performance(&input).matches("<hp:tbl ").count(),
        2
    );
}
