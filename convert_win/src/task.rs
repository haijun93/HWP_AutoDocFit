//! 실제 작업 결과를 목록에 반영한다. 결과 경로를 만들어 추측하지 않는다.
use crate::logic::{FileResult,FileStatus};
use serde_json::Value;
pub fn finish(items:&mut [FileResult],response:&Value) {
    if response["state"]=="failed" {
        let reason=response["error"]["message"].as_str().unwrap_or("변환 작업 실패");
        for item in items{item.status=FileStatus::Failed(reason.into());} return;
    }
    let rows=response["result"].as_array();
    let merged=rows.into_iter().flatten().find(|r|r["source"].is_null()&&r["status"]=="success");
    for item in items {
        let source=item.source.to_string_lossy();
        let row=rows.into_iter().flatten().find(|r|r["source"].as_str()==Some(source.as_ref())).or(merged);
        item.status=match row {
            Some(r) if r["status"]=="success" =>match r["output"].as_str(){Some(p) if !p.is_empty()=>FileStatus::Succeeded(p.into()),_=>FileStatus::Failed("결과 경로가 없습니다.".into())},
            Some(r) if r["status"]=="skipped"=>FileStatus::Skipped(r["error"].as_str().unwrap_or("같은 형식").into()),
            Some(r)=>FileStatus::Failed(r["error"].as_str().unwrap_or("변환 실패").into()),
            None if response["state"]=="cancelled"=>FileStatus::Cancelled,
            None=>FileStatus::Failed("작업 결과에 파일이 없습니다.".into()),
        };
    }
}
#[cfg(test)] mod tests{
    use super::*;use serde_json::json;
    #[test]fn results_use_exact_paths_and_reject_missing_success(){
        let mut items=crate::logic::results(&["a.hwpx".into(),"b.hwpx".into()]);
        finish(&mut items,&json!({"state":"done","result":[{"source":"a.hwpx","status":"success","output":"a(PDF변경) (9).pdf"},{"source":"b.hwpx","status":"success"}]}));
        assert_eq!(items[0].status,FileStatus::Succeeded("a(PDF변경) (9).pdf".into()));assert!(matches!(items[1].status,FileStatus::Failed(_)));
    }
    #[test]fn partial_cancellation_keeps_completed_outputs(){
        let mut items=crate::logic::results(&["a.hwpx".into(),"b.hwpx".into()]);
        finish(&mut items,&json!({"state":"cancelled","result":[{"source":"a.hwpx","status":"success","output":"saved.pdf"}]}));
        assert!(matches!(items[0].status,FileStatus::Succeeded(_)));assert_eq!(items[1].status,FileStatus::Cancelled);
    }
    #[test]fn all_formats_have_nonoverlapping_click_areas(){
        for (i,(_,kind)) in crate::logic::ALL_TARGETS.iter().enumerate(){
            let mut m=crate::logic::Model::default();m.select_output(i);assert_eq!(m.format(),*kind);
            let r=crate::logic::format_rect(i);for j in i+1..12{let other=crate::logic::format_rect(j);assert!(!r.contains(other.left,other.top));}
        }
    }
}
