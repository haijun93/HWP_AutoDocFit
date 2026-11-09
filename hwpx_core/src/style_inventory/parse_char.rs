use serde_json::{Map, Value};

fn local(v: &Value) -> Option<&str> { v.as_str().map(crate::style_inventory::tag::tag) }
fn first<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    if local(&node["tag"]) == Some(name) { return Some(node); }
    node.get("children")?.as_array()?.iter().find_map(|c| first(c,name))
}
fn lang_values(node: Option<&Value>, cast_str: bool) -> Map<String,Value> {
    let mut out=Map::new();
    if let Some(attrs)=node.and_then(|n|n.get("attrib")).and_then(Value::as_object) {
        for lang in ["hangul","latin","hanja","japanese","other","symbol","user"] {
            if let Some(Value::String(raw))=attrs.get(lang) {
                let val=if cast_str {Value::String(raw.clone())} else {raw.parse::<i64>().map(Value::from).unwrap_or_else(|_|Value::String(raw.clone()))}; out.insert(lang.into(),val);
            }
        }
    }
    out
}
fn int_attr(node:&Value, name:&str, default:i64)->Value { node.get("attrib").and_then(|a|a.get(name)).and_then(Value::as_str).unwrap_or("").parse::<i64>().map(Value::from).unwrap_or(Value::from(default)) }
fn attr(node:&Value,name:&str,default:Value)->Value { node.get("attrib").and_then(|a|a.get(name)).cloned().unwrap_or(default) }
pub fn parse_char(e:&Value, fonts:&Value)->Value {
 let refv=lang_values(first(e,"fontRef"),true); let mut font=Map::new();
 for lang in ["hangul","latin","hanja"] { let face=fonts.get(lang).and_then(|x|x.get(refv.get(lang).and_then(Value::as_str).unwrap_or(""))).cloned().unwrap_or(Value::Null); font.insert(lang.into(),face); }
 let field=|tag:&str,key:&str,def:&str| first(e,tag).map(|n|attr(n,key,Value::String(def.into()))).unwrap_or(Value::String(def.into()));
 let hangul=|tag:&str,default:i64| lang_values(first(e,tag),false).remove("hangul").unwrap_or(Value::from(default));
 let mut out=Map::new(); out.insert("id".into(),attr(e,"id",Value::Null)); out.insert("font".into(),Value::Object(font));
 let height=int_attr(e,"height",0).as_i64().unwrap_or(0) as f64/100.0; out.insert("size_pt".into(),serde_json::json!(height));
 out.insert("color".into(),attr(e,"textColor",Value::String("#000000".into())));
 out.insert("shade".into(),crate::style_inventory::shade::shade(e.get("attrib").and_then(|a|a.get("shadeColor")).and_then(Value::as_str)));
 for (key,tag) in [("bold","bold"),("italic","italic"),("superscript","supscript"),("subscript","subscript")] {out.insert(key.into(),Value::Bool(first(e,tag).is_some()));}
 for (key,tag,att,def) in [("underline","underline","type","NONE"),("strikeout","strikeout","shape","NONE"),("outline","outline","type","NONE"),("shadow","shadow","type","NONE")] {out.insert(key.into(),field(tag,att,def));}
 for (key,tag,def) in [("ratio","ratio",100),("spacing","spacing",0),("rel_size","relSz",100),("offset","offset",0)] {out.insert(key.into(),hangul(tag,def));}
 out.insert("border_fill".into(),attr(e,"borderFillIDRef",Value::Null)); Value::Object(out)
}

