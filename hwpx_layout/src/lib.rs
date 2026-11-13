//! 한/글이 저장한 HWPX의 줄 배치 캐시(`hp:linesegarray`)를 읽어
//! 문단별 줄 수·줄 시작 위치·쪽 번호를 계산한다(COMX1 연구 프로토타입).
//!
//! 위치 단위는 한/글 COM `GetPos()`의 세 번째 값과 같다(UTF-16 글자 1, 컨트롤 8,
//! 탭 8, 줄바꿈 등 글자 컨트롤 1). 실측: `docs/COMX1_LINESEG_2026-11-13.md`.
//!
//! 주의: lineseg는 한/글이 마지막으로 **저장할 때** 계산한 값이다. 한/글은 파일을 열 때
//! lineseg가 있으면 줄 나눔(textpos)을 다시 계산하지 않으므로, XML만 고친 문단은
//! lineseg를 지워 한/글이 다시 배치하게 해야 하고, 그런 파일의 lineseg를 이 도구로
//! 읽어 판단하면 안 된다(`stale` 위험).

use quick_xml::events::{BytesStart, Event};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineSeg {
    pub textpos: i64,
    pub vertpos: i64,
    pub vertsize: i64,
    pub textheight: i64,
    pub baseline: i64,
    pub spacing: i64,
    pub horzpos: i64,
    pub horzsize: i64,
    pub flags: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Para {
    /// 구역 번호(0부터)
    pub section: usize,
    /// 바깥에서 안쪽으로 컨트롤 이름(본문은 빈 목록, 표 칸이면 ["tbl"], 표 안 글상자면 ["tbl","rect"])
    pub ctx: Vec<String>,
    /// 자기 목록(본문·칸·글상자) 안 문단 순번(0부터). 본문은 COM 문단 번호와 같다.
    pub index: usize,
    /// 이 문단을 품은 본문 문단의 전체 순번(본문 문단은 자기 자신)
    pub body_para: usize,
    pub page_break: bool,
    pub column_break: bool,
    pub text: String,
    /// COM 위치 단위 문단 길이(문단 끝 위치)
    pub len: i64,
    pub lines: Vec<LineSeg>,
    /// 줄마다 쪽(1부터). 본문 밖 문단은 품은 본문 문단의 컨트롤 위치 줄의 쪽.
    pub pages: Vec<u32>,
    /// 부모 문단 안에서 이 문단을 품은 컨트롤의 위치(본문 문단은 None)
    anchor: Option<(usize, i64)>,
}

#[derive(Debug, Default)]
pub struct Layout {
    pub paras: Vec<Para>,
    pub pages: u32,
    /// lineseg가 없는 문단 수(한/글이 저장하지 않았거나 XML 편집으로 지운 문단)
    pub missing: usize,
}

const MAX_SECTION: u64 = 256 * 1024 * 1024;

fn local(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&b| b == b':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().with_checks(false).flatten().find(|a| local(a.key.as_ref()) == key)
        .map(|a| String::from_utf8_lossy(&a.value).into_owned())
}

fn seg_from(e: &BytesStart) -> LineSeg {
    let mut s = LineSeg::default();
    for a in e.attributes().with_checks(false).flatten() {
        let v: i64 = std::str::from_utf8(&a.value).ok().and_then(|x| x.parse().ok()).unwrap_or(0);
        match local(a.key.as_ref()) {
            b"textpos" => s.textpos = v,
            b"vertpos" => s.vertpos = v,
            b"vertsize" => s.vertsize = v,
            b"textheight" => s.textheight = v,
            b"baseline" => s.baseline = v,
            b"spacing" => s.spacing = v,
            b"horzpos" => s.horzpos = v,
            b"horzsize" => s.horzsize = v,
            b"flags" => s.flags = v,
            _ => {}
        }
    }
    s
}

fn utf16_len(s: &str) -> i64 {
    s.encode_utf16().count() as i64
}

/// `t` 안 빈 요소의 위치 폭. 탭은 컨트롤(8), 나머지 글자 컨트롤은 1.
fn inline_width(name: &[u8]) -> (i64, Option<char>) {
    match name {
        b"tab" => (8, Some('\t')),
        b"lineBreak" => (1, Some('\n')),
        b"hyphen" => (1, Some('-')),
        b"nbSpace" | b"fwSpace" => (1, Some(' ')),
        _ => (0, None), // markpenBegin/End, titleMark 등 범위 표시
    }
}

/// section XML 하나를 읽어 문단 목록에 덧붙인다.
pub fn parse_section(xml: &[u8], section: usize, out: &mut Vec<Para>) -> Result<(), String> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut elems: Vec<Vec<u8>> = Vec::new();
    // 열린 문단(out 안 인덱스) 스택
    let mut pstack: Vec<usize> = Vec::new();
    // 목록 스택: (컨트롤 이름, 다음 문단 순번, 부모 문단 out 인덱스, 컨트롤 위치)
    let mut lists: Vec<(Option<String>, usize, Option<(usize, i64)>)> = vec![(None, 0, None)];
    let mut ctx: Vec<String> = Vec::new();
    let mut last_body = 0usize;
    let mut buf = Vec::new();
    loop {
        let ev = reader.read_event_into(&mut buf).map_err(|e| format!("XML 파싱 실패(section{section}): {e}"))?;
        match ev {
            Event::DocType(_) => return Err("DTD 거부".into()),
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(ev, Event::Empty(_));
                let name = local(e.name().as_ref()).to_vec();
                let parent = elems.last().map(|v| v.as_slice());
                match name.as_slice() {
                    b"p" if !is_empty => {
                        let lst = lists.last_mut().unwrap();
                        let index = lst.1;
                        lst.1 += 1;
                        let anchor = lst.2;
                        let id = out.len();
                        let body_para = match anchor {
                            None => { last_body = id; id }
                            Some((pi, _)) => out[pi].body_para,
                        };
                        out.push(Para {
                            section, ctx: ctx.clone(), index, body_para,
                            page_break: attr(e, b"pageBreak").as_deref() == Some("1"),
                            column_break: attr(e, b"columnBreak").as_deref() == Some("1"),
                            anchor, ..Default::default()
                        });
                        pstack.push(id);
                    }
                    b"subList" if !is_empty => {
                        // 이 목록을 품은 컨트롤: 가장 가까운 run의 자식 요소
                        let ctrl = elems.iter().rposition(|n| n == b"run")
                            .and_then(|i| elems.get(i + 1)).map(|n| String::from_utf8_lossy(n).into_owned())
                            .unwrap_or_else(|| "?".into());
                        let anchor = pstack.last().map(|&pi| (pi, out[pi].len.saturating_sub(8).max(0)));
                        ctx.push(ctrl.clone());
                        lists.push((Some(ctrl), 0, anchor));
                    }
                    b"lineseg" if parent == Some(b"linesegarray") => {
                        if let Some(&pi) = pstack.last() {
                            out[pi].lines.push(seg_from(e));
                        }
                    }
                    _ => {
                        if let Some(&pi) = pstack.last() {
                            if parent == Some(b"run") {
                                // run 바로 아래 t가 아닌 요소 = 컨트롤 1개(8). 위치는 여는 순간 더한다.
                                if name.as_slice() != b"t" {
                                    out[pi].len += 8;
                                }
                            } else if parent == Some(b"t") {
                                let (w, ch) = inline_width(&name);
                                out[pi].len += w;
                                if let Some(c) = ch { out[pi].text.push(c); }
                            }
                        }
                    }
                }
                if !is_empty {
                    elems.push(name);
                }
            }
            Event::Text(t) => {
                if elems.last().map(|v| v.as_slice()) == Some(b"t") {
                    if let Some(&pi) = pstack.last() {
                        let s = t.decode().map_err(|e| format!("텍스트 해석 실패: {e}"))?;
                        out[pi].len += utf16_len(&s);
                        out[pi].text.push_str(&s);
                    }
                }
            }
            Event::GeneralRef(r) => {
                if elems.last().map(|v| v.as_slice()) == Some(b"t") {
                    if let Some(&pi) = pstack.last() {
                        let name = String::from_utf8_lossy(&r).into_owned();
                        let ch = match name.as_str() {
                            "amp" => Some('&'), "lt" => Some('<'), "gt" => Some('>'), "quot" => Some('"'), "apos" => Some('\''),
                            n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
                            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
                            _ => return Err(format!("알 수 없는 엔티티 &{name};")),
                        };
                        if let Some(c) = ch {
                            out[pi].len += c.len_utf16() as i64;
                            out[pi].text.push(c);
                        }
                    }
                }
            }
            Event::End(e) => {
                let name = local(e.name().as_ref()).to_vec();
                match name.as_slice() {
                    b"p" => { pstack.pop(); }
                    b"subList" => { lists.pop(); ctx.pop(); }
                    _ => {}
                }
                elems.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    let _ = last_body;
    Ok(())
}

/// 쪽 계산: 본문 흐름에서 vertpos가 줄면(쪽 맨 위로 돌아가면) 새 쪽, 쪽 나누기 문단, 구역 시작.
/// 다단(단 나누기)은 vertpos가 단마다 돌아가므로 이 규칙이 쪽으로 잘못 센다(연구 범위 밖, 문서에 기록).
pub fn assign_pages(paras: &mut [Para]) -> u32 {
    let mut page = 0u32;
    let mut last_v: Option<i64> = None;
    let mut last_section: Option<usize> = None;
    for i in 0..paras.len() {
        if paras[i].anchor.is_some() {
            continue;
        }
        let p = &mut paras[i];
        let mut pages = Vec::with_capacity(p.lines.len());
        if last_section != Some(p.section) {
            page += 1;
            last_v = None;
            last_section = Some(p.section);
        }
        for (k, s) in p.lines.iter().enumerate() {
            let back = last_v.is_some_and(|v| s.vertpos < v);
            let brk = k == 0 && p.page_break && last_v.is_some();
            if back || brk {
                page += 1;
            }
            last_v = Some(s.vertpos);
            pages.push(page);
        }
        if p.lines.is_empty() {
            pages.push(page.max(1));
        }
        p.pages = pages;
    }
    // 본문 밖 문단: 품은 문단에서 컨트롤 위치가 든 줄의 쪽(조상을 먼저 처리하도록 순서대로)
    for i in 0..paras.len() {
        if let Some((pi, pos)) = paras[i].anchor {
            let parent = &paras[pi];
            let k = parent.lines.iter().rposition(|s| s.textpos <= pos).unwrap_or(0);
            let pg = parent.pages.get(k).copied().unwrap_or(1);
            paras[i].pages = vec![pg; paras[i].lines.len().max(1)];
        }
    }
    page.max(1)
}

pub fn read_layout(path: &Path) -> Result<Layout, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("파일 열기 실패: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("HWPX 압축 열기 실패: {e}"))?;
    let mut sections: Vec<(u64, String)> = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?.to_string();
        if let Some(rest) = name.strip_prefix("Contents/section") {
            if let Some(num) = rest.strip_suffix(".xml") {
                if let Ok(n) = num.parse::<u64>() {
                    if entry.size() > MAX_SECTION {
                        return Err(format!("구역 XML이 너무 큼: {name}"));
                    }
                    sections.push((n, name));
                }
            }
        }
    }
    if sections.is_empty() {
        return Err("Contents/section*.xml 없음".into());
    }
    sections.sort();
    let mut paras = Vec::new();
    for (idx, (_, name)) in sections.iter().enumerate() {
        let mut entry = zip.by_name(name).map_err(|e| e.to_string())?;
        let mut xml = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut xml).map_err(|e| e.to_string())?;
        parse_section(&xml, idx, &mut paras)?;
    }
    let pages = assign_pages(&mut paras);
    let missing = paras.iter().filter(|p| p.lines.is_empty()).count();
    Ok(Layout { paras, pages, missing })
}

impl Para {
    pub fn line_starts(&self) -> Vec<i64> {
        self.lines.iter().map(|s| s.textpos).collect()
    }
    pub fn is_body(&self) -> bool {
        self.ctx.is_empty()
    }
}

pub fn to_json(layout: &Layout) -> serde_json::Value {
    use serde_json::json;
    json!({
        "pages": layout.pages,
        "paragraphs": layout.paras.len(),
        "missing_lineseg": layout.missing,
        "paras": layout.paras.iter().map(|p| json!({
            "section": p.section, "ctx": p.ctx, "index": p.index, "body_para": p.body_para,
            "page_break": p.page_break, "len": p.len, "text": p.text,
            "starts": p.line_starts(), "pages": p.pages,
            "lines": p.lines.iter().map(|s| [s.textpos, s.vertpos, s.vertsize, s.textheight, s.baseline, s.spacing, s.horzpos, s.horzsize, s.flags]).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section""#;

    fn seg(tp: i64, vp: i64) -> String {
        format!(r#"<hp:lineseg textpos="{tp}" vertpos="{vp}" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="42520" flags="393216"/>"#)
    }

    fn sample() -> String {
        format!(
            r#"<hs:sec {NS}><hp:p pageBreak="0"><hp:run><hp:secPr/><hp:ctrl><hp:colPr/></hp:ctrl></hp:run><hp:run><hp:t>가나</hp:t></hp:run><hp:linesegarray>{}</hp:linesegarray></hp:p><hp:p pageBreak="0"><hp:run><hp:tbl><hp:tr><hp:tc><hp:subList><hp:p><hp:run><hp:t>칸&amp;</hp:t></hp:run><hp:linesegarray>{}</hp:linesegarray></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl><hp:t>A<hp:tab/>B</hp:t></hp:run><hp:linesegarray>{}{}</hp:linesegarray></hp:p><hp:p pageBreak="1"><hp:run><hp:t>다</hp:t></hp:run><hp:linesegarray>{}</hp:linesegarray></hp:p></hs:sec>"#,
            seg(0, 0), seg(0, 0), seg(0, 1600), seg(9, 52000), seg(0, 0)
        )
    }

    #[test]
    fn parses_positions_contexts_and_pages() {
        let mut paras = Vec::new();
        parse_section(sample().as_bytes(), 0, &mut paras).unwrap();
        let pages = assign_pages(&mut paras);
        assert_eq!(paras.len(), 4);
        // 본문 0: 컨트롤 2개(16) + 글자 2
        assert_eq!(paras[0].len, 18);
        assert_eq!(paras[0].text, "가나");
        // 문서 순서(전위): 표를 품은 본문 문단이 먼저, 그 안 칸 문단이 다음
        // 본문 1: 표 8 + A 1 + 탭 8 + B 1
        assert_eq!(paras[1].len, 18);
        assert_eq!(paras[1].index, 1);
        assert_eq!(paras[1].text, "A	B");
        assert_eq!(paras[1].line_starts(), vec![0, 9]);
        // 표 칸 문단: 엔티티 풀기, 품은 본문 문단 번호
        assert_eq!(paras[2].ctx, vec!["tbl".to_string()]);
        assert_eq!(paras[2].text, "칸&");
        assert_eq!(paras[2].body_para, 1);
        assert_eq!(paras[2].index, 0);
        // vertpos 감소로 새 쪽: 두 번째 줄은 1쪽(52000 > 1600), 쪽 나누기 문단은 2쪽
        assert_eq!(paras[1].pages, vec![1, 1]);
        assert_eq!(paras[3].pages, vec![2]);
        assert_eq!(paras[2].pages, vec![1]);
        assert_eq!(pages, 2);
    }

    #[test]
    fn vertpos_reset_starts_new_page() {
        let xml = format!(r#"<hs:sec {NS}><hp:p><hp:run><hp:t>a</hp:t></hp:run><hp:linesegarray>{}{}</hp:linesegarray></hp:p></hs:sec>"#, seg(0, 70000), seg(1, 0));
        let mut paras = Vec::new();
        parse_section(xml.as_bytes(), 0, &mut paras).unwrap();
        assert_eq!(assign_pages(&mut paras), 2);
        assert_eq!(paras[0].pages, vec![1, 2]);
    }

    #[test]
    fn missing_lineseg_is_counted_not_guessed() {
        let xml = format!(r#"<hs:sec {NS}><hp:p><hp:run><hp:t>a</hp:t></hp:run></hp:p></hs:sec>"#);
        let mut paras = Vec::new();
        parse_section(xml.as_bytes(), 0, &mut paras).unwrap();
        assign_pages(&mut paras);
        assert!(paras[0].lines.is_empty());
        assert!(paras[0].line_starts().is_empty());
    }

    #[test]
    fn rejects_doctype() {
        let mut paras = Vec::new();
        assert!(parse_section(br#"<!DOCTYPE x [<!ENTITY a "b">]><x/>"#, 0, &mut paras).is_err());
    }

    #[test]
    fn reads_zip_from_temp_dir() {
        let dir = std::env::temp_dir().join(format!("hwpx-layout-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.hwpx");
        {
            let f = std::fs::File::create(&path).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opt = zip::write::SimpleFileOptions::default();
            z.start_file("mimetype", opt).unwrap();
            std::io::Write::write_all(&mut z, b"application/hwp+zip").unwrap();
            z.start_file("Contents/section0.xml", opt).unwrap();
            std::io::Write::write_all(&mut z, sample().as_bytes()).unwrap();
            z.finish().unwrap();
        }
        let layout = read_layout(&path).unwrap();
        assert_eq!(layout.paras.len(), 4);
        assert_eq!(layout.pages, 2);
        assert_eq!(layout.missing, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
