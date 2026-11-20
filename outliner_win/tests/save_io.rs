//! 엔진 직접 호출 왕복 + 저장(원자적 교체)·SHA-256 충돌 시험. 모두 임시 폴더의 복사본만 쓴다.
use outliner_win::SAMPLE_MD;
use outliner_win::docio::{self, SaveError};
use outliner_win::edit::*;
use outliner_win::sha256::{hex, sha256};
use outliner_win::view::View;
use std::path::PathBuf;

struct Nop(String);
impl EditHost for Nop {
    fn read_text(&mut self) -> String {
        self.0.clone()
    }
    fn selection(&mut self) -> (usize, usize) {
        let n = self.0.encode_utf16().count();
        (n, n)
    }
    fn set_text(&mut self, t: &str, _c: usize) {
        self.0 = t.to_string();
    }
    fn composing(&mut self) -> bool {
        false
    }
    fn candidate_open(&mut self) -> bool {
        false
    }
    fn request_complete(&mut self) {}
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("outliner_win_save_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 샘플의 임시 복사본을 열어 편집기를 만든다.
fn open_copy(dir: &std::path::Path) -> (PathBuf, Editor<Nop>) {
    let path = dir.join("copy.md");
    std::fs::write(&path, SAMPLE_MD).unwrap();
    let (doc, text, info) = docio::load_file(&path).unwrap();
    let mut view = View::from_markdown(&text).unwrap();
    view.doc = doc;
    let base = outliner_core::serialize(&view.doc);
    (path, Editor::new(view, Nop(String::new()), Some(info), base))
}

#[test]
fn engine_requests_roundtrip_directly_without_a_process() {
    // handle_request 직접 호출: parse → edit(set_text, insert, indent) → serialize
    let doc = docio::parse_doc("- 가\n- 나\n").unwrap();
    let d = docio::edit_doc(&doc, "set_text", doc.blocks[0].id, Some("가가")).unwrap();
    let d = docio::edit_doc(&d, "insert", d.blocks[0].id, Some("새 블록")).unwrap();
    let d = docio::edit_doc(&d, "indent", d.blocks[1].id, None).unwrap();
    assert_eq!(outliner_core::serialize(&d), "- 가가\n - 새 블록\n- 나\n");
    // 엔진 오류는 문자열로 돌아온다(직접 set_text 요청은 오류라는 점도 유지).
    let e = docio::edit_doc(&d, "indent", d.blocks[0].id, None).unwrap_err();
    assert!(e.contains("INVALID_OPERATION") && e.contains("들여쓸 수 없습니다"), "{e}");
    let raw = docio::request("set_text", serde_json::json!({}), "- a\n").unwrap_err();
    assert!(raw.contains("INVALID_OPERATION"), "{raw}");
}

#[test]
fn edit_then_save_roundtrips_through_the_file_and_original_sample_is_untouched() {
    let dir = temp_dir("roundtrip");
    let before = std::fs::read(env!("CARGO_MANIFEST_DIR").to_string() + "/tests/sample.md").unwrap();
    let (path, mut ed) = open_copy(&dir);
    assert!(!ed.dirty());
    ed.view.set_selected(0, 10);
    ed.start_edit_selected();
    ed.host.0 = "2026년 업무 계획 (수정) #계획".into();
    ed.key(KeyEvent::new(VK_RETURN, 0)); // 끝에서 Enter → 새 빈 블록
    assert!(ed.dirty());
    assert!(ed.save());
    assert!(!ed.dirty(), "저장 뒤 변경 표시 해제");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.starts_with("- 2026년 업무 계획 (수정) #계획\n- \n- 1분기\n"), "{saved}");
    // 다시 읽어 같은 문서인지.
    let (doc2, text2, info2) = docio::load_file(&path).unwrap();
    assert_eq!(outliner_core::serialize(&doc2), saved);
    assert_eq!(text2, saved);
    assert_eq!(info2.sha, sha256(saved.as_bytes()));
    assert_eq!(ed.file().unwrap().sha, info2.sha, "저장 뒤 기억한 SHA-256 이 파일과 같다");
    // 두 번째 저장도 충돌 없이 된다(SHA-256 갱신 확인).
    ed.host.0 = "".into();
    ed.key(KeyEvent::new(VK_TAB, 0));
    assert!(ed.save());
    // 원본 샘플은 그대로.
    let after = std::fs::read(env!("CARGO_MANIFEST_DIR").to_string() + "/tests/sample.md").unwrap();
    assert_eq!(before, after);
    // 임시 파일 찌꺼기가 남지 않는다.
    let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(leftovers, vec!["copy.md".to_string()], "{leftovers:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changed_file_is_not_overwritten_and_user_is_warned() {
    let dir = temp_dir("conflict");
    let (path, mut ed) = open_copy(&dir);
    ed.view.set_selected(1, 10);
    ed.start_edit_selected();
    ed.host.0 = "내 수정".into();
    ed.key(KeyEvent::new(VK_DOWN, 0)); // 확정
    // 다른 곳(예: 다른 창)이 같은 파일을 바꿈.
    let theirs = format!("{SAMPLE_MD}- 다른 곳에서 추가한 줄\n");
    std::fs::write(&path, &theirs).unwrap();
    assert!(!ed.save(), "SHA-256 불일치 → 저장 안 함");
    assert!(ed.conflict);
    let msg = ed.message.clone().unwrap();
    assert!(msg.contains("SHA-256 불일치") && msg.starts_with('⚠'), "{msg}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), theirs, "남의 변경을 덮어쓰지 않았다");
    assert!(ed.dirty(), "내 변경은 아직 저장 안 됨");
    // 같은 내용으로 되돌려 쓰면(바이트가 같아지면) 해시가 같아 저장된다 — 해시는 내용 기준.
    std::fs::write(&path, SAMPLE_MD).unwrap();
    assert!(ed.save());
    assert!(!ed.conflict);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deleted_file_and_unwritable_target_report_errors_and_keep_text() {
    let dir = temp_dir("errors");
    let (path, mut ed) = open_copy(&dir);
    ed.view.set_selected(0, 10);
    ed.start_edit_selected();
    ed.host.0 = "보존해야 하는 글".into();
    std::fs::remove_file(&path).unwrap();
    ed.key(KeyEvent::new(VK_ESCAPE, 0)); // 확정 안 된 글은 버려지는 정상 동작
    let mut ctrl_s = KeyEvent::new(VK_S, 0);
    ctrl_s.ctrl = true;
    ed.start_edit_selected();
    ed.host.0 = "보존해야 하는 글".into();
    ed.key(ctrl_s);
    assert!(ed.conflict && ed.message.as_deref().unwrap().contains("사라져"), "{:?}", ed.message);
    assert_eq!(ed.host.0, "보존해야 하는 글", "저장 실패에도 EDIT 글은 그대로");
    assert_eq!(ed.view.doc.blocks[0].text, "보존해야 하는 글", "문서에는 확정돼 있음");
    // 저장 경로가 디렉터리인 경우(엔진 IO 오류).
    let mut info = docio::FileInfo { path: dir.clone(), sha: sha256(b"") };
    let err = docio::save_checked(&mut info, "- a\n").unwrap_err();
    assert!(matches!(err, SaveError::Other(_)), "{err:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_file_hashes_the_bytes_it_parsed_and_rejects_non_utf8() {
    let dir = temp_dir("load");
    let path = dir.join("a.md");
    std::fs::write(&path, "- 한글\r\n- b\r\n").unwrap();
    let (doc, text, info) = docio::load_file(&path).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(text, "- 한글\r\n- b\r\n");
    assert_eq!(hex(&info.sha), hex(&sha256("- 한글\r\n- b\r\n".as_bytes())));
    assert_eq!(doc.newline, "\r\n", "CRLF 문서는 CRLF 로 저장된다");
    std::fs::write(&path, [0xFFu8, 0xFE, 0x41]).unwrap();
    assert!(docio::load_file(&path).unwrap_err().contains("UTF-8"));
    assert!(docio::load_file(&dir.join("없음.md")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn crlf_document_edit_keeps_line_endings() {
    let dir = temp_dir("crlf");
    let path = dir.join("c.md");
    std::fs::write(&path, "- a\r\n- b\r\n").unwrap();
    let (doc, text, info) = docio::load_file(&path).unwrap();
    let mut view = View::from_markdown(&text).unwrap();
    view.doc = doc;
    let base = outliner_core::serialize(&view.doc);
    let mut ed = Editor::new(view, Nop(String::new()), Some(info), base);
    ed.view.set_selected(0, 10);
    ed.start_edit_selected();
    ed.key(KeyEvent::new(VK_RETURN, 0));
    assert!(ed.save());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "- a\r\n- \r\n- b\r\n");
    let _ = std::fs::remove_dir_all(&dir);
}
