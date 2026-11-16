//! KVF 전용 가상 항목·작업 저널. 참조 경로만 저장하며 파일 시스템 항목을 조작하지 않는다.
use crate::sqlite::{Arg, Connection};
use std::path::Path;

pub const STORE_VERSION: i64 = 1;
pub const JOURNAL_LIMIT: i64 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualItem {
    pub id: String,
    pub location: String,
    pub source_ref: String,
    pub display_name: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    pub operation_id: String,
    pub kind: String,
    pub item_id: String,
    pub from_location: Option<String>,
    pub to_location: Option<String>,
    pub display_name: String,
    pub source_ref: String,
    pub undone: bool,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Self::open_at(path.to_str().ok_or("가상 저장소 경로 오류")?)
    }
    fn open_at(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path)?;
        conn.exec("PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS kvf_meta(key TEXT PRIMARY KEY,value INTEGER NOT NULL); INSERT OR IGNORE INTO kvf_meta(key,value) VALUES('schema_version',0);")?;
        let mut version = 0;
        conn.query(
            "SELECT value FROM kvf_meta WHERE key='schema_version'",
            &[],
            |r| version = r.int(0),
        )?;
        if version > STORE_VERSION {
            return Err(format!("지원하지 않는 가상 저장소 버전: {version}"));
        }
        if version < 1 {
            conn.exec("BEGIN IMMEDIATE; CREATE TABLE IF NOT EXISTS virtual_item(id TEXT PRIMARY KEY,location TEXT NOT NULL,source_ref TEXT NOT NULL,display_name TEXT NOT NULL,created_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS virtual_item_location ON virtual_item(location); CREATE TABLE IF NOT EXISTS operation_journal(operation_id TEXT PRIMARY KEY,kind TEXT NOT NULL,item_id TEXT NOT NULL,from_location TEXT,to_location TEXT,display_name TEXT NOT NULL,undone INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL,redone INTEGER NOT NULL DEFAULT 0,source_ref TEXT NOT NULL DEFAULT ''); UPDATE kvf_meta SET value=1 WHERE key='schema_version'; COMMIT;")?;
        }
        let mut has_redone = false;
        conn.query("PRAGMA table_info(operation_journal)", &[], |r| {
            if r.text(1) == "redone" { has_redone = true; }
        })?;
        if !has_redone {
            conn.exec("ALTER TABLE operation_journal ADD COLUMN redone INTEGER NOT NULL DEFAULT 0")?;
        }
        let mut has_source_ref = false;
        conn.query("PRAGMA table_info(operation_journal)", &[], |r| { if r.text(1)=="source_ref" {has_source_ref=true;} })?;
        if !has_source_ref { conn.exec("ALTER TABLE operation_journal ADD COLUMN source_ref TEXT NOT NULL DEFAULT ''")?; conn.exec("UPDATE operation_journal SET source_ref=COALESCE((SELECT source_ref FROM virtual_item WHERE virtual_item.id=operation_journal.item_id),'')")?; }
        let mut has_from_name = false;
        conn.query("PRAGMA table_info(operation_journal)", &[], |r| { if r.text(1)=="from_display_name" {has_from_name=true;} })?;
        if !has_from_name { conn.exec("ALTER TABLE operation_journal ADD COLUMN from_display_name TEXT NOT NULL DEFAULT ''")?; conn.exec("UPDATE operation_journal SET from_display_name=display_name")?; }
        Ok(Self { conn })
    }

    pub fn version(&self) -> Result<i64, String> {
        let mut version = 0;
        self.conn.query(
            "SELECT value FROM kvf_meta WHERE key='schema_version'",
            &[],
            |r| version = r.int(0),
        )?;
        Ok(version)
    }

    pub fn add_reference(
        &self,
        id: &str,
        location: &str,
        source_ref: &str,
        display_name: &str,
        created_at: i64,
    ) -> Result<(), String> {
        if id.is_empty() || location.is_empty() || source_ref.is_empty() {
            return Err("가상 항목 ID·위치·경로 참조는 필수입니다".into());
        }
        self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,?)", &[Arg::Text(id),Arg::Text(location),Arg::Text(source_ref),Arg::Text(display_name),Arg::Int(created_at)])?;
        Ok(())
    }

    pub fn list(&self, location: &str) -> Result<Vec<VirtualItem>, String> {
        let mut items = Vec::new();
        self.conn.query("SELECT id,location,source_ref,display_name,created_at FROM virtual_item WHERE location=? ORDER BY created_at,id", &[Arg::Text(location)], |r| items.push(VirtualItem{id:r.text(0),location:r.text(1),source_ref:r.text(2),display_name:r.text(3),created_at:r.int(4)}))?;
        Ok(items)
    }

    pub fn move_virtual(&self, operation_id: &str, item_id: &str, to: &str) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut before = None;
            self.conn.query(
                "SELECT location,display_name,source_ref FROM virtual_item WHERE id=?",
                &[Arg::Text(item_id)],
                |r| before = Some((r.text(0), r.text(1), r.text(2))),
            )?;
            let (from, name, reference) = before.ok_or("가상 항목을 찾을 수 없습니다")?;
            if from == to {
                return Err("같은 가상 위치로 이동할 수 없습니다".into());
            }
            let display = unique_name(&self.conn,to,&name,None)?;
            self.conn.run("UPDATE virtual_item SET location=?,display_name=? WHERE id=?", &[Arg::Text(to),Arg::Text(&display),Arg::Text(item_id)])?;
            self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref,from_display_name) VALUES(?,'move',?,?,?,?,0,strftime('%s','now'),?,?)", &[Arg::Text(operation_id),Arg::Text(item_id),Arg::Text(&from),Arg::Text(to),Arg::Text(&display),Arg::Text(&reference),Arg::Text(&name)])?;
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn move_many(&self, operation_id:&str, item_ids:&[String], to:&str)->Result<(),String>{
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result=(||{if item_ids.is_empty(){return Err("이동할 가상 항목이 없습니다".into())} for item_id in item_ids{let mut before=None;self.conn.query("SELECT location,display_name,source_ref FROM virtual_item WHERE id=?",&[Arg::Text(item_id)],|r|before=Some((r.text(0),r.text(1),r.text(2))))?;let(from,name,reference)=before.ok_or("가상 항목을 찾을 수 없습니다")?;if from==to{return Err("같은 가상 위치로 이동할 수 없습니다".into())}let display=unique_name(&self.conn,to,&name,None)?;self.conn.run("UPDATE virtual_item SET location=?,display_name=? WHERE id=?",&[Arg::Text(to),Arg::Text(&display),Arg::Text(item_id)])?;let op=format!("{operation_id}:{item_id}");self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref,from_display_name) VALUES(?,'move',?,?,?,?,0,strftime('%s','now'),?,?)",&[Arg::Text(&op),Arg::Text(item_id),Arg::Text(&from),Arg::Text(to),Arg::Text(&display),Arg::Text(&reference),Arg::Text(&name)])?;}Ok(())})();match result{Ok(())=>self.conn.exec("COMMIT"),Err(e)=>{let _=self.conn.exec("ROLLBACK");Err(e)}}
    }

    pub fn copy_many(&self, operation_id:&str, item_ids:&[String], to:&str, created_at:i64)->Result<(),String>{
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result=(||{if item_ids.is_empty(){return Err("복제할 가상 항목이 없습니다".into())} for(n,item_id)in item_ids.iter().enumerate(){let mut source=None;self.conn.query("SELECT source_ref,location,display_name FROM virtual_item WHERE id=?",&[Arg::Text(item_id)],|r|source=Some((r.text(0),r.text(1),r.text(2))))?;let(reference,from,name)=source.ok_or("복제할 가상 항목을 찾을 수 없습니다")?;let copy_id=format!("{operation_id}:{n}");let display=unique_name(&self.conn,to,&name,None)?;self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,?)",&[Arg::Text(&copy_id),Arg::Text(to),Arg::Text(&reference),Arg::Text(&display),Arg::Int(created_at)])?;let op=format!("{operation_id}:{n}");self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref) VALUES(?,'copy',?,?,?, ?,0,strftime('%s','now'),?)",&[Arg::Text(&op),Arg::Text(&copy_id),Arg::Text(&from),Arg::Text(to),Arg::Text(&display),Arg::Text(&reference)])?;}Ok(())})();match result{Ok(())=>self.conn.exec("COMMIT"),Err(e)=>{let _=self.conn.exec("ROLLBACK");Err(e)}}
    }

    pub fn virtual_copy(
        &self,
        operation_id: &str,
        source_id: &str,
        copy_id: &str,
        to: &str,
        name: &str,
        created_at: i64,
    ) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut source = None;
            self.conn.query(
                "SELECT source_ref,location,display_name FROM virtual_item WHERE id=?",
                &[Arg::Text(source_id)],
                |r| source = Some((r.text(0), r.text(1), r.text(2))),
            )?;
            let (reference, from, source_name) =
                source.ok_or("복제할 가상 항목을 찾을 수 없습니다")?;
            let display = unique_name(&self.conn,to,if name.is_empty(){&source_name}else{name},None)?;
            self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,?)", &[Arg::Text(copy_id),Arg::Text(to),Arg::Text(&reference),Arg::Text(&display),Arg::Int(created_at)])?;
            self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref) VALUES(?,'copy',?,?,?, ?,0,strftime('%s','now'),?)", &[Arg::Text(operation_id),Arg::Text(copy_id),Arg::Text(&from),Arg::Text(to),Arg::Text(&display),Arg::Text(&reference)])?;
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn journal(&self) -> Result<Vec<JournalEntry>, String> {
        let mut entries = Vec::new();
        self.conn.query("SELECT operation_id,kind,item_id,from_location,to_location,display_name,source_ref,undone FROM operation_journal ORDER BY created_at DESC,rowid DESC LIMIT 100", &[], |r| entries.push(JournalEntry{operation_id:r.text(0),kind:r.text(1),item_id:r.text(2),from_location:(!r.text(3).is_empty()).then(||r.text(3)),to_location:(!r.text(4).is_empty()).then(||r.text(4)),display_name:r.text(5),source_ref:r.text(6),undone:r.int(7)!=0}))?;
        Ok(entries)
    }

    pub fn undo(&self, operation_id: &str) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut entries=Vec::new();
            self.conn.query("SELECT kind,item_id,from_location,to_location,undone,redone,from_display_name FROM operation_journal WHERE operation_id=? OR operation_id LIKE ? ORDER BY rowid", &[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))], |r| entries.push((r.text(0),r.text(1),r.text(2),r.text(3),r.int(4)!=0,r.int(5)!=0,r.text(6))))?;
            if entries.is_empty(){return Err("작업 기록을 찾을 수 없습니다".into())}
            if entries.iter().any(|e|e.4||e.5){return Err("이미 되돌린 작업입니다".into())}
            for (kind,id,from,to,_,_,from_name) in entries {
                if kind=="move" {self.conn.run("UPDATE virtual_item SET location=?,display_name=? WHERE id=? AND location=?", &[Arg::Text(&from),Arg::Text(&from_name),Arg::Text(&id),Arg::Text(&to)])?;}
                else if kind=="copy" {self.conn.run("DELETE FROM virtual_item WHERE id=?", &[Arg::Text(&id)])?;}
                else {return Err("지원하지 않는 작업 종류입니다".into());}
            }
            self.conn.run("UPDATE operation_journal SET undone=1 WHERE operation_id=? OR operation_id LIKE ?", &[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))])?;
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn redo(&self, operation_id: &str) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result=(|| { let mut entries=Vec::new(); self.conn.query("SELECT kind,item_id,from_location,to_location,undone,redone,display_name,source_ref FROM operation_journal WHERE operation_id=? OR operation_id LIKE ? ORDER BY rowid", &[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))], |r| entries.push((r.text(0),r.text(1),r.text(2),r.text(3),r.int(4)!=0,r.int(5)!=0,r.text(6),r.text(7))))?;
            if entries.is_empty(){return Err("작업 기록을 찾을 수 없습니다".into())} if entries.iter().any(|e|!e.4||e.5){return Err("다시 실행할 수 없는 작업입니다".into())}
            for(kind,id,from,to,_,_,name,source_ref) in entries {
                if kind=="move" { self.conn.run("UPDATE virtual_item SET location=?,display_name=? WHERE id=? AND location=?", &[Arg::Text(&to),Arg::Text(&name),Arg::Text(&id),Arg::Text(&from)])?; }
                else if kind=="copy" { self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,strftime('%s','now'))", &[Arg::Text(&id),Arg::Text(&to),Arg::Text(&source_ref),Arg::Text(&name)])?; }
                else { return Err("지원하지 않는 작업 종류입니다".into()); }
            }
            self.conn.run("UPDATE operation_journal SET undone=0,redone=1 WHERE operation_id=? OR operation_id LIKE ?", &[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))])?; Ok(()) })();
        match result { Ok(())=>self.conn.exec("COMMIT"),Err(e)=>{let _=self.conn.exec("ROLLBACK");Err(e)} }
    }
}

fn unique_name(conn:&Connection,location:&str,name:&str,except:Option<&str>)->Result<String,String>{
    let exists=|candidate:&str|->Result<bool,String>{let mut found=false;conn.query("SELECT id FROM virtual_item WHERE location=? AND display_name=?", &[Arg::Text(location),Arg::Text(candidate)], |r|{if except!=Some(r.text(0).as_str()){found=true;}})?;Ok(found)};
    if !exists(name)? { return Ok(name.to_string()); }
    let (stem,ext)=name.rsplit_once('.').filter(|(s,_)|!s.is_empty()).map(|(s,e)|(s.to_string(),format!(".{e}"))).unwrap_or((name.to_string(),String::new()));
    for n in 2..=9999 {let candidate=format!("{stem} ({n}){ext}");if !exists(&candidate)?{return Ok(candidate)}}
    Err("겹치는 이름 번호를 정할 수 없습니다".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    use std::path::PathBuf;
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "kvf-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn db(&self) -> PathBuf {
            self.0.join("virtual.db")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn initializes_and_migrates_version_zero() {
        let s = Store::open_at(":memory:").unwrap();
        assert_eq!(s.version().unwrap(), 1);
    }
    #[test]
    fn reference_and_journal_roundtrip_and_undo() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/문서/원본.txt", "원본.txt", 1)
            .unwrap();
        s.move_virtual("op1", "a", "two").unwrap();
        assert_eq!(s.list("two").unwrap()[0].source_ref, "C:/문서/원본.txt");
        assert_eq!(s.journal().unwrap()[0].operation_id, "op1");
        s.undo("op1").unwrap();
        assert_eq!(s.list("one").unwrap().len(), 1);
        assert_eq!(s.list("two").unwrap().len(), 0);
    }
    #[test]
    fn virtual_copy_copies_reference_without_touching_file() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "X:/원본/자료.txt", "자료.txt", 1)
            .unwrap();
        s.virtual_copy("op2", "a", "b", "two", "자료 (2).txt", 2)
            .unwrap();
        let copied = &s.list("two").unwrap()[0];
        assert_eq!(copied.source_ref, "X:/원본/자료.txt");
        s.undo("op2").unwrap();
        assert!(s.list("two").unwrap().is_empty());
    }
    #[test]
    fn missing_item_move_rolls_back() {
        let s = Store::open_at(":memory:").unwrap();
        assert!(s.move_virtual("bad", "none", "two").is_err());
        assert!(s.journal().unwrap().is_empty());
    }

    #[test] fn move_collision_adds_number_and_preserves_reference(){let s=Store::open_at(":memory:").unwrap();s.add_reference("a","one","C:/원본/자료.txt","자료.txt",1).unwrap();s.add_reference("b","two","C:/다른/자료.txt","자료.txt",1).unwrap();s.move_virtual("m","a","two").unwrap();let moved=s.list("two").unwrap().into_iter().find(|item|item.id=="a").unwrap();assert_eq!(moved.display_name,"자료 (2).txt");assert_eq!(moved.source_ref,"C:/원본/자료.txt");}
    #[test] fn copy_collision_adds_number(){let s=Store::open_at(":memory:").unwrap();s.add_reference("a","one","C:/자료.txt","자료.txt",1).unwrap();s.add_reference("b","two","C:/기존.txt","자료.txt",1).unwrap();s.virtual_copy("c","a","copy","two","",2).unwrap();assert_eq!(s.list("two").unwrap().iter().find(|i|i.id=="copy").unwrap().display_name,"자료 (2).txt");}
    #[test] fn repeated_collision_uses_next_free_number(){let s=Store::open_at(":memory:").unwrap();for(id,name)in[("a","자료.txt"),("b","자료.txt"),("c","자료 (2).txt")] {s.add_reference(id,"one","C:/자료.txt",name,1).unwrap();}s.move_virtual("m","a","two").unwrap();s.move_virtual("n","b","two").unwrap();assert_eq!(s.list("two").unwrap().iter().map(|i|i.display_name.as_str()).collect::<Vec<_>>(),vec!["자료.txt","자료 (2).txt"]);}
    #[test] fn multi_move_is_undone_as_one_operation(){let s=Store::open_at(":memory:").unwrap();for id in ["a","b"]{s.add_reference(id,"one","C:/자료.txt",id,1).unwrap();}s.move_many("batch",&["a".into(),"b".into()],"two").unwrap();s.undo("batch").unwrap();assert_eq!(s.list("one").unwrap().len(),2);assert!(s.list("two").unwrap().is_empty());}
    #[test] fn multi_copy_is_undone_as_one_operation(){let s=Store::open_at(":memory:").unwrap();for id in ["a","b"]{s.add_reference(id,"one","C:/자료.txt",id,1).unwrap();}s.copy_many("batch",&["a".into(),"b".into()],"two",2).unwrap();assert_eq!(s.list("two").unwrap().len(),2);s.undo("batch").unwrap();assert!(s.list("two").unwrap().is_empty());}
    #[test] fn move_redo_restores_location(){let s=Store::open_at(":memory:").unwrap();s.add_reference("a","one","C:/a","a",1).unwrap();s.move_many("m",&["a".into()],"two").unwrap();s.undo("m").unwrap();s.redo("m").unwrap();assert_eq!(s.list("two").unwrap().len(),1);}
    #[test] fn undo_collision_move_restores_original_display_name(){let s=Store::open_at(":memory:").unwrap();s.add_reference("a","one","C:/a","a",1).unwrap();s.add_reference("b","two","C:/b","a",1).unwrap();s.move_virtual("m","a","two").unwrap();assert_eq!(s.list("two").unwrap().iter().find(|item|item.id=="a").unwrap().display_name,"a (2)");s.undo("m").unwrap();assert_eq!(s.list("one").unwrap()[0].display_name,"a");}
    #[test] fn copy_redo_restores_virtual_record(){let s=Store::open_at(":memory:").unwrap();s.add_reference("a","one","C:/a","a",1).unwrap();s.copy_many("c",&["a".into()],"two",2).unwrap();s.undo("c").unwrap();s.redo("c").unwrap();assert_eq!(s.list("two").unwrap()[0].source_ref,"C:/a");}
    #[test] fn journal_survives_reopen_and_restores_undo_state(){let t=Temp::new();{let s=Store::open(&t.db()).unwrap();s.add_reference("a","one","C:/a","a",1).unwrap();s.move_virtual("m","a","two").unwrap();}let s=Store::open(&t.db()).unwrap();assert_eq!(s.journal().unwrap()[0].operation_id,"m");s.undo("m").unwrap();}
    #[test] fn failed_multi_operation_rolls_back_every_item(){let s=Store::open_at(":memory:").unwrap();s.add_reference("a","one","C:/a","a",1).unwrap();assert!(s.move_many("m",&["a".into(),"missing".into()],"two").is_err());assert_eq!(s.list("one").unwrap().len(),1);assert!(s.list("two").unwrap().is_empty());assert!(s.journal().unwrap().is_empty());}
    #[test] fn same_location_move_is_rejected_without_journal(){let s=Store::open_at(":memory:").unwrap();s.add_reference("a","one","C:/a","a",1).unwrap();assert!(s.move_virtual("m","a","one").is_err());assert!(s.journal().unwrap().is_empty());}
    #[test] fn empty_batch_is_rejected(){let s=Store::open_at(":memory:").unwrap();assert!(s.move_many("m",&[],"two").is_err());assert!(s.copy_many("c",&[],"two",1).is_err());}
    #[test] fn undo_then_redo_state_is_recorded(){let s=Store::open_at(":memory:").unwrap();s.add_reference("a","one","C:/a","a",1).unwrap();s.move_many("m",&["a".into()],"two").unwrap();s.undo("m").unwrap();s.redo("m").unwrap();assert!(!s.journal().unwrap()[0].undone);}
}
