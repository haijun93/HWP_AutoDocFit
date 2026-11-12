//! 창 상태 전이. Win32·DB·파일 접근 없이 다음 상태와 할 일만 반환한다.
#[derive(Clone,Debug,Default,PartialEq,Eq)]
pub struct Context { pub box_id:Option<i64>,pub space:Option<String>,pub query:String,pub selection:Option<i64> }
#[derive(Clone,Debug,PartialEq,Eq)]
pub enum View {
    Hidden, Loading(Context), List(Context), Search(Context),
    Empty{reason:String,context:Context}, Failed{error:String,context:Context}, Minimized(Box<View>),
}
impl View {
    pub fn name(&self)->&'static str {match self {Self::Hidden=>"hidden",Self::Loading(_)=>"loading",Self::List(_)=>"list",Self::Search(_)=>"search",Self::Empty{..}=>"empty",Self::Failed{..}=>"failed",Self::Minimized(_)=>"minimized"}}
    pub fn context(&self)->Option<&Context> {match self {Self::Loading(c)|Self::List(c)|Self::Search(c)|Self::Empty{context:c,..}|Self::Failed{context:c,..}=>Some(c),Self::Minimized(v)=>v.context(),Self::Hidden=>None}}
    fn visible(&self)->bool {!matches!(self,Self::Hidden|Self::Minimized(_))}
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Model { pub view:View,pub seq:u64,pub sticky:bool,pub pending_open:Option<u64> }
impl Default for Model {fn default()->Self {Self{view:View::Hidden,seq:0,sticky:false,pending_open:None}}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum Caller {Hotkey,Tray,SecondRun}
#[derive(Clone,Debug,PartialEq,Eq)] pub enum ReadResult { Items{box_id:i64}, Empty(String), Failed(String) }
#[derive(Clone,Debug,PartialEq,Eq)] pub enum Event {
    Call(Caller), Loaded{seq:u64,result:ReadResult}, Query(String), Escape, Close, Outside,
    Minimize, ChangeBox(i64), ChangeSpace(Option<String>), Select(Option<i64>), Refresh,
    Open, Opened{seq:u64,result:Result<(),String>}, SetSticky(bool),
}
#[derive(Clone,Debug,PartialEq,Eq)] pub enum Effect {
    Show, Foreground, Hide, ClearItems, Read{seq:u64,context:Context}, Draw, Filter,
    Open{seq:u64,item_id:i64}, Notice(String),
}
fn loading(mut m:Model,c:Context,mut effects:Vec<Effect>)->(Model,Vec<Effect>) {
    m.seq+=1; m.pending_open=None; m.view=View::Loading(c.clone());
    effects.push(Effect::Read{seq:m.seq,context:c}); (m,effects)
}
fn hide(mut m:Model)->(Model,Vec<Effect>) {
    m.seq+=1; m.pending_open=None; m.view=View::Hidden;
    (m,vec![Effect::Hide,Effect::ClearItems])
}
pub fn step(current:&Model,event:Event)->(Model,Vec<Effect>) {
    let mut m=current.clone();
    match event {
        Event::SetSticky(value)=>{m.sticky=value;(m,vec![])},
        Event::Call(caller)=>match &m.view {
            View::Hidden=>loading(m,Context::default(),vec![Effect::Show]),
            View::Minimized(saved)=>{let c=saved.context().cloned().unwrap_or_default();loading(m,c,vec![Effect::Show])},
            _ if caller==Caller::Hotkey && m.sticky=>hide(m),
            _=>(m,vec![Effect::Foreground]),
        },
        Event::Loaded{seq,result} if seq==m.seq && matches!(m.view,View::Loading(_))=>{
            let mut c=m.view.context().unwrap().clone();
            m.view=match result {
                ReadResult::Items{box_id}=>{c.box_id=Some(box_id);if c.query.is_empty(){View::List(c)}else{View::Search(c)}},
                ReadResult::Empty(reason)=>View::Empty{reason,context:c},
                ReadResult::Failed(error)=>View::Failed{error,context:c},
            };
            (m,vec![Effect::Filter,Effect::Draw])
        },
        Event::Query(query) if matches!(m.view,View::List(_)|View::Search(_))=>{
            let mut c=m.view.context().unwrap().clone();c.query=query;c.selection=None;
            m.pending_open=None;m.view=if c.query.is_empty(){View::List(c)}else{View::Search(c)};
            (m,vec![Effect::Filter,Effect::Draw])
        },
        Event::Escape if matches!(m.view,View::Search(_))=>step(&m,Event::Query(String::new())),
        Event::Close|Event::Escape if m.view.visible()=>hide(m),
        Event::Outside if m.view.visible() && !m.sticky=>hide(m),
        Event::Minimize if m.view.visible()=>{m.seq+=1;m.pending_open=None;m.view=View::Minimized(Box::new(m.view));(m,vec![Effect::Hide])},
        Event::ChangeBox(id) if id>0 && matches!(m.view,View::List(_)|View::Search(_))=>{
            let mut c=m.view.context().unwrap().clone();c.box_id=Some(id);c.query.clear();c.selection=None;loading(m,c,vec![])
        },
        Event::ChangeSpace(space) if matches!(m.view,View::List(_)|View::Search(_)) && space.as_deref().is_none_or(|s|["core","basis","reference","temp"].contains(&s))=>{
            let mut c=m.view.context().unwrap().clone();c.space=space;c.query.clear();c.selection=None;m.pending_open=None;m.view=View::List(c);(m,vec![Effect::Filter,Effect::Draw])
        },
        Event::Select(selection) if matches!(m.view,View::List(_)|View::Search(_))=>{
            let mut c=m.view.context().unwrap().clone();c.selection=selection;m.pending_open=None;
            m.view=if c.query.is_empty(){View::List(c)}else{View::Search(c)};(m,vec![Effect::Draw])
        },
        Event::Refresh if matches!(m.view,View::List(_)|View::Search(_)|View::Empty{..}|View::Failed{..})=>{
            let c=m.view.context().unwrap().clone();loading(m,c,vec![])
        },
        Event::Open if matches!(m.view,View::List(_)|View::Search(_)) && m.pending_open.is_none()=>{
            if let Some(item_id)=m.view.context().unwrap().selection {m.seq+=1;m.pending_open=Some(m.seq);let seq=m.seq;(m,vec![Effect::Open{seq,item_id}])}else{(m,vec![])}
        },
        Event::Opened{seq,result} if m.pending_open==Some(seq) && matches!(m.view,View::List(_)|View::Search(_))=>{
            m.pending_open=None;
            match result {
                Ok(()) if !m.sticky=>hide(m),
                Ok(())=>(m,vec![]),
                Err(error)=>{let c=m.view.context().unwrap().clone();loading(m,c,vec![Effect::Notice(error)])},
            }
        },
        _=>(m,vec![]),
    }
}

#[cfg(test)] mod tests {
    use super::*;
    fn context()->Context {Context{box_id:Some(7),space:Some("core".into()),query:String::new(),selection:Some(42)}}
    fn model(view:View)->Model {Model{view,seq:3,sticky:false,pending_open:None}}
    fn list()->Model {model(View::List(context()))}
    fn search()->Model {let mut c=context();c.query="자료".into();model(View::Search(c))}
    fn visible()->Vec<Model> {vec![model(View::Loading(context())),list(),search(),model(View::Empty{reason:"없음".into(),context:context()}),model(View::Failed{error:"오류".into(),context:context()})]}
    fn no_op(m:&Model,e:Event) {assert_eq!(step(m,e),(m.clone(),vec![]));}
    #[test] fn row01_hidden_call_loads() {for caller in [Caller::Hotkey,Caller::Tray,Caller::SecondRun] {let(m,e)=step(&Model::default(),Event::Call(caller));assert!(matches!(m.view,View::Loading(_)));assert_eq!(e,vec![Effect::Show,Effect::Read{seq:1,context:Context::default()}]);}}
    #[test] fn row02_loading_items() {let m=model(View::Loading(context()));let(n,e)=step(&m,Event::Loaded{seq:3,result:ReadResult::Items{box_id:7}});assert_eq!(n.view,View::List(context()));assert_eq!(e,vec![Effect::Filter,Effect::Draw]);}
    #[test] fn row03_loading_empty() {let(n,_)=step(&model(View::Loading(context())),Event::Loaded{seq:3,result:ReadResult::Empty("상자 없음".into())});assert!(matches!(n.view,View::Empty{reason,..} if reason=="상자 없음"));}
    #[test] fn row04_loading_failed() {let(n,_)=step(&model(View::Loading(context())),Event::Loaded{seq:3,result:ReadResult::Failed("잠김".into())});assert!(matches!(n.view,View::Failed{error,..} if error=="잠김"));}
    #[test] fn row05_list_query() {let(n,e)=step(&list(),Event::Query("한글".into()));assert!(matches!(n.view,View::Search(_)));assert_eq!(n.view.context().unwrap().query,"한글");assert_eq!(e,vec![Effect::Filter,Effect::Draw]);}
    #[test] fn row06_search_clear() {let(n,_)=step(&search(),Event::Query(String::new()));assert!(matches!(n.view,View::List(_)));assert!(n.view.context().unwrap().query.is_empty());}
    #[test] fn row06_search_escape() {let(n,e)=step(&search(),Event::Escape);assert!(matches!(n.view,View::List(_)));assert_eq!(e,vec![Effect::Filter,Effect::Draw]);}
    #[test] fn row07_box_change() {for m in [list(),search()] {let(n,e)=step(&m,Event::ChangeBox(8));let c=n.view.context().unwrap();assert_eq!(c.box_id,Some(8));assert!(c.query.is_empty());assert!(c.selection.is_none());assert_eq!(e,vec![Effect::Read{seq:4,context:c.clone()}]);}}
    #[test] fn row08_space_change_clears_query() {for m in [list(),search()] {let(n,e)=step(&m,Event::ChangeSpace(Some("basis".into())));assert!(matches!(n.view,View::List(_)));let c=n.view.context().unwrap();assert_eq!(c.space.as_deref(),Some("basis"));assert!(c.query.is_empty());assert_eq!(e,vec![Effect::Filter,Effect::Draw]);}}
    #[test] fn row09_open_requests_effect_first() {for m in [list(),search()] {let(n,e)=step(&m,Event::Open);assert_eq!(n.view,m.view);assert_eq!(e,vec![Effect::Open{seq:4,item_id:42}]);assert_eq!(n.pending_open,Some(4));}}
    #[test] fn row09_open_success_hides() {for m in [list(),search()] {let(p,_)=step(&m,Event::Open);let(n,e)=step(&p,Event::Opened{seq:4,result:Ok(())});assert_eq!(n.view,View::Hidden);assert_eq!(e,vec![Effect::Hide,Effect::ClearItems]);}}
    #[test] fn row09_sticky_success_stays() {let mut m=search();m.sticky=true;let(p,_)=step(&m,Event::Open);let(n,e)=step(&p,Event::Opened{seq:4,result:Ok(())});assert_eq!(n.view,m.view);assert!(e.is_empty());assert_eq!(n.pending_open,None);}
    #[test] fn row09_open_failure_notifies_and_reloads() {let m=search();let(p,_)=step(&m,Event::Open);let(n,e)=step(&p,Event::Opened{seq:4,result:Err("移動".into())});assert_eq!(n.view,View::Loading(context_with_query()));assert_eq!(e,vec![Effect::Notice("移動".into()),Effect::Read{seq:5,context:context_with_query()}]);}
    fn context_with_query()->Context {let mut c=context();c.query="資料".replace("資料","자료");c}
    #[test] fn row10_f5_loads_each_result_state() {for m in visible().into_iter().filter(|m|!matches!(m.view,View::Loading(_))) {let(n,e)=step(&m,Event::Refresh);assert!(matches!(n.view,View::Loading(_)));assert!(matches!(&e[..],[Effect::Read{seq:4,..}]));}}
    #[test] fn row11_close_hides_all_visible() {for m in visible() {let(n,e)=step(&m,Event::Close);assert_eq!(n.view,View::Hidden);assert_eq!(e,vec![Effect::Hide,Effect::ClearItems]);}}
    #[test] fn row11_escape_without_query_hides() {for m in visible().into_iter().filter(|m|!matches!(m.view,View::Search(_))) {assert_eq!(step(&m,Event::Escape).0.view,View::Hidden);}}
    #[test] fn row11_outside_hides_nonsticky() {for m in visible() {assert_eq!(step(&m,Event::Outside).0.view,View::Hidden);}}
    #[test] fn row11_sticky_ignores_outside() {for mut m in visible() {m.sticky=true;no_op(&m,Event::Outside);}}
    #[test] fn row12_minimize_retains_view_and_items() {for m in visible() {let(n,e)=step(&m,Event::Minimize);assert_eq!(n.view,View::Minimized(Box::new(m.view)));assert_eq!(e,vec![Effect::Hide]);}}
    #[test] fn row13_hotkey_toggles_sticky() {for mut m in visible() {m.sticky=true;let(n,e)=step(&m,Event::Call(Caller::Hotkey));assert_eq!(n.view,View::Hidden);assert_eq!(e,vec![Effect::Hide,Effect::ClearItems]);}}
    #[test] fn row14_tray_second_run_foreground() {for m in visible() {for sticky in [false,true] {for c in [Caller::Tray,Caller::SecondRun] {let mut m=m.clone();m.sticky=sticky;assert_eq!(step(&m,Event::Call(c)),(m,vec![Effect::Foreground]));}}}}
    #[test] fn nonsticky_hotkey_foregrounds() {for m in visible() {assert_eq!(step(&m,Event::Call(Caller::Hotkey)),(m,vec![Effect::Foreground]));}}
    #[test] fn row15_minimized_reload_restores_context() {for m in visible() {for c in [Caller::Hotkey,Caller::Tray,Caller::SecondRun] {let(min,_)=step(&m,Event::Minimize);let(n,e)=step(&min,Event::Call(c));assert_eq!(n.view,View::Loading(m.view.context().unwrap().clone()));assert_eq!(e[0],Effect::Show);assert!(matches!(e[1],Effect::Read{..}));}}}
    #[test] fn restore_search_after_reload() {let(min,_)=step(&search(),Event::Minimize);let(load,_)=step(&min,Event::Call(Caller::Hotkey));let(n,_)=step(&load,Event::Loaded{seq:load.seq,result:ReadResult::Items{box_id:7}});assert_eq!(n.view,search().view);}
    #[test] fn impossible_hidden_input() {let m=Model::default();for e in [Event::Open,Event::Query("x".into()),Event::ChangeSpace(None),Event::ChangeBox(1),Event::Select(Some(1)),Event::Escape,Event::Refresh,Event::Minimize] {no_op(&m,e);}}
    #[test] fn impossible_loading_open() {no_op(&model(View::Loading(context())),Event::Open);}
    #[test] fn impossible_empty_failed_open() {for m in visible().into_iter().filter(|m|matches!(m.view,View::Empty{..}|View::Failed{..})) {no_op(&m,Event::Open);}}
    #[test] fn impossible_minimized_keyboard() {let(min,_)=step(&search(),Event::Minimize);for e in [Event::Open,Event::Query("x".into()),Event::Escape,Event::Refresh,Event::Select(Some(5)),Event::ChangeBox(2),Event::ChangeSpace(None)] {no_op(&min,e);}}
    #[test] fn late_read_sequence_discarded() {let m=model(View::Loading(context()));for seq in [0,2,4] {no_op(&m,Event::Loaded{seq,result:ReadResult::Items{box_id:7}});}}
    #[test] fn late_read_after_close_discarded() {let(m,_)=step(&model(View::Loading(context())),Event::Close);no_op(&m,Event::Loaded{seq:3,result:ReadResult::Items{box_id:7}});}
    #[test] fn late_read_after_minimize_discarded() {let(m,_)=step(&model(View::Loading(context())),Event::Minimize);no_op(&m,Event::Loaded{seq:3,result:ReadResult::Items{box_id:7}});}
    #[test] fn unsolicited_open_result_ignored() {no_op(&list(),Event::Opened{seq:3,result:Ok(())});}
    #[test] fn stale_open_result_ignored() {let(p,_)=step(&list(),Event::Open);no_op(&p,Event::Opened{seq:3,result:Ok(())});let(n,_)=step(&p,Event::Select(Some(1)));no_op(&n,Event::Opened{seq:4,result:Ok(())});}
    #[test] fn no_selection_no_open() {let mut c=context();c.selection=None;no_op(&model(View::List(c)),Event::Open);}
    #[test] fn duplicate_open_ignored() {let(p,_)=step(&list(),Event::Open);no_op(&p,Event::Open);}
    #[test] fn invalid_box_space_ignored() {no_op(&list(),Event::ChangeBox(0));no_op(&search(),Event::ChangeSpace(Some("bad".into())));}
    #[test] fn names_are_status_values() {assert_eq!(Model::default().view.name(),"hidden");assert_eq!(visible().iter().map(|m|m.view.name()).collect::<Vec<_>>(),vec!["loading","list","search","empty","failed"]);assert_eq!(View::Minimized(Box::new(View::Hidden)).name(),"minimized");}
    #[test] fn sticky_setting_is_explicit() {let(n,e)=step(&list(),Event::SetSticky(true));assert!(n.sticky);assert!(e.is_empty());}
    #[test] fn events_on_non_loading_cannot_replace_view() {for m in [list(),search(),Model::default()] {no_op(&m,Event::Loaded{seq:m.seq,result:ReadResult::Failed("old".into())});}}
}
