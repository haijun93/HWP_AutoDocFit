use outliner_core::{backlinks, daily_page, edit, parse, search, serialize, OutlinerError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

#[derive(Deserialize)] struct Request { id:Option<Value>, op:String, document:Option<String>, args:Option<Value> }
fn handle(v:Value)->Value {
 let id=v.get("id").cloned().unwrap_or(Value::Null);
 let req:Request=match serde_json::from_value(v){Ok(r)=>r,Err(e)=>return json!({"id":id,"ok":false,"error":{"code":"INVALID_REQUEST","message":e.to_string()}})};
 let args=req.args.unwrap_or(Value::Null);
 let result:Result<Value,OutlinerError>=(||{match req.op.as_str(){
  "parse"=>{let d=parse(req.document.as_deref().unwrap_or(""))?;serde_json::to_value(d).map_err(|e|OutlinerError::Invalid(e.to_string()))},
  "serialize"=>{let d=parse(req.document.as_deref().unwrap_or(""))?;Ok(json!({"markdown":serialize(&d)}))},
  "edit"=>{let mut d=parse(req.document.as_deref().unwrap_or(""))?;let op=args["action"].as_str().unwrap_or("");let id=args["block_id"].as_u64().unwrap_or(0);let text=args["text"].as_str();edit(&mut d,op,id,text)?;Ok(json!({"markdown":serialize(&d),"document":d}))},
  "search"=>{let d=parse(req.document.as_deref().unwrap_or(""))?;let ids=search(&d,args["query"].as_str().unwrap_or(""))?;Ok(json!({"block_ids":ids}))},
  "backlinks"=>{let d=parse(req.document.as_deref().unwrap_or(""))?;let target=args["target"].as_str().unwrap_or("");Ok(json!({"block_ids":backlinks(&d,target)}))},
  "daily_page"=>Ok(json!({"filename":daily_page(args["date"].as_str().unwrap_or(""))?})),
  _=>Err(OutlinerError::Invalid(format!("지원하지 않는 명령: {}",req.op)))
 }})();
 match result{Ok(v)=>json!({"id":req.id,"ok":true,"result":v}),Err(e)=>json!({"id":req.id,"ok":false,"error":{"code":e.code(),"message":e.to_string()}})}
}
fn main(){let stdin=io::stdin();let mut out=io::BufWriter::new(io::stdout().lock());for line in stdin.lock().lines(){let response=match line{Ok(s)=>match serde_json::from_str::<Value>(&s){Ok(v)=>handle(v),Err(e)=>json!({"id":null,"ok":false,"error":{"code":"INVALID_JSON","message":e.to_string()}})},Err(e)=>json!({"id":null,"ok":false,"error":{"code":"IO_ERROR","message":e.to_string()}})};let _=writeln!(out,"{}",response);let _=out.flush();}}
