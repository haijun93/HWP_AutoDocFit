//! 키 → 컨트롤 매핑(표시 모델의 항목 표). 키 이름·종류·기본값·허용 범위는 `docfit_config` 의 스키마를 그대로 쓰고,
//! 이 파일은 (1) 화면 이름·안내문·묶음, (2) 이번 단계에서 편집하지 않는 키(미구현·소유자 둘 이상·내부)만 보탠다.
use crate::guides::GUIDES;
use crate::labels::LABELS;
use serde_json::Value;

/// 좌측 트리의 설정 묶음(Flutter `guideCategories` 10개 + 화면 밖 키를 모은 `internal`).
pub struct GroupDef {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
}

pub const GROUPS: [GroupDef; 11] = [
    GroupDef { id: "report", title: "보고서 모양", subtitle: "제목·항목기호·여백" },
    GroupDef { id: "paragraph", title: "문단 간격", subtitle: "항목 사이 띄우기" },
    GroupDef { id: "sentence", title: "문장·줄 정리", subtitle: "줄 끝 단어·자간" },
    GroupDef { id: "table", title: "표", subtitle: "머리글·표 글꼴" },
    GroupDef { id: "font", title: "항목기호 글꼴", subtitle: "□ ㅇ - ※ 글꼴·크기" },
    GroupDef { id: "cards", title: "작업 카드 기본값", subtitle: "처음 켜 둘 옵션" },
    GroupDef { id: "result", title: "결과·기록", subtitle: "저장·검수·보고서" },
    GroupDef { id: "general", title: "일반", subtitle: "탐색기 메뉴" },
    GroupDef { id: "program", title: "프로그램·네트워크", subtitle: "창·업데이트" },
    GroupDef { id: "dev", title: "개발자 도구", subtitle: "도형 편집기" },
    GroupDef { id: "internal", title: "내부 값(화면 밖)", subtitle: "편집하지 않는 내부 키" },
];

/// 편집하지 않는 키의 사유.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadOnly {
    /// 이번 단계에서 컨트롤을 만들지 않은 종류(괄호 안: 필요한 컨트롤).
    Unimplemented(&'static str),
    /// 소유자가 둘 이상이어서 임의로 쓰지 않는 키(설계 문서 ⑤-1 기호·설명).
    MultiOwner(&'static str),
    /// 화면 밖 내부 키(`save_settings` 불가).
    Internal(&'static str),
}

impl ReadOnly {
    /// 줄 끝 꼬리표 글자.
    pub fn chip(&self) -> &'static str {
        match self {
            ReadOnly::Unimplemented(_) => "미구현",
            ReadOnly::MultiOwner(_) => "관리 창",
            ReadOnly::Internal(_) => "내부",
        }
    }

    /// 안내문 아래에 붙는 설명.
    pub fn note(&self) -> &'static str {
        match self {
            ReadOnly::Unimplemented(s) | ReadOnly::MultiOwner(s) | ReadOnly::Internal(s) => s,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Check,
    Number { integer: bool, positive: bool },
    Text,
    /// (저장 값, 화면 이름) 목록.
    Choice(Vec<(String, String)>),
    /// 색 견본 9개 중 하나(`controls::COLORS`).
    Color,
    /// 글꼴 이름: 직접 입력 + 설치된 글꼴 목록에서 고르기.
    Font,
    /// 폴더 경로: 직접 입력 + [찾아보기] 대화상자.
    Folder,
    /// 문자열 목록(추가·삭제·순서). 값은 JSON 배열.
    List { max: usize },
    /// 단축키(키를 눌러 입력).
    Hotkey,
    ReadOnly(ReadOnly),
}

impl Control {
    pub fn is_editable(&self) -> bool {
        !matches!(self, Control::ReadOnly(_))
    }

    /// 값이 입력 글(문자열)로 저장되는 종류.
    pub fn is_text_like(&self) -> bool {
        matches!(self, Control::Number { .. } | Control::Text | Control::Font | Control::Folder | Control::Hotkey)
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    /// 점으로 이은 잎 키(예: `symbol_fonts.□.size`).
    pub key: String,
    pub label: String,
    pub summary: String,
    /// 검색 대상 글(이름 + 안내 제목 + 요약, 소문자).
    pub haystack: String,
    pub group: usize,
    pub control: Control,
    pub default: Value,
}

const INTEGER_KEYS: [&str; 5] = ["retry_body", "retry_table", "punctuation_threshold", "linespacing_min", "linespacing_max"];

fn guide_of(key: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static str)> {
    GUIDES.iter().find(|g| g.0 == key).or_else(|| GUIDES.iter().find(|g| g.0 == key.split('.').next().unwrap_or("")))
}

pub fn group_index(id: &str) -> usize {
    GROUPS.iter().position(|g| g.id == id).unwrap_or(8)
}

/// 항목 이름: Python `fields()` 와 같은 규칙(`LABELS[루트] · 머리글 · 글꼴`…).
pub fn label_of(key: &str) -> String {
    let mut parts = key.split('.');
    let root = parts.next().unwrap_or("");
    let mut label = LABELS.iter().find(|l| l.0 == root).map(|l| l.1.to_string()).unwrap_or_else(|| root.to_string());
    for part in parts {
        label.push_str(" · ");
        label.push_str(match part {
            "header" => "머리글",
            "body" => "본문",
            "font" => "글꼴",
            "size" => "크기 (pt)",
            other => other,
        });
    }
    label
}

/// Flutter `_categoryOf` 와 같은 규칙: 안내문의 분류가 우선, 없으면 그룹 규칙.
fn group_of(key: &str) -> usize {
    if let Some(g) = guide_of(key) {
        return group_index(g.1);
    }
    let root = key.split('.').next().unwrap_or("");
    let id = match root {
        "symbol_fonts" | "table_fonts" => "font",
        "developer_mode" | "freeze_tracing" => "dev",
        r if r.starts_with("std_") || r == "label_symbols" || r == "title_owner_text" => "report",
        _ => "program",
    };
    group_index(id)
}

/// 소유자가 둘 이상인 키(설계 문서 ⑤-1 표) 중 앱 `settings.json` 의 화면 노출 키.
fn multi_owner(key: &str) -> Option<&'static str> {
    match key {
        "mongttang_search" => Some("위쪽 [관리…] → [창·검색]에서 검색 프로그램과 설정을 함께 바꿉니다."),
        "always_on_top" => Some("위쪽 [관리…] → [창·검색]에서 열려 있는 Rust 창의 표시 상태와 함께 바꿉니다."),
        _ => None,
    }
}

/// 키 이름으로 정해지는 복잡한 컨트롤(스키마 종류는 모두 text).
fn special_control(key: &str) -> Option<Control> {
    match key {
        "color" => Some(Control::Color),
        "hwp_font_folder" | "paste_add_folder" => Some(Control::Folder),
        k if k.ends_with(".font") => Some(Control::Font),
        _ => None,
    }
}

fn internal_note(key: &str) -> &'static str {
    match key {
        "theme_mode" => "소유자 4(설계 5-A): 전용 명령으로만 변경",
        "menu_display_mode" => "전용 명령(set_menu_display_mode)으로만 변경",
        "kclip_settings_mode" | "outliner_ui" | "settings_ui" => "화면 방식 전환 키: 해당 화면의 전용 명령으로만 변경",
        k if k.ends_with("_rev") => "마이그레이션이 쓰는 내부 번호",
        _ => "내부·전용 경로 키(save_settings 불가)",
    }
}

fn make_item(key: &str, kind: &str, default: &Value) -> Item {
    let label = label_of(key);
    let guide = guide_of(key);
    let summary = guide.map(|g| g.3.to_string()).unwrap_or_default();
    let title = guide.map(|g| g.2).unwrap_or("");
    let haystack = format!("{label} {title} {summary}").to_lowercase();
    let control = if let Some(note) = multi_owner(key) {
        Control::ReadOnly(ReadOnly::MultiOwner(note))
    } else if let Some(c) = special_control(key) {
        c
    } else {
        match kind {
            "boolean" => Control::Check,
            "list" => Control::List { max: if key == "hidden_tabs" { 13 } else { docfit_config::LIST_MAX_ITEMS } },
            "number" => Control::Number {
                integer: INTEGER_KEYS.contains(&key),
                positive: key.ends_with(".size") || key == "std_title_subtitle_pt",
            },
            "choice" => Control::Choice(vec![
                ("kfind".into(), "새 검색 엔진 (기본)".into()),
                ("legacy".into(), "기존 검색 엔진".into()),
            ]),
            _ => Control::Text,
        }
    };
    Item { key: key.into(), label, summary, haystack, group: group_of(key), control, default: default.clone() }
}

/// 항목 전체: 화면 노출 잎(`docfit_config::visible_schema`) + 화면 밖 최상위 키.
pub fn build_items() -> Vec<Item> {
    let mut items: Vec<Item> = docfit_config::visible_schema().iter().map(|(k, kind, d)| make_item(k, kind, d)).collect();
    let roots: std::collections::HashSet<String> =
        items.iter().map(|i| i.key.split('.').next().unwrap_or("").to_string()).collect();
    let defaults = docfit_config::defaults();
    if let Some(map) = defaults.as_object() {
        for (key, value) in map {
            if roots.contains(key) {
                continue;
            }
            let label = LABELS.iter().find(|l| l.0 == key).map(|l| l.1.to_string()).unwrap_or_else(|| key.clone());
            let note = internal_note(key);
            let haystack = format!("{key} {label} {note}").to_lowercase();
            items.push(Item {
                key: key.clone(),
                label: key.clone(),
                summary: String::new(),
                haystack,
                group: group_index("internal"),
                control: Control::ReadOnly(ReadOnly::Internal(note)),
                default: value.clone(),
            });
        }
    }
    items
}

/// 시연용 항목(설정 키가 아님): 목록 컨트롤은 이제 실제 키(`hidden_tabs`·`library_excluded_folders`)가 쓰지만 단축키 키는 앱 설정에 없어, 두 컨트롤을 실제 그리기·동작 코드로 보이기 위해
/// 캡처 모드(`--demo-controls`)와 시험에서만 모델에 덧붙인다. 실제 창에는 나타나지 않는다.
pub fn demo_items() -> Vec<Item> {
    let mk = |key: &str, label: &str, summary: &str, control: Control, default: Value| Item {
        key: key.into(),
        label: label.into(),
        summary: summary.into(),
        haystack: format!("{label} {summary}").to_lowercase(),
        group: group_index("program"),
        control,
        default,
    };
    vec![
        mk("demo.excluded_apps", "시연: 제외할 프로그램 목록", "목록 컨트롤 시연(설정 키 아님). [편집…]으로 추가·삭제·순서를 바꿉니다.", Control::List { max: 100 }, Value::Array(vec![])),
        mk("demo.hotkey", "시연: 단축키 입력", "단축키 컨트롤 시연(설정 키 아님). 칸을 눌러 키를 누르면 입력됩니다.", Control::Hotkey, Value::String("Ctrl+Alt+Space".into())),
    ]
}

/// 메뉴 번호 → 이름(Flutter `tab_navigation_shell.dart` 의 `_labels` 순서, 0 = 문서편집).
pub const MENU_NAMES: [&str; 16] = [
    "문서편집", "표템플릿", "표그림판", "다이어그램", "마인드맵", "붙임사진 편집", "파일명 일괄변경", "몽땅검색",
    "문서뷰", "사진뷰", "파일변환", "PDF 편집", "설정", "사용 방법", "아웃라이너", "검색 벤치마크",
];

/// 목록 항목을 패널에 보일 글로: 메뉴 번호 목록은 "3  다이어그램", 그 밖은 그대로.
pub fn list_entry_label(key: &str, entry: &str) -> String {
    if docfit_config::list_kind(key) == docfit_config::ListKind::MenuNumber {
        if let Some(name) = entry.trim().parse::<usize>().ok().and_then(|n| MENU_NAMES.get(n)) {
            return format!("{entry}  {name}");
        }
    }
    entry.to_string()
}

/// 저장 값을 입력칸 글로(문자열은 그대로, 그 밖은 JSON 한 줄).
pub fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "켬".into() } else { "끔".into() },
        Value::Array(a) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string())))
            .collect::<Vec<_>>()
            .join(", "),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// 입력 글이 규칙을 어길 때의 이유(한국어). 규칙은 `docfit_config::validate_patch` 와 같다.
pub fn explain(item: &Item, text: &str) -> Option<String> {
    match &item.control {
        Control::Number { integer, positive } => {
            let t = text.trim();
            let Ok(n) = t.parse::<f64>() else {
                return Some(format!("{}에 유효한 숫자를 입력해 주세요.", item.label));
            };
            if !n.is_finite() || t.is_empty() {
                return Some(format!("{}에 유효한 숫자를 입력해 주세요.", item.label));
            }
            if !(0.0..=1000.0).contains(&n) {
                return Some("0 이상 1000 이하의 숫자만 쓸 수 있습니다.".into());
            }
            if *positive && n <= 0.0 {
                return Some("0보다 큰 숫자를 입력해 주세요.".into());
            }
            if *integer && n.fract() != 0.0 {
                return Some("소수점 없는 정수를 입력해 주세요.".into());
            }
            None
        }
        Control::Font => crate::controls::explain_font(text),
        Control::Folder => crate::controls::explain_folder(text).or_else(|| {
            if text.chars().count() > 4096 || text.contains('\0') { Some("사용할 수 없는 문자가 있습니다.".into()) } else { None }
        }),
        Control::Hotkey => crate::controls::normalize_hotkey(text).err(),
        Control::Text => {
            if text.chars().count() > 4096 {
                Some("4096자를 넘을 수 없습니다.".into())
            } else if text.contains('\0') {
                Some("사용할 수 없는 문자가 있습니다.".into())
            } else {
                None
            }
        }
        _ => None,
    }
}
