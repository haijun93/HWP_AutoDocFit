//! RSXML4 처리기 4종(표 칸 너비·기본 표 서식·정렬 재확인·연결 부호 탭)의 조사·처리 흐름.
//! 파이썬 `표너비_hwpx_처리`·`기본표서식_hwpx_처리`·`연결부호탭_hwpx_처리(source, target, selections)` 와 같은 순서·같은 실패 조건.
use crate::config::{unsupported, Config};
use crate::conn;
use crate::dom::Doc;
use crate::rules::{format_kind, protected_tables, R};
use crate::tbl::{self, HIdx};
use crate::tstyle::{self, Pool};
use crate::zipio::{self, Contents};
use crate::{check_other_xml, parse_header, parse_sections, parse_sections_sorted, section_number_key, Processor};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn int_list(v: &Value) -> R<Vec<usize>> {
    let arr = v.as_array().ok_or("선택 목록이 배열이 아닙니다.")?;
    arr.iter().map(|x| x.as_u64().map(|u| u as usize).ok_or_else(|| "정수가 아닌 번호".to_string())).collect()
}

fn all_empty(sel: &Value) -> R<bool> {
    let obj = sel.as_object().ok_or("selections 는 객체여야 합니다.")?;
    for v in obj.values() {
        int_list(v)?;
    }
    Ok(obj.values().all(|v| v.as_array().map_or(false, |a| a.is_empty())))
}

fn copy_out(source: &Path, target: &Path) -> R<usize> {
    zipio::copy_file_safely(source, target)?;
    Ok(0)
}

fn check_config4(p: Processor, cfg: &Config) -> R<()> {
    match p {
        Processor::TableWidth | Processor::TableStyle | Processor::TableStyleAlign if !cfg.title_owner_text.is_empty() => {
            unsupported("제목_담당자_글 설정(title_owner_text)은 서식표 판별에 반영하지 못합니다.")
        }
        Processor::ConnectorTab if cfg.standard_margin_planned => {
            unsupported("표준서식이 편집 여백을 바꿀 예정인 경우의 최종 본문 폭(_최종_본문폭)은 반영하지 못합니다.")
        }
        _ => Ok(()),
    }
}

/// `표들(root)`(표 칸 너비): 본문 직속 문단 안 표(root > * > * > tbl)와 그 표가 놓인 문단
fn top_tables(d: &Doc) -> Vec<(usize, usize)> {
    let mut v = vec![];
    for &p in &d.nodes[0].children {
        for &r in &d.nodes[p].children {
            for &t in &d.nodes[r].children {
                if d.tag(t) == "tbl" {
                    v.push((t, p));
                }
            }
        }
    }
    v
}

fn width_target(d: &Doc, t: usize) -> R<bool> {
    Ok(format_kind(d, t)?.is_none() && tbl::is_target(d, t)?)
}

type Loaded = (Contents, Vec<(String, usize, Doc)>, usize, Doc);

fn load(source: &Path) -> R<Loaded> {
    let contents = zipio::read_hwpx_safely(source)?;
    check_other_xml(&contents)?;
    let sections = parse_sections(&contents)?;
    let (hidx, h) = parse_header(&contents)?;
    Ok((contents, sections, hidx, h))
}

fn style_of(cfg: &Config) -> R<Option<tstyle::Style>> {
    match &cfg.table_style {
        Some(v) => tstyle::parse_style(v),
        None => Ok(None),
    }
}

pub fn survey(p: Processor, source: &Path, cfg: &Config) -> R<Value> {
    check_config4(p, cfg)?;
    let mut m = serde_json::Map::new();
    match p {
        Processor::TableWidth => {
            let (_, sections, _, _) = load(source)?;
            for (n, _, d) in &sections {
                let mut v = vec![];
                if tbl::section_width(d, None)?.map_or(false, |w| w != 0) {
                    for (i, (t, _)) in top_tables(d).into_iter().enumerate() {
                        if width_target(d, t)? {
                            v.push(i);
                        }
                    }
                }
                m.insert(n.clone(), json!(v));
            }
        }
        Processor::TableStyle | Processor::TableStyleAlign => {
            let style = style_of(cfg)?;
            let (_, sections, _, h) = load(source)?;
            if style.is_none() {
                for (n, _, _) in &sections {
                    m.insert(n.clone(), json!([]));
                }
            } else {
                let mut order: Vec<usize> = (0..sections.len()).collect();
                order.sort_by_key(|&i| section_number_key(&sections[i].0));
                let idx = HIdx::new(&h)?;
                let roots: Vec<&Doc> = order.iter().map(|&i| &sections[i].2).collect();
                let head: HashSet<(usize, usize)> = tstyle::head_block_tables(&roots, &idx).into_iter().map(|(ri, n)| (order[ri], n)).collect();
                let mut prot: Vec<HashSet<usize>> = vec![];
                for (_, _, d) in &sections {
                    prot.push(protected_tables(d)?);
                }
                for (k, (n, _, d)) in sections.iter().enumerate() {
                    let mut v = vec![];
                    for (i, t) in d.tables().into_iter().enumerate() {
                        if tstyle::is_target(d, t)? && !head.contains(&(k, t)) && !prot[k].contains(&t) {
                            v.push(i);
                        }
                    }
                    m.insert(n.clone(), json!(v));
                }
            }
        }
        Processor::ConnectorTab => {
            let contents = zipio::read_hwpx_safely(source)?;
            check_other_xml(&contents)?;
            parse_header(&contents)?;
            let sections = parse_sections_sorted(&contents)?;
            for (n, _, d) in &sections {
                let k = if cfg.connector_tab_enabled { conn::containers(d).into_iter().map(|c| conn::groups_in(d, c).len()).sum::<usize>() } else { 0 };
                m.insert(n.clone(), json!((0..k).collect::<Vec<_>>()));
            }
        }
        _ => return Err("RSXML4 처리기가 아닙니다.".into()),
    }
    Ok(Value::Object(m))
}

pub fn process(p: Processor, source: &Path, target: &Path, sel: Option<&Value>, cfg: &Config) -> R<usize> {
    check_config4(p, cfg)?;
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(source), std::fs::canonicalize(target)) {
        if a == b {
            return Err("입력과 출력이 같은 파일입니다(원본 보존).".into());
        }
    }
    let sel: Value = match sel {
        Some(v) => v.clone(),
        None => survey(p, source, cfg)?,
    };
    match p {
        Processor::TableWidth => {
            let (mut contents, mut sections, _, h) = load(source)?;
            if all_empty(&sel)? {
                return copy_out(source, target);
            }
            let idx = HIdx::new(&h)?;
            let mut count = 0;
            for (name, items) in sel.as_object().unwrap() {
                let items = int_list(items)?;
                let (_, ci, d) = sections.iter_mut().find(|(n, _, _)| n == name).ok_or_else(|| format!("구역이 없습니다: {name}"))?;
                let found = top_tables(d);
                let host: HashMap<usize, usize> = found.iter().copied().collect();
                let width = tbl::section_width(d, None)?;
                for number in items {
                    let table = found.get(number).ok_or("list index out of range")?.0;
                    if !width_target(d, table)? {
                        return Err("처리 중 표 구조가 바뀌어 표 칸 너비 맞춤을 중단했습니다.".into());
                    }
                    let reserve = tbl::host_reserve(d, host.get(&table).copied(), table, &idx)?;
                    if tbl::fit_table(d, table, width, &idx, reserve)? {
                        count += 1;
                    }
                }
                contents[*ci].1 = d.tostring();
            }
            zipio::write_hwpx_safely(&contents, target)?;
            Ok(count)
        }
        Processor::TableStyle | Processor::TableStyleAlign => {
            let style = style_of(cfg)?;
            let (mut contents, mut sections, hidx, mut h) = load(source)?;
            let obj = sel.as_object().ok_or("selections 는 객체여야 합니다.")?;
            let any = !all_empty(&sel)?;
            let Some(style) = style.filter(|_| any) else {
                return copy_out(source, target);
            };
            let mut pool = Pool::new(&h, &style)?;
            let mut count = 0;
            for (name, items) in obj {
                let items = int_list(items)?;
                let (_, ci, d) = sections.iter_mut().find(|(n, _, _)| n == name).ok_or_else(|| format!("구역이 없습니다: {name}"))?;
                let found = d.tables();
                for index in items {
                    let table = *found.get(index).ok_or("list index out of range")?;
                    if !tstyle::is_target(d, table)? {
                        return Err("처리 중 표 구조가 바뀌어 기본 표 서식 적용을 중단했습니다.".into());
                    }
                    tstyle::apply_table_style(d, &mut h, &mut pool, table)?;
                    count += 1;
                }
                contents[*ci].1 = d.tostring();
            }
            contents[hidx].1 = h.tostring();
            zipio::write_hwpx_safely(&contents, target)?;
            Ok(count)
        }
        Processor::ConnectorTab => {
            let mut contents = zipio::read_hwpx_safely(source)?;
            check_other_xml(&contents)?;
            let (hidx, mut h) = parse_header(&contents)?;
            let mut sections = parse_sections_sorted(&contents)?;
            if all_empty(&sel)? {
                return copy_out(source, target);
            }
            let mut body = vec![];
            for (_, _, d) in &sections {
                // `_최종_본문폭(root) or _구역_본문폭(root)`: 쪽 정보가 없으면 42520
                body.push(match tbl::section_width(d, None)? {
                    Some(w) if w != 0 => w,
                    Some(_) => 0,
                    None => 42520,
                });
            }
            let stats = {
                let mut refs: Vec<&mut Doc> = sections.iter_mut().map(|(_, _, d)| d).collect();
                conn::align_connectors(&mut h, &mut refs, &body)?
            };
            if stats.paragraphs == 0 {
                return copy_out(source, target);
            }
            for (_, ci, d) in &sections {
                contents[*ci].1 = d.tostring();
            }
            contents[hidx].1 = h.tostring();
            zipio::write_hwpx_safely(&contents, target)?;
            Ok(stats.paragraphs)
        }
        _ => Err("RSXML4 처리기가 아닙니다.".into()),
    }
}
