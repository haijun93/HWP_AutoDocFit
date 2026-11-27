//! 앱 화면에만 있던 사진 기능의 화면 상태와 배치(창과 무관한 순수 부분):
//! 둘째 도구줄, 즐겨찾기·태그·중복 보기, 크게 보기, 사본 편집 입력.
use crate::layout::{Rect, ROW2_H, TOOLBAR_H, scale};
use crate::library::{Mark, key, resolved_key};
use crate::model::Doc;
use crate::pixels::Image;
use grid_core::file_ops::{FileResult, Operation};
use grid_core::items::Only;
use std::collections::HashMap;
use std::sync::Arc;

/// 둘째 도구줄. 왼쪽은 보기, 오른쪽은 선택한 사진에 하는 일.
#[derive(Clone, Copy, Debug)]
pub struct Row2 {
    pub bar: Rect,
    pub buttons: [(Command, Rect); 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    FavoritesOnly,
    TagFilter,
    Duplicates,
    View,
    Favorite,
    Tags,
    Export,
    Edit,
}

impl Command {
    pub const ALL: [Command; 8] = [
        Command::FavoritesOnly,
        Command::TagFilter,
        Command::Duplicates,
        Command::View,
        Command::Favorite,
        Command::Tags,
        Command::Export,
        Command::Edit,
    ];
    /// 선택한 사진이 있어야 하는 단추인지와, 있어야 하는 수.
    pub fn enabled(self, selected: usize) -> bool {
        match self {
            Command::FavoritesOnly | Command::TagFilter | Command::Duplicates => true,
            Command::View | Command::Edit => selected == 1,
            Command::Favorite | Command::Tags | Command::Export => selected > 0,
        }
    }
}

impl Row2 {
    pub fn new(w: i32, dpi: u32) -> Row2 {
        let s = |v: i32| scale(v, dpi);
        let top = s(TOOLBAR_H);
        let bar = Rect::new(0, top, w, top + s(ROW2_H));
        let (y0, y1) = (top + s(5), top + s(ROW2_H - 5));
        let mut buttons = [(Command::FavoritesOnly, Rect::default()); 8];
        let mut x = s(16);
        for (slot, (command, width)) in buttons.iter_mut().zip([
            (Command::FavoritesOnly, 96),
            (Command::TagFilter, 116),
            (Command::Duplicates, 92),
        ]) {
            *slot = (command, Rect::new(x, y0, x + s(width), y1));
            x += s(width + 6);
        }
        let mut right = w - s(16);
        for (slot, (command, width)) in buttons.iter_mut().skip(3).rev().zip([
            (Command::Edit, 80),
            (Command::Export, 100),
            (Command::Tags, 66),
            (Command::Favorite, 90),
            (Command::View, 78),
        ]) {
            *slot = (command, Rect::new(right - s(width), y0, right, y1));
            right -= s(width + 6);
        }
        Row2 { bar, buttons }
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<Command> {
        self.buttons.iter().find(|(_, r)| r.contains(x, y)).map(|(c, _)| *c)
    }
}

/// 크게 보기. 열 때의 목록을 그대로 들고 있어, 보는 중 필터가 바뀌어도 순서가 흔들리지 않는다.
#[derive(Clone, Debug)]
pub struct Viewer {
    pub items: Vec<Doc>,
    pub index: usize,
    pub playing: bool,
    /// 지금 사진의 해독 결과. None 이면 읽는 중.
    pub image: Option<Result<Arc<Image>, String>>,
}

impl Viewer {
    pub fn open(items: Vec<Doc>, index: usize) -> Option<Viewer> {
        (index < items.len()).then(|| Viewer { items, index, playing: false, image: None })
    }
    pub fn current(&self) -> &Doc {
        &self.items[self.index]
    }
    /// 앞뒤로 한 장. 끝에서는 멈추고, 슬라이드쇼 중에만 처음으로 돈다. 사진이 바뀌면 true.
    pub fn step(&mut self, forward: bool) -> bool {
        let last = self.items.len() - 1;
        let next = match (forward, self.index) {
            (true, i) if i < last => i + 1,
            (true, _) if self.playing => 0,
            (false, i) if i > 0 => i - 1,
            _ => self.index,
        };
        let changed = next != self.index;
        if changed {
            self.index = next;
            self.image = None;
        }
        changed
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ViewerLayout {
    pub bar: Rect,
    pub prev: Rect,
    pub next: Rect,
    pub play: Rect,
    pub favorite: Rect,
    pub tags: Rect,
    pub edit: Rect,
    pub open: Rect,
    pub close: Rect,
    pub canvas: Rect,
    pub info: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewerHit {
    Prev,
    Next,
    Play,
    Favorite,
    Tags,
    Edit,
    Open,
    Close,
}

impl ViewerLayout {
    pub fn new(w: i32, h: i32, dpi: u32) -> ViewerLayout {
        let s = |v: i32| scale(v, dpi);
        let bar = Rect::new(0, 0, w, s(48));
        let info = Rect::new(0, (h - s(56)).max(bar.bottom), w, h);
        let mut right = w - s(12);
        let mut button = |width: i32| {
            let r = Rect::new(right - s(width), s(9), right, s(39));
            right -= s(width + 6);
            r
        };
        let close = button(64);
        let open = button(104);
        let edit = button(84);
        let tags = button(60);
        let favorite = button(96);
        let play = button(96);
        let next = button(40);
        let prev = button(40);
        ViewerLayout { bar, prev, next, play, favorite, tags, edit, open, close, canvas: Rect::new(0, bar.bottom, w, info.top), info }
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<ViewerHit> {
        [
            (self.prev, ViewerHit::Prev),
            (self.next, ViewerHit::Next),
            (self.play, ViewerHit::Play),
            (self.favorite, ViewerHit::Favorite),
            (self.tags, ViewerHit::Tags),
            (self.edit, ViewerHit::Edit),
            (self.open, ViewerHit::Open),
            (self.close, ViewerHit::Close),
        ]
        .into_iter()
        .find(|(r, _)| r.contains(x, y))
        .map(|(_, hit)| hit)
    }
}

/// 비율을 지켜 `area` 안에 놓는다. 작은 그림은 키우지 않는다.
pub fn fit(width: u32, height: u32, area: &Rect) -> Rect {
    let (aw, ah) = (area.width().max(1) as f64, area.height().max(1) as f64);
    let ratio = (aw / width.max(1) as f64).min(ah / height.max(1) as f64).min(1.0);
    let (w, h) = (((width as f64 * ratio).round() as i32).max(1), ((height as f64 * ratio).round() as i32).max(1));
    let (x, y) = (area.left + (area.width() - w) / 2, area.top + (area.height() - h) / 2);
    Rect::new(x, y, x + w, y + h)
}

/// 사진뷰에만 있는 화면 상태.
#[derive(Clone, Debug, Default)]
pub struct Photo {
    pub marks: HashMap<String, Mark>,
    pub favorites_only: bool,
    pub tag: Option<String>,
    /// 중복 보기: 묶음마다의 경로.
    pub duplicates: Option<Vec<Vec<String>>>,
    pub viewer: Option<Viewer>,
    /// 사본 저장·중복 찾기 진행·결과 한 줄.
    pub status: String,
    /// 중복 찾기가 도는 중(단추가 '중복 취소'가 된다).
    pub searching: bool,
}

impl Photo {
    pub fn mark(&self, path: &str) -> Option<&Mark> {
        if self.marks.is_empty() { None } else { self.marks.get(&key(path)).or_else(|| self.marks.get(&resolved_key(path))) }
    }

    pub fn filtering(&self) -> bool {
        self.favorites_only || self.tag.is_some() || self.duplicates.is_some()
    }

    /// 지금 보기 조건에 맞는 경로만 남기는 필터(조건이 없으면 None).
    pub fn only(&self, docs: &[Doc]) -> Option<Only> {
        if !self.filtering() {
            return None;
        }
        let order: Option<HashMap<&str, u32>> = self.duplicates.as_ref().map(|groups| {
            groups.iter().flatten().enumerate().map(|(i, p)| (p.as_str(), i as u32)).collect()
        });
        let wanted = self.tag.as_ref().map(|t| t.to_lowercase());
        let rank = docs
            .iter()
            .filter(|d| {
                let mark = self.mark(&d.path);
                (!self.favorites_only || mark.is_some_and(|m| m.favorite))
                    && wanted.as_ref().is_none_or(|t| mark.is_some_and(|m| m.tags.iter().any(|x| x.to_lowercase() == *t)))
            })
            .filter_map(|d| match &order {
                Some(order) => order.get(d.path.as_str()).map(|rank| (d.path.clone(), *rank)),
                None => Some((d.path.clone(), 0)),
            })
            .collect();
        Some(Only { rank, ranked: order.is_some() })
    }

    /// 중복 보기에서 옮기거나 이름을 바꾼 사진은 새 경로로 묶음에 남는다.
    pub fn follow_renames(&mut self, op: Operation, results: &[FileResult]) {
        if !matches!(op, Operation::Move | Operation::Rename) {
            return;
        }
        let Some(groups) = &mut self.duplicates else { return };
        for result in results.iter().filter(|r| r.error.is_none()) {
            if let Some(target) = &result.target {
                for path in groups.iter_mut().flatten().filter(|p| **p == result.source.path) {
                    path.clone_from(&target.path);
                }
            }
        }
    }

    /// 중복 묶음 번호(1부터).
    pub fn group_of(&self, path: &str) -> Option<usize> {
        self.duplicates.as_ref()?.iter().position(|g| g.iter().any(|p| p == path)).map(|i| i + 1)
    }

    /// 선택한 사진의 즐겨찾기를 어떻게 바꿀지: 하나라도 아니면 모두 켜고, 모두 켜져 있으면 모두 끈다.
    pub fn favorite_flips(&self, paths: &[String]) -> Vec<String> {
        let on = |p: &String| self.mark(p).is_some_and(|m| m.favorite);
        let turn_on = paths.iter().any(|p| !on(p));
        paths.iter().filter(|p| on(p) != turn_on).cloned().collect()
    }

    /// 선택한 사진에 붙은 태그(합집합).
    pub fn tags_of(&self, paths: &[String]) -> Vec<String> {
        let set: std::collections::BTreeSet<String> =
            paths.iter().filter_map(|p| self.mark(p)).flat_map(|m| m.tags.iter().cloned()).collect();
        set.into_iter().collect()
    }
}

/// 사본 편집 값.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditSpec {
    pub quarter_turns: u32,
    pub brightness: f64,
    pub contrast: f64,
    pub crop: Option<[i64; 4]>,
}

pub const EDIT_DEFAULT: &str = "90 1.0 1.0";
pub const EDIT_HELP: &str = "회전(0·90·180·270) 밝기(0.1~5) 대비(0.1~5) [자르기: 왼 위 오른 아래]. 원본은 두고 _edited 사본을 만듭니다.";

/// `회전 밝기 대비 [왼 위 오른 아래]` (빈칸·쉼표 구분).
pub fn parse_edit(text: &str) -> Result<EditSpec, String> {
    let words: Vec<&str> = text.split([' ', ',', '\t']).filter(|w| !w.is_empty()).collect();
    if words.len() != 3 && words.len() != 7 {
        return Err("값은 3개(회전 밝기 대비) 또는 7개(+자르기 4개)여야 합니다".into());
    }
    let turn: i64 = words[0].parse().map_err(|_| "회전은 0·90·180·270 중 하나입니다".to_string())?;
    if turn % 90 != 0 {
        return Err("회전은 90° 단위만 됩니다".into());
    }
    let factor = |w: &str, name: &str| {
        w.parse::<f64>().ok().filter(|v| *v > 0.0 && *v <= 5.0).ok_or_else(|| format!("{name}는 0보다 크고 5 이하여야 합니다"))
    };
    let (brightness, contrast) = (factor(words[1], "밝기")?, factor(words[2], "대비")?);
    let crop = if words.len() == 7 {
        let mut values = [0i64; 4];
        for (slot, word) in values.iter_mut().zip(&words[3..]) {
            *slot = word.parse().map_err(|_| "자르기 좌표는 정수여야 합니다".to_string())?;
        }
        if values[2] <= values[0] || values[3] <= values[1] {
            return Err("자르기 영역이 비었습니다".into());
        }
        Some(values)
    } else {
        None
    };
    Ok(EditSpec { quarter_turns: (turn.rem_euclid(360) / 90) as u32, brightness, contrast, crop })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(path: &str) -> Doc {
        Doc { path: path.into(), name: path.rsplit('\\').next().unwrap().into(), ext: "jpg".into(), size: 1, mtime: 0.0, cloud: false }
    }

    #[test]
    fn row2_buttons_do_not_overlap_at_minimum_width_and_every_command_is_reachable() {
        for (w, dpi) in [(820, 96), (1100, 96), (1230, 144)] {
            let row = Row2::new(w, dpi);
            let mut seen = Vec::new();
            for (i, (command, a)) in row.buttons.iter().enumerate() {
                assert!(a.left >= 0 && a.right <= w, "{w}: {command:?} 가 창 밖");
                for (other, b) in &row.buttons[i + 1..] {
                    assert!(!a.overlaps(b), "{w}@{dpi}: {command:?} 와 {other:?} 가 겹친다");
                }
                assert_eq!(row.hit(a.left + 1, a.top + 1), Some(*command));
                seen.push(*command);
            }
            assert!(Command::ALL.iter().all(|c| seen.contains(c)));
            let v = ViewerLayout::new(w, 720, dpi);
            assert!(v.prev.left >= 0 && v.canvas.height() > 0);
        }
        assert!(!Command::View.enabled(2) && Command::Export.enabled(2) && !Command::Tags.enabled(0));
    }

    #[test]
    fn viewer_stops_at_the_ends_and_wraps_only_while_playing() {
        let mut v = Viewer::open(vec![doc(r"C:\a.jpg"), doc(r"C:\b.jpg")], 0).unwrap();
        assert!(!v.step(false));
        assert!(v.step(true) && v.index == 1);
        assert!(!v.step(true), "마지막에서 멈춘다");
        v.playing = true;
        assert!(v.step(true) && v.index == 0, "슬라이드쇼는 처음으로");
        assert!(Viewer::open(vec![], 0).is_none());
    }

    #[test]
    fn fit_keeps_ratio_and_never_enlarges() {
        let area = Rect::new(0, 0, 100, 100);
        assert_eq!(fit(400, 200, &area), Rect::new(0, 25, 100, 75));
        assert_eq!(fit(10, 10, &area), Rect::new(45, 45, 55, 55));
    }

    #[test]
    fn filters_combine_and_duplicates_keep_group_order() {
        let docs = [doc(r"C:\a.jpg"), doc(r"C:\b.jpg"), doc(r"C:\c.jpg")];
        let mut photo = Photo::default();
        assert!(photo.only(&docs).is_none());
        photo.marks.insert(key(r"C:\a.jpg"), Mark { favorite: true, tags: vec!["Site".into()] });
        photo.marks.insert(key(r"C:\b.jpg"), Mark { favorite: false, tags: vec!["site".into()] });
        photo.tag = Some("SITE".into());
        assert_eq!(photo.only(&docs).unwrap().rank.len(), 2);
        photo.favorites_only = true;
        assert!(photo.only(&docs).unwrap().rank.contains_key(r"C:\a.jpg"));
        assert_eq!(photo.only(&docs).unwrap().rank.len(), 1);

        let mut photo = Photo { duplicates: Some(vec![vec![r"C:\c.jpg".into(), r"C:\a.jpg".into()]]), ..Photo::default() };
        let only = photo.only(&docs).unwrap();
        assert!(only.ranked && only.rank[r"C:\c.jpg"] == 0 && only.rank[r"C:\a.jpg"] == 1 && only.rank.len() == 2);
        assert_eq!((photo.group_of(r"C:\a.jpg"), photo.group_of(r"C:\b.jpg")), (Some(1), None));

        let all: Vec<String> = docs.iter().map(|d| d.path.clone()).collect();
        photo.marks.insert(key(r"C:\a.jpg"), Mark { favorite: true, tags: vec!["x".into()] });
        assert_eq!(photo.favorite_flips(&all), all[1..], "아닌 것만 켠다");
        assert_eq!(photo.favorite_flips(&all[..1]), all[..1], "모두 켜져 있으면 끈다");
        assert_eq!(photo.tags_of(&all), ["x"]);
    }

    #[test]
    fn renamed_photo_stays_in_its_duplicate_group() {
        let mut photo = Photo { duplicates: Some(vec![vec![r"C:\a.jpg".into(), r"C:\b.jpg".into()]]), ..Photo::default() };
        let result = FileResult { source: doc(r"C:\b.jpg"), target: Some(doc(r"C:\새 이름.jpg")), error: None };
        photo.follow_renames(Operation::Copy, std::slice::from_ref(&result));
        assert_eq!(photo.group_of(r"C:\b.jpg"), Some(1), "복사는 원본이 남는다");
        photo.follow_renames(Operation::Rename, &[result]);
        assert_eq!((photo.group_of(r"C:\새 이름.jpg"), photo.group_of(r"C:\b.jpg")), (Some(1), None));
    }

    #[test]
    fn edit_input_is_validated() {
        assert_eq!(parse_edit(EDIT_DEFAULT).unwrap(), EditSpec { quarter_turns: 1, brightness: 1.0, contrast: 1.0, crop: None });
        assert_eq!(parse_edit("-90, 1.5, 0.5, 0, 0, 10, 20").unwrap(), EditSpec { quarter_turns: 3, brightness: 1.5, contrast: 0.5, crop: Some([0, 0, 10, 20]) });
        for bad in ["", "45 1 1", "0 0 1", "0 1 6", "0 1 1 5 5 5 9", "0 1 1 2", "a 1 1", "0 nan 1"] {
            assert!(parse_edit(bad).is_err(), "{bad}");
        }
    }
}
