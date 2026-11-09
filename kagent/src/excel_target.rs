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
    let pattern = re(&format!(r#"(?i){}\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#, regex::escape(name)));
    let q = if value.chars().any(|c| " #;:= '\"".contains(c)) { format!("\"{value}\"") } else { value.to_string() };
    let replacement = format!("{name}={q}");
    let found = pattern.find_iter(raw).find(|m| m.start() == 0 || !raw[..m.start()].chars().last().is_some_and(|c| c == '-' || c == '_' || c.is_alphanumeric()));
    if let Some(m) = found { let mut out=raw.to_string(); out.replace_range(m.range(), &replacement); out }
    else { format!("{} {replacement}", raw.trim_end()) }
}
fn open_tag(tag: &str, attrs: &str) -> String {
    let attrs = attrs.trim();
    if attrs.is_empty() { format!("<{tag}>") } else { format!("<{tag} {attrs}>") }
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
fn normalize_attr_spaces(raw:&str)->String {
    let mut out=String::new();let mut quote=None;let mut pending=false;
    for ch in raw.trim().chars(){
        if let Some(q)=quote {out.push(ch);if ch==q{quote=None;}continue;}
        if ch=='\''||ch=='"'{if pending&&!out.is_empty(){out.push(' ');}pending=false;quote=Some(ch);out.push(ch);}
        else if ch.is_whitespace(){pending=true;}
        else {if pending&&!out.is_empty(){out.push(' ');}pending=false;out.push(ch);}
    }
    out
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
        if joined!=body {
            let range=c.get(0).unwrap().range();
            if joined.trim().is_empty() {
                let start=range.start.saturating_sub(1);
                out.replace_range(start..range.end, "");
            } else {
                out.replace_range(range, &format!("style='{joined}'"));
            }
        }
    }
    out
}
fn style_value(raw:&str,key:&str)->Option<String>{let p=re(&format!(r"(?i)(?:^|[;\s]){}\s*:\s*([^;]+)",regex::escape(key)));p.captures(raw).map(|c|c[1].trim().to_string())}
fn class_styles(html:&str)->std::collections::HashMap<String,String>{
    let blocks=re(r"(?is)<style\b[^>]*>(.*?)</style\s*>");let rules=re(r"(?is)\.([\w-]+)\s*\{([^}]*)\}");let mut map=std::collections::HashMap::new();
    for block in blocks.captures_iter(html){for rule in rules.captures_iter(&block[1]){map.insert(rule[1].to_ascii_lowercase(),re(r"\s+").replace_all(&rule[2]," ").into_owned());}}
    map
}
fn set_bgcolor(raw:&str,value:&str)->String {
    let attr=re(r#"(?i)bgcolor\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#);
    if let Some(m)=attr.find_iter(raw).find(|m|m.start()==0 || !raw[..m.start()].chars().last().is_some_and(|c|c.is_alphanumeric()||c=='_'||c=='-')) {
        let mut out=raw.to_string();out.replace_range(m.range(),&format!("bgcolor=\"{}\"",value.to_ascii_uppercase()));out
    } else {format!("{} bgcolor=\"{}\"",raw.trim_end(),value.to_ascii_uppercase())}
}
fn plain(inner:&str)->String {
    let tags=re(r"<[^>]*>"); let entities=re(r"(?i)&nbsp;|&#160;|&#12288;");
    let s=entities.replace_all(&tags.replace_all(inner," ")," ").into_owned();
    re(&format!("[{BLANKS}]+")).replace_all(&s," ").trim().to_string()
}
fn css_color(value: &str) -> Option<String> {
    let text=value.trim().to_ascii_lowercase();
    if text.is_empty() || text=="none" || text=="transparent" { return None; }
    let hex=if text.len()==7 && text.starts_with('#') { Some(text.to_ascii_uppercase()) }
        else if text.len()==4 && text.starts_with('#') { Some(format!("#{}{}{}{}{}{}", &text[1..2],&text[1..2],&text[2..3],&text[2..3],&text[3..4],&text[3..4]).to_ascii_uppercase()) } else { None };
    if hex.is_some(){return hex;}
    let rgb=re(r"(?i)^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})(?:\s*,\s*([0-9.]+))?\s*\)$");
    if let Some(c)=rgb.captures(&text){if c.get(4).is_some_and(|a|a.as_str().parse::<f32>().unwrap_or(1.0)==0.0){return None;}let v:Vec<u8>=(1..=3).map(|i|c[i].parse::<u16>().unwrap_or(0).min(255) as u8).collect();return Some(format!("#{:02X}{:02X}{:02X}",v[0],v[1],v[2]));}
    let named=[("black","#000000"),("white","#FFFFFF"),("red","#FF0000"),("green","#008000"),("blue","#0000FF"),("yellow","#FFFF00"),("aqua","#00FFFF"),("cyan","#00FFFF"),("fuchsia","#FF00FF"),("magenta","#FF00FF"),("silver","#C0C0C0"),("gray","#808080"),("grey","#808080"),("lime","#00FF00"),("maroon","#800000"),("navy","#000080"),("olive","#808000"),("purple","#800080"),("teal","#008080"),("orange","#FFA500")];
    let first=text.split_whitespace().next().unwrap_or(""); named.iter().find(|(n,_)|*n==first).map(|(_,v)|v.to_string())
}
fn numeric(s:&str)->bool { re(r"(?i)^(?:(?:[+-]?(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?)|\(\s*[₩$€£¥]?\s*(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?\s*\)|[+-]?[₩$€£¥]\s?[+-]?(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?|[▲▼△▽]\s*[+-]?(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?)$").is_match(s) }
pub fn is_numeric_text(s:&str)->bool { numeric(s) }
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
    let classes=class_styles(html);
    if !keep { new=strip_style_blocks(&new); }
    // Cell transformation, preserving source markup and attribute ordering.
    let cell=re(r#"(?is)(<(td|th)\b)((?:"[^"]*"|'[^']*'|[^>"'])*)>(.*?)</(td|th)\s*>"#);
    new=cell.replace_all(&new,|c:&Captures|{
        let tag=&c[2]; let mut attrs=c[3].trim().to_string(); let inner=&c[4];
        let blank_re=re(&format!(r"(?is)^[{BLANKS}]*$")); let tags=re(r"<[^>]*>");
        let entities=re(r"(?i)&nbsp;|&#160;|&#12288;");
        let tags_stripped=tags.replace_all(inner," ");
        let stripped=entities.replace_all(&tags_stripped," ");
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
        let current=attr_map(&attrs);let cls=current.get("class").and_then(|v|classes.get(&v.to_ascii_lowercase())).map(String::as_str).unwrap_or("");
        let inline=re(r#"(?i)style\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)"#).captures(&attrs).and_then(|c|c.get(1)).map(|m|m.as_str().trim_matches(['"','\'']).to_string()).unwrap_or_default();
        if keep {
            let fill=current.get("bgcolor").and_then(|v|css_color(v))
                .or_else(||style_value(&inline,"background-color").or_else(||style_value(&inline,"background")).and_then(|v|css_color(&v)))
                .or_else(||style_value(cls,"background-color").or_else(||style_value(cls,"background")).and_then(|v|css_color(&v)));
            if let Some(color)=fill {attrs=set_bgcolor(&attrs,&color);}
        }
        attrs=strip_shading_attrs(&attrs,keep);
        attrs=normalize_attr_spaces(&attrs);
        if isnum { attrs=set_attr(&attrs,"align","right"); attrs=set_style(&attrs,&[("text_align","right".into())]); }
        else {let align=style_value(&inline,"text-align").or_else(||style_value(cls,"text-align")).or_else(||current.get("align").cloned()).unwrap_or_default().to_ascii_lowercase();if ["left","center","right","justify"].contains(&align.as_str()){attrs=set_attr(&attrs,"align",&align);attrs=set_style(&attrs,&[("text_align",align)]);}}
        let style=format!("padding-top:1.4pt;padding-right:5.1pt;padding-bottom:1.4pt;padding-left:5.1pt");
        attrs=set_style(&attrs,&[("padding_top","1.4pt".into()),("padding_right","5.1pt".into()),("padding_bottom","1.4pt".into()),("padding_left","5.1pt".into())]);
        let valign=style_value(&inline,"vertical-align").or_else(||style_value(cls,"vertical-align")).or_else(||current.get("valign").cloned()).unwrap_or_else(||"middle".into()).to_ascii_lowercase();
        attrs=set_attr(&attrs,"valign",&valign); attrs=set_style(&attrs,&[("vertical_align",valign)]);
        let weight=style_value(&inline,"font-weight").or_else(||style_value(cls,"font-weight")).unwrap_or_default();
        if tag.eq_ignore_ascii_case("th")||weight=="bold"||weight=="bolder"||weight.parse::<u16>().is_ok_and(|v|v>=600){attrs=set_style(&attrs,&[("font_weight","bold".into())]);}
        for prop in ["color","border"] {if style_value(&inline,prop).is_none(){if let Some(v)=style_value(cls,prop){attrs=set_style(&attrs,&[(if prop=="color"{"color"}else{"border"},v)]);}}}
        let _=style;
        let attrs = if !keep { re(r#"(?i)\s*\b(?:bgcolor|background)\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).replace_all(&attrs, " ").into_owned() } else { attrs };
        let opening = if attrs.trim().is_empty() { c[1].to_string() } else { format!("{}{}{}",&c[1],if c[1].ends_with(char::is_whitespace){""}else{" "},attrs.trim()) };

        format!("{opening}>{body}</{tag}>")
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
            h=p.replace(&h,|c:&Captures| format!("{}{:0width$}",&c[1],0,width=c[2].len())).into_owned();
        }
        for key in ["StartHTML","EndHTML","StartFragment","EndFragment"] {
            let p=re(&format!(r"({key}:)(\d+)"));
            let val=match key {"StartHTML"=>header.len(),"EndHTML"=>header.len()+body.len(),"StartFragment"=>header.len()+new.find("<!--StartFragment-->").map(|i|i+20).unwrap_or(0),_=>header.len()+new.find("<!--EndFragment-->").unwrap_or(body.len())};
            h=p.replace(&h,|c:&Captures| format!("{}{:0width$}",&c[1],val,width=c[2].len())).into_owned();
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shading_attribute_rules() {
        assert_eq!(strip_shading_attrs(" bgcolor=\"#abc\" style='background:#abc;color:red'", true), " bgcolor=\"#abc\" style='color:red'");
        assert_eq!(strip_shading_attrs(" bgcolor=\"#abc\" style='background:#abc'", true), " bgcolor=\"#abc\"");
        assert_eq!(strip_shading_attrs(" bgcolor=\"#abc\" style='color:red'", false), " style='color:red'");
    }
    #[test]
    fn inline_background_survives_keep_mode() {
        assert_eq!(css_color("#abc").as_deref(), Some("#AABBCC"));
        assert_eq!(style_value("background:#abc","background").as_deref(), Some("#abc"));
        let input=b"<html xmlns:x='urn:schemas-microsoft-com:office:excel'><table><col width='72'><tr><td style='background:#abc'>x</td></tr></table></html>";
        let output=String::from_utf8(convert(input,"keep")).unwrap();
        assert!(output.contains("bgcolor=\"#AABBCC\""),"{output}");
    }
    #[test]
    fn numeric_cell_has_single_attribute_separator() {
        let input=b"<html xmlns:x='urn:schemas-microsoft-com:office:excel'><table><col width='72'><tr><td align='left'> 88,180 </td></tr></table></html>";
        let output=String::from_utf8(convert(input,"keep")).unwrap();
        assert!(output.contains("<td align=right"),"{output}");
    }
}
fn normalize_widths(html:&str)->String {
    let col=re(r#"(?is)<col\b((?:"[^"]*"|'[^']*'|[^>"'])*)>"#); let mut widths=Vec::new();
    for c in col.captures_iter(html){let a=attr_map(&c[1]);let mut w=a.get("width").and_then(|v|v.parse::<f64>().ok()).unwrap_or(0.);if w<=0.{if let Some(s)=a.get("style"){if let Some(m)=re(r"(?i)(?:^|[;\s])width\s*:\s*([\d.]+)\s*pt").captures(s){w=m[1].parse::<f64>().unwrap_or(0.)/0.75;}}}let span=a.get("span").and_then(|v|v.parse::<usize>().ok()).unwrap_or(1).max(1);widths.extend(std::iter::repeat(w).take(span));}
    if widths.is_empty()||widths.iter().any(|w|*w<=0.){return html.to_string()}
    let cell=re(r#"(?is)<(td|th)\b((?:"[^"]*"|'[^']*'|[^>"'])*)>"#); let rows=re(r"(?is)<tr\b[^>]*>.*?</tr\s*>");
    let mut taken=std::collections::HashSet::new();let mut placed=Vec::<(usize,usize,usize,usize)>::new();
    for (y,row) in rows.find_iter(html).enumerate(){let text=&html[row.range()];let mut x=0;for c in cell.captures_iter(text){while taken.contains(&(y,x)){x+=1;}let a=attr_map(&c[2]);let cs=a.get("colspan").and_then(|v|v.parse::<usize>().ok()).unwrap_or(1).max(1);let rs=a.get("rowspan").and_then(|v|v.parse::<usize>().ok()).unwrap_or(1).max(1);if x+cs>widths.len(){return html.to_string()}for dy in 0..rs{for dx in 0..cs{taken.insert((y+dy,x+dx));}}placed.push((y,c.get(0).unwrap().start(),x,cs));x+=cs;}}
    if let Some(first)=col.find(html){let last=col.find_iter(html).last().unwrap();let cols=widths.iter().map(|w|format!("<col width={} style='width:{}pt'>",fmt(*w),fmt(*w*0.75))).collect::<Vec<_>>().join("\n");let mut s=html.to_string();s.replace_range(first.start()..last.end(),&cols);let row_ranges:Vec<_>=rows.find_iter(&s).map(|m|m.range()).collect();
        for (y,range) in row_ranges.into_iter().enumerate().rev(){let mut row=s[range.clone()].to_string();let row_spans:Vec<_>=placed.iter().filter(|p|p.0==y).collect();let mut ix=0;row=cell.replace_all(&row,|c:&Captures|{let p=row_spans[ix];ix+=1;let (_,_,x,cs)=*p;let w:f64=widths[x..x+cs].iter().sum();let attrs=set_style(&set_attr(&c[2],"width",&fmt(w)),&[("width",format!("{}pt",fmt(w*0.75)))]);format!("<{}{}>",&c[1],attrs.trim_end())}).into_owned();s.replace_range(range,&row);}
        let total:f64=widths.iter().sum();let table=re(r#"(?is)<table\b((?:"[^"]*"|'[^']*'|[^>"'])*)>"#);s=table.replace(&s,|c:&Captures|{let a=re(r#"(?i)\s*\bcellpadding\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).replace_all(&c[1],"").into_owned();let a=set_style(&set_attr(&a,"width",&fmt(total)),&[("width",format!("{}pt",fmt(total*0.75)))]);open_tag("table", &a)}).into_owned();return s;}
    html.to_string()
}
