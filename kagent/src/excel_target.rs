//! Excel clipboard HTML target conversion. Python parity implementation in Rust.
use regex::{Captures, Regex};

pub const MAX_HTML_BYTES: usize = 8 * 1024 * 1024;
const BLANKS: &str = r"\s\u{00a0}\u{3000}\u{2000}-\u{200a}\u{202f}\u{205f}";

fn re(p: &str) -> Regex { Regex::new(p).expect("static regex") }
fn attr_map(raw: &str) -> std::collections::HashMap<String,String> {
    let r = re(r#"([^\s=/>"']+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s>]+))?"#);
    r.captures_iter(raw).map(|c| (c[1].to_ascii_lowercase(), c.get(2).map(|x| x.as_str().trim_matches(['"','\'']).to_string()).unwrap_or_default())).collect()
}
fn set_attr(raw: &str, name: &str, value: &str) -> String {
    let p = re(&format!(r#"(?i)\b{}\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#, regex::escape(name)));
    let q = if value.chars().any(|c| " #;:= '\"".contains(c)) { format!("\"{value}\"") } else { value.to_string() };
    let replacement = format!("{name}={q}");
    if p.is_match(raw) { p.replacen(raw, 1, replacement.as_str()).into_owned() } else { format!("{} {replacement}", raw.trim_end()) }
}
fn set_style(raw: &str, decls: &[(&str,String)]) -> String {
    let p = re(r#"(?i)\bstyle\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)"#);
    let mut body = p.captures(raw).and_then(|c|c.get(1)).map(|m|m.as_str().trim_matches(['"','\'']).to_string()).unwrap_or_default();
    for (key,value) in decls {
        let name = key.replace('_', "-");
        let d = re(&format!(r"(?i)\b{}\s*:\s*[^;]*", regex::escape(&name)));
        if d.is_match(&body) { body=d.replacen(&body,1,format!("{name}:{value}")).into_owned(); }
        else { if !body.trim().is_empty() { body=body.trim_end().trim_end_matches(';').to_string()+";"; } body+=&format!("{name}:{value}"); }
    }
    let replacement=format!("style='{body}'");
    if p.is_match(raw) { p.replacen(raw,1,replacement.as_str()).into_owned() } else { format!("{} {replacement}",raw.trim_end()) }
}
fn strip_shading_attrs(raw:&str, keep:bool)->String {
    let style=re(r#"(?i)\bstyle\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)"#);
    let mut out=raw.to_string();
    if !keep { let a=re(r#"(?i)\s*\b(?:bgcolor|background)\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#); out=a.replace_all(&out,"").into_owned(); }
    if let Some(c)=style.captures(&out) {
        let m=c.get(1).unwrap(); let body=m.as_str().trim_matches(['"','\'']);
        let mut kept=Vec::new();
        for part in body.split(';') { let name=part.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
            if !(name.starts_with("background")||name=="mso-pattern"||name=="mso-background-source"||name=="mso-shading") { kept.push(part); } }
        let joined=kept.join(";");
        if joined!=body { let rep=if joined.trim().is_empty(){String::new()}else{format!("style='{joined}'")}; out.replace_range(c.get(0).unwrap().range(),&rep); }
    }
    out
}
fn plain(inner:&str)->String {
    let tags=re(r"<[^>]*>"); let entities=re(r"(?i)&nbsp;|&#160;|&#12288;");
    let s=entities.replace_all(&tags.replace_all(inner," ")," ").into_owned();
    re(&format!("[{BLANKS}]+")).replace_all(&s," ").trim().to_string()
}
fn numeric(s:&str)->bool { re(r"(?i)^(?:(?:[+-]?(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?)|\(\s*[₩$€£¥]?\s*(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?\s*\)|[+-]?[₩$€£¥]\s?[+-]?(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?|[▲▼△▽]\s*[+-]?(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?)$").is_match(s) }
fn fmt(v:f64)->String { let s=format!("{v:.2}"); s.trim_end_matches('0').trim_end_matches('.').to_string() }

pub fn convert(input:&[u8], shading:&str)->Vec<u8> {
    if input.is_empty() || input.len()>MAX_HTML_BYTES { return input.to_vec(); }
    let Ok(raw)=std::str::from_utf8(input) else{return input.to_vec()};
    std::panic::catch_unwind(|| transform(raw, shading=="keep")).unwrap_or_else(|_|input.to_vec())
}
fn transform(raw:&str,keep:bool)->Vec<u8>{
    let start_re=re(r"StartHTML:\s*(\d+)");
    let (header,html)=if let Some(c)=start_re.captures(raw){let n=c[1].parse::<usize>().unwrap_or(0);if n>0&&n<raw.len()&&raw.is_char_boundary(n){(&raw[..n],&raw[n..])}else{("",raw)}}else{("",raw)};
    if !re(r"(?i)<t[dh]\b").is_match(html){return raw.as_bytes().to_vec()}
    let mut new=html.to_string();
    if !keep { new=strip_style_blocks(&new); }
    // Cell transformation, preserving source markup and attribute ordering.
    let cell=re(r#"(?is)(<(td|th)\b)((?:"[^"]*"|'[^']*'|[^>"'])*)>(.*?)</(td|th)\s*>"#);
    new=cell.replace_all(&new,|c:&Captures|{
        let tag=&c[2]; let mut attrs=c[3].to_string(); let inner=&c[4];
        let blank_re=re(&format!(r"(?is)^[{BLANKS}]*$")); let tags=re(r"<[^>]*>");
        let stripped=tags.replace_all(inner,"");
        let blank=blank_re.is_match(&stripped);
        let numeric_text=plain(inner); let isnum=numeric(&numeric_text);
        let mut body=inner.to_string();
        if blank && !inner.is_empty() {body.clear();}
        else if isnum {
            let lead=re(&format!(r"(?i)^(?:[{BLANKS}]|&nbsp;|&#160;|&#12288;|<span\b[^>]*mso-spacerun[^>]*>(?:[{BLANKS}]|&nbsp;|&#160;|&#12288;)*</span\s*>)+"));
            body=lead.replace(&body,"").into_owned();
            let trail=re(&format!(r"(?i)(?:[{BLANKS}]|&nbsp;|&#160;|&#12288;|<span\b[^>]*mso-spacerun[^>]*>(?:[{BLANKS}]|&nbsp;|&#160;|&#12288;)*</span\s*>)+$"));
            if let Some(m)=trail.find(&body){body.replace_range(m.range()," ");}
        }
        attrs=strip_shading_attrs(&attrs,keep);
        if isnum { attrs=set_attr(&attrs,"align","right"); attrs=set_style(&attrs,&[("text_align","right".into())]); }
        let style=format!("padding-top:1.4pt;padding-right:5.1pt;padding-bottom:1.4pt;padding-left:5.1pt");
        attrs=set_style(&attrs,&[("padding_top","1.4pt".into()),("padding_right","5.1pt".into()),("padding_bottom","1.4pt".into()),("padding_left","5.1pt".into())]);
        attrs=set_attr(&attrs,"valign","middle"); attrs=set_style(&attrs,&[("vertical_align","middle".into())]);
        if tag.eq_ignore_ascii_case("th"){attrs=set_style(&attrs,&[("font_weight","bold".into())]);}
        let _=style;
        format!("<{tag} {}>{body}</{tag}>",attrs.trim())
    }).into_owned();
    // Width normalization is only applied when every declared <col> has a positive width.
    new=normalize_widths(&new);
    // Replace legacy stylesheet padding rules.
    let td_rule=re(r"(?is)(td\s*\{)([^{}]*)(\})");
    new=td_rule.replace_all(&new,|c:&Captures|{
        let body=re(r"(?i)(?:padding(?:-(?:top|right|bottom|left))?|mso-ignore)\s*:[^;]*;?\s*").replace_all(&c[2],"").into_owned();
        format!("{}{}{}{}",&c[1],body.trim_end(),if body.trim_end().ends_with(';')||body.trim().is_empty(){""}else{";"},"padding-top:1.4pt;padding-right:5.1pt;padding-bottom:1.4pt;padding-left:5.1pt;")+&c[3]
    }).into_owned();
    if keep {new=strip_style_blocks(&new);}
    if new==html {return raw.as_bytes().to_vec()}
    if header.is_empty(){new.into_bytes()} else {
        let mut h=header.to_string(); let body=new.as_bytes();
        for key in ["StartHTML","EndHTML","StartFragment","EndFragment"] {
            let p=re(&format!(r"({key}:)(\d+)"));
            let val=match key {"StartHTML"=>header.len(),"EndHTML"=>header.len()+body.len(),"StartFragment"=>header.len()+new.find("<!--StartFragment-->").map(|i|i+20).unwrap_or(0),_=>header.len()+new.find("<!--EndFragment-->").unwrap_or(body.len())};
            h=p.replace(&h,format!("${{1}}{:0width$}",val,width=p.captures(&h).and_then(|c|c.get(2)).map(|m|m.as_str().len()).unwrap_or(10))).into_owned();
        }
        [h.into_bytes(),body.to_vec()].concat()
    }
}
fn strip_style_blocks(html:&str)->String {
    let block=re(r"(?is)(<style\b[^>]*>)(.*?)(</style\s*>)");
    block.replace_all(html,|c:&Captures|{
        let rule=re(r"\{([^{}]*)\}"); rule.replace_all(&c[2],|r:&Captures|{
            let parts:Vec<&str>=r[1].split(';').filter(|p|{let n=p.split(':').next().unwrap_or("").trim().to_ascii_lowercase();!(n.starts_with("background")||n=="mso-pattern"||n=="mso-background-source"||n=="mso-shading")}).collect(); format!("{{{}}}",parts.join(";"))
        }).to_string().pipe(|s|format!("{}{}{}",&c[1],s,&c[3]))
    }).into_owned()
}
trait Pipe:Sized{fn pipe<R>(self,f:impl FnOnce(Self)->R)->R{f(self)}} impl<T> Pipe for T{}
fn normalize_widths(html:&str)->String {
    let col=re(r#"(?is)<col\b((?:"[^"]*"|'[^']*'|[^>"'])*)>"#); let mut widths=Vec::new();
    for c in col.captures_iter(html){let a=attr_map(&c[1]);let mut w=a.get("width").and_then(|v|v.parse::<f64>().ok()).unwrap_or(0.);if w<=0.{if let Some(s)=a.get("style"){if let Some(m)=re(r"(?i)(?:^|[;\s])width\s*:\s*([\d.]+)\s*pt").captures(s){w=m[1].parse::<f64>().unwrap_or(0.)/0.75;}}}let span=a.get("span").and_then(|v|v.parse::<usize>().ok()).unwrap_or(1).max(1);widths.extend(std::iter::repeat(w).take(span));}
    if widths.is_empty()||widths.iter().any(|w|*w<=0.){return html.to_string()}
    let cell=re(r#"(?is)<(td|th)\b((?:"[^"]*"|'[^']*'|[^>"'])*)>"#); let rows=re(r"(?is)<tr\b[^>]*>.*?</tr\s*>");
    let mut taken=std::collections::HashSet::new();let mut placed=Vec::<(usize,usize,usize,usize)>::new();
    for (y,row) in rows.find_iter(html).enumerate(){let text=&html[row.range()];let mut x=0;for c in cell.captures_iter(text){while taken.contains(&(y,x)){x+=1;}let a=attr_map(&c[2]);let cs=a.get("colspan").and_then(|v|v.parse::<usize>().ok()).unwrap_or(1).max(1);let rs=a.get("rowspan").and_then(|v|v.parse::<usize>().ok()).unwrap_or(1).max(1);if x+cs>widths.len(){return html.to_string()}for dy in 0..rs{for dx in 0..cs{taken.insert((y+dy,x+dx));}}placed.push((y,c.get(0).unwrap().start(),x,cs));x+=cs;}}
    if let Some(first)=col.find(html){let last=col.find_iter(html).last().unwrap();let cols=widths.iter().map(|w|format!("<col width={} style='width:{}pt'>",fmt(*w),fmt(*w*0.75))).collect::<Vec<_>>().join("\n");let mut s=html.to_string();s.replace_range(first.start()..last.end(),&cols);let row_ranges:Vec<_>=rows.find_iter(&s).map(|m|m.range()).collect();
        for (y,range) in row_ranges.into_iter().enumerate().rev(){let mut row=s[range.clone()].to_string();let row_spans:Vec<_>=placed.iter().filter(|p|p.0==y).collect();let mut ix=0;row=cell.replace_all(&row,|c:&Captures|{let p=row_spans[ix];ix+=1;let (_,_,x,cs)=*p;let w:f64=widths[x..x+cs].iter().sum();let attrs=set_style(&set_attr(&c[2],"width",&fmt(w)),&[("width",format!("{}pt",fmt(w*0.75)))]);format!("<{} {}>",&c[1],attrs)}).into_owned();s.replace_range(range,&row);}
        let total:f64=widths.iter().sum();let table=re(r#"(?is)<table\b((?:"[^"]*"|'[^']*'|[^>"'])*)>"#);s=table.replace(&s,|c:&Captures|{let a=re(r#"(?i)\s*\bcellpadding\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).replace_all(&c[1],"").into_owned();let a=set_style(&set_attr(&a,"width",&fmt(total)),&[("width",format!("{}pt",fmt(total*0.75)))]);format!("<table {}>",a.trim())}).into_owned();return s;}
    html.to_string()
}
