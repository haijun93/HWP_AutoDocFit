//! Isolated synthetic index for manual UI/pipe measurements. Never overwrite data.
use kfind_core::{Sentence, Writer, WriterOptions};
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = std::path::absolute(PathBuf::from(
        args.next().ok_or("fixture INDEX_DIR [COUNT]")?,
    ))?;
    let count: u64 = args
        .next()
        .map(|n| n.to_string_lossy().parse())
        .transpose()?
        .unwrap_or(3000);
    if count == 0 || count > 100_000 {
        return Err("COUNT must be 1..100000".into());
    }
    if root.exists() {
        return Err("fixture directory already exists; choose a new isolated directory".into());
    }
    std::fs::create_dir_all(root.join("documents"))?;
    let mut table = std::fs::File::create_new(root.join("doctable.jsonl"))?;
    let mut writer = Writer::open(&root, WriterOptions::default())?;
    for id in 1..=count {
        let heading = format!("예산집행 계획 {id}");
        let body = format!("지역 사업 예산집행 추진 계획과 세부 일정 문서 {id}");
        let text = format!("{heading}\n{body}\n성과 점검 및 결과 보고\n");
        let path = root.join("documents").join(format!("합성문서-{id:05}.txt"));
        std::fs::write(&path, text)?;
        writer.add_document(
            id,
            id as i64,
            &[
                Sentence {
                    para: 0,
                    level: (id % 4) as u8,
                    text: &heading,
                },
                Sentence {
                    para: 1,
                    level: 5,
                    text: &body,
                },
                Sentence {
                    para: 2,
                    level: 5,
                    text: "성과 점검 및 결과 보고",
                },
            ],
        )?;
        let entry = json!({"op":"put","key":format!("fixture:{id}"),"id":id,
            "meta":{"path":path.to_string_lossy(),"size":std::fs::metadata(&path)?.len(),
                "mtime":id,"date":id,"ref_date":"2026-11-13","scopes":["tm"],
                "level_src":"py","level_rule":1},
            "spans":[0,0,heading.chars().count(),1,0,body.chars().count(),2,0,15]});
        writeln!(table, "{entry}")?;
    }
    writer.commit()?;
    writeln!(table, "{}", json!({"op":"next","id":count+1}))?;
    table.sync_all()?;
    println!("{} synthetic documents: {}", count, root.display());
    Ok(())
}
