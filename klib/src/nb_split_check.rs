//! 업무자료함 색인 분리 비교 도구(NBIDX1): `klib nb-split-compare <자료함 폴더 사본>`.
//!
//! 옮기기 전(자료함 DB의 옛 업무자료함 표 + 공용 색인)과 옮긴 뒤(별도 DB + 별도 색인)를 같은 질의로 견준다.
//! 사용자 자료 원본이 아니라 **사본 폴더**에서만 돌린다(이 도구가 그 사본을 옮긴다).
//!
//! - 표 비교: 업무자료함 표 6개의 모든 행(열 값 그대로)이 같은지.
//! - 검색 비교: 노트북마다 문단 글에서 뽑은 질의(낱말·이웃 낱말 쌍·출처 이름)로 nb_search와 같은 전처리를 거쳐
//!   (1) 노트북 전체 20건 (2) 출처마다 5건(출처 비교·근거 찾기와 같은 범위)을 찾아, 결과 문단 목록과 순서가 같은지.
//!   (3) 개수 제한 없이(10,000건) 찾은 문단 집합이 같은지(찾을 수 있는 문단을 잃지 않았는지).
//!   점수는 색인 전체 통계(BM25: 전체 문서 수·낱말이 든 문서 수·평균 길이)에 따라 달라진다. 옛 공용 색인에는 가상 폴더
//!   문서가 섞여 있어, 같은 노트북 안에서도 순위가 가상 폴더 내용에 따라 움직였다. 그래서 (1)(2)의 순서 차이는 따로 센다.

use std::path::Path;

use crate::json::Value;
use crate::search::SearchIndex;

type Row = (i64, i64, String, String, String);

fn hits(ix: &SearchIndex, nb: i64, query: &str, scope: Option<&[i64]>, limit: usize) -> Result<(Vec<Row>, Vec<f32>), String> {
    let terms = crate::nb_transform::query_terms(query);
    let q = if terms.is_empty() { query.to_string() } else { terms.join(" ") };
    let found = ix.search_notebook(nb, &q, scope, limit).map_err(|e| e.to_string())?;
    Ok((found.iter().map(|h| (h.source_id, h.ordinal, h.display_name.clone(), h.location.clone(), h.body.clone())).collect(), found.iter().map(|h| h.score).collect()))
}

/// 표마다 (표 이름, 열 목록, 행). `cols_of`가 있으면 그 열 목록으로 읽는다(옮긴 뒤를 옛 표의 열 그대로 견줌).
fn dump(c: &rusqlite::Connection, schema: &str, cols_of: Option<&[(String, Vec<String>, Vec<String>)]>) -> Result<Vec<(String, Vec<String>, Vec<String>)>, String> {
    let mut out = Vec::new();
    for t in ["notebook", "nb_source", "nb_paragraph", "nb_note", "nb_citation", "nb_note_tag"] {
        let exists: bool = c.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE type='table' AND name=?1)"), [t], |r| r.get(0)).map_err(|e| e.to_string())?;
        let cols: Vec<String> = match cols_of.and_then(|list| list.iter().find(|(name, ..)| name == t)) {
            Some((_, cols, _)) => cols.clone(),
            None if exists => { let mut s = c.prepare(&format!("SELECT name FROM pragma_table_info('{t}','{schema}') ORDER BY cid")).map_err(|e| e.to_string())?; s.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())? }
            None => Vec::new(),
        };
        if !exists || cols.is_empty() { out.push((t.to_string(), cols, Vec::new())); continue; }
        let sql = format!("SELECT {} FROM {schema}.{t} ORDER BY rowid", cols.iter().map(|c| format!("quote({c})")).collect::<Vec<_>>().join("||'|'||"));
        let rows: Vec<String> = { let mut s = c.prepare(&sql).map_err(|e| e.to_string())?; s.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())? };
        out.push((t.to_string(), cols, rows));
    }
    Ok(out)
}

/// 노트북마다 질의 목록: 문단 낱말(2글자 이상, 처음 나온 순서) 최대 300개, 이웃 낱말 쌍 최대 100개, 출처 이름.
fn queries(c: &rusqlite::Connection, schema: &str, nb: i64) -> Result<Vec<String>, String> {
    let mut s = c.prepare(&format!("SELECT p.body FROM {schema}.nb_paragraph p JOIN {schema}.nb_source s ON s.id=p.source_id WHERE s.notebook_id=?1 ORDER BY p.source_id,p.ordinal")).map_err(|e| e.to_string())?;
    let bodies: Vec<String> = s.query_map([nb], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())?;
    let mut words: Vec<String> = Vec::new();
    let mut pairs: Vec<String> = Vec::new();
    for body in &bodies {
        let toks: Vec<String> = body.split(|ch: char| !ch.is_alphanumeric()).filter(|w| w.chars().count() >= 2).map(str::to_string).collect();
        for w in &toks { if words.len() < 300 && !words.contains(w) { words.push(w.clone()); } }
        for pair in toks.windows(2) { let q = format!("{} {}", pair[0], pair[1]); if pairs.len() < 100 && !pairs.contains(&q) { pairs.push(q); } }
    }
    let mut names: Vec<String> = { let mut s = c.prepare(&format!("SELECT display_name FROM {schema}.nb_source WHERE notebook_id=?1 ORDER BY id")).map_err(|e| e.to_string())?; s.query_map([nb], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())? };
    words.append(&mut pairs);
    words.append(&mut names);
    Ok(words)
}

struct Snapshot { tables: Vec<(String, Vec<String>, Vec<String>)>, results: Vec<(String, Vec<Row>, Vec<f32>)> }

fn snapshot(c: &rusqlite::Connection, schema: &str, ix: &SearchIndex, plan: &[(i64, Vec<i64>, Vec<String>)], cols_of: Option<&[(String, Vec<String>, Vec<String>)]>) -> Result<Snapshot, String> {
    let mut results = Vec::new();
    for (nb, sources, qs) in plan {
        for q in qs {
            let (rows, scores) = hits(ix, *nb, q, None, 20)?;
            results.push((format!("nb{nb} 전체 '{q}'"), rows, scores));
            let (mut all, _) = hits(ix, *nb, q, None, 10_000)?;
            all.sort();
            results.push((format!("nb{nb} 제한 없음 '{q}'"), all, Vec::new()));
            for sid in sources {
                let (rows, scores) = hits(ix, *nb, q, Some(&[*sid]), 5)?;
                results.push((format!("nb{nb} 출처{sid} '{q}'"), rows, scores));
            }
        }
    }
    Ok(Snapshot { tables: dump(c, schema, cols_of)?, results })
}

/// 사본 폴더에서 옮기기 전후를 견준다. 결과 JSON.
pub fn run(dir: &Path) -> Result<Value, String> {
    let library = dir.join("library.db");
    if !library.is_file() { return Err(format!("library.db가 없습니다: {}", library.display())); }
    if crate::nb_store::db_path(&crate::nb_store::root_for(&library)).exists() { return Err("이미 옮긴 폴더입니다. 옮기기 전 사본으로 돌려 주세요.".into()); }
    // 옮기기 전: 옛 표 + 공용 색인.
    let before = {
        let c = rusqlite::Connection::open(&library).map_err(|e| e.to_string())?;
        let ix = SearchIndex::open_or_create(dir.join("tantivy")).map_err(|e| e.to_string())?;
        if ix.needs_rebuild() { return Err("공용 색인 스키마가 달라 비교할 수 없습니다(사본의 공용 색인이 버려졌습니다).".into()); }
        let plan = plan(&c, "main")?;
        (snapshot(&c, "main", &ix, &plan, None)?, plan)
    };
    let (before, plan) = before;
    // 옮기기(앱이 처음 열 때와 같은 경로).
    let started = std::time::Instant::now();
    let db = crate::db::Database::open(&library.to_string_lossy()).map_err(|e| e.to_string())?;
    let migrate_ms = started.elapsed().as_millis() as i64;
    let ix = db.nb_index()?;
    let after = snapshot(db.connection(), "nb", &ix, &plan, Some(&before.tables))?;

    let mut table_diffs = Vec::new();
    let mut rows_total = 0i64;
    for ((t, cols, rb), (_, _, ra)) in before.tables.iter().zip(after.tables.iter()) {
        rows_total += rb.len() as i64;
        if rb != ra {
            let differing = rb.iter().zip(ra.iter()).filter(|(a, b)| a != b).count() + rb.len().abs_diff(ra.len());
            table_diffs.push(Value::obj(vec![("table", t.clone().into()), ("columns", cols.join(",").into()), ("before_rows", (rb.len() as i64).into()), ("after_rows", (ra.len() as i64).into()), ("differing_rows", (differing as i64).into())]));
        }
    }
    let (mut diff, mut order_only, mut nonempty, mut max_delta, mut ties) = (Vec::new(), 0i64, 0i64, 0f32, 0i64);
    for ((label, rb, sb), (_, ra, sa)) in before.results.iter().zip(after.results.iter()) {
        if !rb.is_empty() { nonempty += 1; }
        for (x, y) in sb.iter().zip(sa.iter()) { max_delta = max_delta.max((x - y).abs()); }
        if rb != ra {
            let mut a = rb.clone(); let mut b = ra.clone(); a.sort(); b.sort();
            if a == b { order_only += 1; }
            // 각 목록 안에서 같은 점수(소수 넷째 자리)끼리를 (출처, 문단) 순으로 놓았을 때 같아지면: 동점 정렬 차이뿐.
            let canon = |rows: &[Row], scores: &[f32]| { let mut v: Vec<(i64, i64, i64)> = rows.iter().zip(scores).map(|(r, s)| ((-(s * 10_000.0).round()) as i64, r.0, r.1)).collect(); v.sort(); v.into_iter().map(|(_, a, b)| (a, b)).collect::<Vec<_>>() };
            if canon(rb, sb) == canon(ra, sa) { ties += 1; }
            if diff.len() < 40 { diff.push(Value::obj(vec![("query", label.clone().into()),
                ("before", Value::Arr(rb.iter().zip(sb).map(|(r, s)| format!("{}:{}@{s:.4}", r.0, r.1).into()).collect())),
                ("after", Value::Arr(ra.iter().zip(sa).map(|(r, s)| format!("{}:{}@{s:.4}", r.0, r.1).into()).collect()))])); }
        }
    }
    let differing = before.results.iter().zip(after.results.iter()).filter(|(b, a)| b.1 != a.1).count() as i64;
    let unlimited: Vec<_> = before.results.iter().zip(after.results.iter()).filter(|(b, _)| b.0.contains(" 제한 없음 ")).collect();
    let unlimited_diffs = unlimited.iter().filter(|(b, a)| b.1 != a.1).count() as i64;
    let unlimited_hits: i64 = unlimited.iter().map(|(b, _)| b.1.len() as i64).sum();
    let m = db.nb_migration();
    Ok(Value::obj(vec![
        ("migration", m.to_json()),
        ("migrate_ms", migrate_ms.into()),
        ("notebooks", (plan.len() as i64).into()),
        ("tables_rows", rows_total.into()),
        ("table_diffs", Value::Arr(table_diffs)),
        ("searches", (before.results.len() as i64).into()),
        ("unlimited_searches", (unlimited.len() as i64).into()),
        ("unlimited_hits", unlimited_hits.into()),
        ("unlimited_set_diffs", unlimited_diffs.into()),
        ("searches_with_hits", nonempty.into()),
        ("search_diffs", differing.into()),
        ("search_order_only_diffs", order_only.into()),
        ("search_tie_only_diffs", ties.into()),
        ("max_score_delta", (max_delta as f64).into()),
        ("diff_samples", Value::Arr(diff)),
    ]))
}

fn plan(c: &rusqlite::Connection, schema: &str) -> Result<Vec<(i64, Vec<i64>, Vec<String>)>, String> {
    let ids: Vec<i64> = { let mut s = c.prepare(&format!("SELECT id FROM {schema}.notebook ORDER BY id")).map_err(|e| e.to_string())?; s.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())? };
    let mut out = Vec::new();
    for nb in ids {
        let sources: Vec<i64> = { let mut s = c.prepare(&format!("SELECT id FROM {schema}.nb_source WHERE notebook_id=?1 ORDER BY id")).map_err(|e| e.to_string())?; s.query_map([nb], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())? };
        out.push((nb, sources, queries(c, schema, nb)?));
    }
    Ok(out)
}
