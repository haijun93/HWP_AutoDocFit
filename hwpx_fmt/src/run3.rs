//! RSXML3 처리기 5종의 조사·처리 흐름(파이썬 `*_hwpx_처리(source, target, selections)` 와 같은 순서·같은 실패 조건).
use crate::config::Config;
use crate::dom::Doc;
use crate::fmt3::{self, AttachCtx, AttachState, ColonChars, Key};
use crate::rules::{is_section_name, py_ws, R};
use crate::zipio::{self, Contents};
use crate::{check_other_xml, parse_header, parse_sections, parse_sections_sorted, Processor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

type Sections = Vec<(String, usize, Doc)>;

fn pair_list(v: &Value) -> R<Vec<(usize, usize)>> {
    let arr = v.as_array().ok_or("선택 목록이 배열이 아닙니다.")?;
    let mut out = vec![];
    for x in arr {
        let a = x.as_array().filter(|a| a.len() == 2).ok_or("선택 항목은 [번호, 번호] 쌍이어야 합니다.")?;
        let n = |i: usize| a[i].as_u64().map(|u| u as usize).ok_or("정수가 아닌 번호");
        out.push((n(0)?, n(1)?));
    }
    Ok(out)
}

fn int_list(v: &Value) -> R<Vec<usize>> {
    let arr = v.as_array().ok_or("선택 목록이 배열이 아닙니다.")?;
    arr.iter().map(|x| x.as_u64().map(|u| u as usize).ok_or_else(|| "정수가 아닌 번호".to_string())).collect()
}

fn pairs_json(v: &[(usize, usize)]) -> Value {
    Value::Array(v.iter().map(|&(a, b)| json!([a, b])).collect())
}

fn is_all_empty(sel: &Value) -> R<bool> {
    let obj = sel.as_object().ok_or("selections 는 객체여야 합니다.")?;
    Ok(obj.values().all(|v| v.as_array().map_or(false, |a| a.is_empty())))
}

fn copy_out(source: &Path, target: &Path) -> R<usize> {
    zipio::copy_file_safely(source, target)?;
    Ok(0)
}

fn sections_for(p: Processor, contents: &Contents) -> R<Sections> {
    match p {
        Processor::ColonLabel | Processor::BlankLines | Processor::Emphasis => parse_sections_sorted(contents),
        _ => parse_sections(contents),
    }
}

/// 파이썬 처리기 첫머리의 설정 검사(반영하지 못하는 설정은 거부).
fn check_config3(p: Processor, cfg: &Config) -> R<()> {
    use crate::config::unsupported;
    match p {
        Processor::BlankLines if !cfg.title_owner_text.is_empty() => {
            unsupported("제목_담당자_글 설정(title_owner_text)은 서식표 판별에 반영하지 못합니다.")
        }
        _ => Ok(()),
    }
}

pub fn survey(p: Processor, source: &Path, cfg: &Config) -> R<Value> {
    check_config3(p, cfg)?;
    let contents = zipio::read_hwpx_safely(source)?;
    check_other_xml(&contents)?;
    let sections = sections_for(p, &contents)?;
    let mut m = serde_json::Map::new();
    match p {
        Processor::AsteriskSuper => {
            let (_, h) = parse_header(&contents)?;
            for (n, _, d) in &sections {
                m.insert(n.clone(), pairs_json(&fmt3::asterisk_survey(d, &h)));
            }
        }
        Processor::AttachFont => {
            let (_, h) = parse_header(&contents)?;
            let docs: Vec<&Doc> = sections.iter().map(|(_, _, d)| d).collect();
            let ctx = AttachCtx::new(&h, &docs, cfg)?;
            for (n, _, d) in &sections {
                m.insert(n.clone(), pairs_json(&ctx.survey(&h, d)?));
            }
        }
        Processor::ColonLabel => {
            parse_header(&contents)?;
            for (n, _, d) in &sections {
                let v = if cfg.colon_label_enabled { fmt3::colon_survey(d) } else { vec![] };
                m.insert(n.clone(), json!(v));
            }
        }
        Processor::BlankLines => crate::run3b::survey_blank(&sections, cfg, &mut m)?,
        Processor::Emphasis => crate::run3b::survey_emphasis(&sections, cfg, &mut m)?,
        _ => return Err("RSXML3 처리기가 아닙니다.".into()),
    }
    Ok(Value::Object(m))
}

pub fn process(p: Processor, source: &Path, target: &Path, sel: Option<&Value>, cfg: &Config) -> R<usize> {
    check_config3(p, cfg)?;
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(source), std::fs::canonicalize(target)) {
        if a == b {
            return Err("입력과 출력이 같은 파일입니다(원본 보존).".into());
        }
    }
    let mut contents = zipio::read_hwpx_safely(source)?;
    check_other_xml(&contents)?;
    let mut sections = sections_for(p, &contents)?;
    let sel: Value = match sel {
        Some(v) => v.clone(),
        None => survey(p, source, cfg)?,
    };
    match p {
        Processor::AsteriskSuper => {
            let (hidx, mut h) = parse_header(&contents)?;
            if is_all_empty(&sel)? {
                return copy_out(source, target);
            }
            let sup = fmt3::super_ids(&h);
            let mut cache: HashMap<Key, Key> = HashMap::new();
            let mut count = 0;
            for (name, items) in sel.as_object().unwrap() {
                let items = pair_list(items)?;
                let (_, idx, d) = sections.iter_mut().find(|(n, _, _)| n == name).ok_or_else(|| format!("구역이 없습니다: {name}"))?;
                count += fmt3::asterisk_apply(d, &mut h, &items, &sup, &mut cache)?;
                contents[*idx].1 = d.tostring();
            }
            contents[hidx].1 = h.tostring();
            zipio::write_hwpx_safely(&contents, target)?;
            Ok(count)
        }
        Processor::AttachFont => {
            let (hidx, mut h) = parse_header(&contents)?;
            let mut ctx = {
                let docs: Vec<&Doc> = sections.iter().map(|(_, _, d)| d).collect();
                AttachCtx::new(&h, &docs, cfg)?
            };
            if is_all_empty(&sel)? {
                return copy_out(source, target);
            }
            let mut st = AttachState::new();
            let mut count = 0;
            for (name, items) in sel.as_object().unwrap() {
                let items = pair_list(items)?;
                let (_, idx, d) = sections.iter_mut().find(|(n, _, _)| n == name).ok_or_else(|| format!("구역이 없습니다: {name}"))?;
                count += fmt3::attach_apply(&mut ctx, &mut st, &mut h, d, &items)?;
                contents[*idx].1 = d.tostring();
            }
            contents[hidx].1 = h.tostring();
            zipio::write_hwpx_safely(&contents, target)?;
            Ok(count)
        }
        Processor::ColonLabel => {
            let (hidx, mut h) = parse_header(&contents)?;
            let obj = sel.as_object().ok_or("selections 는 객체여야 합니다.")?;
            for v in obj.values() {
                int_list(v)?;
            }
            if is_all_empty(&sel)? {
                return copy_out(source, target);
            }
            let before: Vec<String> = sections.iter().map(|(_, _, d)| nows(&d.jstr_walk(0))).collect();
            // 모든 구역의 묶음을 먼저 찾는다(수정 전). 컨테이너 순서 = 구역 순서 × (뿌리, subList 들)
            let all: Vec<Vec<fmt3::ColonGroup>> = sections
                .iter()
                .map(|(_, _, d)| {
                    let mut g = vec![];
                    for c in fmt3::colon_containers(d) {
                        g.extend(fmt3::groups_in(d, c));
                    }
                    g
                })
                .collect();
            let mut ch = ColonChars::new(&h);
            let mut stats = (0usize, 0usize);
            for (k, groups) in all.iter().enumerate() {
                let d = &mut sections[k].2;
                for g in groups {
                    fmt3::colon_apply_group(d, &mut h, &mut ch, g, cfg.ratio_reduce, &mut stats)?;
                }
            }
            for (k, (_, _, d)) in sections.iter().enumerate() {
                if nows(&d.jstr_walk(0)) != before[k] {
                    return Err("콜론 라벨 정렬 중 글 보존 검사에 실패했습니다.".into());
                }
            }
            if stats.0 == 0 {
                return copy_out(source, target);
            }
            for (_, idx, d) in &sections {
                contents[*idx].1 = d.tostring();
            }
            contents[hidx].1 = h.tostring();
            zipio::write_hwpx_safely(&contents, target)?;
            Ok(stats.0)
        }
        Processor::BlankLines => crate::run3b::process_blank(&mut contents, sections, &sel, source, target, cfg),
        Processor::Emphasis => crate::run3b::process_emphasis(&mut contents, sections, &sel, source, target, cfg),
        _ => Err("RSXML3 처리기가 아닙니다.".into()),
    }
}

fn nows(s: &str) -> String {
    s.chars().filter(|c| !py_ws(*c)).collect()
}

#[allow(dead_code)]
fn _keep(n: &str) -> bool {
    is_section_name(n)
}
