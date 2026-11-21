#![cfg(windows)]
use grid_core::{png, shell_thumb};
use std::{path::PathBuf, time::Instant};

#[test]
#[ignore = "requires an interactive Windows shell with installed thumbnail handlers; see docs/THUMBSHELL1_RESULT_2026-11-22.md"]
fn shell_thumbnail_benchmark_synthetic_only() {
    let root =
        PathBuf::from(std::env::var_os("SHELL_THUMB_FIXTURES").expect("SHELL_THUMB_FIXTURES"));
    for ext in ["jpg", "png", "pdf", "txt"] {
        let p = root.join(format!("sample.{ext}"));
        let start = Instant::now();
        match shell_thumb::thumbnail(&p, 256) {
            Ok(b) => {
                let elapsed = start.elapsed().as_secs_f64();
                png::write(
                    &root.join(format!("preview-{ext}-256.png")),
                    b.width as usize,
                    b.height as usize,
                    &b.bgra,
                )
                .unwrap();
                eprintln!(
                    "type={ext} result=ok dims={}x{} first_s={elapsed:.4}",
                    b.width, b.height
                );
                let start = Instant::now();
                let _ = shell_thumb::thumbnail(&p, 256);
                eprintln!("type={ext} warm_s={:.4}", start.elapsed().as_secs_f64());
            }
            Err(e) => eprintln!("type={ext} result=error error={e}"),
        }
    }
    for size in [64, 128, 256, 512] {
        let p = root.join("sample.jpg");
        if let Ok(b) = shell_thumb::thumbnail(&p, size) {
            png::write(
                &root.join(format!("quality-{size}.png")),
                b.width as usize,
                b.height as usize,
                &b.bgra,
            )
            .unwrap();
            eprintln!("quality request={size} dims={}x{}", b.width, b.height);
        }
    }
    let files: Vec<_> = (0..20)
        .map(|i| root.join(format!("large-{i:02}.jpg")))
        .collect();
    let start = Instant::now();
    let successes = files
        .iter()
        .filter(|p| shell_thumb::thumbnail(p, 256).is_ok())
        .count();
    eprintln!(
        "large20 successes={successes} total_s={:.4}",
        start.elapsed().as_secs_f64()
    );
}
