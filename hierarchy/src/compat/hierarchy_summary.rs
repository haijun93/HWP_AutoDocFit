use serde_json::Value;
use super::display_role::display_role;
pub fn hierarchy_summary(a: &Value) -> String {
    let mut lines=vec!["들여쓰기 → 글꼴 → 크기 → 항목기호 순으로 판정; 문단 위 여백은 보조값".to_string(),"논리적 구조: 1단계 제목 > 2단계 장 > 3단계 중제목(Ⅰ) > 4단계 소제목(□) > 5단계(ㅇ) > 6단계(-); 부연설명은 관련 항목과 같은 쪽".to_string(),"원문자(① 등)는 서식에 따라 5단계 ㅇ, 6단계 -, 부연설명 기호를 대신할 수 있음".to_string()];
    if let Some(kind)=a["document_type"].as_str(){let d=match kind{"A형"=>"1~6단계 모두 사용(장 포함)","B형"=>"장 없이 1·3~6단계 사용(중제목부터)","C형"=>"장·중제목 없이 1·4~6단계 사용(소제목부터)","기타"=>"보도자료·답변자료 등 그 밖의 서식",_=>""};lines.push(format!("문서 유형: {kind} — {d}"));}
    if let Some(styles)=a["styles"].as_array(){for item in styles{let role=display_role(item["role"].as_str().unwrap_or(""));let marker=item["marker"].as_str().unwrap_or("");let count=item["count"].as_u64().unwrap_or(0);let indent=item["indent_hwpunit"].as_f64().unwrap_or(0.0)/100.0;let font=item["font"].as_str().filter(|s|!s.is_empty()).unwrap_or("미상");let size=item["size_pt"].as_f64().unwrap_or(0.0);let spaces=item["prev_spacing_values"].as_array().map(|v|v.iter().filter_map(Value::as_f64).map(|n|format!("{}pt",n/100.0)).collect::<Vec<_>>().join(", ")).filter(|s|!s.is_empty()).unwrap_or("없음".into());lines.push(format!("{role} [{marker}] {count}개 | 들여쓰기 {}pt | {font} {}pt | 문단 위 여백 {spaces}",trim_float(indent),trim_float(size)));}}
    if let Some(w)=a["warnings"].as_array(){let values:Vec<&str>=w.iter().take(10).filter_map(Value::as_str).collect();if !values.is_empty(){lines.push(format!("구조 확인 필요: {}",values.join("; ")));}}
    lines.join("\n")
}
fn trim_float(n:f64)->String{let mut s=format!("{n}");if s.contains('.') {while s.ends_with('0'){s.pop();}if s.ends_with('.'){s.pop();}}s}
