//! 파이썬 설정값 전달(RSXML2). CLI `--config 파일.json` 으로 받는다. 형식:
//! ```json
//! { "page_number": {"pos": "...", "formatType": "...", "sideChar": "..."},   // 표준서식_설정["쪽번호"] (문자열 값만)
//!   "title_owner_text": "",            // 제목_담당자_글
//!   "indent_rules": {},                // 표준서식_설정["내어쓰기_규칙"] (자간 초기화 보호 구간에 영향)
//!   "marker_scheme_active": false }    // 활성_항목기호_체계 가 비어 있지 않은가
//! ```
//! RSXML3 추가 키: "std_symbols"(표준서식_기호_사용, 기본 true), "symbol_rules"(표준서식_설정["기호_규칙"], [[기호,?,글꼴,크기],…]),
//! "colon_label_enabled"(알파_콜론라벨_정렬_사용, 기본 true), "ratio_reduce"(장평_줄이기_사용, 기본 false),
//! "blank_line_profile"(표준서식_설정["빈줄_서식"], 파이썬 blank_lines.learn 이 만든 사전), "emphasis_words"(표준서식_설정["강조_낱말"] 목록).
//! RSXML4 추가 키: "table_style"(활성_표서식_프로필: 파이썬 learn_from_table/default_style 이 만든 사전), "connector_tab_enabled"(알파_연결부호_탭정렬_사용, 기본 true),
//! "standard_margin_planned"(표준서식이 편집 여백을 바꿀 예정이라 `_최종_본문폭` 이 쪽 정보와 달라지는가, 기본 false; true 면 연결 부호 탭은 거부).
//! 알 수 없는 키, 반영하지 못하는 값은 조용히 무시하지 않고 `미지원 입력:` 오류로 거부한다(종료 코드 4).
use crate::rules::R;

pub const UNSUPPORTED: &str = "미지원 입력:";

pub fn unsupported<T>(why: &str) -> R<T> {
    Err(format!("{UNSUPPORTED} {why}"))
}

#[derive(Debug, Clone)]
pub struct Config {
    pub page_number: Vec<(String, String)>,
    pub title_owner_text: String,
    pub indent_rules_nonempty: bool,
    pub marker_scheme_active: bool,
    pub std_symbols: bool,
    pub symbol_rules: Vec<(String, Option<String>, Option<f64>)>,
    pub colon_label_enabled: bool,
    pub ratio_reduce: bool,
    pub blank_line_profile: Option<serde_json::Value>,
    pub emphasis_words: Vec<serde_json::Value>,
    pub table_style: Option<serde_json::Value>,
    pub connector_tab_enabled: bool,
    pub standard_margin_planned: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            page_number: vec![],
            title_owner_text: String::new(),
            indent_rules_nonempty: false,
            marker_scheme_active: false,
            std_symbols: true,
            symbol_rules: vec![],
            colon_label_enabled: true,
            ratio_reduce: false,
            blank_line_profile: None,
            emphasis_words: vec![],
            table_style: None,
            connector_tab_enabled: true,
            standard_margin_planned: false,
        }
    }
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
                "std_symbols" => c.std_symbols = val.as_bool().ok_or_else(|| format!("{UNSUPPORTED} std_symbols 는 true/false 여야 합니다."))?,
                "colon_label_enabled" => {
                    c.colon_label_enabled = val.as_bool().ok_or_else(|| format!("{UNSUPPORTED} colon_label_enabled 는 true/false 여야 합니다."))?
                }
                "connector_tab_enabled" => {
                    c.connector_tab_enabled = val.as_bool().ok_or_else(|| format!("{UNSUPPORTED} connector_tab_enabled 는 true/false 여야 합니다."))?
                }
                "standard_margin_planned" => {
                    c.standard_margin_planned = val.as_bool().ok_or_else(|| format!("{UNSUPPORTED} standard_margin_planned 는 true/false 여야 합니다."))?
                }
                "table_style" => {
                    c.table_style = match val {
                        serde_json::Value::Null => None,
                        serde_json::Value::Object(_) => Some(val.clone()),
                        _ => return unsupported("table_style 은 객체여야 합니다."),
                    }
                }
                "ratio_reduce" => c.ratio_reduce = val.as_bool().ok_or_else(|| format!("{UNSUPPORTED} ratio_reduce 는 true/false 여야 합니다."))?,
                "symbol_rules" => {
                    let arr = match val {
                        serde_json::Value::Null => vec![],
                        serde_json::Value::Array(a) => a.clone(),
                        _ => return unsupported("symbol_rules 는 배열이어야 합니다."),
                    };
                    for r in arr {
                        // 파이썬은 rule[0]=='ㅇ' 인 첫 규칙의 rule[2](글꼴)·float(rule[3])(크기)만 읽는다.
                        let a = r.as_array().ok_or_else(|| format!("{UNSUPPORTED} symbol_rules 항목은 배열이어야 합니다."))?;
                        let m = match a.first().and_then(|x| x.as_str()) {
                            Some(m) => m.to_string(),
                            None => return unsupported("symbol_rules 항목의 첫 값(기호)이 문자열이 아닙니다."),
                        };
                        if m != "ㅇ" {
                            c.symbol_rules.push((m, None, None));
                            continue;
                        }
                        let face = match a.get(2) {
                            Some(serde_json::Value::String(s)) => s.clone(),
                            _ => return unsupported("symbol_rules 'ㅇ' 항목의 글꼴(셋째 값)이 문자열이 아닙니다."),
                        };
                        let size = match a.get(3) {
                            Some(serde_json::Value::Number(n)) => n.as_f64(),
                            _ => None,
                        };
                        match size {
                            Some(f) => c.symbol_rules.push((m, Some(face), Some(f))),
                            None => return unsupported("symbol_rules 'ㅇ' 항목의 크기(넷째 값)가 숫자가 아닙니다."),
                        }
                    }
                }
                "blank_line_profile" => {
                    c.blank_line_profile = match val {
                        serde_json::Value::Null => None,
                        serde_json::Value::Object(_) => Some(val.clone()),
                        _ => return unsupported("blank_line_profile 은 객체여야 합니다."),
                    }
                }
                "emphasis_words" => {
                    c.emphasis_words = match val {
                        serde_json::Value::Null => vec![],
                        serde_json::Value::Array(a) => {
                            if a.iter().any(|x| !x.is_object()) {
                                return unsupported("emphasis_words 의 항목은 객체여야 합니다.");
                            }
                            a.clone()
                        }
                        _ => return unsupported("emphasis_words 는 배열이어야 합니다."),
                    }
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
