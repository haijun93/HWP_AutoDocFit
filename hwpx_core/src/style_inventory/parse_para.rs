use serde_json::{Map, Value};
fn local(v:&Value)->Option<&str>{v.as_str().map(crate::style_inventory::tag::tag)}
fn first<'a>(n:&'a Value,name:&str)->Option<&'a Value>{if local(&n["tag"])==Some(name){return Some(n)} n.get("children")?.as_array()?.iter().find_map(|c|first(c,name))}
fn attr(n:&Value,k:&str,d:Value)->Value{n.get("attrib").and_then(|a|a.get(k)).cloned().unwrap_or(d)}
fn int_attr(n:&Value,k:&str,d:i64)->Value{n.get("attrib").and_then(|a|a.get(k)).and_then(Value::as_str).unwrap_or("").parse::<i64>().map(Value::from).unwrap_or(Value::from(d))}
fn yes(n:Option<&Value>,k:&str)->Value{Value::Bool(n.and_then(|v|v.get("attrib")).and_then(|a|a.get(k)).and_then(Value::as_str)==Some("1"))}
fn margin(n:Option<&Value>,k:&str)->Value{first(n.unwrap_or(&Value::Null),k).map(|v|int_attr(v,"value",0)).unwrap_or(Value::from(0))}
pub fn parse_para(e:&Value)->Value{
 let align=first(e,"align");let heading=first(e,"heading");let brk=first(e,"breakSetting");let m=first(e,"margin");let ls=first(e,"lineSpacing");let border=first(e,"border");let mut o=Map::new();
 o.insert("id".into(),attr(e,"id",Value::Null));o.insert("align".into(),align.map(|n|attr(n,"horizontal",Value::Null)).unwrap_or(Value::String("JUSTIFY".into())));
 let h=if let Some(h)=heading {let t=h.get("attrib").and_then(|a|a.get("type")).and_then(Value::as_str).unwrap_or("NONE");if t!="NONE"{Some(serde_json::json!({"type":h.get("attrib").and_then(|a|a.get("type")).cloned().unwrap_or(Value::Null),"level":int_attr(h,"level",0),"numbering":attr(h,"idRef",Value::Null)}))}else{None}}else{None};o.insert("heading".into(),h.unwrap_or(Value::Null));
 for(k,n)in[("left","left"),("right","right"),("indent","intent"),("prev","prev"),("next","next")]{o.insert(k.into(),margin(m,n));}
 let spacing=ls.map(|n|{let raw=n.get("attrib").and_then(|a|a.get("value")).and_then(Value::as_str).unwrap_or("0").parse::<f64>().unwrap_or(0.0) as i64;serde_json::json!({"type":attr(n,"type",Value::Null),"value":raw})}).unwrap_or(Value::Null);o.insert("line_spacing".into(),spacing);
 o.insert("tab".into(),attr(e,"tabPrIDRef",Value::Null));o.insert("border_fill".into(),border.map(|b|attr(b,"borderFillIDRef",Value::Null)).unwrap_or(Value::Null));
 for(k,a)in[("keep_with_next","keepWithNext"),("keep_lines","keepLines"),("page_break_before","pageBreakBefore"),("widow_orphan","widowOrphan")]{o.insert(k.into(),yes(brk,a));}
 o.insert("break_word".into(),brk.map(|b|attr(b,"breakNonLatinWord",Value::Null)).unwrap_or(Value::Null));o.insert("snap_to_grid".into(),Value::Bool(e.get("attrib").and_then(|a|a.get("snapToGrid")).and_then(Value::as_str)==Some("1")));o.insert("condense".into(),int_attr(e,"condense",0));Value::Object(o)
}
