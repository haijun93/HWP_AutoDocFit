//! 읽을 때만 연결하고 반환 전에 닫는다. 데이터베이스 변경 문장은 두지 않는다.
use std::path::{Path, Component};
use crate::{sqlite::{Connection,Arg},open_rules};
pub const LIMIT: usize = 5000;
#[derive(Clone,Debug,PartialEq)] pub struct BoxRow { pub id:i64,pub name:String,pub description:String,pub created_at:f64,pub updated_at:f64 }
#[derive(Clone,Debug,PartialEq)] pub struct Item { pub id:i64,pub box_id:i64,pub kind:String,pub source_path:String,pub origin_path:String,pub managed_path:String,pub display_name:String,pub space:String,pub created_at:f64 }
#[derive(Clone,Debug,PartialEq)] pub enum EmptyReason { NoDatabase, NoBoxes, NoItems }
#[derive(Clone,Debug,PartialEq)] pub enum Snapshot { Empty(EmptyReason), Loaded { boxes:Vec<BoxRow>,box_id:i64,items:Vec<Item>,truncated:bool } }
pub fn read(root:&Path,selected:Option<i64>)->Result<Snapshot,String> {
    read_inner(root,selected).map_err(|e| if e.contains("locked") || e.contains("busy") { "?? ??? ??? ?? ????. F5? ??".into() } else { e })
}
fn read_inner(root:&Path,selected:Option<i64>)->Result<Snapshot,String> {
    let path=root.join("library.db");
    match std::fs::metadata(&path) {
        Err(e) if e.kind()==std::io::ErrorKind::NotFound => return Ok(Snapshot::Empty(EmptyReason::NoDatabase)),
        Err(e) => return Err(e.to_string()), _=>{}
    }
    let conn=Connection::open_readonly(path.to_str().ok_or("자료 경로 오류")?)?;
    let mut boxes=Vec::new();
    conn.query("SELECT id,name,description,created_at,updated_at FROM box ORDER BY updated_at DESC,id DESC",&[],|r|boxes.push(BoxRow{id:r.int(0),name:r.text(1),description:r.text(2),created_at:r.real(3),updated_at:r.real(4)}))?;
    // 표가 없는 DB는 빈 자료가 아니라 오류다. 빈 상자 목록이어도 item 구조를 확인한다.
    let mut columns=Vec::new();
    conn.query("PRAGMA table_info(item)",&[],|r|columns.push(r.text(1)))?;
    for required in ["id","box_id","kind","source_path","display_name","space","created_at"] {
        if !columns.iter().any(|c|c==required) { return Err(format!("item 열이 없습니다: {required}")); }
    }
    if boxes.is_empty() { return Ok(Snapshot::Empty(EmptyReason::NoBoxes)); }
    let box_id=selected.filter(|id|boxes.iter().any(|b|b.id==*id)).unwrap_or(boxes[0].id);
    let column=|name:&str| if columns.iter().any(|c|c==name) {name.to_string()} else {"''".into()};
    let sql=format!("SELECT id,box_id,kind,source_path,{},{},display_name,space,created_at FROM item WHERE box_id=? ORDER BY created_at DESC,id DESC LIMIT 5001",column("origin_path"),column("managed_path"));
    let mut items=Vec::new();
    conn.query(&sql,&[Arg::Int(box_id)],|r|items.push(Item{id:r.int(0),box_id:r.int(1),kind:r.text(2),source_path:r.text(3),origin_path:r.text(4),managed_path:r.text(5),display_name:r.text(6),space:r.text(7),created_at:r.real(8)}))?;
    let truncated=items.len()>LIMIT;
    items.truncate(LIMIT);
    // 목록이 비어도 상자 선택 메뉴를 만들 자료는 반환한다. 화면은 NoItems로 판정한다.
    Ok(Snapshot::Loaded{boxes,box_id,items,truncated})
}
#[derive(Clone,Debug,PartialEq,Eq)] pub enum OpenTarget { Web(String), Copy(String) }
#[derive(Clone,Debug,PartialEq,Eq)] pub enum OpenError { NoCopy, UnsafePath, OutsideSpace, Missing, Executable, UnsafeUrl }
/// 파일 존재 여부는 호출자가 관측해 넘긴다. 경로 판정 자체는 부작용 없는 순수 함수다.
pub fn openable(root:&Path,item:&Item,exists:bool)->Result<OpenTarget,OpenError> {
    if item.kind=="link" { return open_rules::safe_web_url(&item.source_path).map(|s|OpenTarget::Web(s.into())).ok_or(OpenError::UnsafeUrl); }
    if item.managed_path.is_empty() { return Err(OpenError::NoCopy); }
    let path=Path::new(&item.managed_path);
    if !path.is_absolute() || item.managed_path.split(['\\','/']).any(|s|s==".." || s==".") || path.components().any(|c|matches!(c,Component::ParentDir)) { return Err(OpenError::UnsafePath); }
    if item.box_id <= 0 || !["core","basis","reference","temp"].contains(&item.space.as_str()) { return Err(OpenError::OutsideSpace); }
    let expected=root.join(item.box_id.to_string()).join(&item.space);
    if path.parent()!=Some(expected.as_path()) { return Err(OpenError::OutsideSpace); }
    // ADS·끝 점/공백은 Windows의 다른 이름 해석을 피하도록 거부한다.
    let leaf=path.file_name().and_then(|s|s.to_str()).ok_or(OpenError::UnsafePath)?;
    if leaf.contains(':') || leaf.ends_with(['.',' ']) { return Err(OpenError::UnsafePath); }
    if open_rules::forbidden(&item.managed_path) { return Err(OpenError::Executable); }
    if !exists { return Err(OpenError::Missing); }
    Ok(OpenTarget::Copy(item.managed_path.clone()))
}
pub fn filter<'a>(items:&'a [Item],space:Option<&str>,query:&str)->Vec<&'a Item> {
    let query=query.to_lowercase();
    items.iter().filter(|i|space.is_none_or(|s|s==i.space) && (query.is_empty() || [&i.display_name,&i.source_path,&i.origin_path,&i.managed_path].iter().any(|s|s.to_lowercase().contains(&query)))).collect()
}


#[cfg(test)] mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64,Ordering};
    static NEXT:AtomicU64=AtomicU64::new(0);
    struct Fixture { root:std::path::PathBuf }
    impl Fixture {
        fn new(legacy:bool)->Self {
            let root=std::env::temp_dir().join(format!("kvf-data-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
            std::fs::create_dir_all(&root).unwrap();
            let c=Connection::open(root.join("library.db").to_str().unwrap()).unwrap();
            c.exec("CREATE TABLE box(id INTEGER PRIMARY KEY,name TEXT,description TEXT,created_at REAL,updated_at REAL); CREATE TABLE item(id INTEGER PRIMARY KEY,box_id INTEGER,kind TEXT,source_path TEXT,display_name TEXT,space TEXT,created_at REAL)").unwrap();
            if !legacy { c.exec("ALTER TABLE item ADD COLUMN origin_path TEXT NOT NULL DEFAULT ''; ALTER TABLE item ADD COLUMN managed_path TEXT NOT NULL DEFAULT ''").unwrap(); }
            Self{root}
        }
        fn conn(&self)->Connection {Connection::open(self.root.join("library.db").to_str().unwrap()).unwrap()}
        fn populate(&self) { self.conn().exec("INSERT INTO box VALUES(1,'첫 상자','설명',1,1),(2,'최신 상자','',2,2); INSERT INTO item(id,box_id,kind,source_path,display_name,space,created_at) VALUES(1,1,'file','C:/origin/Report.txt','한글 Report','core',1)").unwrap(); }
        fn read(&self,id:Option<i64>)->Snapshot {read(&self.root,id).unwrap()}
        fn item(&self)->Item {Item{id:1,box_id:1,kind:"file".into(),source_path:"C:/origin/Report.txt".into(),origin_path:"C:/원본/자료.txt".into(),managed_path:self.root.join("1/core/report.txt").to_string_lossy().into(),display_name:"한글 Report".into(),space:"core".into(),created_at:1.0}}
    }
    impl Drop for Fixture {fn drop(&mut self){std::fs::remove_dir_all(&self.root).unwrap();}}
    fn loaded(s:Snapshot)->(Vec<BoxRow>,i64,Vec<Item>,bool) { match s {Snapshot::Loaded{boxes,box_id,items,truncated}=>(boxes,box_id,items,truncated),_=>panic!("expected loaded")}}
    #[test] fn no_database_does_not_create() {let f=Fixture::new(false);std::fs::remove_file(f.root.join("library.db")).unwrap();assert_eq!(f.read(None),Snapshot::Empty(EmptyReason::NoDatabase));assert!(!f.root.join("library.db").exists());}
    #[test] fn no_boxes() {let f=Fixture::new(false);assert_eq!(f.read(None),Snapshot::Empty(EmptyReason::NoBoxes));}
    #[test] fn boxes_latest_first() {let f=Fixture::new(false);f.populate();let (b,id,_,_)=loaded(f.read(None));assert_eq!(b.iter().map(|b|b.id).collect::<Vec<_>>(),vec![2,1]);assert_eq!(id,2);assert_eq!(b[1].description,"설명");}
    #[test] fn selected_box_items() {let f=Fixture::new(false);f.populate();let (_,id,i,t)=loaded(f.read(Some(1)));assert_eq!(id,1);assert_eq!(i[0].display_name,"한글 Report");assert!(!t);}
    #[test] fn missing_selection_falls_back() {let f=Fixture::new(false);f.populate();assert_eq!(loaded(f.read(Some(99))).1,2);}
    #[test] fn empty_items_keep_boxes() {let f=Fixture::new(false);f.populate();let(b,_,i,_)=loaded(f.read(Some(2)));assert_eq!(b.len(),2);assert!(i.is_empty());}
    #[test] fn legacy_missing_columns() {let f=Fixture::new(true);f.populate();let i=loaded(f.read(Some(1))).2;assert_eq!(i[0].origin_path,"");assert_eq!(i[0].managed_path,"");}
    #[test] fn current_copy_columns() {let f=Fixture::new(false);f.populate();f.conn().exec("UPDATE item SET origin_path='origin',managed_path='copy'").unwrap();let i=loaded(f.read(Some(1))).2;assert_eq!((&*i[0].origin_path,&*i[0].managed_path),("origin","copy"));}
    #[test] fn missing_table_is_failure() {let f=Fixture::new(false);f.conn().exec("DROP TABLE item").unwrap();assert!(read(&f.root,None).is_err());}
    #[test] fn corrupt_db_is_failure() {let f=Fixture::new(false);std::fs::write(f.root.join("library.db"),b"not a database").unwrap();assert!(read(&f.root,None).is_err());}
    #[test] fn cap_5000() {let f=Fixture::new(false);f.populate();f.conn().exec("WITH RECURSIVE n(x) AS(SELECT 2 UNION ALL SELECT x+1 FROM n WHERE x<5002) INSERT INTO item(id,box_id,kind,source_path,display_name,space,created_at) SELECT x,1,'file','','item','temp',x FROM n").unwrap();let(_,_,i,t)=loaded(f.read(Some(1)));assert_eq!(i.len(),5000);assert!(t);assert_eq!(i[0].id,5002);}
    #[test] fn readonly_rejects_writes_and_no_create() {let f=Fixture::new(false);let c=Connection::open_readonly(f.root.join("library.db").to_str().unwrap()).unwrap();assert!(c.exec("INSERT INTO box VALUES(1,'a','',1,1)").is_err());assert!(Connection::open_readonly(f.root.join("absent.db").to_str().unwrap()).is_err());assert!(!f.root.join("absent.db").exists());}
    #[test] fn db_bytes_unchanged() {let f=Fixture::new(false);f.populate();let path=f.root.join("library.db");let before=std::fs::read(&path).unwrap();f.read(Some(1));assert_eq!(before,std::fs::read(path).unwrap());}
    #[test] fn wal_reader_sees_commits() {let f=Fixture::new(false);let writer=f.conn();writer.exec("PRAGMA journal_mode=WAL").unwrap();f.populate();assert_eq!(loaded(f.read(Some(1))).2.len(),1);writer.exec("INSERT INTO item(id,box_id,kind,source_path,display_name,space,created_at) VALUES(2,1,'file','','new','temp',2)").unwrap();assert_eq!(loaded(f.read(Some(1))).2.len(),2);}
    #[test] fn lock_returns_error_within_bound() {let f=Fixture::new(false);let writer=f.conn();writer.exec("BEGIN EXCLUSIVE").unwrap();let start=std::time::Instant::now();assert!(read(&f.root,None).is_err());assert!(start.elapsed()<std::time::Duration::from_secs(2));writer.exec("ROLLBACK").unwrap();}
    #[test] fn normal_copy() {let f=Fixture::new(false);let i=f.item();std::fs::create_dir_all(f.root.join("1/core")).unwrap();std::fs::write(&i.managed_path,b"copy").unwrap();assert_eq!(openable(&f.root,&i,Path::new(&i.managed_path).exists()),Ok(OpenTarget::Copy(i.managed_path.clone())));}
    #[test] fn no_copy_never_uses_origin() {let f=Fixture::new(false);let mut i=f.item();i.managed_path.clear();assert_eq!(openable(&f.root,&i,true),Err(OpenError::NoCopy));}
    #[test] fn traversal() {let f=Fixture::new(false);let mut i=f.item();i.managed_path=f.root.join("1/core/../secret.txt").to_string_lossy().into();assert_eq!(openable(&f.root,&i,true),Err(OpenError::UnsafePath));}
    #[test] fn outside_space() {let f=Fixture::new(false);let mut i=f.item();i.managed_path=f.root.join("2/core/file.txt").to_string_lossy().into();assert_eq!(openable(&f.root,&i,true),Err(OpenError::OutsideSpace));}
    #[test] fn missing_copy() {let f=Fixture::new(false);assert_eq!(openable(&f.root,&f.item(),false),Err(OpenError::Missing));}
    #[test] fn executable_copy() {let f=Fixture::new(false);let mut i=f.item();i.managed_path=f.root.join("1/core/a.EXE").to_string_lossy().into();assert_eq!(openable(&f.root,&i,true),Err(OpenError::Executable));}
    #[test] fn web_link() {let f=Fixture::new(false);let mut i=f.item();i.kind="link".into();i.source_path="https://example.com/a".into();assert_eq!(openable(&f.root,&i,false),Ok(OpenTarget::Web(i.source_path.clone())));}
    #[test] fn unsafe_link() {let f=Fixture::new(false);let mut i=f.item();i.kind="link".into();for url in ["file:///a","javascript:alert(1)","https://bad url"] {i.source_path=url.into();assert_eq!(openable(&f.root,&i,true),Err(OpenError::UnsafeUrl));}}
    #[test] fn invalid_space() {let f=Fixture::new(false);let mut i=f.item();i.space="../core".into();assert_eq!(openable(&f.root,&i,true),Err(OpenError::OutsideSpace));}
    #[test] fn all_spaces_filter() {let f=Fixture::new(false);let a=f.item();let mut b=a.clone();b.space="temp".into();let items=[a,b];assert_eq!(filter(&items,None,"").len(),2);assert_eq!(filter(&items,Some("core"),"").len(),1);}
    #[test] fn korean_filter() {let f=Fixture::new(false);assert_eq!(filter(&[f.item()],None,"한글").len(),1);}
    #[test] fn case_insensitive_filter() {let f=Fixture::new(false);assert_eq!(filter(&[f.item()],None,"rEpOrT").len(),1);}
    #[test] fn paths_filter() {let f=Fixture::new(false);let i=[f.item()];for q in ["origin","원본","core"] {assert_eq!(filter(&i,None,q).len(),1);}}
    #[test] fn combined_filter() {let f=Fixture::new(false);let i=[f.item()];assert_eq!(filter(&i,Some("temp"),"한글").len(),0);assert_eq!(filter(&i,None,"없음").len(),0);}
}
