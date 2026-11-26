//! OUTLINEUI3: 합성 그래프·UTF-16·IME 경계·실제 Win32 EDIT/그리기 경로.
mod common;
use common::*;
use outliner_win::edit::*;
use outliner_win::links::{self, RefKind};
use outliner_win::pages::PageStore;
use outliner_win::shell::{SearchMode, Shell, PromptKind};
use outliner_win::window::{Harness, WinHost};
use std::sync::Mutex;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static LOCK: Mutex<()> = Mutex::new(());

fn shell() -> Shell<Fake> {
    let d = temp_dir("ui3");
    write(&d, "회의록.md", "- 대상 예산 #업무\r\n- 입력\r\n");
    write(&d, "예산 계획.md", "- 다른 페이지의 예산 #재정\n  id:: stable-budget\n- 임시 블록\n");
    let mut sh = Shell::with_store(Fake::default(), PageStore::new(d, None), fixed_clock).unwrap();
    sh.open_page("회의록").unwrap();
    sh.editor.view.set_selected(1, 20);
    sh.editor.start_edit_selected();
    sh
}

fn type_popup(sh: &mut Shell<Fake>, text: &str) {
    sh.editor.host.type_text(text);
    sh.editor.refresh_popup();
}

#[test]
fn trigger_boundaries_code_urls_and_utf16_replacement() {
    for text in ["https://server/path", "단어#태그", "단어/명령", "`[[예산", "```\n#태그", "/two words"] {
        assert!(links::any_completion_context(text, text.encode_utf16().count()).is_none(), "{text}");
    }
    let text = "😀 한글 #예";
    let ctx = links::any_completion_context(text, text.encode_utf16().count()).unwrap();
    assert_eq!(ctx.kind, RefKind::Tag);
    let (new, caret) = links::apply_completion(text, &ctx, "예산 계획");
    assert_eq!(new, "😀 한글 #[[예산 계획]]");
    assert_eq!(caret, new.encode_utf16().count());
}

#[test]
fn slash_filters_korean_and_inserts_date_headings_and_reference_caret() {
    for (query, want, caret) in [
        ("/오늘", "2026-11-21", 10), ("/h2", "## ", 3),
        ("/page", "[[]]", 2), ("/block", "(())", 2),
    ] {
        let mut sh = shell();
        type_popup(&mut sh, query);
        assert!(sh.editor.accept_popup(None));
        assert_eq!(sh.editor.host.text, want);
        assert_eq!(sh.editor.host.sel, (caret, caret));
        if query == "/page" { assert_eq!(sh.editor.popup.as_ref().unwrap().ctx.kind, RefKind::Page); }
    }
}

#[test]
fn slash_status_is_real_core_status_and_undoes_once_with_input() {
    let mut sh = shell();
    let before = sh.editor.markdown();
    type_popup(&mut sh, "검토 /DONE");
    assert!(sh.editor.accept_popup(None));
    assert_eq!(sh.editor.view.doc.blocks[1].status.as_deref(), Some("DONE"));
    assert_eq!(sh.editor.host.text, "검토 ");
    assert!(sh.editor.finish_edit());
    assert!(sh.editor.markdown().contains("- DONE 검토 \r\n"));
    assert!(sh.editor.history_step(true));
    assert_eq!(sh.editor.markdown(), before);
}

#[test]
fn tags_use_graph_and_memory_and_stay_closed_after_accept() {
    let mut sh = shell();
    type_popup(&mut sh, "😀 #재");
    assert_eq!(sh.editor.popup.as_ref().unwrap().items, ["재정"]);
    assert!(sh.editor.accept_popup(None));
    assert_eq!(sh.editor.host.text, "😀 #재정");
    assert!(sh.editor.popup.is_none());
    type_popup(&mut sh, "😀 #업");
    assert!(sh.editor.popup.is_none(), "같은 태그 트리거는 Esc/완성 뒤 억제");
    type_popup(&mut sh, "입력 ");
    type_popup(&mut sh, "입력 #업");
    assert_eq!(sh.editor.popup.as_ref().unwrap().items, ["업무"]);
}

#[test]
fn block_candidates_show_text_and_cross_page_only_persistent_ids() {
    let mut sh = shell();
    type_popup(&mut sh, "((다른 페이지");
    let p = sh.editor.popup.as_ref().unwrap();
    assert_eq!(p.items, ["stable-budget"]);
    assert!(p.labels[0].contains("예산 계획 · 다른 페이지의 예산"));
    assert!(sh.editor.accept_popup(None));
    assert_eq!(sh.editor.host.text, "((stable-budget))");
    type_popup(&mut sh, "((임시 블록");
    assert!(sh.editor.popup.is_none());
}

#[test]
fn local_block_reference_allocates_persistent_id_and_survives_reparse() {
    let mut sh = shell();
    let before = sh.editor.markdown();
    type_popup(&mut sh, "((대상");
    assert!(sh.editor.accept_popup(None));
    let id = sh.editor.view.doc.blocks[0].properties.get("id").unwrap().clone();
    assert_eq!(id.len(), 36);
    assert_eq!(sh.editor.host.text, format!("(({id}))"));
    assert!(sh.editor.finish_edit());
    let doc = outliner_core::parse(&sh.editor.markdown()).unwrap();
    let resolved = outliner_core::resolve_block_ref(&doc, &id);
    assert_eq!(resolved.backlinks.len(), 1);
    assert!(sh.editor.history_step(true));
    assert_eq!(sh.editor.markdown(), before);
}

#[test]
fn stale_popup_selection_and_ime_click_leave_input_untouched() {
    let mut sh = shell();
    type_popup(&mut sh, "[[예산");
    sh.editor.host.type_text("[[회의");
    assert!(!sh.editor.accept_popup(None));
    assert_eq!(sh.editor.host.text, "[[회의");
    sh.editor.host.sel = (0, 2);
    assert!(!sh.editor.accept_popup(None));
    type_popup(&mut sh, "/h1");
    sh.editor.host.composing = true;
    assert!(!sh.editor.accept_popup(None));
    sh.editor.refresh_popup();
    assert_eq!(sh.editor.host.text, "/h1");
    assert_eq!(sh.editor.popup.as_ref().unwrap().ctx.query, "h1");
}

#[test]
fn ime_owns_arrows_escape_candidate_and_commit_key_needs_second_press() {
    for trigger in ["[[예", "((대상", "#재", "/h1"] {
        let mut sh = shell();
        type_popup(&mut sh, trigger);
        sh.editor.host.composing = true;
        assert_eq!(sh.editor.key(KeyEvent::new(VK_DOWN, 100)), KeyResult::Pass);
        assert_eq!(sh.editor.key(KeyEvent::new(VK_ESCAPE, 101)), KeyResult::Pass);
        sh.editor.host.candidate = true;
        assert_eq!(sh.editor.key(KeyEvent::process(VK_RETURN, 102)), KeyResult::Pass);
        sh.editor.host.candidate = false;
        assert_eq!(sh.editor.key(KeyEvent::new(VK_RETURN, 110)), KeyResult::Swallow);
        assert_eq!(sh.editor.host.text, trigger, "확정 키로 후보를 고르지 않는다");
        sh.editor.key(KeyEvent::new(VK_RETURN, 120));
        assert_eq!(sh.editor.host.text, trigger, "같은 물리 키의 후속 메시지 억제");
        sh.editor.key(KeyEvent::new(VK_RETURN, 210));
        assert_ne!(sh.editor.host.text, trigger);
        assert_eq!(sh.editor.view.doc.blocks.len(), 2);
    }
}

#[test]
fn ctrl_n_p_navigate_popup_and_escape_suppresses_only_same_kind() {
    let mut sh = shell();
    type_popup(&mut sh, "/");
    let mut key = KeyEvent::new(b'N' as u16, 100); key.ctrl = true;
    assert_eq!(sh.editor.key(key), KeyResult::Swallow);
    assert_eq!(sh.editor.popup.as_ref().unwrap().selected, 1);
    key.vk = b'P' as u16;
    sh.editor.key(key);
    assert_eq!(sh.editor.popup.as_ref().unwrap().selected, 0);
    sh.editor.dismiss_popup();
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_none());
    type_popup(&mut sh, "#재");
    assert_eq!(sh.editor.popup.as_ref().unwrap().ctx.kind, RefKind::Tag);
}

#[test]
fn quick_search_recent_one_char_pages_blocks_and_explicit_create() {
    let mut sh = shell();
    type_popup(&mut sh, "저장 전 유일문구");
    assert!(sh.begin_search(SearchMode::Quick));
    assert_eq!(sh.hits[0].page, "회의록");
    sh.set_query("예");
    assert_eq!(sh.hits[0].page, "예산 계획");
    assert!(sh.hits.iter().all(|h| h.block.is_none()));
    sh.set_query("유일문구");
    assert!(sh.hits.iter().any(|h| h.page == "회의록" && h.block == Some(1)));
    assert!(!sh.store.as_ref().unwrap().exists("유일문구"));
    let row = sh.hits.len() - 1;
    assert!(sh.create_page.is_some());
    sh.open_hit(row, 20).unwrap();
    assert_eq!(sh.page.as_deref(), Some("유일문구"));
    assert!(sh.editor.editing());
    assert!(sh.store.as_ref().unwrap().exists("유일문구"));
}

#[test]
fn quick_search_never_proposes_invalid_or_existing_case_variant_names() {
    let mut sh = shell();
    sh.begin_search(SearchMode::Quick);
    for q in ["../escape", "CON/escape", "회의록", "예산 계획.md"] {
        sh.set_query(q);
        assert!(sh.create_page.is_none(), "{q}");
    }
    sh.set_query("없는페이지");
    sh.close_search();
    assert!(!sh.store.as_ref().unwrap().exists("없는페이지"));
    assert_eq!(sh.search_mode, SearchMode::Legacy);
}

#[test]
fn palette_filters_executes_existing_commands_and_opens_name_prompt() {
    let mut sh = shell();
    sh.begin_search(SearchMode::Commands);
    assert_eq!(sh.hits.len(), 11);
    sh.set_query("left");
    sh.open_hit(0, 20).unwrap();
    assert!(!sh.left_open);
    sh.begin_search(SearchMode::Commands);
    sh.set_query("새 페이지");
    sh.open_hit(0, 20).unwrap();
    assert_eq!(sh.prompt, Some(PromptKind::NewPage));
    sh.cancel_prompt();
    sh.begin_search(SearchMode::Commands);
    sh.set_query("TODO");
    sh.open_hit(0, 20).unwrap();
    assert_eq!(sh.editor.view.doc.blocks[1].status.as_deref(), Some("TODO"));
}

#[test]
fn duplicate_graph_ids_are_not_suggested_and_catalog_never_writes() {
    let sh = shell();
    let store = sh.store.as_ref().unwrap();
    write(&store.root, "중복.md", "- 중복\n  id:: stable-budget\n");
    let before = read(&store.root, "예산 계획.md");
    let (_, blocks) = outliner_win::find::completion_catalog(store, Some("회의록"));
    assert!(blocks.iter().all(|(id, _)| id != "stable-budget"));
    assert_eq!(read(&store.root, "예산 계획.md"), before);
}

fn harness(kind: ui_theme::Kind, dpi: u32) -> Harness {
    let d = temp_dir("ui3win");
    write(&d, "회의록.md", "- 예산 검토 #업무\n- 입력\n");
    write(&d, "예산 계획.md", "- 오늘 예산 검토\n  id:: budget-id\n");
    let mut sh = Shell::with_store(WinHost::new(), PageStore::new(d, None), fixed_clock).unwrap();
    sh.open_page("회의록").unwrap();
    Harness::new_shell(sh, "OUTLINEUI3", kind, dpi, 1100 * dpi as i32 / 96, 700 * dpi as i32 / 96).unwrap()
}

#[test]
fn win32_shortcuts_search_ime_guard_click_create_and_palette_prompt() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness(ui_theme::Kind::Light, 96);
    assert!(h.list_key_modifiers(b'K' as u16, true, false, false));
    assert_eq!(h.shell().search_mode, SearchMode::Quick);
    h.search_message(WM_IME_STARTCOMPOSITION, 0, 0);
    h.type_search("한글조합");
    assert_eq!(h.shell().query, "", "조합 중 검색어/결과 갱신 금지");
    h.search_message(WM_KEYDOWN, VK_RETURN as usize, 0);
    assert!(!h.shell().store.as_ref().unwrap().exists("한글조합"));
    h.search_message(WM_IME_ENDCOMPOSITION, 0, 0);
    h.refresh_search();
    assert_eq!(h.shell().query, "한글조합");
    h.search_message(WM_KEYDOWN, VK_RETURN as usize, 0);
    assert!(!h.shell().store.as_ref().unwrap().exists("한글조합"), "IME 종료 직후 중복 Enter 억제");
    let lay = h.layout_now();
    h.click_at(lay.center.l + 30, lay.center.t + lay.head_h + lay.result_h / 2, false);
    assert_eq!(h.shell().page.as_deref(), Some("한글조합"));
    h.list_key_modifiers(b'P' as u16, true, false, true);
    h.type_search("이름 변경");
    let lay = h.layout_now();
    h.click_at(lay.center.l + 30, lay.center.t + lay.head_h + lay.result_h / 2, false);
    assert_eq!(h.shell().prompt, Some(PromptKind::Rename));
    assert_eq!(h.search_text(), "한글조합");
}

#[test]
fn win32_edit_ime_popup_mouse_and_six_theme_dpi_renders() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for (kind, label) in [(ui_theme::Kind::Light, "light"), (ui_theme::Kind::Dark, "dark"), (ui_theme::Kind::HighContrast, "hc")] {
        for dpi in [96, 192] {
            let mut h = harness(kind, dpi);
            h.editor().view.set_selected(1, 20);
            h.editor().start_edit_selected();
            h.editor().host.set_text("😀 /h2", 6);
            h.layout();
            // 초기 EDIT가 글꼴보다 작으면 192 DPI에서 내부 서식 영역이 0으로 남아 글이 보이지 않는다.
            let mut format_rect: windows_sys::Win32::Foundation::RECT = unsafe { std::mem::zeroed() };
            unsafe { SendMessageW(h.editor().host.edit, 0x00B2, 0, &mut format_rect as *mut _ as isize); }
            assert!(format_rect.right > format_rect.left && format_rect.bottom > format_rect.top, "{kind:?}/{dpi}: 내부 글쓰기 영역");
            let p = h.editor().popup.clone().unwrap();
            let lay = h.layout_now();
            let er = outliner_win::paint_shell::edit_rect_of(h.shell(), &lay).unwrap();
            let r = outliner_win::layout::popup_rect(&lay, er, p.items.len());
            let edit = h.editor().host.edit;
            unsafe { SendMessageW(edit, WM_IME_STARTCOMPOSITION, 0, 0); }
            h.click_at(r.l + 20, r.t + lay.popup_h / 2, false);
            assert_eq!(h.editor().host.read_text(), "😀 /h2");
            unsafe { SendMessageW(edit, WM_IME_ENDCOMPOSITION, 0, 0); }
            h.layout();
            let shot = h.render().unwrap();
            assert!(shot.bgra.iter().any(|b| *b != 0));
            if let Some(dir) = std::env::var_os("OUTLINEUI3_CAPTURES") {
                std::fs::create_dir_all(&dir).unwrap();
                outliner_win::png::write(&std::path::PathBuf::from(dir).join(format!("slash_{label}_{dpi}.png")), shot.width, shot.height, &shot.bgra).unwrap();
            }
            h.click_at(r.l + 20, r.t + lay.popup_h / 2, false);
            assert_eq!(h.editor().host.read_text(), "## 😀 ");
        }
    }
}
