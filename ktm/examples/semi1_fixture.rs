//! Synthetic fixture for --semi1-probe; refuses to overwrite existing output.
use kfind_core::{Sentence, Writer, WriterOptions};
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    if root.exists() {
        return Err("output already exists".into());
    }
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let index = root.join("index");
    let mut writer = Writer::open(&index, WriterOptions::default()).map_err(|e| e.to_string())?;
    let sources = [
        ("2026 공사 계약.md", "2026", "공사 계약 검토"),
        ("공사 계약 검토.txt", "2026 추진", "공사 계약 절차"),
        ("제외 문서.txt", "2025", "공사 계약"),
    ];
    let mut table = String::new();
    let mut paths = Vec::new();
    for (i, (name, first, second)) in sources.iter().enumerate() {
        let path = root.join(name);
        std::fs::write(&path, format!("{first}\n{second}")).map_err(|e| e.to_string())?;
        paths.push(path.to_string_lossy().into_owned());
        let id = i as u64 + 1;
        writer
            .add_document(
                id,
                0,
                &[
                    Sentence {
                        para: 0,
                        level: 0,
                        text: first,
                    },
                    Sentence {
                        para: 1,
                        level: 8,
                        text: second,
                    },
                ],
            )
            .map_err(|e| e.to_string())?;
        table.push_str(&serde_json::json!({"op":"put","key":format!("k{id}"),"id":id,"meta":{"path":paths[i]}}).to_string());
        table.push('\n');
    }
    writer.commit().map_err(|e| e.to_string())?;
    std::fs::write(index.join("doctable.jsonl"), table).map_err(|e| e.to_string())?;
    let rows: Vec<_> = sources
        .iter()
        .enumerate()
        .flat_map(|(i, (_, first, second))| {
            [
                (paths[i].as_str(), 0, *first),
                (paths[i].as_str(), 1, *second),
            ]
        })
        .collect();
    ktm::paragraphs::create_fixture(&root.join("content.sqlite"), &rows)?;
    let settings = ktm::settings::Settings {
        related_queries: true,
        hotkey: "Ctrl+Alt+F11".into(),
        ..Default::default()
    };
    settings.save(&root.join("settings.json"))?;
    Ok(())
}
