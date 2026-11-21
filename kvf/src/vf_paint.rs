//! 퀵가상폴더 그리기. 창 핸들이 아니라 `Scene`(보이는 상태)과 `Chrome`(vf_ui의 사각형)만 받으므로
//! 실제 창과 시험용 화면 저장(vf_capture)이 같은 함수를 쓴다.
use crate::{
    frame,
    util::wide,
    vf_data::{BoxRow, Item},
    vf_layout::Layout,
    vf_store::VirtualItem,
    vf_ui::{self, Chrome, PanelGeom},
};
use windows_sys::Win32::{
    Foundation::RECT,
    Graphics::Gdi::*,
};

pub const SPACES: [Option<&str>; 5] = [None, Some("core"), Some("basis"), Some("reference"), Some("temp")];
pub const LABELS: [&str; 5] = ["전체", "핵심자료", "기초자료", "참고자료", "임시저장소"];
const NOTE_TEXT: &str = "5,000개 초과 생략";
pub const TOOLBAR_LABELS: [&str; 4] = ["영역 ▾", "목록", "큰 아이콘", "새로고침"];

/// 항목 종류 꼬리표(한글 화면에 영어 꼬리표가 섞이지 않게). 모르는 종류는 그대로 보인다.
pub fn kind_label(kind: &str) -> &str {
    match kind {
        "file" => "파일",
        "folder" => "폴더",
        "link" => "링크",
        other => other,
    }
}

pub fn window_title(name: &str) -> String {
    format!("퀵가상폴더 · {name} ▾")
}

/// 한 줄(또는 한 칸)이 가리키는 항목.
#[derive(Clone, Copy, Debug)]
pub enum RowRef<'a> {
    Library(&'a Item),
    Virtual(&'a VirtualItem),
}

/// 그리는 순간의 모든 입력.
pub struct Scene<'a> {
    pub dpi: i32,
    pub title: String,
    pub sticky: bool,
    pub layout: &'a Layout,
    pub boxes: &'a [BoxRow],
    pub items: &'a [Item],
    pub filtered: &'a [i64],
    /// 영역 번호별 가상 항목.
    pub virt: &'a [Vec<VirtualItem>],
    pub top: usize,
    pub query: String,
    pub message: String,
    pub selected_id: Option<i64>,
    pub truncated: bool,
    pub last_operation: &'a str,
    pub drag_target: Option<&'a str>,
    pub context_box: Option<i64>,
    pub context_space: Option<&'a str>,
}
impl<'a> Scene<'a> {
    /// 알림 문구가 있으면 자료함 항목 줄은 숨긴다(불러오는 중·오류·빈 상자 문구와 겹치지 않게).
    pub fn library_len(&self) -> usize {
        if self.message.is_empty() { self.filtered.len() } else { 0 }
    }
    pub fn rows_len(&self, panel: usize) -> usize {
        self.library_len() + self.virt.get(panel).map_or(0, |v| v.len())
    }
    /// 영역 맨 위에 보이는 가상 항목 수(자료함 항목보다 앞에 둔다 — 끌어다 놓은 항목이 바로 보이게).
    pub fn virtual_len(&self, panel: usize) -> usize {
        self.virt.get(panel).map_or(0, |v| v.len())
    }
    pub fn row(&self, panel: usize, index: usize) -> Option<RowRef<'a>> {
        let virtual_len = self.virtual_len(panel);
        if index < virtual_len {
            self.virt.get(panel)?.get(index).map(RowRef::Virtual)
        } else {
            let id = self.filtered.get(index - virtual_len).filter(|_| self.library_len() > 0)?;
            self.items.iter().find(|i| i.id == *id).map(RowRef::Library)
        }
    }
    pub fn box_name(&self, id: Option<i64>) -> &str {
        id.and_then(|id| self.boxes.iter().find(|b| b.id == id)).map(|b| b.name.as_str()).unwrap_or("오늘의 업무")
    }
    pub fn space_label(space: Option<&str>) -> &'static str {
        space.and_then(|v| SPACES.iter().position(|x| *x == Some(v))).map(|i| LABELS[i]).unwrap_or("전체")
    }
    pub fn active_index(&self) -> usize {
        self.layout.panels.iter().position(|p| p.id == self.layout.active_panel).unwrap_or(0)
    }
    pub fn panel_box(&self, index: usize) -> Option<i64> {
        self.layout.panels.get(index).and_then(|p| p.box_id).or(if index == self.active_index() { self.context_box } else { None })
    }
    pub fn panel_space(&self, index: usize) -> Option<&str> {
        self.layout.panels.get(index).and_then(|p| p.space.as_deref()).or(if index == self.active_index() { self.context_space } else { None })
    }
    pub fn address(&self) -> String {
        let a = self.active_index();
        format!("상자 › {} › 영역 › {}", self.box_name(self.panel_box(a)), Self::space_label(self.panel_space(a)))
    }
    pub fn status_line(&self) -> String {
        let active = self.active_index();
        let virtual_selected = self.layout.panels.get(active).map_or(0, |p| {
            p.selection.iter().filter(|id| self.virt.get(active).is_some_and(|v| v.iter().any(|i| i.id == **id))).count()
        });
        vf_ui::status_text(self.filtered.len(), usize::from(self.selected_id.is_some()) + virtual_selected, self.layout.panel_count, self.last_operation)
    }
}

fn r(c: &crate::vf_layout::Rect) -> RECT {
    RECT { left: c.left, top: c.top, right: c.right, bottom: c.bottom }
}

/// 그림 전체. `mem`은 `ch.client` 크기의 메모리 DC.
pub unsafe fn draw(mem: HDC, sc: &Scene, ch: &Chrome) {
    unsafe {
        let font = CreateFontW(-vf_ui::sc(sc.dpi, 14), 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET as u32, 0, 0, 0, 0, wide("맑은 고딕").as_ptr());
        let oldfont = SelectObject(mem, font);
        SetBkMode(mem, TRANSPARENT as i32);
        let fill = |rect: RECT, color| {
            let brush = CreateSolidBrush(color);
            FillRect(mem, &rect, brush);
            DeleteObject(brush);
        };
        let outline = |rect: RECT, color| {
            let brush = CreateSolidBrush(color);
            FrameRect(mem, &rect, brush);
            DeleteObject(brush);
        };
        let text = |value: &str, mut rect: RECT, flags| {
            if rect.right <= rect.left || rect.bottom <= rect.top {
                return;
            }
            let value = wide(value);
            DrawTextW(mem, value.as_ptr(), (value.len() - 1) as i32, &mut rect, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | flags);
        };
        let palette = ui_theme::palette();
        let s = |v: i32| vf_ui::sc(sc.dpi, v);
        let pad = s(vf_ui::PAD);
        fill(r(&ch.client), palette.bg);
        SetTextColor(mem, palette.text);

        // 제목줄
        fill(r(&ch.head), palette.plain_head_bg);
        text(&sc.title, r(&ch.title), DT_END_ELLIPSIS);
        for (index, label) in ["×", "□", "—", if sc.sticky { "◆" } else { "◇" }].iter().enumerate() {
            text(label, r(&ch.window_buttons[index]), DT_CENTER);
        }

        // 주소줄
        fill(r(&ch.address), palette.bg);
        text("위치:", r(&ch.address_label), DT_LEFT);
        text(&sc.address(), r(&ch.address_text), DT_END_ELLIPSIS);

        // 도구 막대
        fill(r(&ch.toolbar), palette.plain_head_bg);
        let icons_active = sc.layout.panels.get(sc.active_index()).is_some_and(|p| p.view == "icons");
        for (index, label) in TOOLBAR_LABELS.iter().enumerate() {
            let button = r(&ch.toolbar_buttons[index]);
            if (index == 1 && !icons_active) || (index == 2 && icons_active) {
                fill(button, palette.selected);
                SetTextColor(mem, palette.row_text(true, palette.text));
            }
            outline(button, palette.border);
            text(label, button, DT_CENTER | DT_END_ELLIPSIS);
            SetTextColor(mem, palette.text);
        }

        // 탭
        let active_space = sc.panel_space(sc.active_index()).map(str::to_owned);
        for (index, label) in LABELS.iter().enumerate() {
            let tab = ch.tabs[index];
            if SPACES[index] == active_space.as_deref() {
                fill(RECT { left: tab.left, top: tab.bottom - s(3), right: tab.right, bottom: tab.bottom }, palette.accent_line);
            }
            text(label, r(&tab), DT_CENTER | DT_END_ELLIPSIS);
        }
        if let Some(q) = &ch.query_row {
            text(&format!("검색: {} · {}개 · Esc로 지우기", sc.query, sc.filtered.len()), RECT { left: pad, top: q.top, right: q.right - pad, bottom: q.bottom }, DT_END_ELLIPSIS);
        }

        // 영역
        for split in &ch.splits {
            fill(r(split), palette.border);
        }
        let active = sc.active_index();
        for (index, geom) in ch.panels.iter().enumerate() {
            draw_panel(mem, sc, ch, index, geom, index == active, &fill, &outline, &text);
        }

        // 상태줄
        fill(r(&ch.status), palette.plain_head_bg);
        SetTextColor(mem, palette.text);
        let mut status_text = r(&ch.status_text);
        if let Some(note) = &ch.status_note {
            // 안내 칸을 고정 170px로 잡으면 좁은 창에서 상태 문구가 일찍 잘리므로 실제 글자 폭만 쓴다.
            let label = wide(NOTE_TEXT);
            let mut size = std::mem::zeroed();
            let measured = if GetTextExtentPoint32W(mem, label.as_ptr(), (label.len() - 1) as i32, &mut size) != 0 { size.cx + pad } else { 0 };
            let left = if measured > 0 { (note.right - measured).max(note.left) } else { note.left };
            text(NOTE_TEXT, RECT { left, top: note.top, right: note.right, bottom: note.bottom }, DT_END_ELLIPSIS | DT_RIGHT);
            status_text.right = (left - pad).max(status_text.left);
        }
        text(&sc.status_line(), status_text, DT_END_ELLIPSIS);
        frame::own_border(mem, r(&ch.client));
        SelectObject(mem, oldfont);
        DeleteObject(font);
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_panel(
    mem: HDC,
    sc: &Scene,
    _ch: &Chrome,
    index: usize,
    geom: &PanelGeom,
    is_active: bool,
    fill: &dyn Fn(RECT, u32),
    outline: &dyn Fn(RECT, u32),
    text: &dyn Fn(&str, RECT, u32),
) {
    unsafe {
        let palette = ui_theme::palette();
        let pad = vf_ui::sc(sc.dpi, vf_ui::PAD);
        let rect = r(&geom.rect);
        // Windows 11에서는 틀 테두리를 DWM이 그리므로 활성 영역 표시는 직접 그린다(영역이 둘 이상일 때만).
        if is_active && sc.layout.panel_count > 1 {
            outline(rect, palette.accent_line);
        }
        let panel_id = format!("p{}", index + 1);
        if sc.drag_target == Some(panel_id.as_str()) {
            // 놓을 영역: 활성 영역 테두리와 구별되도록 머리글을 강조색 바탕으로 칠한다.
            fill(r(&geom.header), palette.selected);
            outline(rect, palette.accent_line);
            outline(RECT { left: rect.left + 1, top: rect.top + 1, right: rect.right - 1, bottom: rect.bottom - 1 }, palette.accent_line);
            fill(RECT { left: rect.left, top: rect.top, right: rect.right, bottom: rect.top + vf_ui::sc(sc.dpi, 3).min(geom.rect.height()) }, palette.accent_line);
        }
        SetTextColor(mem, palette.text);
        text(
            &format!("{} › {}", sc.box_name(sc.panel_box(index)), Scene::space_label(sc.panel_space(index))),
            RECT { left: geom.header.left + pad, top: geom.header.top, right: geom.header.right - pad, bottom: geom.header.bottom },
            DT_END_ELLIPSIS,
        );
        // 머리글 아래 가는 줄(머리글과 목록 경계)
        if geom.header.height() > 1 {
            fill(RECT { left: geom.header.left, top: geom.header.bottom - 1, right: geom.header.right, bottom: geom.header.bottom }, palette.border);
        }
        if !sc.message.is_empty() {
            // 좁은 영역에서도 끝의 안내(F5 등)가 잘리지 않게 두 줄까지 접는다.
            let mut m = RECT { left: geom.message.left + pad, top: geom.message.top + vf_ui::sc(sc.dpi, 3), right: geom.message.right - pad, bottom: geom.message.bottom };
            if m.right > m.left && m.bottom > m.top {
                let value = wide(&sc.message);
                DrawTextW(mem, value.as_ptr(), (value.len() - 1) as i32, &mut m, DT_WORDBREAK | DT_NOPREFIX | DT_END_ELLIPSIS);
            }
        }
        let len = sc.rows_len(index);
        let panel_selection = sc.layout.panels.get(index).map(|p| p.selection.as_slice()).unwrap_or(&[]);
        for offset in 0..geom.capacity() {
            let Some(row) = sc.row(index, sc.top + offset) else { break };
            let _ = len;
            let slot = geom.slot(offset);
            let selected = match row {
                RowRef::Library(item) => is_active && sc.selected_id == Some(item.id),
                RowRef::Virtual(v) => panel_selection.iter().any(|id| *id == v.id),
            };
            let (name, line) = match row {
                RowRef::Library(item) => {
                    let category = SPACES.iter().position(|space| *space == Some(item.space.as_str())).unwrap_or(0);
                    (item.display_name.as_str(), format!("[{}] {} · {}", kind_label(&item.kind), item.display_name, LABELS[category]))
                }
                RowRef::Virtual(v) => (v.display_name.as_str(), format!("[가상] {}", v.display_name)),
            };
            if selected {
                fill(r(&slot), palette.selected);
            }
            SetTextColor(mem, palette.row_text(selected, palette.text));
            if geom.icons {
                let icon_h = vf_ui::sc(sc.dpi, vf_ui::ICON_H);
                text("▣", RECT { left: slot.left, top: slot.top, right: slot.right, bottom: (slot.top + icon_h).min(slot.bottom) }, DT_CENTER);
                // 이름은 두 줄까지 접어 앞 몇 글자만 보이는 문제를 줄인다.
                let mut nr = RECT { left: slot.left + pad, top: slot.top + icon_h, right: slot.right - pad, bottom: slot.bottom };
                if nr.right > nr.left && nr.bottom > nr.top {
                    let value = wide(name);
                    DrawTextW(mem, value.as_ptr(), (value.len() - 1) as i32, &mut nr, DT_CENTER | DT_WORDBREAK | DT_NOPREFIX | DT_END_ELLIPSIS | DT_EDITCONTROL);
                }
            } else {
                text(&line, RECT { left: slot.left + pad, top: slot.top, right: slot.right - pad, bottom: slot.bottom }, DT_END_ELLIPSIS);
            }
        }
        SetTextColor(mem, palette.text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn virtual_rows_come_first_then_library_rows_and_message_hides_library() {
        let c = crate::vf_capture::case("virtual_rows").unwrap();
        let mk = |message: &str| Scene {
            dpi: 96,
            title: String::new(),
            sticky: false,
            layout: &c.layout,
            boxes: &c.boxes,
            items: &c.items,
            filtered: &c.filtered,
            virt: &c.virt,
            top: 0,
            query: String::new(),
            message: message.into(),
            selected_id: None,
            truncated: false,
            last_operation: "",
            drag_target: None,
            context_box: None,
            context_space: None,
        };
        let s = mk("");
        assert!(matches!(s.row(0, 0), Some(RowRef::Virtual(_))));
        assert!(matches!(s.row(0, 2), Some(RowRef::Virtual(_))));
        assert!(matches!(s.row(0, 3), Some(RowRef::Library(i)) if i.id == 1));
        assert_eq!(s.rows_len(1), 3 + 12);
        assert!(s.row(0, 3 + 12).is_none());
        let m = mk("불러오는 중…");
        assert_eq!(m.rows_len(0), 3);
        assert!(m.row(0, 3).is_none());
    }
    #[test]
    fn kind_labels_are_korean_and_unknown_kinds_pass_through() {
        assert_eq!((kind_label("file"), kind_label("folder"), kind_label("link")), ("파일", "폴더", "링크"));
        assert_eq!(kind_label("기타"), "기타");
    }
    #[test]
    fn status_counts_virtual_selection_of_active_panel() {
        let c = crate::vf_capture::case("drag_target").unwrap();
        let s = Scene {
            dpi: 96, title: String::new(), sticky: false, layout: &c.layout, boxes: &c.boxes, items: &c.items,
            filtered: &c.filtered, virt: &c.virt, top: 0, query: String::new(), message: String::new(),
            selected_id: None, truncated: false, last_operation: "", drag_target: None, context_box: None, context_space: None,
        };
        assert_eq!(s.status_line(), "12개 항목 · 2개 선택 · 2개 영역");
    }
}
