//! 퀵몽땅검색 창 순수 레이아웃 계산 및 테스트.
//! 모든 OS에서 단위 시험(cargo test)으로 검증한다.

use windows_sys::Win32::Foundation::RECT;

// ---- 기본 치수 (96 DPI 기준 논리 픽셀) ----
pub const HEAD_H: i32 = 32;
pub const CAPTION_W: i32 = 46;
pub const SEARCH_BOX_H: i32 = 32;
pub const TAB_H: i32 = 28;
pub const FILTER_H: i32 = 24;
pub const ROW_H: i32 = 70;
pub const ROW_FOLDER_W: i32 = 28;
pub const STATUS_H: i32 = 24;
pub const PREVIEW_H: i32 = 64;

/// 상단 캡션 단추.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeadButton {
    Minimize,
    Close,
}

pub const HEAD_BUTTONS: [HeadButton; 2] = [HeadButton::Minimize, HeadButton::Close];

/// 상단 캡션 단추의 가로 영역 [left, right) 계산.
pub fn head_button_span(b: HeadButton, width: i32, button_w: i32) -> (i32, i32) {
    let i = HEAD_BUTTONS.iter().position(|x| *x == b).unwrap_or(0) as i32;
    let from_right = HEAD_BUTTONS.len() as i32 - i;
    let left = width - from_right * button_w;
    (left, left + button_w)
}

/// 좌표 (x, y)가 어느 캡션 단추 위인지 판정.
pub fn head_button_hit(x: i32, y: i32, width: i32, button_w: i32, head: i32) -> Option<HeadButton> {
    if y < 0 || y >= head || x < 0 || x >= width {
        return None;
    }
    HEAD_BUTTONS.iter().copied().find(|b| {
        let (l, r) = head_button_span(*b, width, button_w);
        x >= l && x < r
    })
}

/// 단추 글리프 (Segoe MDL2 Assets).
pub fn head_button_glyph(b: HeadButton) -> char {
    match b {
        HeadButton::Minimize => '\u{E921}', // ChromeMinimize
        HeadButton::Close => '\u{E8BB}',    // ChromeClose
    }
}

/// 파일명 확장자에 따른 표시 꼬리표.
pub fn extension_tag(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "hwp" | "hwpx" => "HWP",
        "pdf" => "PDF",
        "doc" | "docx" => "DOC",
        "txt" => "TXT",
        "xls" | "xlsx" | "csv" => "XLS",
        "ppt" | "pptx" => "PPT",
        _ => "문서",
    }
}

/// 결과 목록 시작 Y 좌표 계산.
pub fn calc_list_top(
    head_h: i32,
    search_h: i32,
    tab_h: i32,
    filter_h: i32,
    has_autocomplete: bool,
    ac_h: i32,
) -> i32 {
    let base = head_h + search_h + tab_h + filter_h;
    if has_autocomplete {
        base + ac_h
    } else {
        base
    }
}

/// 탭 3개([본문], [위계], [파일명])의 RECT 계산.
pub fn calc_tab_rects(pad: i32, tab_y: i32, tab_h: i32, width: i32) -> [RECT; 3] {
    let avail = (width - 2 * pad).max(0);
    let tab_w = avail / 3;
    let l0 = pad;
    let r0 = l0 + tab_w;
    let l1 = r0;
    let r1 = l1 + tab_w;
    let l2 = r1;
    let r2 = width - pad;
    [
        RECT { left: l0, top: tab_y, right: r0, bottom: tab_y + tab_h },
        RECT { left: l1, top: tab_y, right: r1, bottom: tab_y + tab_h },
        RECT { left: l2, top: tab_y, right: r2, bottom: tab_y + tab_h },
    ]
}

/// 탭 히트 판정 (0: 본문, 1: 위계, 2: 파일명).
pub fn hit_tab(x: i32, y: i32, pad: i32, tab_y: i32, tab_h: i32, width: i32) -> Option<usize> {
    if y < tab_y || y >= tab_y + tab_h || x < pad || x >= width - pad {
        return None;
    }
    let rects = calc_tab_rects(pad, tab_y, tab_h, width);
    rects.iter().position(|r| x >= r.left && x < r.right)
}

/// 결과 행의 RECT 계산.
pub fn calc_row_rect(top_y: i32, index: usize, scroll_top: usize, row_h: i32, width: i32) -> RECT {
    let y = top_y + (index.saturating_sub(scroll_top)) as i32 * row_h;
    RECT {
        left: 0,
        top: y,
        right: width,
        bottom: y + row_h,
    }
}

/// 행 오른쪽 [폴더 열기] 단추 영역 계산.
pub fn calc_folder_button_rect(row_top: i32, row_right: i32, margin_x: i32, margin_y: i32, btn_w: i32, btn_h: i32) -> RECT {
    RECT {
        left: row_right - btn_w - margin_x,
        top: row_top + margin_y,
        right: row_right - margin_x,
        bottom: row_top + margin_y + btn_h,
    }
}

/// 폴더 열기 단추 히트 판정.
pub fn hit_folder_button(x: i32, y: i32, btn_rect: RECT) -> bool {
    x >= btn_rect.left && x < btn_rect.right && y >= btn_rect.top && y < btn_rect.bottom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_head_button_hit_layout() {
        let width = 700;
        let button_w = 46;
        let head = 32;

        // Minimize: [608, 654), Close: [654, 700)
        assert_eq!(head_button_hit(654, 10, width, button_w, head), Some(HeadButton::Close));
        assert_eq!(head_button_hit(699, 10, width, button_w, head), Some(HeadButton::Close));
        assert_eq!(head_button_hit(608, 10, width, button_w, head), Some(HeadButton::Minimize));
        assert_eq!(head_button_hit(653, 10, width, button_w, head), Some(HeadButton::Minimize));

        // 단추 밖
        assert_eq!(head_button_hit(607, 10, width, button_w, head), None);
        assert_eq!(head_button_hit(100, 10, width, button_w, head), None);
        assert_eq!(head_button_hit(660, 32, width, button_w, head), None);
        assert_eq!(head_button_hit(660, -1, width, button_w, head), None);
    }

    #[test]
    fn test_head_button_glyphs() {
        assert_eq!(head_button_glyph(HeadButton::Minimize), '\u{E921}');
        assert_eq!(head_button_glyph(HeadButton::Close), '\u{E8BB}');
    }

    #[test]
    fn test_extension_tag_detection() {
        assert_eq!(extension_tag("보고서.hwp"), "HWP");
        assert_eq!(extension_tag("공문.hwpx"), "HWP");
        assert_eq!(extension_tag("결과.pdf"), "PDF");
        assert_eq!(extension_tag("계약서.docx"), "DOC");
        assert_eq!(extension_tag("참고.txt"), "TXT");
        assert_eq!(extension_tag("통계.xlsx"), "XLS");
        assert_eq!(extension_tag("데이터.csv"), "XLS");
        assert_eq!(extension_tag("발표.pptx"), "PPT");
        assert_eq!(extension_tag("기타.zip"), "문서");
        assert_eq!(extension_tag("확장자없음"), "문서");
    }

    #[test]
    fn test_calc_list_top_with_and_without_autocomplete() {
        let top_without_ac = calc_list_top(32, 44, 28, 32, false, 28);
        assert_eq!(top_without_ac, 136);

        let top_with_ac = calc_list_top(32, 44, 28, 32, true, 28);
        assert_eq!(top_with_ac, 164);
    }

    #[test]
    fn test_tab_rects_and_hits() {
        let pad = 12;
        let tab_y = 70;
        let tab_h = 28;
        let width = 700;

        let rects = calc_tab_rects(pad, tab_y, tab_h, width);
        assert_eq!(rects[0].left, 12);
        assert_eq!(rects[2].right, 688);
        assert!(rects[0].right == rects[1].left);
        assert!(rects[1].right == rects[2].left);

        // 첫 번째 탭 클릭
        assert_eq!(hit_tab(15, 80, pad, tab_y, tab_h, width), Some(0));
        // 두 번째 탭 클릭
        assert_eq!(hit_tab(300, 80, pad, tab_y, tab_h, width), Some(1));
        // 세 번째 탭 클릭
        assert_eq!(hit_tab(600, 80, pad, tab_y, tab_h, width), Some(2));
        // 탭 밖 클릭
        assert_eq!(hit_tab(5, 80, pad, tab_y, tab_h, width), None);
        assert_eq!(hit_tab(695, 80, pad, tab_y, tab_h, width), None);
        assert_eq!(hit_tab(300, 60, pad, tab_y, tab_h, width), None);
        assert_eq!(hit_tab(300, 100, pad, tab_y, tab_h, width), None);
    }

    #[test]
    fn test_row_rect_and_folder_button() {
        let top_y = 140;
        let row_h = 70;
        let width = 700;

        let r0 = calc_row_rect(top_y, 0, 0, row_h, width);
        assert_eq!(r0.top, 140);
        assert_eq!(r0.bottom, 210);
        assert_eq!(r0.left, 0);
        assert_eq!(r0.right, 700);

        let r1 = calc_row_rect(top_y, 1, 0, row_h, width);
        assert_eq!(r1.top, 210);
        assert_eq!(r1.bottom, 280);

        // 스크롤 시
        let r1_scrolled = calc_row_rect(top_y, 1, 1, row_h, width);
        assert_eq!(r1_scrolled.top, 140);

        // 폴더 열기 단추
        let btn_rc = calc_folder_button_rect(r0.top, r0.right, 6, 4, 28, 22);
        assert_eq!(btn_rc.left, 700 - 28 - 6);
        assert_eq!(btn_rc.right, 700 - 6);
        assert_eq!(btn_rc.top, 144);
        assert_eq!(btn_rc.bottom, 166);

        assert!(hit_folder_button(670, 150, btn_rc));
        assert!(!hit_folder_button(660, 150, btn_rc));
        assert!(!hit_folder_button(670, 140, btn_rc));
    }

    #[test]
    fn test_frame_hit_integration() {
        use crate::frame::Hit;
        // 크기 조절 가장자리(6px)와 상단 캡션(32px) 판정
        let (w, h, grip, head) = (700, 510, 6, 32);
        let hit = |x: i32, y: i32, on_btn: bool| {
            crate::frame::frame_hit(x, y, w, h, grip, head, on_btn, false, false)
        };
        // 상단 캡션 단추 위면 HTCLIENT
        assert_eq!(hit(680, 10, true), Hit::Client);
        // 상단 제목 영역은 HTCAPTION
        assert_eq!(hit(100, 10, false), Hit::Caption);
        // 좌측/우측/하단 모서리 크기 조절
        assert_eq!(hit(2, 200, false), Hit::Left);
        assert_eq!(hit(698, 200, false), Hit::Right);
        assert_eq!(hit(350, 508, false), Hit::Bottom);
        assert_eq!(hit(698, 508, false), Hit::BottomRight);
    }

    #[test]
    fn test_preview_layout_and_visible_count() {
        let bottom = 600;
        let top_y = 140;
        let status_h = 24;
        let rh = 70;

        // 선택 없을 때: preview_h = 0
        let visible_without_sel = ((bottom - top_y - status_h).max(0) / rh).max(0) as usize;
        assert_eq!(visible_without_sel, (600 - 140 - 24) / 70); // 436 / 70 = 6

        // 선택 있을 때: preview_h = PREVIEW_H (64)
        let preview_h = PREVIEW_H;
        let visible_with_sel = ((bottom - top_y - status_h - preview_h).max(0) / rh).max(0) as usize;
        assert_eq!(visible_with_sel, (600 - 140 - 24 - 64) / 70); // 372 / 70 = 5

        // 미리보기 영역 상단 및 하단 좌표
        let prev_y = bottom - status_h - preview_h;
        assert_eq!(prev_y, 600 - 24 - 64); // 512
    }
}
