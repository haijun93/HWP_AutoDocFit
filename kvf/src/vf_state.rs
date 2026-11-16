//! 창 상태 전이. Win32·DB·파일 접근 없이 다음 상태와 할 일만 반환한다.
use crate::vf_layout::Layout;
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
pub struct Model { pub view:View,pub seq:u64,pub sticky:bool,pub pending_open:Option<u64>,pub explorer:Layout,pub pending_operation:Option<PendingOperation> }
#[derive(Clone,Debug,PartialEq,Eq)] pub struct PendingOperation {pub seq:u64,pub operation_id:String,pub kind:VirtualOperationKind}
#[derive(Clone,Debug,PartialEq,Eq)] pub enum VirtualOperationKind {Move{source_panel:String,target_panel:String,item_ids:Vec<String>},Copy{source_panel:String,target_panel:String,item_ids:Vec<String>}}
impl Default for Model {fn default()->Self {Self{view:View::Hidden,seq:0,sticky:false,pending_open:None,explorer:Layout::default(),pending_operation:None}}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum Caller {Hotkey,Tray,SecondRun}
#[derive(Clone,Debug,PartialEq,Eq)] pub enum ReadResult { Items{box_id:i64}, Empty(String), EmptyBox{box_id:i64,reason:String}, Failed(String) }
#[derive(Clone,Debug,PartialEq,Eq)] pub enum Event {
    Call(Caller), Loaded{seq:u64,result:ReadResult}, Query(String), Escape, Close, Outside,
    Minimize, ChangeBox(i64), ChangeSpace(Option<String>), Select(Option<i64>), Refresh,
    LocateCopy, LocateOrigin, Open, Opened{seq:u64,result:Result<(),String>}, SetSticky(bool),
    SetPanelCount(u8), ActivatePanel(String), SelectPanel{panel_id:String,selection:Vec<String>},
    RequestVirtualMove{operation_id:String,source_panel:String,target_panel:String,item_ids:Vec<String>},
    RequestVirtualCopy{operation_id:String,source_panel:String,target_panel:String,item_ids:Vec<String>},
    VirtualOperationCompleted{seq:u64,result:Result<(),String>},
}
#[derive(Clone,Debug,PartialEq,Eq)] pub enum Effect {
    Show, Foreground, Hide, ClearItems, Read{seq:u64,context:Context}, Draw, Filter,
    Locate{seq:u64,item_id:i64,origin:bool}, Open{seq:u64,item_id:i64}, Notice(String),
    PersistLayout(Layout), PersistVirtualOperation{seq:u64,operation_id:String,kind:VirtualOperationKind},
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
        Event::SetPanelCount(count) if (1..=4).contains(&count)=>{let context=m.view.context().cloned();m.explorer.set_count(count);if let Some(c)=context{for p in &mut m.explorer.panels{if p.box_id.is_none(){p.box_id=c.box_id;}if p.space.is_none(){p.space=c.space.clone();}}}let layout=m.explorer.clone();(m,vec![Effect::PersistLayout(layout),Effect::Draw])},
        Event::ActivatePanel(id) if m.explorer.panels.iter().any(|p|p.id==id)=>{m.explorer.active_panel=id;let layout=m.explorer.clone();(m,vec![Effect::PersistLayout(layout),Effect::Draw])},
        Event::SelectPanel{panel_id,selection} if m.explorer.panels.iter().any(|p|p.id==panel_id)=>{m.explorer.panels.iter_mut().find(|p|p.id==panel_id).unwrap().selection=selection;(m,vec![Effect::Draw])},
        Event::RequestVirtualMove{operation_id,source_panel,target_panel,item_ids} if m.pending_operation.is_none()&&!item_ids.is_empty()&&source_panel!=target_panel&&m.explorer.panels.iter().any(|p|p.id==source_panel)&&m.explorer.panels.iter().any(|p|p.id==target_panel)=>{
            m.seq+=1;let seq=m.seq;let kind=VirtualOperationKind::Move{source_panel,target_panel,item_ids};m.pending_operation=Some(PendingOperation{seq,operation_id:operation_id.clone(),kind:kind.clone()});(m,vec![Effect::PersistVirtualOperation{seq,operation_id,kind}])
        },
        Event::RequestVirtualCopy{operation_id,source_panel,target_panel,item_ids} if m.pending_operation.is_none()&&!item_ids.is_empty()&&source_panel!=target_panel&&m.explorer.panels.iter().any(|p|p.id==source_panel)&&m.explorer.panels.iter().any(|p|p.id==target_panel)=>{
            m.seq+=1;let seq=m.seq;let kind=VirtualOperationKind::Copy{source_panel,target_panel,item_ids};m.pending_operation=Some(PendingOperation{seq,operation_id:operation_id.clone(),kind:kind.clone()});(m,vec![Effect::PersistVirtualOperation{seq,operation_id,kind}])
        },
        Event::VirtualOperationCompleted{seq,result} if m.pending_operation.as_ref().is_some_and(|p|p.seq==seq)=>{m.pending_operation=None;let effects=match result{Ok(())=>vec![Effect::Draw],Err(e)=>vec![Effect::Notice(e),Effect::Draw]};(m,effects)},
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
                ReadResult::EmptyBox{box_id,reason}=>{c.box_id=Some(box_id);View::Empty{reason,context:c}},
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
        Event::ChangeBox(id) if id>0 && matches!(m.view,View::List(_)|View::Search(_)|View::Empty{..})=>{
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
        Event::LocateCopy|Event::LocateOrigin if matches!(m.view,View::List(_)|View::Search(_)) && m.pending_open.is_none()=>{
            if let Some(item_id)=m.view.context().unwrap().selection {m.seq+=1;m.pending_open=Some(m.seq);let seq=m.seq;(m,vec![Effect::Locate{seq,item_id,origin:matches!(event,Event::LocateOrigin)}])}else{(m,vec![])}
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
    fn model(view:View)->Model {Model{view,seq:3,sticky:false,pending_open:None,..Model::default()}}
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

#[cfg(test)]mod window_extension_tests{
 use super::*;
 #[test]fn empty_box_retains_id_and_can_switch(){let(load,_)=step(&Model::default(),Event::Call(Caller::Tray));let(empty,_)=step(&load,Event::Loaded{seq:load.seq,result:ReadResult::EmptyBox{box_id:2,reason:"빈 상자".into()}});assert_eq!(empty.view.context().unwrap().box_id,Some(2));assert_eq!(step(&empty,Event::Open).0,empty);let(next,e)=step(&empty,Event::ChangeBox(1));assert!(matches!(next.view,View::Loading(_)));assert!(matches!(e[0],Effect::Read{..}));}
}

#[cfg(test)]mod explorer_state_tests{
 use super::*;
 #[test]fn panel_focus_and_selection_are_independent(){let mut m=Model::default();m=step(&m,Event::SetPanelCount(3)).0;assert_eq!(m.explorer.template,crate::vf_layout::Template::LeftOneRightTwo);m=step(&m,Event::SelectPanel{panel_id:"p1".into(),selection:vec!["a".into()]}).0;m=step(&m,Event::SelectPanel{panel_id:"p2".into(),selection:vec!["b".into()]}).0;m=step(&m,Event::ActivatePanel("p2".into())).0;assert_eq!(m.explorer.active_panel,"p2");assert_eq!(m.explorer.panels[0].selection,vec!["a"]);assert_eq!(m.explorer.panels[1].selection,vec!["b"]);}
 #[test]fn panel_layout_starts_at_current_box_and_activation_persists(){let c=Context{box_id:Some(7),space:Some("basis".into()),..Default::default()};let m=Model{view:View::List(c),..Default::default()};let(m,_)=step(&m,Event::SetPanelCount(2));assert_eq!(m.explorer.panels[1].box_id,Some(7));assert_eq!(m.explorer.panels[1].space.as_deref(),Some("basis"));let(m,e)=step(&m,Event::ActivatePanel("p2".into()));assert_eq!(m.explorer.active_panel,"p2");assert!(matches!(&e[..],[Effect::PersistLayout(_),Effect::Draw]));}
 #[test]fn panel_count_bounds_and_persist_effect(){for n in 1..=4{let(m,e)=step(&Model::default(),Event::SetPanelCount(n));assert_eq!(m.explorer.panel_count,n);assert!(matches!(&e[0],Effect::PersistLayout(_)));}let(m,e)=step(&Model::default(),Event::SetPanelCount(5));assert_eq!(m,Model::default());assert!(e.is_empty());}
 #[test]fn virtual_move_and_copy_are_async_and_stale_safe(){let mut base=Model::default();base.explorer.set_count(2);for event in [Event::RequestVirtualMove{operation_id:"m".into(),source_panel:"p1".into(),target_panel:"p2".into(),item_ids:vec!["x".into()]},Event::RequestVirtualCopy{operation_id:"c".into(),source_panel:"p1".into(),target_panel:"p2".into(),item_ids:vec!["x".into()]}]{let(p,e)=step(&base,event);assert!(matches!(e[0],Effect::PersistVirtualOperation{..}));let seq=p.pending_operation.as_ref().unwrap().seq;let(n,e)=step(&p,Event::VirtualOperationCompleted{seq:seq+1,result:Ok(())});assert_eq!(n,p);assert!(e.is_empty());let(n,e)=step(&p,Event::VirtualOperationCompleted{seq,result:Ok(())});assert!(n.pending_operation.is_none());assert_eq!(e,vec![Effect::Draw]);}}
 #[test]fn invalid_virtual_operation_does_not_start(){let mut m=Model::default();m.explorer.set_count(2);let bad=Event::RequestVirtualMove{operation_id:"x".into(),source_panel:"p1".into(),target_panel:"p9".into(),item_ids:vec!["a".into()]};assert_eq!(step(&m,bad),(m,vec![]));}
}

#[cfg(test)]mod locate_state_tests{
 use super::*;
 #[test]fn locations_require_selection_and_complete_through_step(){let c=Context{box_id:Some(1),selection:Some(5),..Default::default()};let m=Model{view:View::List(c),..Default::default()};for(event,origin)in[(Event::LocateCopy,false),(Event::LocateOrigin,true)]{let(n,e)=step(&m,event);assert_eq!(e,vec![Effect::Locate{seq:1,item_id:5,origin}]);assert_eq!(step(&n,Event::LocateCopy).0,n);assert_eq!(step(&n,Event::Opened{seq:1,result:Ok(())}).0.view,View::Hidden);}}
 #[test]fn impossible_locations_in_unusable_views(){for view in [View::Hidden,View::Loading(Context::default()),View::Empty{reason:String::new(),context:Context::default()},View::Failed{error:String::new(),context:Context::default()},View::Minimized(Box::new(View::Hidden))]{let m=Model{view,..Default::default()};for e in[Event::LocateCopy,Event::LocateOrigin]{assert_eq!(step(&m,e),(m.clone(),vec![]));}}}
}
