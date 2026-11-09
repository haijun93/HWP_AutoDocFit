//! Rust port of `docfit_core.style_hierarchy.analyze_hierarchy`'s public rows.
//! The document marker scheme is inferred once, before paragraph statistics.
use crate::{P, Parser};
use std::collections::{HashMap, HashSet};

const ROLES:[&str;6]=["장","중제목","소제목","본문","내용","부연설명"];
#[derive(Clone)] struct Rec{text:String,left:i32,indent:i32,font:Option<String>,size:f64,marker:String,marker_role:String}
fn canonical(s:&str)->String{match s{"ㅁ"|"■"=>"□".into(),"○"|"◦"|"☞"=>"ㅇ".into(),"•"|"·"|"‧"|"∙"|"⋅"|"ㆍ"|"●"=>"•".into(),_=>s.into()}}
fn marker_family(s:&str)->String{
 let t=s.trim_start();let first=t.split_whitespace().next().unwrap_or("");if first.is_empty(){return String::new()}
 if t.len()>=4&&t.as_bytes()[0..4].iter().all(|b|b.is_ascii_digit())&&t.contains('.') {return String::new()}
 if t.starts_with("문서번호")||t.starts_with("참고자료:")||t.starts_with("붙임")||t.starts_with("첨부"){return String::new()}
 if let Some((prefix,_))=first.split_once('.') {if prefix.chars().all(|c|c.is_ascii_digit())&&prefix.len()>2{return String::new()}}
 if first.chars().all(|c|c.is_ascii_digit())&&first.len()>=4{return String::new()}
 if t.starts_with("문서번호")||t.starts_with("참고자료:")||t.starts_with("붙임")||t.starts_with("첨부"){return String::new()}
 if ["제목:","개요:","요약:","부제:","중제목:"].iter().any(|p|first.starts_with(p)){return String::new()}
 if let Some(c)=t.chars().next(){if "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳㉠㉡㉢㉣㉤㉥㉦㉧㉨㉩㉪㉫㉬㉭㉮㉯㉰㉱㉲㉳㉴㉵㉶㉷㉸㉹㉺㉻".contains(c)&&t[c.len_utf8()..].chars().next().map(char::is_whitespace).unwrap_or(true){return if ('①'..='⑳').contains(&c){"①".into()}else if ('㉠'..='㉭').contains(&c){"㉠".into()}else{"㉮".into()}} if "❶❷❸❹❺❻❼❽❾❿".contains(c){return String::new()}}
 if first=="**"{return "**".into()}if first.chars().count()==1&&"□ㅁ■ㅇ○◦☞-*※•·‧∙⋅ㆍ●".contains(first){return first.into()}
 if first.contains('·')&&first.chars().next().map(|c|c.is_ascii_digit()).unwrap_or(false){return String::new()}
 let token=first.trim_end_matches(['.','．']);if !token.is_empty()&&token.chars().all(|c|"ⅠⅡⅢⅣⅤⅥⅦⅧⅨⅩⅪⅫ".contains(c)){return if first.ends_with('.') {"Ⅰ.".into()}else{"Ⅰ".into()}}
 if token.chars().all(|c|c.is_ascii_digit())&&first.ends_with(['.','．']){return "1.".into()}if first.ends_with(')')&&first.chars().take_while(|c|*c!='(').all(|c|c.is_ascii_digit()){return "1)".into()}
 if first.starts_with('(')&&first.ends_with(')'){let inner=&first[1..first.len()-1];if inner.chars().all(|c|c.is_ascii_digit()){return "(1)".into()}if inner.chars().count()==1&&"가나다라마바사아자차카타파하".contains(inner){return "(가)".into()}}
 if first.ends_with(['.','．'])&&token.chars().count()==1&&"가나다라마바사아자차카타파하".contains(token){return "가.".into()}
 if first.starts_with("제목:")||first.starts_with("개요:")||first.starts_with("요약:")||first.starts_with("부제:")||first.starts_with("중제목:"){return String::new()}
 String::new()
}
fn marked(s:&str)->(String,String){
 let t=s.trim_start(); if t.is_empty(){return (String::new(),String::new())}
 let first=t.split_whitespace().next().unwrap_or("");
 if t.starts_with("#") {let n=t.chars().take_while(|c|*c=='#').count();return("#".repeat(n),"소제목".into())}
 if t.starts_with('>') {return(">".into(),"본문".into())}if t.starts_with('|') {return("|".into(),"내용".into())}if t.starts_with("✅") {return("✅".into(),"내용".into())}
 if t.starts_with("2026.")&&t.contains(" 11.") {return(String::new(),String::new())}
 let chapter=(first.starts_with("제")&&first.contains(['장','편','부']))||first.to_ascii_lowercase().starts_with("chapter");
 if chapter{return(if first.to_ascii_lowercase().starts_with("chapter"){t.split_whitespace().take(2).collect::<Vec<_>>().join(" ")}else{first.into()},"장".into())}
 let roman=first.trim_end_matches(['.','．']);
 if (first.ends_with(['.','．'])&& !roman.is_empty()&&roman.chars().all(|c|"IVXLCDM".contains(c)||('Ⅰ'..='Ⅿ').contains(&c)))
  ||(roman.chars().count()==1&&"가나다라마바사아자차카타파하".contains(roman)){return(first.into(),"중제목".into())}
 if ["□","ㅁ","■"].iter().any(|x|t.starts_with(x)) {let m=&t[..t.chars().next().unwrap().len_utf8()];if t[m.len()..].chars().next().map(char::is_whitespace).unwrap_or(false){return(m.into(),"소제목".into())}}
 if first.len()==1&&"ㅇ○◦☞".contains(first)&&t[first.len()..].chars().next().map(char::is_whitespace).unwrap_or(false){return(first.into(),"본문".into())}
 if first=="-"{if t[first.len()..].chars().next().map(char::is_whitespace).unwrap_or(false){return(first.into(),"내용".into())}}
 if first=="**"||first=="*"||first=="※"||["•","·","‧","∙","⋅","ㆍ","●"].contains(&first){return(first.into(),"부연설명".into())}
 if t.starts_with("표") {return("표".into(),"중제목".into())}
 if t.starts_with("Chapter")||t.starts_with("chapter") {return(first.split_whitespace().take(2).collect::<Vec<_>>().join(" "),"장".into())}
 if first.chars().all(|c|c.is_ascii_digit())&&first.len()<=2&&(first.ends_with('.')||first.ends_with('．')){return(first.into(),"소제목".into())}
 if first.chars().all(|c|c.is_ascii_digit())&&first.ends_with(')'){return(first.into(),"내용".into())}
 if first.starts_with('(')&&first.ends_with(')') {let inner=&first[1..first.len()-1];if inner.chars().all(|c|c.is_ascii_digit()){return(first.into(),"부연설명".into())}}
 if first.len()==2&&first.ends_with('.')&&"가나다라마바사아자차카타파하".contains(first.chars().next().unwrap()){return(first.into(),"중제목".into())}
 if first.len()==2&&first.ends_with(')')&&"가나다라마바사아자차카타파하".contains(first.chars().next().unwrap()){return(first.into(),"본문".into())}
 if first.starts_with('❶')||first.starts_with('❷'){return(String::new(),String::new())}
 if let Some(c)=t.chars().next(){if ('①'..='⑳').contains(&c)||('㉠'..='㉻').contains(&c){return(c.to_string(),String::new())}}
 (String::new(),String::new())
}
fn family(m:&str)->String{if ["□","ㅁ","■"].contains(&m){"□".into()}else if ["ㅇ","○","◦","☞"].contains(&m){"ㅇ".into()}else{m.into()}}
fn occ_scheme_marker(s:&str)->bool{let f=marker_family(s);!f.is_empty()&&!["□","ㅇ","-","*","**","※","•"].contains(&family(&f).as_str())}
fn ordinal(s:&str)->Option<Vec<i64>>{let n:Vec<i64>=s.trim_matches(['.','．','(',')']).split(['.','．','-']).map(|p|p.parse().ok()).collect::<Option<_>>()?;(!n.is_empty()).then_some(n)}
fn detect_scheme(rs:&[Rec])->(HashMap<String,String>,HashSet<String>){
 let mut excluded=HashSet::new();let mut i=0;while i<rs.len(){if !rs[i].text.trim_start().starts_with("붙임"){i+=1;continue}let end=(i..rs.len().min(i+30)).find(|j|rs[*j].text.trim_end().ends_with("끝."));if let Some(e)=end{for j in i..=e{excluded.insert(rs[j].text.trim().to_string());}i=e+1}else{i+=1}}
 let occupied:HashSet<String>=rs.iter().map(|r|family(&marker_family(&r.text))).filter(|f|["□","ㅇ","-"].contains(&f.as_str())).collect();let has_box=occupied.contains("□");
 let mut occ:HashMap<String,Vec<String>>=HashMap::new();for r in rs{if excluded.contains(r.text.trim()){continue}let m=marker_family(&r.text);if m.is_empty()||["□","ㅇ","-","*","**","※","•"].contains(&family(&m).as_str()){continue}if has_box&&["가.","Ⅰ","Ⅰ.","I."].contains(&m.as_str()){continue}occ.entry(m.clone()).or_default().push(r.text.clone())}
 let seq:HashSet<String>=occ.iter().filter(|(_,v)|v.windows(2).any(|w|ordinal(w[0].split_whitespace().next().unwrap_or("")).zip(ordinal(w[1].split_whitespace().next().unwrap_or(""))).map(|(a,b)|a.len()==b.len()&&a.last().unwrap()+1==*b.last().unwrap()).unwrap_or(false))).map(|(k,_)|k.clone()).collect();
 let shapes:HashSet<String>=occ.iter().filter(|(_,v)|v.iter().all(|x|!x.chars().any(|c|c.is_ascii_digit()))).map(|(k,_)|k.clone()).collect();let supported=occ.len()>1&&(!seq.is_empty()||!shapes.is_empty());let mut levels=HashMap::new();let mut stack:Vec<(String,usize)>=Vec::new();let mut symbols=HashMap::new();let mut available=1;while available<3&&occupied.contains(*["□","ㅇ","-"].get(available-1).unwrap()){available+=1}
 for r in rs{if excluded.contains(r.text.trim()){continue}let m=marker_family(&r.text);if !occ.contains_key(&m)||(!seq.contains(&m)&&!shapes.contains(&m)&&!(supported&&occ[&m].len()==1)){continue}let ind=r.text.len()-r.text.trim_start().len();let l=if let Some(v)=levels.get(&m){*v}else{while stack.last().map(|x|ind<x.1).unwrap_or(false){stack.pop();}let mut v=stack.last().map(|x|levels[&x.0]+1).unwrap_or(available).min(3);while v<3&&occupied.contains(*["□","ㅇ","-"].get(v-1).unwrap()){v+=1}levels.insert(m.clone(),v);v};stack.push((m.clone(),ind));if m.chars().count()==1&&ordinal(&m).is_none(){symbols.insert(m,match l{1=>"□",2=>"ㅇ",_=>"-"}.to_string());}}
 (symbols,excluded)
}
fn modal<T:Eq+std::hash::Hash+Clone+std::fmt::Debug>(xs:impl Iterator<Item=T>)->Option<T>{let mut c:Vec<(T,usize)>=Vec::new();for x in xs{if let Some((_,n))=c.iter_mut().find(|(v,_)|*v==x){*n+=1}else{c.push((x,1))}}c.into_iter().max_by_key(|(_,n)|*n).map(|x|x.0)}
fn py_run(ps:Vec<P>)->String{
 let mut rs:Vec<Rec>=ps.into_iter().map(|p|Rec{text:p.text,left:p.left,indent:p.indent,font:p.font,size:p.font_size,marker:String::new(),marker_role:String::new()}).collect();let(symbols,excluded)=detect_scheme(&rs);
 for r in &mut rs{let(m,mut role)=marked(&r.text);if excluded.contains(r.text.trim()){continue}let mf=marker_family(&r.text);if !mf.is_empty()&&occ_scheme_marker(&r.text){if let Some(key)=symbols.get(&mf){role=match key.as_str(){"□"=>"소제목".into(),"ㅇ"=>"본문".into(),_=>"내용".into()};}}if !role.is_empty(){r.marker=canonical(&m);r.marker_role=role}}
 let mut ri=HashMap::new();let mut rt=HashMap::new();for name in ROLES{let relevant:Vec<&Rec>=rs.iter().filter(|r|r.marker_role==name).collect();ri.insert(name,modal(relevant.iter().map(|r|r.left+r.indent)));rt.insert(name,modal(relevant.iter().map(|r|(r.font.clone(),r.size.to_bits()))));}
 let mut roles:Vec<String>=Vec::new();for i in 0..rs.len(){let r=&rs[i];let mut role=r.marker_role.clone();if role.is_empty()&& !r.marker.is_empty(){let choices=["본문","내용","부연설명"];let best=choices.iter().filter_map(|n|ri[n].map(|v|((r.left+r.indent).abs_diff(v),*n))).min_by_key(|x|(x.0,choices.iter().position(|n|*n==x.1).unwrap()));role=best.map(|x|x.1.into()).unwrap_or_else(||if i>0&&roles[i-1]=="소제목"{"본문".into()}else if i>0&&["본문","내용"].contains(&roles[i-1].as_str()){"내용".into()}else if i>0&&roles[i-1]=="부연설명"{"부연설명".into()}else{"미분류".into()})}
 else if role.is_empty(){let near=ROLES.iter().filter_map(|n|ri[n].map(|v|((r.left+r.indent).abs_diff(v),*n))).filter(|x|x.0<=100).min_by_key(|x|(x.0,ROLES.iter().position(|n|*n==x.1).unwrap()));role=near.map(|x|x.1.into()).unwrap_or_else(||if i==0&&r.size>0.0&&rs.iter().skip(1).filter(|x|x.size>0.0).map(|x|x.size).reduce(f64::max).map(|x|r.size>x).unwrap_or(false){"제목".into()}else{"미분류".into()})}roles.push(role)}
 let mut lev=Vec::new();let mut marks=Vec::new();let mut cls=Vec::new();for(i,r)in rs.iter().enumerate(){let l=match roles[i].as_str(){"제목"=>0,"장"=>1,"중제목"=>2,"소제목"=>3,"본문"=>4,"내용"=>5,"부연설명"=>6,_=>7};lev.push(l.to_string());let m=if l>0&&l<7{if !r.marker.is_empty(){r.marker.clone()}else{marked(&r.text).0}}else{String::new()};marks.push(esc(&m));cls.push(esc(&roles[i]))}format!("{{\"levels\":[{}],\"markers\":[{}],\"classifications\":[{}]}}",lev.join(","),marks.join(","),cls.join(","))
}
fn esc(s:&str)->String{let mut o=String::from("\"");for c in s.chars(){match c{'"'=>o.push_str("\\\""),'\\'=>o.push_str("\\\\"),'\n'=>o.push_str("\\n"),c if(c as u32)<0x20=>o.push_str(&format!("\\u{:04x}",c as u32)),c=>o.push(c)}}o.push('"');o}
pub fn run_from(raw:Vec<u8>)->Result<String,String>{let ps=Parser{b:&raw,i:0}.root().map_err(|e|e.to_string())?;Ok(py_run(ps))}


