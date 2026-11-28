//! 결과 목록이 비었을 때의 안내(QMSBUG1). '찾았는데 없음'과 '검색하지 못함·색인 준비 중·
//! 일부만 검색함·조건 때문에 숨김'을 구분한다. 입출력과 Win32 호출이 없다.
use crate::tm_state::Phase;
use serde_json::Value;

/// 파일명 결과가 0건일 때 `names_status`로 확인한 검색 범위.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameCoverage {
    /// 색인이 없거나 비어 있어 파일명을 찾았다고 할 수 없다.
    NotSearched(String),
    /// 찾기는 했지만 범위가 일부이거나 확인할 수 없다.
    Limited(String),
    /// 색인 서비스의 드라이브 색인이 정상이다.
    Full(String),
}

impl NameCoverage {
    pub fn message(&self) -> &str {
        match self {
            Self::NotSearched(m) | Self::Limited(m) | Self::Full(m) => m,
        }
    }
}

pub fn name_coverage(status: Result<&Value, &str>) -> NameCoverage {
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            return NameCoverage::Limited(format!("파일명 색인 상태를 확인하지 못했습니다({error})."));
        }
    };
    let state = status["state"].as_str().unwrap_or("");
    let volumes = status["volumes"].as_array().map(Vec::as_slice).unwrap_or_default();
    // `entries`에는 볼륨 뿌리도 들어가므로 볼륨별 항목 수를 먼저 본다.
    let entries = if volumes.is_empty() {
        status["entries"].as_u64().unwrap_or(0)
    } else {
        volumes.iter().filter_map(|v| v["entries"].as_u64()).sum()
    };
    let last_error = status["last_error"].as_str();
    if entries == 0 {
        let why = match state {
            "building" => "파일명 색인을 준비하는 중입니다. 잠시 뒤 다시 검색해 주세요.",
            "empty" => "파일명 색인을 아직 만들지 않았습니다. 색인 서비스가 설치·실행 중인지 확인해 주세요.",
            "failed" => "파일명 색인 만들기에 실패했습니다.",
            _ => "파일명 색인이 비어 있습니다.",
        };
        return NameCoverage::NotSearched(match last_error {
            Some(e) => format!("{why} (마지막 오류: {e})"),
            None => why.into(),
        });
    }
    if status["mode"] != "service" {
        return NameCoverage::Limited(format!(
            "색인 서비스가 아닌 사용자 모드입니다. 파일명은 등록한 폴더 {}곳(항목 {entries}개)에서만 찾았습니다.",
            volumes.len()
        ));
    }
    if state == "building" {
        return NameCoverage::Limited("파일명 색인을 다시 만드는 중이라 이전 색인에서 찾았습니다.".into());
    }
    // 권한 없는 폴더 몇 개("folders unreadable")는 정상이라 알리지 않는다.
    let broken: Vec<&str> = volumes
        .iter()
        .filter(|v| {
            v["entries"].as_u64() == Some(0)
                || v["note"].as_str().is_some_and(|n| n.contains("mft:"))
        })
        .filter_map(|v| v["name"].as_str())
        .collect();
    if !broken.is_empty() {
        return NameCoverage::Limited(format!(
            "일부 드라이브({})의 파일명 색인이 온전하지 않을 수 있습니다.",
            broken.join(", ")
        ));
    }
    if let Some(e) = last_error {
        return NameCoverage::Limited(format!("파일명 색인에 최근 변경이 빠졌을 수 있습니다({e})."));
    }
    NameCoverage::Full(format!(
        "파일명 색인 항목 {entries}개에서 찾았습니다(다른 사용자 계정 폴더는 제외)."
    ))
}

#[derive(Clone, Copy, Debug)]
pub struct Facts<'a> {
    pub phase: Phase,
    /// 아직 답하지 않은 검색 구간이 있다.
    pub pending: bool,
    pub error: Option<&'a str>,
    /// 받은 결과 수.
    pub found: usize,
    /// 화면 조건을 거친 뒤 보이는 수.
    pub visible: usize,
    /// 받은 결과가 설정(제외 폴더·하위 폴더)에서 모두 빠졌다.
    pub hidden_by_settings: bool,
    pub coverage: Option<&'a NameCoverage>,
    /// 기간·형식·폴더 조건이 걸려 있다.
    pub scoped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: &'static str,
    pub detail: String,
}

fn searched(phase: Phase) -> bool {
    matches!(phase, Phase::Searching | Phase::Results | Phase::Error)
}

pub fn notice(f: Facts) -> Option<Notice> {
    let say = |title, detail: String| Some(Notice { title, detail });
    if !searched(f.phase) || f.visible > 0 {
        return None;
    }
    if f.found > 0 {
        return say(
            "조건 때문에 결과가 숨겨져 있습니다",
            format!(
                "찾은 {}건이 모두 가려졌습니다. 결과 내 검색·형식 선택·파일명 제외·접힌 묶음을 확인하세요.",
                f.found
            ),
        );
    }
    if let Some(error) = f.error {
        let title = if error.contains("준비하는 중") {
            "파일명 색인을 준비하는 중입니다"
        } else {
            "검색을 완료하지 못했습니다"
        };
        return say(title, format!("0건은 '파일이 없다'는 뜻이 아닙니다. {error}"));
    }
    if f.pending {
        return say("검색 중…", String::new());
    }
    if f.hidden_by_settings {
        return say(
            "설정 때문에 결과가 숨겨져 있습니다",
            "설정의 '제외 폴더'와 '하위 폴더 포함'을 확인하세요.".into(),
        );
    }
    if let Some(NameCoverage::NotSearched(message)) = f.coverage {
        return say("파일명을 검색하지 못했습니다", message.clone());
    }
    let mut detail = f
        .coverage
        .map(|c| c.message().to_owned())
        .unwrap_or_else(|| "철자·띄어쓰기를 바꿔 보세요.".into());
    if f.scoped {
        detail.push_str(" 기간·형식·폴더 조건이 걸려 있으니 '전체'로도 확인해 보세요.");
    }
    say("검색 결과가 없습니다", detail)
}

/// 요약 줄. 검색하지 못한 상태를 "0개 · 0.00초"로 적지 않는다.
pub fn summary(f: Facts, elapsed_secs: Option<f64>) -> String {
    if searched(f.phase) && f.found == 0 {
        if f.error.is_some() || matches!(f.coverage, Some(NameCoverage::NotSearched(_))) {
            return "검색 못함".into();
        }
        if f.pending {
            return "검색 중…".into();
        }
    }
    match elapsed_secs {
        Some(secs) => format!("{}개 · {secs:.2}초", f.visible),
        None => format!("{}개", f.visible),
    }
}
