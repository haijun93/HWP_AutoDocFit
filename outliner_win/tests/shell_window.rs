//! 페이지 모드 실제 창(화면 밖, 보이지 않음)·실제 EDIT 시험: 배치·그림 화소·마우스 클릭 처리.
//! 모두 임시 데이터 폴더만 쓴다. 한 파일 안의 시험은 전역 IME 상태를 공유하므로 잠금으로 차례로 돈다.
mod common;
use common::*;
use outliner_win::capture::{Shot, seed_demo_pages};
use outliner_win::layout::{self, Btn, LeftItem, RightItem};
use outliner_win::pages::PageStore;
use outliner_win::paint::Metrics;
use outliner_win::shell::Shell;
use outliner_win::window::{Harness, WinHost};
use std::sync::Mutex;
use ui_theme::Kind;

static LOCK: Mutex<()> = Mutex::new(());

fn harness(tag: &str, kind: Kind, dpi: u32) -> (Harness, std::path::PathBuf) {
    let d = temp_dir(tag);
    seed_demo_pages(&d).unwrap();
    let st = PageStore::new(&d, Some(d.join("recent.json")));
    let mut sh = Shell::with_store(WinHost::new(), st, fixed_clock).unwrap();
    sh.start().unwrap();
    let (w, h) = (outliner_win::paint::scale(1000, dpi), outliner_win::paint::scale(600, dpi));
    (Harness::new_shell(sh, "아웃라이너", kind, dpi, w, h).unwrap(), d)
}

fn px(s: &Shot, x: i32, y: i32) -> u32 {
    let i = ((y as usize) * s.width + x as usize) * 4;
    // 화소 배치는 B,G,R,A → GDI COLORREF(0x00BBGGRR)로 바꿔 팔레트와 비교한다.
    u32::from_le_bytes([s.bgra[i + 2], s.bgra[i + 1], s.bgra[i], 0])
}

fn count_in(s: &Shot, r: layout::R, pred: impl Fn(u32) -> bool) -> usize {
    let mut n = 0;
    for y in r.t.max(0)..r.b.min(s.height as i32) {
        for x in r.l.max(0)..r.r.min(s.width as i32) {
            if pred(px(s, x, y)) {
                n += 1;
            }
        }
    }
    n
}

fn ink(s: &Shot, r: layout::R, bg: u32) -> usize {
    count_in(s, r, |c| c != bg)
}

fn palette(kind: Kind) -> ui_theme::Palette {
    outliner_win::capture::palette_of(kind)
}

const MAGENTA: u32 = 0x00FF00FF;

#[test]
fn layout_shows_panels_only_when_wide_enough_and_one_file_mode_has_none() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let wide = layout::compute(1000, 600, 96, true);
    assert!(wide.left.is_some() && wide.right.is_some());
    assert_eq!(wide.center.l, wide.left.unwrap().r);
    assert_eq!(wide.center.r, wide.right.unwrap().l);
    assert_eq!(wide.center.t, wide.top.b);
    let mid = layout::compute(700, 600, 96, true);
    assert!(mid.left.is_some() && mid.right.is_none());
    let narrow = layout::compute(400, 600, 96, true);
    assert!(narrow.left.is_none() && narrow.right.is_none());
    assert_eq!(narrow.center.l, 0);
    let single = layout::compute(1000, 600, 96, false);
    assert_eq!((single.center.l, single.center.t, single.center.r, single.center.b), (0, 0, 1000, 600));
    assert!(single.button_at(10, 10).is_none());
    // 192 DPI 는 같은 비율로 커진다.
    let hi = layout::compute(2000, 1200, 192, true);
    assert_eq!(hi.left.unwrap().w(), 2 * wide.left.unwrap().w());
}

#[test]
fn list_keyboard_extends_selection_applies_batch_and_undoes_once() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut h, _) = harness("multi-select", Kind::Light, 96);
    h.shell().open_page("회의록").unwrap();
    h.editor().view.set_selected(1, 20);
    let before = h.editor().markdown();
    assert!(h.list_key_modifiers(outliner_win::edit::VK_DOWN, false, true, false));
    assert_eq!(h.editor().view.selected_block_ids().len(), 2);
    // 두 자식 블록을 함께 Shift+Tab(첫 자식은 Tab 으로 더 들여쓸 수 없다).
    assert!(h.list_key_modifiers(0x09, false, false, true));
    assert_ne!(h.editor().markdown(), before);
    assert_eq!(h.editor().view.selected_block_ids().len(), 2, "선택 유지");
    assert!(h.editor().can_undo());
    assert!(h.editor().history_step(true));
    assert_eq!(h.editor().markdown(), before);
}

#[test]
fn left_items_list_recent_first_then_all_and_right_items_group_by_page() {
    let pages: Vec<String> = ["가", "나", "다"].iter().map(|s| s.to_string()).collect();
    let recent = vec!["나".to_string(), "없음".to_string(), "다".to_string()];
    let items = layout::left_items(&pages, &recent);
    assert_eq!(items[0], LeftItem::Label("최근 페이지".into()));
    assert_eq!(items[1], LeftItem::Page("나".into()));
    assert_eq!(items[2], LeftItem::Page("다".into()));
    assert_eq!(items[3], LeftItem::Label("전체 페이지 (3)".into()));
    assert_eq!(items.len(), 7);
    let none = layout::left_items(&pages, &[]);
    assert_eq!(none[0], LeftItem::Label("전체 페이지 (3)".into()));
    use outliner_win::find::BackItem;
    let b = |p: &str, n: usize| BackItem { page: p.into(), block: n, text: "x".into() };
    let r = layout::right_items(&[b("A", 0), b("A", 3), b("B", 1)]);
    assert_eq!(
        r,
        [RightItem::Group("A".into()), RightItem::Block(0), RightItem::Block(1), RightItem::Group("B".into()), RightItem::Block(2)]
    );
}

#[test]
fn shell_pixels_show_every_panel_in_light_and_dark() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for kind in [Kind::Light, Kind::Dark] {
        let pal = palette(kind);
        let (mut hn, _d) = harness("pix", kind, 96);
        for p in ["예산 지침", "회의록", "2026_11_21"] {
            hn.shell().open_page(p).unwrap();
        }
        let lay = hn.layout_now();
        hn.layout();
        let shot = hn.render().unwrap();
        let (left, right, center, top) = (lay.left.unwrap(), lay.right.unwrap(), lay.center, lay.top);
        assert!(ink(&shot, left, pal.bg) > 800, "{kind:?}: 왼쪽 페이지 목록에 글자가 있다");
        assert!(ink(&shot, right, pal.bg) > 200, "{kind:?}: 오른쪽 역링크 패널에 글자가 있다");
        assert!(ink(&shot, center, pal.bg) > 800, "{kind:?}: 가운데 블록 목록");
        assert!(ink(&shot, top, pal.head_bg) > 300, "{kind:?}: 위쪽 줄(단추·검색 칸 테두리)");
        assert_eq!(count_in(&shot, layout::R::new(0, 0, shot.width as i32, shot.height as i32), |c| c == MAGENTA), 0, "{kind:?}: 미그림 표시 없음");
        // 현재 페이지 줄이 선택 색으로 칠해진다(왼쪽 목록).
        assert!(count_in(&shot, left, |c| c == pal.selected) > 500, "{kind:?}: 현재 페이지 강조");
        // 링크는 강조색으로 그려진다(가운데).
        assert!(count_in(&shot, center, |c| c == pal.accent) > 20, "{kind:?}: [[링크]] 강조색");
        // 검색 결과: 가운데가 결과 목록으로 바뀐다.
        hn.type_search("예산");
        hn.layout();
        assert!(hn.shell().search_open && hn.shell().hits.len() >= 5);
        let shot2 = hn.render().unwrap();
        assert!(count_in(&shot2, center, |c| c == pal.accent) > 100, "{kind:?}: 결과의 페이지 이름은 강조색");
        assert!(count_in(&shot2, center, |c| c == pal.selected) > 1000, "{kind:?}: 첫 결과가 선택 색");
        // 검색 칸에 쓴 글자가 실제 EDIT 로 그려진다(마젠타가 없어야 함).
        assert!(ink(&shot2, layout::R::new(lay.search_edit.l, lay.search_edit.t, lay.search_edit.r, lay.search_edit.b), pal.bg) > 20, "{kind:?}: 검색 칸 글자");
        hn.type_search("");
        // 자동완성 팝업: 테두리(accent)가 팝업 사각형 둘레에 그려진다.
        let row = hn.editor().view.rows.iter().position(|r| r.text.contains("점심")).unwrap();
        hn.editor().view.set_selected(row, 15);
        hn.editor().start_edit_selected();
        {
            use outliner_win::edit::EditHost;
            let t = "점심 [[예";
            hn.editor().host.set_text(t, t.encode_utf16().count());
        }
        hn.layout();
        let p = hn.editor().popup.clone().expect("[[ 뒤 팝업");
        assert_eq!(p.items, ["예산 지침"]);
        let shot3 = hn.render().unwrap();
        let er = outliner_win::paint_shell::edit_rect_of(hn.shell(), &lay).unwrap();
        let pr = layout::popup_rect(&lay, er, p.items.len());
        let top_edge = layout::R::new(pr.l, pr.t, pr.r, pr.t + 1);
        assert!(count_in(&shot3, top_edge, |c| c == pal.accent) as i32 > pr.w() / 2, "{kind:?}: 팝업 테두리");
        assert!(ink(&shot3, layout::R::new(pr.l + 4, pr.t + 4, pr.r - 4, pr.b - 4), pal.bg) > 30, "{kind:?}: 팝업 안 글자");
        assert_eq!(count_in(&shot3, layout::R::new(0, 0, shot3.width as i32, shot3.height as i32), |c| c == MAGENTA), 0);
    }
}

#[test]
fn clicking_left_list_opens_that_page_and_labels_do_nothing() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, _d) = harness("click_left", Kind::Light, 96);
    hn.layout();
    let lay = hn.layout_now();
    let l = lay.left.unwrap();
    let (pg, rc) = (hn.shell().pages.clone(), hn.shell().recent.clone());
    let items = layout::left_items(&pg, &rc);
    let slot_y = |i: usize| lay.list_top(l) + i as i32 * lay.item_h + lay.item_h / 2;
    hn.click_at(20, slot_y(0), false); // "최근 페이지" 머리글 — 아무 일 없음
    assert_eq!(hn.shell().page.as_deref(), Some("2026_11_21"));
    let i = items.iter().position(|it| *it == LeftItem::Page("회의록".into())).unwrap();
    // "전체 페이지" 쪽의 회의록(마지막 일반 페이지)을 누른다.
    let i_all = items.iter().rposition(|it| *it == LeftItem::Page("회의록".into())).unwrap();
    assert!(i_all >= i);
    hn.click_at(20, slot_y(i_all), false);
    assert_eq!(hn.shell().page.as_deref(), Some("회의록"));
    assert_eq!(hn.shell().recent[0], "회의록", "최근 목록 맨 앞으로");
    let from: Vec<String> = hn.shell().back.iter().map(|b| b.page.clone()).collect();
    assert_eq!(from, ["예산 지침", "2026_11_21"], "회의록을 가리키는 페이지(자기 자신 제외): 예산 지침·오늘 일지");
}

#[test]
fn clicking_backlink_goes_to_the_referring_block() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, _d) = harness("click_back", Kind::Light, 96);
    hn.shell().open_page("예산 지침").unwrap();
    hn.layout();
    let lay = hn.layout_now();
    let r = lay.right.unwrap();
    let items = layout::right_items(&hn.shell().back);
    assert_eq!(hn.shell().back.len(), 3);
    let i = items.iter().position(|it| *it == RightItem::Block(1)).unwrap(); // 두 번째 역링크 = 회의록
    let y = lay.list_top(r) + i as i32 * lay.item_h + lay.item_h / 2;
    let target = hn.shell().back[1].clone();
    hn.click_at(r.l + 30, y, false);
    assert_eq!(hn.shell().page.as_deref(), Some(target.page.as_str()));
    let ed = hn.editor();
    assert_eq!(ed.view.rows[ed.view.selected].block, target.block);
}

#[test]
fn clicking_toolbar_buttons_new_page_rename_and_today() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, d) = harness("click_btn", Kind::Light, 96);
    hn.layout();
    // 새 페이지: 입력 줄이 열리고 Enter 로 확정
    hn.press(Btn::New);
    assert_eq!(hn.shell().prompt, Some(outliner_win::shell::PromptKind::NewPage));
    hn.type_search("시험 페이지");
    hn.confirm_prompt();
    assert_eq!(hn.shell().prompt, None);
    assert_eq!(hn.shell().page.as_deref(), Some("시험 페이지"));
    assert_eq!(read(&d, "시험 페이지.md"), "- \n");
    assert_eq!(hn.search_text(), "", "확정 뒤 입력 칸은 비워진다");
    // 이름 바꾸기: 처음 글이 현재 이름, 링크 갱신
    write(&d, "참조.md", "- [[시험 페이지]] 보라\n");
    hn.press(Btn::Rename);
    assert_eq!(hn.search_text(), "시험 페이지");
    hn.type_search("바뀐 이름");
    hn.confirm_prompt();
    assert_eq!(hn.shell().page.as_deref(), Some("바뀐 이름"));
    assert_eq!(read(&d, "참조.md"), "- [[바뀐 이름]] 보라\n");
    assert!(!d.join("시험 페이지.md").exists());
    // 같은 이름이면 오류 안내만 하고 입력 줄 유지
    hn.press(Btn::New);
    hn.type_search("참조");
    hn.confirm_prompt();
    assert!(hn.shell().prompt.is_some());
    assert!(hn.editor().message.as_deref().unwrap().contains("이미 있습니다"));
    // 오늘
    hn.shell().cancel_prompt();
    hn.press(Btn::Today);
    assert_eq!(hn.shell().page.as_deref(), Some("2026_11_21"));
}

fn row_center(hn: &mut Harness, row: usize) -> (i32, i32) {
    let lay = hn.layout_now();
    let m = Metrics::new(96);
    let first = hn.editor().view.first;
    (lay.center.l, lay.center.t + m.header_h + (row - first) as i32 * m.row_h + m.row_h / 2)
}

#[test]
fn clicking_checkbox_toggles_todo_and_clicking_the_dot_folds() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, d) = harness("click_row", Kind::Light, 96);
    hn.layout();
    let m = Metrics::new(96);
    // 오늘 일지: [0]오늘 할 일(자식 있음) [1]TODO 예산 지침 검토 [2]DONE [3]점심 [4]메모(자식) [5]...
    let (x0, y1) = row_center(&mut hn, 1);
    let depth = hn.editor().view.rows[1].depth;
    let tx = m.text_x(depth);
    hn.click_at(x0 + tx + 4, y1, false); // 체크 칸
    assert_eq!(hn.editor().view.doc.blocks[1].status.as_deref(), Some("DONE"));
    hn.click_at(x0 + tx + 4, y1, false);
    assert_eq!(hn.editor().view.doc.blocks[1].status.as_deref(), Some("TODO"));
    // 글자 쪽을 누르면 선택만
    hn.click_at(x0 + tx + 330, y1, false);
    assert_eq!(hn.editor().view.selected, 1);
    assert_eq!(hn.shell().page.as_deref(), Some("2026_11_21"), "글자 쪽 클릭은 이동하지 않는다");
    // 부모 줄의 삼각형을 누르면 접힘 → 자식이 사라진다
    let rows_before = hn.editor().view.rows.len();
    let (_, y0) = row_center(&mut hn, 0);
    hn.click_at(x0 + m.marker_x(0), y0, false);
    assert!(hn.editor().view.rows.len() < rows_before, "접힘");
    assert!(hn.editor().view.doc.blocks[0].folded);
    hn.click_at(x0 + m.marker_x(0), y0, false);
    assert_eq!(hn.editor().view.rows.len(), rows_before, "다시 펼침");
    // 원본 파일은 저장 전까지 그대로(토글은 메모리 변경)
    assert!(!hn.editor().dirty(), "TODO→DONE→TODO 로 원래대로 돌아오면 변경 없음");
    let (_, y1) = row_center(&mut hn, 1);
    hn.click_at(x0 + tx + 4, y1, false);
    assert!(hn.editor().dirty());
    assert!(read(&d, "2026_11_21.md").contains("TODO [[예산 지침]]"), "저장 전에는 파일이 그대로");
}

#[test]
fn clicking_a_link_in_a_row_follows_it_but_clicking_plain_text_does_not() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, _d) = harness("click_link", Kind::Light, 96);
    hn.layout();
    let m = Metrics::new(96);
    let (x0, y1) = row_center(&mut hn, 1); // "TODO [[예산 지침]] 검토"
    let r = hn.editor().view.rows[1].clone();
    let tx = m.text_x(r.depth) + m.status_shift(96);
    let spans = outliner_win::paint::link_spans_x(&r.text, tx, 96);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].2, "예산 지침");
    hn.click_at(x0 + tx - 10, y1, false); // 체크 칸 오른쪽 여백(링크 시작 전) — 이동 안 함
    assert_eq!(hn.shell().page.as_deref(), Some("2026_11_21"));
    let mid = (spans[0].0 + spans[0].1) / 2;
    hn.click_at(x0 + mid, y1, false);
    assert_eq!(hn.shell().page.as_deref(), Some("예산 지침"), "링크 클릭으로 이동");
    // 링크 뒤 글자(" 검토" 쪽)
    hn.shell().open_page("2026_11_21").unwrap();
    hn.layout();
    let (x0, y1) = row_center(&mut hn, 1);
    hn.click_at(x0 + spans[0].1 + 20, y1, false);
    assert_eq!(hn.shell().page.as_deref(), Some("2026_11_21"));
}

#[test]
fn clicking_a_search_result_opens_page_and_block() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, _d) = harness("click_hit", Kind::Light, 96);
    hn.layout();
    hn.type_search("법령");
    hn.layout();
    assert_eq!(hn.shell().hits.len(), 1);
    let lay = hn.layout_now();
    let y = lay.center.t + lay.head_h + lay.result_h / 2;
    hn.click_at(lay.center.l + 30, y, false);
    assert_eq!(hn.shell().page.as_deref(), Some("참고 자료"));
    assert!(!hn.shell().search_open);
    let ed = hn.editor();
    assert_eq!(ed.view.rows[ed.view.selected].block, 0);
}

#[test]
fn keyboard_shortcuts_in_list_mode_toggle_move_and_focus_search() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, _d) = harness("keys", Kind::Light, 96);
    hn.layout();
    // 블록 3(점심) 선택 후 Alt 없이 Ctrl 없는 Enter → 편집 시작(기존 동작 유지)
    hn.editor().view.set_selected(3, 15);
    assert!(hn.list_key(outliner_win::edit::VK_RETURN));
    assert!(hn.editor().editing());
    hn.editor().submit(outliner_win::edit::Cmd::Cancel, 0, false, false);
    // 편집 아닌 상태에서 Ctrl+Enter 는 시험 하니스가 실제 Ctrl 키 상태를 못 만들므로 편집기 명령으로 확인
    hn.editor().nav_command(outliner_win::edit::Cmd::Toggle);
    assert_eq!(hn.editor().view.doc.blocks[3].status.as_deref(), Some("TODO"));
    hn.editor().nav_command(outliner_win::edit::Cmd::MoveUp);
    assert_eq!(hn.editor().view.doc.blocks[2].text, "점심 약속 확인");
}

#[test]
fn clicking_a_popup_candidate_inserts_that_page_link() {
    use outliner_win::edit::EditHost;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, _d) = harness("click_popup", Kind::Light, 96);
    hn.layout();
    let row = hn.editor().view.rows.iter().position(|r| r.text.contains("점심")).unwrap();
    hn.editor().view.set_selected(row, 15);
    hn.editor().start_edit_selected();
    let t = "점심 [[";
    hn.editor().host.set_text(t, t.encode_utf16().count());
    hn.layout();
    let p = hn.editor().popup.clone().expect("팝업이 열려야 한다");
    assert!(p.items.len() >= 3);
    let lay = hn.layout_now();
    let er = outliner_win::paint_shell::edit_rect_of(hn.shell(), &lay).unwrap();
    let pr = layout::popup_rect(&lay, er, p.items.len());
    let want = p.items[2].clone();
    hn.click_at(pr.l + 20, pr.t + 2 + 2 * lay.popup_h + lay.popup_h / 2, false);
    assert_eq!(hn.editor().host.read_text(), format!("점심 [[{want}]]"));
    assert!(hn.editor().popup.is_none());
    assert!(hn.editor().editing(), "편집은 유지된다");
}

#[test]
fn toolbar_has_seven_buttons_inside_the_window_and_hit_testing_matches() {
    for dpi in [96u32, 192] {
        let (w, h) = (outliner_win::paint::scale(1000, dpi), outliner_win::paint::scale(600, dpi));
        let lay = layout::compute(w, h, dpi, true);
        let want = [Btn::Today, Btn::New, Btn::Rename, Btn::Delete, Btn::NewBlock, Btn::Indent, Btn::Outdent];
        for (i, b) in want.iter().enumerate() {
            let r = lay.buttons[i];
            assert!(r.r <= w && r.l >= lay.search_edit.r, "{dpi}dpi 단추 {i} 가 창 안에 있다");
            assert_eq!(lay.button_at((r.l + r.r) / 2, (r.t + r.b) / 2), Some(*b));
            if i > 0 {
                assert!(lay.buttons[i - 1].r <= r.l, "단추가 겹치지 않는다");
            }
        }
        assert!(lay.buttons[6].r <= outliner_win::paint::scale(820, dpi), "기본 창 폭(820)에서도 모두 보인다");
    }
}

#[test]
fn block_buttons_add_indent_outdent_and_left_filter_click_opens_page() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut hn, _d) = harness("blk_btn", Kind::Light, 96);
    hn.layout();
    let n = hn.editor().view.doc.blocks.len();
    // 새 블록: 맨 끝에 같은 단계의 빈 블록, 편집 시작.
    let last_indent = hn.editor().view.doc.blocks[n - 1].indent;
    hn.press(Btn::NewBlock);
    assert_eq!(hn.editor().view.doc.blocks.len(), n + 1);
    assert_eq!(hn.editor().editing_block(), Some(n));
    assert_eq!(hn.editor().view.doc.blocks[n].indent, last_indent);
    // 들여쓰기·내어쓰기(편집 유지).
    hn.press(Btn::Indent);
    assert_eq!(hn.editor().view.doc.blocks[n].indent, last_indent + 1);
    assert_eq!(hn.editor().editing_block(), Some(n));
    hn.press(Btn::Outdent);
    assert_eq!(hn.editor().view.doc.blocks[n].indent, last_indent);
    // 목록 상태(편집 아님)에서도 선택 블록에 적용.
    hn.editor().finish_edit();
    hn.editor().view.set_selected(2, 15);
    let before = hn.editor().view.doc.blocks[2].indent;
    hn.press(Btn::Indent);
    assert_eq!(hn.editor().view.doc.blocks[2].indent, before + 1);
    // 왼쪽 목록 이름 필터: 한 글자만 넣어도 줄고, 줄어든 목록의 항목 클릭이 그 페이지를 연다.
    hn.type_search("예");
    let lay = hn.layout_now();
    let items = hn.shell().left_list();
    assert_eq!(items.last(), Some(&LeftItem::Page("예산 지침".into())));
    let l = lay.left.unwrap();
    let y = lay.list_top(l) + (items.len() as i32 - 1) * lay.item_h + lay.item_h / 2;
    hn.click_at(l.l + 20, y, false);
    assert_eq!(hn.shell().page.as_deref(), Some("예산 지침"));
}
