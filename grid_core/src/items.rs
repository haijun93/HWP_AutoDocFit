//! 목록 항목(`Doc`)과 검색·정렬(`Collection`). 문서뷰·사진뷰가 같은 필드(경로·이름·확장자·크기·수정시각·클라우드)를 쓴다.
//! 항목은 정렬·검색된 **인덱스 목록(u32)** 으로만 참조해 복사하지 않는다.

#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub path: String,
    pub name: String,
    /// 소문자, 점 없음.
    pub ext: String,
    pub size: u64,
    /// 유닉스 초(UTC).
    pub mtime: f64,
    pub cloud: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Modified,
    Size,
    Ext,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [SortKey::Name, SortKey::Modified, SortKey::Size, SortKey::Ext];
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "이름",
            SortKey::Modified => "수정일",
            SortKey::Size => "크기",
            SortKey::Ext => "종류",
        }
    }
    pub fn next(self) -> SortKey {
        let i = Self::ALL.iter().position(|k| *k == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
    /// 처음 누르면 이름·종류는 오름차순, 수정일·크기는 큰 값(최신)이 먼저.
    pub fn default_descending(self) -> bool {
        matches!(self, SortKey::Modified | SortKey::Size)
    }
}

/// 문서 목록 + 보이는 순서(검색·정렬을 적용한 인덱스 목록).
#[derive(Clone, Debug)]
pub struct Collection {
    pub docs: Vec<Doc>,
    pub query: String,
    pub sort: SortKey,
    pub descending: bool,
    view: Vec<u32>,
    /// 즐겨찾기·태그·중복 보기: 여기 있는 경로만 보인다.
    only: Option<Only>,
}

/// 보일 경로(→ 순번). `ranked` 면 정렬 대신 순번대로 놓는다.
#[derive(Clone, Debug, Default)]
pub struct Only {
    pub rank: std::collections::HashMap<String, u32>,
    pub ranked: bool,
}

impl Default for Collection {
    fn default() -> Self {
        Collection { docs: Vec::new(), query: String::new(), sort: SortKey::Name, descending: false, view: Vec::new(), only: None }
    }
}

impl Collection {
    pub fn new(docs: Vec<Doc>) -> Collection {
        let mut c = Collection { docs, ..Collection::default() };
        c.rebuild();
        c
    }
    pub fn replace(&mut self, docs: Vec<Doc>) {
        self.docs = docs;
        self.rebuild();
    }
    pub fn extend(&mut self, more: Vec<Doc>) {
        self.docs.extend(more);
        self.rebuild();
    }
    pub fn set_query(&mut self, q: &str) {
        self.query = q.to_string();
        self.rebuild();
    }
    pub fn set_only(&mut self, only: Option<Only>) {
        self.only = only;
        self.rebuild();
    }
    pub fn restricted(&self) -> bool {
        self.only.is_some()
    }
    pub fn set_sort(&mut self, key: SortKey, descending: bool) {
        self.sort = key;
        self.descending = descending;
        self.rebuild();
    }
    /// 정렬 기준을 다음으로 돌린다(같은 기준 안에서 방향은 기준의 기본값).
    pub fn cycle_sort(&mut self) {
        let k = self.sort.next();
        self.set_sort(k, k.default_descending());
    }
    pub fn toggle_direction(&mut self) {
        self.descending = !self.descending;
        self.rebuild();
    }
    pub fn total(&self) -> usize {
        self.docs.len()
    }
    pub fn len(&self) -> usize {
        self.view.len()
    }
    pub fn is_empty(&self) -> bool {
        self.view.is_empty()
    }
    /// 보이는 순서 `n` 번째 문서.
    pub fn get(&self, n: usize) -> Option<&Doc> {
        self.view.get(n).and_then(|i| self.docs.get(*i as usize))
    }
    fn rebuild(&mut self) {
        let q = self.query.trim().to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        let mut v: Vec<u32> = (0..self.docs.len() as u32)
            .filter(|i| {
                if self.only.as_ref().is_some_and(|o| !o.rank.contains_key(&self.docs[*i as usize].path)) {
                    return false;
                }
                if terms.is_empty() {
                    return true;
                }
                let d = &self.docs[*i as usize];
                let hay = format!("{} {}", d.name.to_lowercase(), d.ext);
                terms.iter().all(|t| hay.contains(t))
            })
            .collect();
        let docs = &self.docs;
        match self.sort {
            SortKey::Modified => v.sort_by(|a, b| {
                let (x, y) = (&docs[*a as usize], &docs[*b as usize]);
                x.mtime.total_cmp(&y.mtime).then_with(|| x.path.cmp(&y.path))
            }),
            SortKey::Size => v.sort_by(|a, b| {
                let (x, y) = (&docs[*a as usize], &docs[*b as usize]);
                x.size.cmp(&y.size).then_with(|| x.path.cmp(&y.path))
            }),
            // 글 키는 한 번만 만들어 수만 항목에서도 비교마다 할당하지 않는다.
            key => {
                let mut keyed: Vec<(String, u32)> = v
                    .iter()
                    .map(|i| {
                        let d = &docs[*i as usize];
                        let k = if key == SortKey::Ext { format!("{}\u{0}{}", d.ext, d.name.to_lowercase()) } else { d.name.to_lowercase() };
                        (k, *i)
                    })
                    .collect();
                keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| docs[a.1 as usize].path.cmp(&docs[b.1 as usize].path)));
                v = keyed.into_iter().map(|(_, i)| i).collect();
            }
        }
        if self.descending {
            v.reverse();
        }
        if let Some(only) = self.only.as_ref().filter(|o| o.ranked) {
            v.sort_by_key(|i| only.rank.get(&docs[*i as usize].path).copied());
        }
        self.view = v;
    }
}


/// `klib scan --json-lines` 한 줄(`klib/src/scan_cli.rs::emit`) → 항목. 문서·사진 모두 같은 줄 형식이다.
/// 필수 키(path·name·ext·size·mtime·cloud)가 없거나 형식이 틀리면 오류(Python `rust_scan.scan` 과 같은 거절 기준).
pub fn parse_scan_line(line: &str) -> Result<Doc, String> {
    use crate::json::{self, Value};
    let v = json::parse(line)?;
    let Value::Obj(_) = v else { return Err("JSON 줄이 객체가 아닙니다".into()) };
    let text = |k: &str| v.get(k).ok_or_else(|| format!("필수 키 없음: {k}")).and_then(|x| x.as_str().map(str::to_string).ok_or_else(|| format!("{k} 는 글이어야 합니다")));
    let num = |k: &str| v.get(k).ok_or_else(|| format!("필수 키 없음: {k}")).and_then(|x| x.as_f64().ok_or_else(|| format!("{k} 는 숫자여야 합니다")));
    let path = text("path")?;
    let name = text("name")?;
    let ext = text("ext")?.trim_start_matches('.').to_lowercase();
    let size = num("size")?;
    let mtime = num("mtime")?;
    let cloud = v.get("cloud").ok_or("필수 키 없음: cloud")?.as_bool().ok_or("cloud 는 참/거짓이어야 합니다")?;
    if path.is_empty() || name.is_empty() {
        return Err("path·name 이 비어 있습니다".into());
    }
    if size < 0.0 || !size.is_finite() {
        return Err("size 가 음수입니다".into());
    }
    Ok(Doc { path, name, ext, size: size as u64, mtime, cloud })
}
