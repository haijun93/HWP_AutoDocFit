//! 파이썬 설정값 전달(RSXML2). CLI `--config 파일.json` 으로 받는다. 형식:
//! ```json
//! { "page_number": {"pos": "...", "formatType": "...", "sideChar": "..."},   // 표준서식_설정["쪽번호"] (문자열 값만)
//!   "title_owner_text": "",            // 제목_담당자_글
//!   "indent_rules": {},                // 표준서식_설정["내어쓰기_규칙"] (자간 초기화 보호 구간에 영향)
//!   "marker_scheme_active": false }    // 활성_항목기호_체계 가 비어 있지 않은가
//! ```
//! 알 수 없는 키, 반영하지 못하는 값은 조용히 무시하지 않고 `미지원 입력:` 오류로 거부한다(종료 코드 4).
use crate::rules::R;

pub const UNSUPPORTED: &str = "미지원 입력:";

pub fn unsupported<T>(why: &str) -> R<T> {
    Err(format!("{UNSUPPORTED} {why}"))
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub page_number: Vec<(String, String)>,
    pub title_owner_text: String,
    pub indent_rules_nonempty: bool,
    pub marker_scheme_active: bool,
}

impl Config {
    pub fn from_json(v: &serde_json::Value) -> R<Config> {
        let obj = v.as_object().ok_or_else(|| format!("{UNSUPPORTED} 설정은 JSON 객체여야 합니다."))?;
        let mut c = Config::default();
        for (k, val) in obj {
            match k.as_str() {
                "page_number" => {
                    if val.is_null() {
                        continue;
                    }
                    let m = val.as_object().ok_or_else(|| format!("{UNSUPPORTED} page_number 는 객체여야 합니다."))?;
                    for (a, x) in m {
                        match x.as_str() {
                            Some(s) => c.page_number.push((a.clone(), s.to_string())),
                            // 파이썬은 숫자 등을 속성에 넣다가 직렬화에서 실패한다. 같은 결과를 흉내 내지 않고 거부한다.
                            None => return unsupported(&format!("page_number.{a} 값이 문자열이 아닙니다.")),
                        }
                    }
                }
                "title_owner_text" => {
                    c.title_owner_text = match val {
                        serde_json::Value::Null => String::new(),
                        serde_json::Value::String(s) => s.clone(),
                        _ => return unsupported("title_owner_text 는 문자열이어야 합니다."),
                    }
                }
                "indent_rules" => {
                    c.indent_rules_nonempty = match val {
                        serde_json::Value::Null => false,
                        serde_json::Value::Object(m) => !m.is_empty(),
                        _ => return unsupported("indent_rules 는 객체여야 합니다."),
                    }
                }
                "marker_scheme_active" => {
                    c.marker_scheme_active =
                        val.as_bool().ok_or_else(|| format!("{UNSUPPORTED} marker_scheme_active 는 true/false 여야 합니다."))?
                }
                other => return unsupported(&format!("알 수 없는 설정 키 {other:?}")),
            }
        }
        Ok(c)
    }

    pub fn from_path(path: &std::path::Path) -> R<Config> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("설정 파일을 읽을 수 없음: {e}"))?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{UNSUPPORTED} 설정 JSON 오류: {e}"))?;
        Config::from_json(&v)
    }
}
