//! 최소 XML 트리: Python ElementTree(defusedxml 파싱)와 같은 text/tail 모델.
//! 요소는 문서 순서(선행 순회)로 배열에 쌓으므로, 한 요소의 `iter()`는 연속 구간 [i, end)이다.
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Clone)]
pub struct Node {
    pub qname: String,
    pub local: String,
    pub attrs: Vec<(String, String)>,
    pub text: Option<String>,
    pub tail: Option<String>,
    pub children: Vec<usize>,
    /// 이 요소 하위 전체가 끝나는 다음 인덱스(배타). iter() = i..end
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct Doc {
    pub nodes: Vec<Node>,
    /// 루트 앞 XML 선언 원문(없으면 None). 출력에는 항상 UTF-8 선언을 쓴다.
    pub decl: Option<String>,
}

fn local_of(q: &str) -> String {
    q.rsplit(':').next().unwrap_or(q).to_string()
}

fn put_text(nodes: &mut [Node], stack: &[usize], buf: &mut String) {
    if buf.is_empty() {
        return;
    }
    let top = *stack.last().unwrap();
    let s = std::mem::take(buf);
    if let Some(&last) = nodes[top].children.last() {
        nodes[last].tail = Some(s);
    } else {
        nodes[top].text = Some(s);
    }
}

/// XML 1.0 에서 허용하지 않는 글자(expat 도 거부한다).
fn xml_invalid(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}')
}

pub fn parse(data: &[u8]) -> Result<Doc, String> {
    let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(data);
    let text = std::str::from_utf8(data).map_err(|e| format!("UTF-8 이 아닌 XML: {e}"))?;
    if let Some(c) = text.chars().find(|&c| xml_invalid(c)) {
        return Err(format!("XML 에 허용되지 않는 글자 U+{:04X}", c as u32));
    }
    let mut reader = Reader::from_reader(data);
    reader.config_mut().check_end_names = true;
    reader.config_mut().expand_empty_elements = true;
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut buf = String::new();
    let mut decl = None;
    let mut root_done = false;
    let mut tmp = Vec::new();
    loop {
        tmp.clear();
        match reader.read_event_into(&mut tmp).map_err(|e| format!("XML 파싱 실패: {e}"))? {
            Event::Start(e) => {
                if stack.is_empty() {
                    if root_done {
                        return Err("XML 루트가 둘 이상입니다.".into());
                    }
                } else {
                    put_text(&mut nodes, &stack, &mut buf);
                }
                let qname = std::str::from_utf8(e.name().as_ref()).map_err(|e| e.to_string())?.to_string();
                let mut attrs = Vec::new();
                for a in e.attributes() {
                    let a = a.map_err(|e| format!("속성 오류: {e}"))?;
                    let k = std::str::from_utf8(a.key.as_ref()).map_err(|e| e.to_string())?.to_string();
                    let v = a.unescape_value().map_err(|e| format!("속성 값 오류: {e}"))?.into_owned();
                    if v.chars().any(xml_invalid) {
                        return Err("속성 값에 허용되지 않는 글자(문자 참조)".into());
                    }
                    attrs.push((k, v));
                }
                let id = nodes.len();
                nodes.push(Node { local: local_of(&qname), qname, attrs, text: None, tail: None, children: vec![], end: id + 1 });
                if let Some(&p) = stack.last() {
                    nodes[p].children.push(id);
                }
                stack.push(id);
            }
            Event::End(_) => {
                put_text(&mut nodes, &stack, &mut buf);
                let id = stack.pop().ok_or("짝이 맞지 않는 끝 태그")?;
                nodes[id].end = nodes.len();
                if stack.is_empty() {
                    root_done = true;
                }
            }
            Event::Text(t) => {
                let s = t.unescape().map_err(|e| format!("텍스트 오류: {e}"))?;
                if s.chars().any(xml_invalid) {
                    return Err("텍스트에 허용되지 않는 글자(문자 참조)".into());
                }
                if stack.is_empty() {
                    if !s.trim().is_empty() {
                        return Err("루트 밖 글자".into());
                    }
                } else {
                    buf.push_str(&s);
                }
            }
            Event::CData(c) => {
                if !stack.is_empty() {
                    buf.push_str(std::str::from_utf8(&c).map_err(|e| e.to_string())?);
                }
            }
            Event::Decl(d) => decl = Some(String::from_utf8_lossy(&d).into_owned()),
            // defusedxml 처럼 DTD(엔티티 선언 포함)는 받지 않는다.
            Event::DocType(_) => return Err("DOCTYPE/DTD는 허용하지 않습니다.".into()),
            Event::Comment(_) | Event::PI(_) => {} // ElementTree 기본 TreeBuilder 와 같이 버린다.
            Event::Eof => break,
            Event::Empty(_) => unreachable!("expand_empty_elements"),
        }
    }
    if !stack.is_empty() || nodes.is_empty() {
        return Err("XML 이 끝나지 않았습니다.".into());
    }
    Ok(Doc { nodes, decl })
}

fn esc_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn esc_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#09;"),
            _ => out.push(c),
        }
    }
}

impl Doc {
    pub fn tostring(&self) -> Vec<u8> {
        let mut out = String::from("<?xml version='1.0' encoding='utf-8'?>\n");
        // 반복(비재귀) 직렬화: 깊은 중첩에서도 스택이 넘치지 않는다.
        enum Step { Open(usize), Close(usize) }
        let mut stack = vec![Step::Open(0)];
        while let Some(s) = stack.pop() {
            match s {
                Step::Open(i) => {
                    let n = &self.nodes[i];
                    out.push('<');
                    out.push_str(&n.qname);
                    for (k, v) in &n.attrs {
                        out.push(' ');
                        out.push_str(k);
                        out.push_str("=\"");
                        esc_attr(v, &mut out);
                        out.push('"');
                    }
                    if n.children.is_empty() && n.text.is_none() {
                        out.push_str(" />");
                        if let Some(t) = &n.tail { esc_text(t, &mut out); }
                    } else {
                        out.push('>');
                        if let Some(t) = &n.text { esc_text(t, &mut out); }
                        stack.push(Step::Close(i));
                        for &c in n.children.iter().rev() {
                            stack.push(Step::Open(c));
                        }
                    }
                }
                Step::Close(i) => {
                    let n = &self.nodes[i];
                    out.push_str("</");
                    out.push_str(&n.qname);
                    out.push('>');
                    if let Some(t) = &n.tail { esc_text(t, &mut out); }
                }
            }
        }
        out.into_bytes()
    }
}
