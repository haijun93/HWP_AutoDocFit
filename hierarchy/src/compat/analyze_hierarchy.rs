use serde_json::{json, Value};
use std::collections::HashMap;

/// Typed compatibility input used by native Rust callers such as klib.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParagraphInput {
    pub text: String,
    pub left: i64,
    pub indent: i64,
    pub font: String,
    pub size_pt: f64,
    pub prev_spacing: Option<i64>,
    pub indent_hwpunit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphResult {
    pub role: String,
    pub marker: String,
}

/// Analyze native paragraph structs through the same compatibility stages.
/// JSON remains an internal representation of the legacy algorithm only.
pub fn analyze_hierarchy_typed(input: &[ParagraphInput]) -> Vec<ParagraphResult> {
    let paragraphs = Value::Array(input.iter().map(|p| json!({
        "text": p.text, "left": p.left, "indent": p.indent, "font": p.font,
        "size_pt": p.size_pt, "prev_spacing": p.prev_spacing,
        "indent_hwpunit": p.indent_hwpunit,
    })).collect());
    let result = analyze_hierarchy(&paragraphs);
    let roles = result["role_sequence"].as_array().cloned().unwrap_or_default();
    input.iter().zip(roles).map(|(paragraph, role)| {
        let marker = super::leading_marker::leading_marker(&paragraph.text).0;
        ParagraphResult { role: role.as_str().unwrap_or("미분류").to_owned(), marker }
    }).collect()
}
use super::{document_type::document_type, leading_marker::leading_marker, marker_scheme::detect_scheme};

fn modal<T: Clone + Eq + std::hash::Hash>(items: impl Iterator<Item=T>) -> Option<T> {
    let mut seen: Vec<(T, usize)> = Vec::new();
    for item in items { if let Some((_, n)) = seen.iter_mut().find(|(v,_)| *v == item) { *n += 1; } else { seen.push((item,1)); } }
    let mut best: Option<(T, usize)> = None;
    for (value, count) in seen {
        if best.as_ref().map(|(_, best_count)| count > *best_count).unwrap_or(true) {
            best = Some((value, count));
        }
    }
    best.map(|(value, _)| value)
}
fn strv(v: &Value) -> String { v.as_str().unwrap_or("").to_owned() }
fn num(v: &Value) -> f64 { v.as_f64().unwrap_or(0.0) }

/// Stage 01: 원문단 순회, marker/role 및 들여쓰기 중간값을 수집한다.
pub fn stage_01(paragraphs: &Value) -> Value {
    let empty = Vec::new();
    let paras = paragraphs.as_array().unwrap_or(&empty);
    let texts: Vec<String> = paras.iter().map(|item| strv(&item["text"])).collect();
    let scheme = detect_scheme(&texts);
    let records: Vec<Value> = paras.iter().enumerate().map(|(index, item)| {
        let text = strv(&item["text"]);
        let (marker, marker_role) = super::leading_marker::leading_marker_with_scheme(&text, Some(&scheme));
        let indent = item["left"].as_i64().unwrap_or(0) + item["indent"].as_i64().unwrap_or(0);
        let mut record = item.as_object().cloned().unwrap_or_default();
        record.insert("index".into(), json!(index));
        record.insert("marker".into(), json!(marker));
        record.insert("marker_role".into(), json!(marker_role));
        record.insert("indent_hwpunit".into(), json!(indent));
        Value::Object(record)
    }).collect();
    json!({"records": records})
}

/// Stage 02: 역할별 문서 통계와 대표 서식을 집계한다.
pub fn stage_02(records_value: &Value) -> Value {
    let empty = Vec::new();
    let records = records_value["records"].as_array().unwrap_or(&empty);
    let roles = ["장", "중제목", "소제목", "본문", "내용", "부연설명"];
    let mut role_indent = serde_json::Map::new();
    let mut role_typography = serde_json::Map::new();
    for role in roles {
        let marked: Vec<&Value> = records
            .iter()
            .filter(|r| r["marker_role"].as_str() == Some(role))
            .collect();
        let indent = modal(marked.iter().map(|r| r["indent_hwpunit"].as_i64().unwrap_or(0)));
        let typography = modal(marked.iter().map(|r| (r["font"].clone(), r["size_pt"].as_f64().unwrap_or(0.0).to_bits())));
        role_indent.insert(role.into(), indent.map(|n| json!(n)).unwrap_or(Value::Null));
        role_typography.insert(role.into(), typography.map(|(font, bits)| json!([font, f64::from_bits(bits)])).unwrap_or(Value::Null));
    }
    let mut indent_levels: Vec<i64> = records.iter().map(|r|r["indent_hwpunit"].as_i64().unwrap_or(0)).collect();
    indent_levels.sort(); indent_levels.dedup();
    json!({"role_indent":role_indent,"role_typography":role_typography,"indent_levels":indent_levels})
}

pub fn analyze_hierarchy_stages(input: &Value) -> Value {
    let first = stage_01(&input["paragraphs"]);
    let second = stage_02(&first);
    let empty = Vec::new();
    let records = first["records"].as_array().unwrap_or(&empty);
    let mut role_indent = HashMap::new();
    let mut role_typo = HashMap::new();
    for role in ["제목", "장", "중제목", "소제목", "본문", "내용", "부연설명"] {
        role_indent.insert(role, second["role_indent"][role].as_i64());
        let pair = second["role_typography"][role].as_array().cloned().unwrap_or_default();
        role_typo.insert(role, (pair.first().and_then(Value::as_str).unwrap_or("").to_owned(),
                                pair.get(1).and_then(Value::as_f64).unwrap_or(0.0)));
    }
    let mut classified: Vec<Value> = Vec::new();
    let mut circled_rows = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let marker = strv(&record["marker"]);
        let mut role = strv(&record["marker_role"]);
        let indent = record["indent_hwpunit"].as_i64().unwrap_or(0);
        let ambiguous = marker.chars().count() == 1 && marker.chars().next().map(|c|
            ('①'..='⑳').contains(&c) || ('㉠'..='㉻').contains(&c) ||
            ('❶'..='❿').contains(&c) || ('➊'..='➓').contains(&c)).unwrap_or(false);
        if ambiguous {
            let mut candidates = Vec::new();
            for (rank, name) in ["본문", "내용", "부연설명"].iter().enumerate() {
                if let Some(base) = role_indent[name] {
                    let (font, size) = &role_typo[name];
                    let distance = (indent - base).abs();
                    let penalty = if strv(&record["font"]) == *font { 0.0 } else { 150.0 } +
                        (num(&record["size_pt"]) - *size).abs() * 100.0;
                    candidates.push((distance, distance as f64 + penalty, rank, *name));
                }
            }
            role = if let Some(candidate) = candidates.into_iter().min_by(|a,b| (a.0,a.1,a.2).partial_cmp(&(b.0,b.1,b.2)).unwrap()) {
                candidate.3.to_owned()
            } else if index > 0 && classified[index-1]["role"] == "소제목" { "본문".into() }
            else if index > 0 && ["본문", "내용"].contains(&strv(&classified[index-1]["role"]).as_str()) { "내용".into() }
            else if index > 0 && classified[index-1]["role"] == "부연설명" { "부연설명".into() }
            else { "미분류".into() };
            circled_rows.push(json!({"index":index,"role":role}));
        } else if role.is_empty() {
            let mut nearest = Vec::new();
            for (rank, name) in ["제목", "장", "중제목", "소제목", "본문", "내용", "부연설명"].iter().enumerate() {
                if let Some(base) = role_indent[name] {
                    let (font,size)=&role_typo[name]; let distance=(indent-base).abs();
                    let penalty=if strv(&record["font"])==*font {0.0}else{150.0}+(num(&record["size_pt"])-*size).abs()*100.0;
                    nearest.push((distance,distance as f64+penalty,rank,*name));
                }
            }
            nearest.sort_by(|a,b|(a.0,a.1,a.2).partial_cmp(&(b.0,b.1,b.2)).unwrap());
            role = if nearest.first().map(|x|x.0<=100).unwrap_or(false) { nearest[0].3.to_owned() }
            else if index==0 && num(&record["size_pt"])>0.0 && records.iter().skip(1).filter_map(|r|r["size_pt"].as_f64()).any(|s|num(&record["size_pt"])>s) { "제목".into() }
            else { "미분류".into() };
        }
        classified.push(json!({"index":index,"role":role,"marker":marker,"record":record}));
    }
    let role_sequence: Vec<Value> = classified.iter().map(|r|r["role"].clone()).collect();
    let mut grouped: Vec<(String,String,Vec<&Value>)> = Vec::new();
    for rec in &classified {
        let role=strv(&rec["role"]); let marker=if strv(&rec["marker"]).is_empty(){"(없음)".into()}else{strv(&rec["marker"])};
        if let Some(group)=grouped.iter_mut().find(|g|g.0==role&&g.1==marker){group.2.push(rec)}else{grouped.push((role,marker,vec![rec]));}
    }
    let roles=["제목","장","중제목","소제목","본문","내용","부연설명"];
    grouped.sort_by_key(|g|(roles.iter().position(|r|*r==g.0).unwrap_or(9),g.2.iter().map(|r|r["index"].as_u64().unwrap_or(0)).min().unwrap_or(0)));
    let mut styles=Vec::new(); let mut variants=Vec::new();
    for (role,marker,items) in grouped {
        let primary=modal(items.iter().map(|r|json!([r["record"]["font"],r["record"]["size_pt"],r["record"]["indent_hwpunit"],r["record"]["left"],r["record"]["indent"]]))).unwrap_or(Value::Null);
        let vals=primary.as_array().cloned().unwrap_or_default();
        let mut spaces:Vec<i64>=items.iter().filter_map(|r|r["record"]["prev_spacing"].as_i64()).collect(); spaces.sort(); spaces.dedup();
        let spacing=modal(items.iter().filter_map(|r|r["record"]["prev_spacing"].as_i64()));
        styles.push(json!({"role":role,"marker":marker,"count":items.len(),"font":vals.get(0).unwrap_or(&Value::Null),"size_pt":vals.get(1).unwrap_or(&Value::Null),"indent_hwpunit":vals.get(2).unwrap_or(&Value::Null),"left_hwpunit":vals.get(3).unwrap_or(&Value::Null),"first_line_hwpunit":vals.get(4).unwrap_or(&Value::Null),"prev_spacing_values":spaces,"prev_spacing_typical":spacing}));
        let mut sigs:Vec<(Value,usize)>=Vec::new();
        for item in &items { let sig=json!([item["record"]["font"],item["record"]["size_pt"],item["record"]["left"],item["record"]["indent"],item["record"]["prev_spacing"]]); if let Some(x)=sigs.iter_mut().find(|x|x.0==sig){x.1+=1}else{sigs.push((sig,1));} }
        sigs.sort_by_key(|x| std::cmp::Reverse(x.1));
        for (sig,count) in sigs { variants.push(json!({"role":role,"marker":marker,"count":count,"kind":if count>1{"반복"}else{"비반복"},"font":sig[0],"size_pt":sig[1],"left_hwpunit":sig[2],"first_line_hwpunit":sig[3],"prev_spacing_hwpunit":sig[4]})); }
    }
    let mut warnings=Vec::new();
    for (i,rec) in classified.iter().enumerate() {
        let role=strv(&rec["role"]);
        if circled_rows.iter().any(|x|x["index"]==i) && ["본문","내용","부연설명"].iter().all(|r|role_indent[r].is_none()) { warnings.push(json!(format!("{}번째 원문자의 역할은 비교할 서식이 없어 확인이 필요합니다.",i+1))); }
        let later:Vec<String>=classified.iter().skip(i+1).map(|x|strv(&x["role"])).collect();
        if role=="소제목" && later.iter().find(|x|x.as_str()!="부연설명").map(String::as_str)!=Some("본문") {warnings.push(json!(format!("{}번째 4단계 아래에 5단계가 없습니다.",i+1)));}
        if role=="내용" && !classified.iter().take(i).any(|x|x["role"]=="본문") {warnings.push(json!(format!("{}번째 6단계에 앞선 5단계가 없습니다.",i+1)));}
        if role=="부연설명" && (i==0 || !["소제목","본문","내용","부연설명"].contains(&strv(&classified[i-1]["role"]).as_str())) {warnings.push(json!(format!("{}번째 부연설명에 짝이 되는 앞 항목이 없습니다.",i+1)));}
    }
    let dtype=document_type(&Value::Array(role_sequence.clone()),false);
    let six=json!({"version":1,"priority":["indentation","font","size","marker"],"auxiliary":"prev_spacing","indent_levels_hwpunit":second["indent_levels"],"ambiguous_marker_roles":{"원문자":["본문","내용","부연설명"]},"styles":styles,"variants":variants,"role_sequence":role_sequence,"document_type":dtype,"warnings":warnings,"page_policy":{"subheading_with_bodies":true,"optional_explanations_after_subheading":true,"body_with_contents":true,"explanation_with_parent":true}});
    json!({"stage_01":first,"stage_02":second,"stage_03":{"records":circled_rows},"stage_04":{"records":[]},"stage_05":{"role_sequence":six["role_sequence"]},"stage_06":{"styles":six["styles"],"variants":six["variants"],"warnings":six["warnings"],"document_type":six["document_type"]},"result":six})
}

pub fn analyze_hierarchy(input: &Value) -> Value {
    return analyze_hierarchy_stages(&json!({"paragraphs": input})) ["result"].clone();
    #[allow(unreachable_code)]
    let empty = Vec::new(); let paras = input.as_array().unwrap_or(&empty);
    let roles = ["장","중제목","소제목","본문","내용","부연설명"];
    let records: Vec<Value> = paras.iter().enumerate().map(|(i,p)| {
        let text = strv(&p["text"]); let (marker, marker_role) = leading_marker(&text);
        let left = p["left"].as_i64().unwrap_or(0) + p["indent"].as_i64().unwrap_or(0);
        json!({"source":p,"index":i,"text":text,"marker":marker,"marker_role":marker_role,"indent":left})
    }).collect();
    let mut role_indent: HashMap<&str, Option<i64>> = HashMap::new();
    let mut role_typo: HashMap<&str, (String,f64)> = HashMap::new();
    for role in roles {
        let marked: Vec<&Value> = records.iter().filter(|r| r["marker_role"] == role).collect();
        role_indent.insert(role, modal(marked.iter().map(|r| r["indent"].as_i64().unwrap_or(0))));
        let styles: Vec<(String, f64)> = marked.iter().map(|r| (strv(&r["source"]["font"]),num(&r["source"]["size_pt"]))).collect();
        let mut selected = styles.first().cloned().unwrap_or_default(); let mut selected_count=0;
        for value in &styles { let count=styles.iter().filter(|x| *x==value).count(); if count>selected_count {selected=value.clone();selected_count=count;} }
        role_typo.insert(role, selected);
    }
    let mut indent_levels: Vec<i64> = records.iter().map(|r|r["indent"].as_i64().unwrap_or(0)).collect(); indent_levels.sort(); indent_levels.dedup();
    let mut classified: Vec<Value> = Vec::new();
    for (i, rec) in records.iter().enumerate() {
        let marker = strv(&rec["marker"]); let mut role = strv(&rec["marker_role"]); let indent=rec["indent"].as_i64().unwrap_or(0);
        let ambiguous = marker.chars().count()==1 && (('①'..='⑳').contains(&marker.chars().next().unwrap()) || ('㉠'..='㉻').contains(&marker.chars().next().unwrap()) || ('❶'..='❿').contains(&marker.chars().next().unwrap()) || ('➊'..='➓').contains(&marker.chars().next().unwrap()));
        if ambiguous {
            let mut cand: Vec<(i64,f64,usize,&str)> = Vec::new();
            for (rank,name) in ["본문","내용","부연설명"].iter().enumerate() { if let Some(base)=role_indent[name] { let (font,size)=&role_typo[name]; let distance=(indent-base).abs(); let penalty=if strv(&rec["source"]["font"])==*font {0.0}else{150.0}+(num(&rec["source"]["size_pt"])-*size).abs()*100.0; cand.push((distance,distance as f64+penalty,rank,name)); } }
            role = if let Some(c)=cand.into_iter().min_by(|a,b| (a.0,a.1,a.2).partial_cmp(&(b.0,b.1,b.2)).unwrap()) {c.3.into()} else if i>0 && classified[i-1]["role"]=="소제목" {"본문".into()} else if i>0 && ["본문","내용"].contains(&strv(&classified[i-1]["role"]).as_str()) {"내용".into()} else if i>0 && classified[i-1]["role"]=="부연설명" {"부연설명".into()} else {"미분류".into()};
        } else if role.is_empty() {
            let mut nearest: Vec<(i64,f64,usize,&str)> = Vec::new();
            for (rank,name) in roles.iter().enumerate() { if let Some(base)=role_indent[name] {let (font,size)=&role_typo[name];let d=(indent-base).abs();let penalty=if strv(&rec["source"]["font"])==*font {0.0}else{150.0}+(num(&rec["source"]["size_pt"])-*size).abs()*100.0;nearest.push((d,d as f64+penalty,rank,name));} }
            nearest.sort_by(|a,b|(a.0,a.1,a.2).partial_cmp(&(b.0,b.1,b.2)).unwrap());
            role=if nearest.first().map(|x|x.0<=100).unwrap_or(false){nearest[0].3.into()}else if i==0&&num(&rec["source"]["size_pt"])>0.0&&paras.iter().skip(1).filter_map(|p|p["size_pt"].as_f64()).any(|s|num(&rec["source"]["size_pt"])>s){"제목".into()}else{"미분류".into()};
        }
        classified.push(json!({"role":role,"marker":marker,"source":rec["source"]}));
    }
    let order = |r:&str| roles.iter().position(|x|*x==r).unwrap_or(9);
    let mut groups: Vec<(String,String,Vec<&Value>)> = Vec::new();
    for rec in &classified { let role=strv(&rec["role"]);let marker=if strv(&rec["marker"]).is_empty(){"(없음)".into()}else{strv(&rec["marker"])};if let Some(g)=groups.iter_mut().find(|g|g.0==role&&g.1==marker){g.2.push(rec)}else{groups.push((role,marker,vec![rec]));} }
    groups.sort_by_key(|g|(order(&g.0),g.2.iter().map(|r|r["source"]["index"].as_u64().unwrap_or(0)).min().unwrap_or(0)));
    let mut styles=Vec::new(); let mut variants=Vec::new();
    for (role,marker,items) in groups { let primary=items[0];let src=&primary["source"];let mut spaces:Vec<i64>=items.iter().filter_map(|x|x["source"]["prev_spacing"].as_i64()).collect();spaces.sort();spaces.dedup();let count=items.len();
        styles.push(json!({"role":role,"marker":marker,"count":count,"font":src["font"],"size_pt":src["size_pt"],"indent_hwpunit":src["left"].as_i64().unwrap_or(0)+src["indent"].as_i64().unwrap_or(0),"left_hwpunit":src["left"],"first_line_hwpunit":src["indent"],"prev_spacing_values":spaces,"prev_spacing_typical":null}));
        for it in items {variants.push(json!({"role":role,"marker":marker,"count":1,"kind":"비반복","font":it["source"]["font"],"size_pt":it["source"]["size_pt"],"left_hwpunit":it["source"]["left"],"first_line_hwpunit":it["source"]["indent"],"prev_spacing_hwpunit":it["source"]["prev_spacing"]}));}
    }
    let role_seq:Vec<Value>=classified.iter().map(|r|r["role"].clone()).collect();
    json!({"version":1,"priority":["indentation","font","size","marker"],"auxiliary":"prev_spacing","indent_levels_hwpunit":indent_levels,"ambiguous_marker_roles":{"원문자":["본문","내용","부연설명"]},"styles":styles,"variants":variants,"role_sequence":role_seq,"document_type":document_type(&Value::Array(classified.iter().map(|r|r["role"].clone()).collect()),false),"warnings":[],"page_policy":{"subheading_with_bodies":true,"optional_explanations_after_subheading":true,"body_with_contents":true,"explanation_with_parent":true}})
}
