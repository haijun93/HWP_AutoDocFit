//! 표시·편집 모델(창과 무관한 순수 부분): 읽기·변경 추적·검증·검색·저장. 설정 파일은 `docfit_config` 로만 읽고 쓴다.
use crate::schema::{Control, GROUPS, Item, build_items, explain, value_text};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 검색은 이 글자 수부터 즉시 적용(1글자는 Enter 로 확정).
pub const SEARCH_MIN_CHARS: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub error: bool,
}

pub struct Model {
    pub path: PathBuf,
    pub items: Vec<Item>,
    /// `docfit_config::get` 결과(기본값 병합·마이그레이션 적용).
    pub saved: Value,
    /// 읽은 시점 파일 내용의 revision(저장 때 충돌 검사에 쓴다).
    pub revision: String,
    /// 키 → 편집 중인 값(체크는 Bool, 나머지는 입력 글 문자열). 저장 값과 같아지면 지운다.
    pub edits: BTreeMap<String, Value>,
    /// 선택된 묶음(`GROUPS` 번호).
    pub group: usize,
    pub search: String,
    /// 1글자 검색을 Enter 로 확정했는지.
    pub search_forced: bool,
    pub status: Option<Status>,
    /// 설치된 글꼴 이름(글꼴 목록·경고용. 시험에서는 비어 있을 수 있다).
    pub fonts: Vec<String>,
}

pub fn lookup<'a>(root: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(root, |v, part| v.get(part))
}

fn parse_number(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

impl Model {
    /// 설정 파일(없으면 기본값)을 읽어 모델을 만든다. 읽기 전용.
    pub fn load(path: &Path) -> Result<Model, String> {
        let (saved, revision) = Self::read(path)?;
        Ok(Model {
            path: path.to_path_buf(),
            items: build_items(),
            saved,
            revision,
            edits: BTreeMap::new(),
            group: 0,
            search: String::new(),
            search_forced: false,
            status: None,
            #[cfg(windows)]
            fonts: crate::controls::installed_fonts(),
            #[cfg(not(windows))]
            fonts: Vec::new(),
        })
    }

    /// 시연용 항목(목록·단축키 컨트롤)을 덧붙인다. 캡처 모드·시험 전용.
    pub fn add_demo_items(&mut self) {
        self.items.extend(crate::schema::demo_items());
    }

    fn read(path: &Path) -> Result<(Value, String), String> {
        let raw = docfit_config::read_json(path).map_err(|e| e.to_string())?;
        let saved = docfit_config::merged(&raw);
        Ok((saved, docfit_config::revision(&raw)))
    }

    /// 파일을 다시 읽고 모든 편집을 버린다([되돌리기]).
    pub fn revert(&mut self) -> Result<(), String> {
        let (saved, revision) = Self::read(&self.path)?;
        self.saved = saved;
        self.revision = revision;
        self.edits.clear();
        self.status = Some(Status { text: "마지막 저장 값으로 되돌렸습니다.".into(), error: false });
        Ok(())
    }

    pub fn item_index(&self, key: &str) -> Option<usize> {
        self.items.iter().position(|i| i.key == key)
    }

    pub fn saved_value(&self, key: &str) -> Value {
        lookup(&self.saved, key).cloned().unwrap_or(Value::Null)
    }

    /// 화면에 보일 값(편집 중이면 편집 값).
    pub fn current(&self, key: &str) -> Value {
        self.edits.get(key).cloned().unwrap_or_else(|| self.saved_value(key))
    }

    pub fn current_bool(&self, key: &str) -> bool {
        self.current(key).as_bool().unwrap_or(false)
    }

    pub fn current_text(&self, key: &str) -> String {
        value_text(&self.current(key))
    }

    pub fn is_changed(&self, key: &str) -> bool {
        self.edits.contains_key(key)
    }

    pub fn changed_count(&self) -> usize {
        self.edits.len()
    }

    pub fn group_changed(&self, group: usize) -> usize {
        self.items.iter().filter(|i| i.group == group && self.is_changed(&i.key)).count()
    }

    /// 편집 값을 넣는다. 저장 값과 같으면 변경에서 뺀다. 잘못된 값도 초안으로 남겨 이유를 보여 준다.
    pub fn set_value(&mut self, key: &str, value: Value) {
        let value = match (&value, self.item_index(key).map(|i| &self.items[i].control)) {
            (Value::String(s), Some(Control::Number { .. })) => Value::String(s.trim().to_string()),
            _ => value,
        };
        let saved = self.saved_value(key);
        let same = value == saved || (saved.is_null() && matches!(&value, Value::Array(a) if a.is_empty()));
        if same {
            self.edits.remove(key);
        } else {
            self.edits.insert(key.to_string(), value);
        }
        self.status = None;
    }

    pub fn set_text(&mut self, key: &str, text: &str) {
        self.set_value(key, Value::String(text.to_string()));
    }

    /// 목록 값(문자열 배열, 메뉴 번호 목록은 정수 배열을 글로. 그 밖의 항목은 건너뜀).
    pub fn current_list(&self, key: &str) -> Vec<String> {
        match self.current(key) {
            Value::Array(a) => a
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => n.as_i64().map(|n| n.to_string()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn list_max(&self, key: &str) -> usize {
        match self.item_index(key).map(|i| &self.items[i].control) {
            Some(Control::List { max }) => *max,
            _ => 0,
        }
    }

    /// 목록을 값으로 넣는다. 메뉴 번호 목록은 저장 형식(오름차순 정수 배열)으로 바꾼다.
    fn put_list(&mut self, key: &str, list: Vec<String>) {
        if docfit_config::list_kind(key) == docfit_config::ListKind::MenuNumber {
            let mut nums: Vec<i64> = list.iter().filter_map(|s| s.trim().parse().ok()).collect();
            nums.sort_unstable();
            nums.dedup();
            self.set_value(key, Value::Array(nums.into_iter().map(|n| Value::from(n)).collect()));
        } else {
            self.set_value(key, Value::Array(list.into_iter().map(Value::String).collect()));
        }
    }

    /// 목록 끝에 추가(검사: 빈 글·길이·중복·개수). 성공하면 새 항목 번호(메뉴 번호 목록은 정렬되므로 들어간 자리).
    pub fn list_add(&mut self, key: &str, text: &str) -> Result<usize, String> {
        let mut list = self.current_list(key);
        let kind = docfit_config::list_kind(key);
        let entry = crate::controls::check_list_entry_of(kind, &list, text, self.list_max(key))?;
        list.push(entry.clone());
        self.put_list(key, list);
        Ok(self.current_list(key).iter().position(|x| *x == entry).unwrap_or(0))
    }

    pub fn list_remove(&mut self, key: &str, at: usize) -> bool {
        let mut list = self.current_list(key);
        if at >= list.len() {
            return false;
        }
        list.remove(at);
        self.put_list(key, list);
        true
    }

    /// `at` 번째 항목을 `delta`(-1 위·+1 아래)만큼 옮긴다. 옮겨진 새 자리를 돌려준다.
    pub fn list_move(&mut self, key: &str, at: usize, delta: i32) -> Option<usize> {
        if docfit_config::list_kind(key) == docfit_config::ListKind::MenuNumber {
            return None; // 번호 목록은 늘 오름차순이다.
        }
        let mut list = self.current_list(key);
        let to = at as i32 + delta;
        if at >= list.len() || to < 0 || to as usize >= list.len() {
            return None;
        }
        list.swap(at, to as usize);
        self.put_list(key, list);
        Some(to as usize)
    }

    /// 색 선택 순환(견본 ←→).
    pub fn cycle_color(&mut self, key: &str, dir: i32) {
        let n = crate::controls::COLORS.len() as i32;
        let at = crate::controls::color_index(&self.current_text(key)).map(|i| i as i32).unwrap_or(if dir > 0 { -1 } else { 0 });
        let next = (at + dir).rem_euclid(n) as usize;
        self.set_text(key, crate::controls::COLORS[next].0);
    }

    /// 글꼴 고르기 목록(이름에 `filter` 글이 들어 있는 것, 대소문자 무시).
    pub fn font_choices(&self, filter: &str) -> Vec<String> {
        let q = filter.trim().to_lowercase();
        self.fonts.iter().filter(|f| q.is_empty() || f.to_lowercase().contains(&q)).cloned().collect()
    }

    /// 편집 가능하지만 저장은 막지 않는 알림(폴더가 없음·설치되지 않은 글꼴).
    pub fn warning_of(&self, key: &str) -> Option<String> {
        let item = &self.items[self.item_index(key)?];
        let now = self.current_text(key);
        match item.control {
            Control::Folder => crate::controls::folder_warning(&now),
            Control::Font if !self.fonts.is_empty() && !now.trim().is_empty() => {
                let want = now.trim().to_lowercase();
                if self.fonts.iter().any(|f| f.to_lowercase() == want) { None } else { Some("이 PC에 설치된 글꼴 목록에 없습니다(한/글 전용 글꼴일 수 있음).".into()) }
            }
            _ => None,
        }
    }

    pub fn toggle(&mut self, key: &str) {
        let now = self.current_bool(key);
        self.set_value(key, Value::Bool(!now));
    }

    /// 숫자 항목 ±: 현재 글이 숫자가 아니면 저장 값에서 시작한다. 범위를 벗어나면 막는다.
    pub fn step(&mut self, key: &str, delta: i64) {
        let Some(item) = self.item_index(key).map(|i| self.items[i].clone()) else { return };
        let Control::Number { positive, .. } = item.control else { return };
        let base = parse_number(&self.current_text(key)).or_else(|| parse_number(&value_text(&self.saved_value(key)))).unwrap_or(0.0);
        let min = if positive { 1.0 } else { 0.0 };
        let next = (base + delta as f64).clamp(min, 1000.0);
        let text = if next.fract() == 0.0 { format!("{}", next as i64) } else { format!("{next}") };
        self.set_text(key, &text);
    }

    /// 선택지 순환(콤보). `dir` = 1 다음, -1 이전.
    pub fn cycle(&mut self, key: &str, dir: i32) {
        let Some(item) = self.item_index(key).map(|i| self.items[i].clone()) else { return };
        let Control::Choice(options) = &item.control else { return };
        let now = self.current_text(key);
        let at = options.iter().position(|o| o.0 == now).unwrap_or(0) as i32;
        let n = options.len() as i32;
        let next = (at + dir).rem_euclid(n) as usize;
        self.set_value(key, Value::String(options[next].0.clone()));
    }

    /// 항목을 기본값으로(편집 값으로 들어가며 저장 전에는 파일을 건드리지 않는다).
    pub fn reset_to_default(&mut self, key: &str) {
        if let Some(i) = self.item_index(key) {
            if self.items[i].control.is_editable() {
                let d = self.items[i].default.clone();
                self.set_value(key, d);
            }
        }
    }

    /// 이 항목의 입력 오류(이유). 개별 규칙 + 최소≤최대 교차 검사.
    pub fn error_of(&self, key: &str) -> Option<String> {
        let item = &self.items[self.item_index(key)?];
        if let (true, Some(Value::String(s))) = (item.control.is_text_like(), self.edits.get(key)) {
            if let Some(why) = explain(item, s) {
                return Some(why);
            }
            // docfit_config 가 거부하는데 위 규칙에 없는 경우도 놓치지 않는다(시연 항목은 스키마 밖이라 제외).
            if !key.starts_with("demo.") && docfit_config::validate_patch(&json!({ key: s })).is_err() {
                return Some("허용되지 않는 값입니다.".into());
            }
        }
        if (key == "linespacing_min" || key == "linespacing_max") && self.is_changed_or_pair(key) {
            let min = parse_number(&self.current_text("linespacing_min"));
            let max = parse_number(&self.current_text("linespacing_max"));
            if let (Some(a), Some(b)) = (min, max) {
                if a > b {
                    return Some("최소 줄 간격은 최대 줄 간격보다 클 수 없습니다.".into());
                }
            }
        }
        None
    }

    fn is_changed_or_pair(&self, key: &str) -> bool {
        self.is_changed("linespacing_min") || self.is_changed("linespacing_max") || self.is_changed(key)
    }

    pub fn error_count(&self) -> usize {
        self.edits.keys().filter(|k| self.error_of(k).is_some()).count()
            + ["linespacing_min", "linespacing_max"]
                .iter()
                .filter(|k| !self.is_changed(k) && self.error_of(k).is_some())
                .count()
    }

    /// 검색어가 화면 목록을 바꾸는 상태인가.
    pub fn search_active(&self) -> bool {
        let n = self.search.trim().chars().count();
        n >= SEARCH_MIN_CHARS || (n >= 1 && self.search_forced)
    }

    pub fn set_search(&mut self, text: &str) {
        self.search = text.to_string();
        self.search_forced = false;
    }

    /// 지금 우측에 보일 항목 번호. 검색 중이면 전체에서 찾은 결과, 아니면 선택 묶음.
    pub fn visible(&self) -> Vec<usize> {
        if self.search_active() {
            let q = self.search.trim().to_lowercase();
            self.items.iter().enumerate().filter(|(_, i)| i.haystack.contains(&q)).map(|(n, _)| n).collect()
        } else {
            self.items.iter().enumerate().filter(|(_, i)| i.group == self.group).map(|(n, _)| n).collect()
        }
    }

    /// 묶음별 항목 수(검색 중이면 그 묶음의 결과 수).
    pub fn group_count(&self, group: usize) -> usize {
        if self.search_active() {
            let q = self.search.trim().to_lowercase();
            self.items.iter().filter(|i| i.group == group && i.haystack.contains(&q)).count()
        } else {
            self.items.iter().filter(|i| i.group == group).count()
        }
    }

    /// 편집 가능 항목 수 / 편집하지 않는 항목 수(미구현·소유자 둘 이상·내부 구분 포함).
    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for i in &self.items {
            match &i.control {
                Control::ReadOnly(crate::schema::ReadOnly::Unimplemented(_)) => c.unimplemented += 1,
                Control::ReadOnly(crate::schema::ReadOnly::MultiOwner(_)) => c.multi_owner += 1,
                Control::ReadOnly(crate::schema::ReadOnly::Internal(_)) => c.internal += 1,
                _ => c.editable += 1,
            }
        }
        c
    }

    /// 변경 항목을 저장한다(원자적·revision 충돌 검사는 `docfit_config::set`). 성공하면 저장 후 값으로 갱신.
    pub fn save(&mut self) -> Result<usize, String> {
        if self.edits.is_empty() {
            return Err("바뀐 항목이 없습니다.".into());
        }
        let errors = self.error_count();
        if errors > 0 {
            let msg = format!("입력 오류 {errors}건을 고친 뒤 저장해 주세요.");
            self.status = Some(Status { text: msg.clone(), error: true });
            return Err(msg);
        }
        let mut patch = Map::new();
        for (k, v) in &self.edits {
            patch.insert(k.clone(), v.clone());
        }
        patch.insert("__revision".into(), Value::String(self.revision.clone()));
        match docfit_config::set(&self.path, &Value::Object(patch)) {
            Ok(_) => {
                let n = self.edits.len();
                let (saved, revision) = Self::read(&self.path)?;
                self.saved = saved;
                self.revision = revision;
                self.edits.clear();
                self.status = Some(Status { text: format!("저장했습니다. ({n}건)"), error: false });
                Ok(n)
            }
            Err(e) => {
                let text = match e.code {
                    "SETTINGS_CHANGED" => "다른 곳에서 설정이 바뀌었습니다. [되돌리기]로 다시 불러온 뒤 적용해 주세요.".to_string(),
                    "INVALID_INPUT" if e.message.contains("minimum") => "최소 줄 간격은 최대 줄 간격보다 클 수 없습니다.".to_string(),
                    _ => format!("저장하지 못했습니다. ({})", e),
                };
                self.status = Some(Status { text: text.clone(), error: true });
                Err(text)
            }
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Counts {
    pub editable: usize,
    pub unimplemented: usize,
    pub multi_owner: usize,
    pub internal: usize,
}

pub fn group_title(group: usize) -> &'static str {
    GROUPS.get(group).map(|g| g.title).unwrap_or("")
}
