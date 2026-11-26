//! 관리·파일명 고급 기능을 위한 실제 Win32 컨트롤 창. 웹 엔진·Tk 창을 사용하지 않는다.
use crate::{Client, catalog, field_value, request};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver, Sender},
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{Com::CoTaskMemFree, LibraryLoader::GetModuleHandleW},
    UI::{Controls::Dialogs::*, HiDpi::*, Shell::*, WindowsAndMessaging::*},
};

const FILES: usize = 101;
const RULES: usize = 102;
const KIND: usize = 103;
const SAVE_RULE: usize = 104;
const ADD: usize = 110;
const FOLDER: usize = 111;
const AS_FOLDER: usize = 112;
const REMOVE: usize = 113;
const CLEAR: usize = 114;
const SORT: usize = 115;
const RECURSE: usize = 116;
const FILE_UP: usize = 117;
const FILE_DOWN: usize = 118;
const HIDDEN: usize = 119;
const NEW: usize = 120;
const EDIT: usize = 121;
const DELETE: usize = 122;
const UP: usize = 123;
const DOWN: usize = 124;
const TOGGLE: usize = 125;
const PREVIEW: usize = 130;
const APPLY: usize = 131;
const CANCEL: usize = 132;
const UNDO: usize = 133;
const HISTORY: usize = 134;
const HISTORY_LIST: usize = 135;
const SET_NAME: usize = 140;
const SET_SAVE: usize = 141;
const SET_LIST: usize = 142;
const SET_LOAD: usize = 143;
const SET_DELETE: usize = 144;
const SET_IMPORT: usize = 145;
const SET_EXPORT: usize = 146;
const RECENT: usize = 147;
const CONFLICT: usize = 150;
const OVERRIDE: usize = 151;
const OVERRIDE_SAVE: usize = 152;
const EXCLUDE: usize = 153;
const PAGE: usize = 160;
const REFRESH: usize = 161;
const WRITE: usize = 162;
const ITEM_ADD: usize = 163;
const ITEM_DELETE: usize = 164;
const OPEN_CONFIG: usize = 165;
const KVF_ON: usize = 166;
const KVF_OFF: usize = 167;
const KVF_START: usize = 168;
const IMPORT_APP: usize = 169;
const STATUS: usize = 180;
const LB_GETCURSEL: u32 = 0x188;
const LB_GETSEL: u32 = 0x187;
const LB_RESETCONTENT: u32 = 0x184;
const LB_ADDSTRING: u32 = 0x180;
const CB_GETCURSEL: u32 = 0x147;
const CB_SETCURSEL: u32 = 0x14e;
const CB_ADDSTRING: u32 = 0x143;
const CB_RESETCONTENT: u32 = 0x14b;
const BM_GETCHECK: u32 = 0xf0;
const BM_SETCHECK: u32 = 0xf1;

struct Field {
    key: String,
    default: Value,
    choices: Vec<(&'static str, &'static str)>,
    hwnd: HWND,
    label: HWND,
}
struct State {
    rename: bool,
    controls: BTreeMap<usize, HWND>,
    fields: Vec<Field>,
    rules: Vec<Value>,
    entries: Vec<Value>,
    preview: Option<Value>,
    overrides: serde_json::Map<String, Value>,
    excluded: Vec<String>,
    editing: Option<usize>,
    sets: Vec<Value>,
    history: Vec<Value>,
    snapshot: Value,
    current_page: usize,
    deferred_paths: Vec<String>,
    dirty: bool,
    quiet: bool,
    tx: Sender<Value>,
    rx: Receiver<Result<Value, String>>,
    pending: Option<Value>,
    job: Option<String>,
    closing: bool,
    cancel_requested: bool,
    cancel_sent: bool,
    last_result: Option<String>,
    dpi: u32,
    font: HFONT,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn text(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut b = vec![0; n + 1];
        let n = GetWindowTextW(h, b.as_mut_ptr(), b.len() as i32);
        String::from_utf16_lossy(&b[..n.max(0) as usize])
    }
}
unsafe fn st<'a>(h: HWND) -> &'a mut State {
    unsafe { &mut *(GetWindowLongPtrW(h, GWLP_USERDATA) as *mut State) }
}
fn sc(v: i32, d: u32) -> i32 {
    (v as i64 * d as i64 / 96) as i32
}
fn child(
    s: &mut State,
    parent: HWND,
    id: usize,
    cls: &str,
    title: &str,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    unsafe {
        let handle = CreateWindowExW(
            if cls == "EDIT" { WS_EX_CLIENTEDGE } else { 0 },
            wide(cls).as_ptr(),
            wide(title).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            sc(x, s.dpi),
            sc(y, s.dpi),
            sc(w, s.dpi),
            sc(h, s.dpi),
            parent,
            id as _,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(handle, WM_SETFONT, s.font as usize, 0);
        s.controls.insert(id, handle);
        handle
    }
}
fn button(s: &mut State, p: HWND, id: usize, t: &str, x: i32, y: i32, w: i32) {
    child(s, p, id, "BUTTON", t, WS_TABSTOP, x, y, w, 28);
}
fn label(s: &mut State, p: HWND, id: usize, t: &str, x: i32, y: i32, w: i32) {
    child(s, p, id, "STATIC", t, 0, x, y, w, 24);
}
fn edit(s: &mut State, p: HWND, id: usize, t: &str, x: i32, y: i32, w: i32) -> HWND {
    child(
        s,
        p,
        id,
        "EDIT",
        t,
        WS_TABSTOP | ES_AUTOHSCROLL as u32,
        x,
        y,
        w,
        26,
    )
}
fn combo(s: &mut State, p: HWND, id: usize, items: &[&str], x: i32, y: i32, w: i32) -> HWND {
    let handle = child(
        s,
        p,
        id,
        "COMBOBOX",
        "",
        WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
        x,
        y,
        w,
        300,
    );
    unsafe {
        for t in items {
            SendMessageW(handle, CB_ADDSTRING, 0, wide(t).as_ptr() as isize);
        }
        SendMessageW(handle, CB_SETCURSEL, 0, 0);
    }
    handle
}
fn selection(s: &State, id: usize) -> usize {
    unsafe {
        SendMessageW(
            s.controls[&id],
            if id == FILES || id == RULES {
                LB_GETCURSEL
            } else {
                CB_GETCURSEL
            },
            0,
            0,
        )
        .max(0) as usize
    }
}
fn set_status(s: &State, t: &str) {
    unsafe {
        SetWindowTextW(s.controls[&STATUS], wide(t).as_ptr());
    }
}
fn list(s: &State, id: usize, rows: impl Iterator<Item = String>) {
    unsafe {
        let h = s.controls[&id];
        SendMessageW(h, LB_RESETCONTENT, 0, 0);
        let dc = GetDC(h);
        let old = SelectObject(dc, s.font);
        let mut width = 0;
        for row in rows {
            SendMessageW(h, LB_ADDSTRING, 0, wide(&row).as_ptr() as isize);
            let text = wide(&row);
            let mut size: SIZE = std::mem::zeroed();
            GetTextExtentPoint32W(
                dc,
                text.as_ptr(),
                text.len().saturating_sub(1) as i32,
                &mut size,
            );
            width = width.max(size.cx);
        }
        SendMessageW(h, 0x194, (width + sc(16, s.dpi)) as usize, 0);
        SelectObject(dc, old);
        ReleaseDC(h, dc);
    }
}
fn selected(s: &State) -> Vec<usize> {
    unsafe {
        (0..s.entries.len())
            .filter(|i| SendMessageW(s.controls[&FILES], LB_GETSEL, *i, 0) > 0)
            .collect()
    }
}
fn ids(s: &State) -> Vec<String> {
    selected(s)
        .iter()
        .filter_map(|i| s.entries[*i]["id"].as_str().map(str::to_owned))
        .collect()
}
fn send(s: &mut State, r: Value) {
    if s.pending.is_some()
        || (s.job.is_some() && !matches!(r["op"].as_str(), Some("cancel" | "job_status")))
    {
        return;
    }
    if s.tx.send(r.clone()).is_ok() {
        crate::trace(&json!({"phase":"request","request":r}));
        s.pending = Some(r);
        busy_controls(s);
        set_status(s, "처리 중…");
    } else {
        set_status(s, "작업 연결이 종료되었습니다.");
    }
}

fn busy_controls(s: &State) {
    let phase = crate::phase(
        s.pending.is_some(),
        s.job.is_some(),
        s.cancel_requested || s.cancel_sent,
        s.closing,
    );
    let idle = phase == crate::Phase::Idle;
    unsafe {
        for (id, handle) in &s.controls {
            if *id != STATUS {
                EnableWindow(
                    *handle,
                    i32::from(idle || (*id == CANCEL && s.job.is_some())),
                );
            }
        }
    }
    crate::trace(&json!({"phase":format!("{phase:?}")}));
}
fn invalidate(s: &mut State) {
    s.preview = None;
    set_status(s, "미리보기를 다시 확인해 주세요.");
}
fn rule_rows(s: &State) {
    let c = catalog();
    list(
        s,
        RULES,
        s.rules.iter().enumerate().map(|(i, r)| {
            let name = c
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["kind"] == r["kind"])
                .and_then(|x| x["label"].as_str())
                .unwrap_or("");
            format!(
                "{} {}. {}",
                if r["enabled"] == false {
                    "꺼짐"
                } else {
                    "켬"
                },
                i + 1,
                name
            )
        }),
    );
}
pub fn choices(key: &str, kind: &str) -> Vec<(&'static str, &'static str)> {
    match key {
        "position" => {
            if kind == "doc_title" {
                vec![
                    ("replace", "이름 대신"),
                    ("prefix", "이름 앞"),
                    ("suffix", "이름 뒤"),
                ]
            } else if matches!(kind, "date" | "exif_date" | "number") {
                vec![
                    ("prefix", "이름 앞"),
                    ("suffix", "이름 뒤"),
                    ("replace", "이름 대신"),
                ]
            } else {
                vec![("prefix", "이름 앞"), ("suffix", "이름 뒤")]
            }
        }
        "count" => vec![("all", "모두"), ("first", "첫 하나")],
        "source" => vec![
            ("mtime", "수정일"),
            ("ctime", "만든 날"),
            ("exif", "촬영일"),
            ("today", "오늘"),
        ],
        "which" => vec![("first", "첫 숫자"), ("last", "끝 숫자")],
        "origin" => vec![("start", "앞에서"), ("end", "뒤에서")],
        "nest" => vec![("outer", "바깥"), ("inner", "안쪽")],
        "underscore" => vec![
            ("keep", "유지"),
            ("to_space", "밑줄 → 공백"),
            ("from_space", "공백 → 밑줄"),
        ],
        "width" => vec![("keep", "유지"), ("half", "반각"), ("full", "전각")],
        "mode" => {
            if kind == "case" {
                vec![
                    ("lower", "소문자"),
                    ("upper", "대문자"),
                    ("title", "단어 첫 글자"),
                    ("capitalize", "첫 글자"),
                    ("sentence", "문장형"),
                ]
            } else {
                vec![("lower", "소문자"), ("upper", "대문자")]
            }
        }
        _ => vec![],
    }
}
fn field_label(k: &str) -> &str {
    match k {
        "find" => "찾을 글자",
        "to" => "바꿀 글자 ($1·날짜·번호 지원)",
        "regex" => "정규식",
        "case" => "대소문자 구분",
        "count" => "바꿀 횟수",
        "text" => "넣을 글자 ({orig}·{date} 지원)",
        "names" => "새 이름 (한 줄에 하나)",
        "keep_ext" => "확장자 유지",
        "position" => "넣을 위치",
        "sep" => "구분 문자",
        "source" => "날짜 기준",
        "format" => "날짜 형식 (YYYYMMDD)",
        "start" => "시작",
        "step" => "간격",
        "digits" => "자리수",
        "per_folder" => "폴더마다 세기",
        "delta" => "늘릴 값 (음수: 줄임)",
        "which" => "대상 숫자",
        "end" => "끝",
        "origin" => "기준",
        "open" => "여는 문자",
        "close" => "닫는 문자",
        "nest" => "겹친 괄호",
        "keep_marks" => "괄호 남기기",
        "nfc" => "한글 자모 합치기",
        "collapse_spaces" => "연속 공백 정리",
        "underscore" => "공백·밑줄",
        "strip" => "앞뒤 공백 지우기",
        "width" => "전각·반각",
        "mode" => "방식",
        "ext" => "확장자",
        "template" => "이름 틀 ({date}_{dept}_{title})",
        "dept" => "부서",
        "max_length" => "제목 최대 길이",
        "always_on_top" => "Rust 창을 항상 위에 표시",
        "mongttang_search" => "퀵몽땅검색 켜기 (Ctrl+Alt+Space)",
        "key" => "준말",
        "value" => "본말·내용",
        "title" => "제목",
        _ => k,
    }
}
fn form(s: &mut State, p: HWND, values: &Value, kind: &str) {
    s.quiet = true;
    unsafe {
        for f in s.fields.drain(..) {
            DestroyWindow(f.hwnd);
            DestroyWindow(f.label);
        }
    }
    s.controls.retain(|id, _| !(*id >= 300 && *id < 500));
    for (i, (key, value)) in values
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| k.as_str() != "kind" && k.as_str() != "enabled")
        .enumerate()
    {
        let x = 16 + (i % 2) as i32 * 525;
        let y = 365 + (i / 2) as i32 * 48;
        let id = 300 + i;
        let choices = if s.rename { choices(key, kind) } else { vec![] };
        let title = child(s, p, 400 + i, "STATIC", field_label(key), 0, x, y, 490, 20);
        let h = if value.is_boolean() {
            child(
                s,
                p,
                id,
                "BUTTON",
                "켬",
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                x,
                y + 20,
                490,
                26,
            )
        } else if !choices.is_empty() {
            let labels: Vec<_> = choices.iter().map(|(_, l)| *l).collect();
            combo(s, p, id, &labels, x, y + 20, 490)
        } else if key == "names" || key == "value" {
            child(
                s,
                p,
                id,
                "EDIT",
                value.as_str().unwrap_or(""),
                WS_TABSTOP | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | WS_VSCROLL,
                x,
                y + 20,
                490,
                48,
            )
        } else {
            edit(
                s,
                p,
                id,
                &value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
                x,
                y + 20,
                490,
            )
        };
        unsafe {
            if value.is_boolean() {
                SendMessageW(h, BM_SETCHECK, usize::from(value == &json!(true)), 0);
            } else if !choices.is_empty() {
                let at = choices
                    .iter()
                    .position(|(v, _)| Some(*v) == value.as_str())
                    .unwrap_or(0);
                SendMessageW(h, CB_SETCURSEL, at, 0);
            }
        }
        s.fields.push(Field {
            key: key.clone(),
            default: value.clone(),
            choices,
            hwnd: h,
            label: title,
        });
    }
    s.quiet = false;
}
fn form_values(s: &State) -> Result<Value, String> {
    let mut v = serde_json::Map::new();
    for f in &s.fields {
        let value = if !f.choices.is_empty() {
            let at = unsafe { SendMessageW(f.hwnd, CB_GETCURSEL, 0, 0) };
            json!(f.choices.get(at.max(0) as usize).ok_or("선택값 오류")?.0)
        } else {
            field_value(
                &f.default,
                &text(f.hwnd),
                unsafe { SendMessageW(f.hwnd, BM_GETCHECK, 0, 0) } == 1,
            )?
        };
        v.insert(f.key.clone(), value);
    }
    Ok(Value::Object(v))
}
fn confirm(p: HWND, t: &str) -> bool {
    unsafe {
        MessageBoxW(
            p,
            wide(t).as_ptr(),
            wide("확인").as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        ) == IDYES
    }
}

pub fn pick_files(parent: HWND) -> Vec<PathBuf> {
    unsafe {
        let mut buf = vec![0u16; 65536];
        let mut of: OPENFILENAMEW = std::mem::zeroed();
        of.lStructSize = std::mem::size_of_val(&of) as u32;
        of.hwndOwner = parent;
        of.lpstrFile = buf.as_mut_ptr();
        of.nMaxFile = buf.len() as u32;
        of.Flags = OFN_EXPLORER | OFN_ALLOWMULTISELECT | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR;
        if GetOpenFileNameW(&mut of) == 0 {
            return vec![];
        }
        let parts: Vec<String> = buf
            .split(|v| *v == 0)
            .take_while(|x| !x.is_empty())
            .map(String::from_utf16_lossy)
            .collect();
        if parts.len() == 1 {
            vec![parts[0].clone().into()]
        } else {
            parts[1..]
                .iter()
                .map(|n| Path::new(&parts[0]).join(n))
                .collect()
        }
    }
}
pub fn pick_folder(parent: HWND) -> Option<PathBuf> {
    unsafe {
        let title = wide("폴더 선택");
        let mut bi: BROWSEINFOW = std::mem::zeroed();
        bi.hwndOwner = parent;
        bi.lpszTitle = title.as_ptr();
        bi.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
        let pidl = SHBrowseForFolderW(&bi);
        if pidl.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 32768];
        let ok = SHGetPathFromIDListW(pidl, buf.as_mut_ptr());
        CoTaskMemFree(pidl.cast());
        if ok == 0 {
            None
        } else {
            let n = buf.iter().position(|v| *v == 0).unwrap();
            Some(String::from_utf16_lossy(&buf[..n]).into())
        }
    }
}
pub fn open_path(path: &Path) {
    unsafe {
        ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            wide(&path.to_string_lossy()).as_ptr(),
            null(),
            null(),
            SW_SHOW,
        );
    }
}

fn setup(s: &mut State, p: HWND) {
    if s.rename {
        button(s, p, ADD, "파일 추가", 16, 12, 90);
        button(s, p, FOLDER, "폴더 펼치기", 110, 12, 100);
        button(s, p, AS_FOLDER, "폴더 자체", 214, 12, 90);
        child(
            s,
            p,
            RECURSE,
            "BUTTON",
            "하위 폴더 포함",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            312,
            12,
            140,
            28,
        );
        button(s, p, REMOVE, "선택 제거", 460, 12, 90);
        button(s, p, CLEAR, "비우기", 554, 12, 70);
        combo(
            s,
            p,
            SORT,
            &[
                "이름 ↑",
                "경로 ↑",
                "크기 ↑",
                "수정일 ↑",
                "만든 날 ↑",
                "이름 ↓",
                "경로 ↓",
                "크기 ↓",
                "수정일 ↓",
                "만든 날 ↓",
            ],
            630,
            12,
            130,
        );
        button(s, p, UP, "규칙 ↑", 768, 12, 88);
        button(s, p, DOWN, "규칙 ↓", 860, 12, 88);
        child(
            s,
            p,
            HIDDEN,
            "BUTTON",
            "숨김 포함",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            952,
            12,
            120,
            28,
        );
        button(s, p, FILE_UP, "파일 ↑", 430, 44, 85);
        button(s, p, FILE_DOWN, "파일 ↓", 522, 44, 85);
        label(s, p, 900, "파일 목록 · Ctrl/Shift 다중 선택", 16, 50, 390);
        label(s, p, 901, "적용할 규칙 (위에서 아래 순서)", 640, 50, 420);
        child(
            s,
            p,
            FILES,
            "LISTBOX",
            "",
            WS_BORDER
                | WS_VSCROLL
                | WS_HSCROLL
                | WS_TABSTOP
                | LBS_EXTENDEDSEL as u32
                | LBS_NOTIFY as u32
                | LBS_NOINTEGRALHEIGHT as u32,
            16,
            76,
            610,
            225,
        );
        child(
            s,
            p,
            RULES,
            "LISTBOX",
            "",
            WS_BORDER | WS_VSCROLL | WS_TABSTOP | LBS_NOTIFY as u32,
            640,
            76,
            425,
            158,
        );
        let c = catalog();
        let labels: Vec<_> = c
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["label"].as_str().unwrap())
            .collect();
        combo(s, p, KIND, &labels, 640, 240, 425);
        button(s, p, NEW, "규칙 추가", 640, 272, 100);
        button(s, p, EDIT, "편집", 744, 272, 70);
        button(s, p, DELETE, "삭제", 818, 272, 70);
        button(s, p, TOGGLE, "켬/끔", 892, 272, 80);
        combo(
            s,
            p,
            CONFLICT,
            &["충돌 시 차단", "충돌 이름에 번호"],
            16,
            310,
            155,
        );
        edit(s, p, OVERRIDE, "", 180, 310, 270);
        button(s, p, OVERRIDE_SAVE, "선택 이름 직접 지정", 454, 310, 170);
        button(s, p, EXCLUDE, "선택 제외/복원", 640, 310, 150);
        button(s, p, SAVE_RULE, "규칙 입력 확정", 800, 310, 140);
        edit(s, p, SET_NAME, "", 16, 580, 155);
        button(s, p, SET_SAVE, "세트 저장", 178, 580, 90);
        combo(s, p, SET_LIST, &[], 276, 580, 190);
        button(s, p, SET_LOAD, "불러오기", 474, 580, 90);
        button(s, p, SET_DELETE, "세트 삭제", 572, 580, 90);
        button(s, p, SET_IMPORT, "세트 가져오기", 670, 580, 120);
        button(s, p, SET_EXPORT, "세트 내보내기", 798, 580, 120);
        button(s, p, RECENT, "최근 치환", 926, 580, 130);
        button(s, p, PREVIEW, "미리보기", 16, 618, 115);
        button(s, p, APPLY, "이름 변경 적용", 140, 618, 140);
        button(s, p, CANCEL, "중단", 290, 618, 85);
        button(s, p, HISTORY, "이력 조회", 384, 618, 100);
        button(s, p, UNDO, "선택 이력 되돌리기", 494, 618, 180);
        combo(s, p, HISTORY_LIST, &[], 684, 618, 370);
        send(s, request("list", json!({})));
    } else {
        combo(
            s,
            p,
            PAGE,
            &[
                "글자 상용구",
                "본문 상용구",
                "가상 폴더 창",
                "앱 정보",
                "창·검색",
            ],
            16,
            12,
            250,
        );
        button(s, p, REFRESH, "새로고침", 280, 12, 100);
        button(s, p, OPEN_CONFIG, "설정 원본 열기", 390, 12, 140);
        button(s, p, KVF_ON, "가상 폴더 켜기", 540, 12, 140);
        button(s, p, KVF_OFF, "끄기", 690, 12, 75);
        child(
            s,
            p,
            KVF_START,
            "BUTTON",
            "로그인 때 자동 실행",
            WS_TABSTOP | BS_AUTOCHECKBOX as u32,
            780,
            12,
            210,
            28,
        );
        child(
            s,
            p,
            FILES,
            "LISTBOX",
            "",
            WS_BORDER | WS_VSCROLL | WS_HSCROLL | WS_TABSTOP | LBS_NOTIFY as u32,
            16,
            62,
            1040,
            270,
        );
        button(s, p, ITEM_ADD, "새 상용구", 16, 615, 110);
        button(s, p, WRITE, "선택 상용구 저장", 138, 615, 160);
        button(s, p, ITEM_DELETE, "선택 삭제", 310, 615, 110);
        button(s, p, IMPORT_APP, "내장 상용구 가져오기", 432, 615, 190);
        send(s, json!({"op":"idioms","payload":{"action":"get"}}));
    }
}
fn refresh_page(s: &mut State) {
    let page = selection(s, PAGE);
    s.current_page = page;
    send(
        s,
        match page {
            0 | 1 => json!({"op":"idioms","payload":{"action":"get"}}),
            2 => json!({"op":"kvf","payload":{"action":"status"}}),
            4 => json!({"op":"runtime_settings","payload":{"action":"get"}}),
            _ => json!({"op":"about"}),
        },
    );
}

fn action(s: &mut State, p: HWND, id: usize, code: usize) -> Result<(), String> {
    if id == CANCEL && s.job.is_some() && !s.cancel_sent {
        s.cancel_requested = true;
        set_status(s, "안전한 지점에서 중단합니다…");
        return Ok(());
    }
    if id >= 300 && id < 400 && !s.quiet && matches!(code, 0 | 1 | 0x300) {
        s.dirty = true;
    }
    if s.pending.is_some() {
        return Ok(());
    }
    if s.job.is_some() {
        return Ok(());
    }
    if s.rename {
        match id {
            ADD | FOLDER | AS_FOLDER => {
                let paths = if id == ADD {
                    pick_files(p)
                } else {
                    pick_folder(p).into_iter().collect()
                };
                if !paths.is_empty() {
                    s.preview = None;
                    send(
                        s,
                        request(
                            "add_paths",
                            json!({"paths":paths,"recursive":unsafe{SendMessageW(s.controls[&RECURSE],BM_GETCHECK,0,0)}==1,"include_hidden":unsafe{SendMessageW(s.controls[&HIDDEN],BM_GETCHECK,0,0)}==1,"as_folder":id==AS_FOLDER}),
                        ),
                    );
                }
            }
            REMOVE => {
                s.preview = None;
                send(s, request("remove", json!({"ids":ids(s)})));
            }
            FILE_UP | FILE_DOWN => {
                s.preview = None;
                send(
                    s,
                    request(
                        "move",
                        json!({"ids":ids(s),"delta":if id==FILE_UP{-1}else{1}}),
                    ),
                );
            }
            RECENT => send(s, request("replace_mru_list", json!({}))),
            CLEAR => {
                s.preview = None;
                s.overrides.clear();
                s.excluded.clear();
                send(s, request("clear", json!({})));
            }
            SORT if code == 1 => {
                s.preview = None;
                let selected = selection(s, SORT);
                let k = ["name", "path", "size", "mtime", "ctime"][selected % 5];
                send(s, request("sort", json!({"key":k,"reverse":selected>=5})));
            }
            NEW | EDIT => {
                if s.dirty && !confirm(p, "확정하지 않은 규칙 입력을 버릴까요?") {
                    return Ok(());
                }
                let c = catalog();
                let r = if id == NEW {
                    let mut r = c[selection(s, KIND)]["defaults"].clone();
                    r["kind"] = c[selection(s, KIND)]["kind"].clone();
                    s.editing = None;
                    r
                } else {
                    let i = selection(s, RULES);
                    let Some(r) = s.rules.get(i).cloned() else {
                        return Ok(());
                    };
                    s.editing = Some(i);
                    r
                };
                let kind = r["kind"].as_str().unwrap_or("").to_owned();
                form(s, p, &r, &kind);
                s.snapshot = r;
                s.dirty = false;
                crate::trace(&json!({"phase":"rule_form","kind":kind,"fields":s.fields.len()}));
            }
            SAVE_RULE => {
                if !s.snapshot.is_object() {
                    return Ok(());
                }
                let mut rule = form_values(s)?;
                crate::trace(
                    &json!({"phase":"rule_commit","values":rule,"raw":s.fields.iter().map(|f|json!({"key":f.key,"text":text(f.hwnd),"hwnd":f.hwnd as usize})).collect::<Vec<_>>()}),
                );
                rule["kind"] = s.snapshot["kind"].clone();
                rule["enabled"] = s.snapshot.get("enabled").cloned().unwrap_or(json!(true));
                let recent = if rule["kind"] == "replace" {
                    Some(json!({"find":rule["find"],"to":rule["to"]}))
                } else {
                    None
                };
                if let Some(i) = s.editing {
                    if i < s.rules.len() {
                        s.rules[i] = rule;
                    }
                } else {
                    s.rules.push(rule);
                    s.editing = Some(s.rules.len() - 1);
                }
                rule_rows(s);
                invalidate(s);
                s.dirty = false;
                if let Some(args) = recent {
                    send(s, request("replace_mru_add", args));
                }
            }
            DELETE | UP | DOWN | TOGGLE => {
                let i = selection(s, RULES);
                if i < s.rules.len() {
                    match id {
                        DELETE => {
                            s.rules.remove(i);
                        }
                        UP if i > 0 => s.rules.swap(i, i - 1),
                        DOWN if i + 1 < s.rules.len() => s.rules.swap(i, i + 1),
                        TOGGLE => s.rules[i]["enabled"] = json!(s.rules[i]["enabled"] == false),
                        _ => {}
                    }
                    rule_rows(s);
                    s.editing = None;
                    s.snapshot = Value::Null;
                    form(s, p, &json!({}), "");
                    invalidate(s);
                }
            }
            EXCLUDE => {
                for id in ids(s) {
                    if let Some(i) = s.excluded.iter().position(|x| *x == id) {
                        s.excluded.remove(i);
                    } else {
                        s.excluded.push(id);
                    }
                }
                invalidate(s);
            }
            OVERRIDE_SAVE => {
                let t = text(s.controls[&OVERRIDE]);
                for id in ids(s) {
                    if t.is_empty() {
                        s.overrides.remove(&id);
                    } else {
                        s.overrides.insert(id, json!(t));
                    }
                }
                invalidate(s);
            }
            PREVIEW => {
                if s.dirty && !s.fields.is_empty() {
                    return Err("먼저 규칙 입력 확정 단추를 눌러 주세요.".into());
                }
                send(
                    s,
                    request(
                        "preview",
                        json!({"rules":s.rules,"overrides":s.overrides,"excluded":s.excluded,"on_conflict":if selection(s,CONFLICT)==1{"number"}else{"block"}}),
                    ),
                );
            }
            CONFLICT if code == 1 => {
                s.preview = None;
            }
            APPLY => {
                let plan = s.preview.as_ref().ok_or("먼저 미리보기를 확인해 주세요.")?;
                if confirm(
                    p,
                    &format!(
                        "{}개 이름을 바꿀까요? 실행 파일·확장자 변경 경고도 확인해 주세요.",
                        plan["summary"]["ok"]
                    ),
                ) {
                    let args = json!({"plan_id":plan["plan_id"],"confirm_executables":true});
                    send(s, request("apply", args));
                }
            }
            HISTORY => send(s, request("history", json!({}))),
            UNDO => {
                let i = selection(s, HISTORY_LIST);
                let run = s
                    .history
                    .get(i)
                    .ok_or("이력 조회 후 되돌릴 작업을 선택해 주세요.")?;
                if confirm(p, "선택한 작업의 파일명을 되돌릴까요?") {
                    send(s, request("undo", json!({"run_id":run["id"]})));
                }
            }
            SET_SAVE => send(
                s,
                request(
                    "ruleset_save",
                    json!({"name":text(s.controls[&SET_NAME]),"rules":s.rules}),
                ),
            ),
            SET_DELETE => {
                if let Some(set) = s.sets.get(selection(s, SET_LIST)) {
                    if confirm(p, "선택한 규칙 세트를 삭제할까요?") {
                        send(s, request("ruleset_delete", json!({"name":set["name"]})));
                    }
                }
            }
            SET_LOAD => {
                if let Some(set) = s.sets.get(selection(s, SET_LIST)) {
                    s.rules = set["rules"].as_array().cloned().unwrap_or_default();
                    rule_rows(s);
                    s.preview = None;
                    s.dirty = false;
                }
            }
            SET_IMPORT => {
                if let Some(path) = pick_files(p).first() {
                    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                    if bytes.len() > 1_000_000 {
                        return Err("세트 파일이 너무 큽니다.".into());
                    }
                    let raw = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                    send(s, request("ruleset_import", json!({"json":raw})));
                }
            }
            SET_EXPORT => {
                if let Some(set) = s.sets.get(selection(s, SET_LIST)) {
                    send(s, request("ruleset_export", json!({"name":set["name"]})));
                }
            }
            _ => {}
        }
    } else {
        let page = selection(s, PAGE);
        match id {
            WRITE if page == 4 => {
                if confirm(
                    p,
                    "열려 있는 Rust 창의 표시 상태와 검색 프로그램 설정을 적용할까요?",
                ) {
                    send(
                        s,
                        json!({"op":"runtime_settings","payload":{"action":"set","values":form_values(s)?}}),
                    );
                }
            }
            PAGE if code == 1 => {
                if s.dirty && !confirm(p, "편집 중인 상용구 입력을 버릴까요?") {
                    unsafe {
                        SendMessageW(s.controls[&PAGE], CB_SETCURSEL, s.current_page, 0);
                    }
                    return Ok(());
                }
                s.dirty = false;
                form(s, p, &json!({}), "");
                refresh_page(s);
            }
            REFRESH => {
                if !s.dirty || confirm(p, "저장하지 않은 입력을 버리고 다시 읽을까요?")
                {
                    refresh_page(s);
                }
            }
            KVF_ON | KVF_OFF | KVF_START => {
                if confirm(p, "가상 폴더 실행 상태와 자동 실행 설정을 변경할까요?")
                {
                    send(
                        s,
                        json!({"op":"kvf","payload":{"action":if id==KVF_OFF{"disable"}else{"enable"},"args":{"autostart":unsafe{SendMessageW(s.controls[&KVF_START],BM_GETCHECK,0,0)}==1}}}),
                    );
                } else if id == KVF_START {
                    unsafe {
                        SendMessageW(
                            s.controls[&KVF_START],
                            BM_SETCHECK,
                            usize::from(s.snapshot["autostart"] == true),
                            0,
                        );
                    }
                }
            }
            OPEN_CONFIG => {
                if let Some(path) =
                    s.snapshot["config_files"][if page == 1 { "body" } else { "text" }].as_str()
                {
                    let path = Path::new(path);
                    if path.is_file() {
                        open_path(path);
                    }
                }
            }
            FILES if code == 1 && page < 2 => {
                if s.dirty && !confirm(p, "편집 중인 상용구 입력을 버릴까요?") {
                    if let Some(previous) = s.editing {
                        unsafe {
                            SendMessageW(s.controls[&FILES], 0x186, previous, 0);
                        }
                    }
                    return Ok(());
                }
                let i = selection(s, FILES);
                let key = if page == 0 { "text" } else { "body" };
                if let Some(item) = s.snapshot[key].get(i).cloned() {
                    form(
                        s,
                        p,
                        &json!({"key":item["key"],"value":item.get("value").or(item.get("title")).cloned().unwrap_or(json!(""))}),
                        "",
                    );
                    s.editing = Some(i);
                    s.dirty = false;
                }
            }
            ITEM_ADD if page < 2 => {
                s.editing = None;
                form(s, p, &json!({"key":"","value":""}), "");
                s.dirty = false;
            }
            WRITE | ITEM_DELETE if page < 2 => {
                if s.snapshot["source"] == "hwpx" {
                    return Err("상용구 설정 원본 열기에서 HWPX 문서를 수정해 주세요.".into());
                }
                let key = if page == 0 { "text" } else { "body" };
                let mut entries = s.snapshot[key].as_array().cloned().unwrap_or_default();
                if id == ITEM_DELETE {
                    let i = selection(s, FILES);
                    if i < entries.len() && confirm(p, "선택 상용구를 삭제할까요?") {
                        entries.remove(i);
                    } else {
                        return Ok(());
                    }
                } else {
                    let mut item = form_values(s)?;
                    if page == 1 {
                        item["title"] = item["value"].clone();
                    }
                    if let Some(i) = s.editing {
                        if i < entries.len() {
                            item["old_key"] = entries[i]["key"].clone();
                            entries[i] = item;
                        }
                    } else if page == 1 {
                        let Some(path) = pick_files(p).first().cloned() else {
                            return Ok(());
                        };
                        if confirm(p, "본문 HWP를 상용구로 등록할까요?") {
                            send(
                                s,
                                json!({"op":"idioms","payload":{"action":"add_body","args":{"key":item["key"],"title":item["title"],"path":path}}}),
                            );
                        }
                        return Ok(());
                    } else {
                        entries.push(item);
                    }
                }
                if confirm(p, "상용구를 저장할까요? 한/글을 종료한 뒤 진행해 주세요.")
                {
                    send(
                        s,
                        json!({"op":"idioms","payload":{"action":if page==0{"save_text"}else{"save_body"},"args":{"entries":entries}}}),
                    );
                }
            }
            IMPORT_APP => {
                if confirm(p, "내장 상용구를 기존 목록에 추가할까요?") {
                    send(s, json!({"op":"idioms","payload":{"action":"import_app"}}));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn response(s: &mut State, p: HWND, r: Value, reply: Value) {
    if reply["ok"] == false {
        if r["op"] == "job_status" {
            s.job = None;
            s.cancel_requested = false;
            s.cancel_sent = false;
            busy_controls(s);
        }
        set_status(s, reply["message"].as_str().unwrap_or("작업 실패"));
        return;
    }
    if let Some(id) = reply["job_id"].as_str() {
        if r["op"] != "job_status" && r["op"] != "cancel" {
            s.job = Some(id.into());
        }
    }
    if r["op"] == "job_status" {
        if matches!(
            reply["state"].as_str(),
            Some("done" | "failed" | "cancelled")
        ) {
            s.job = None;
            s.cancel_requested = false;
            s.cancel_sent = false;
            s.preview = None;
            s.dirty = false;
            let result = reply["result"].clone();
            let summary = if reply["state"] == "failed" {
                format!(
                    "실패: {}",
                    reply["error"]["message"].as_str().unwrap_or("작업 실패")
                )
            } else {
                format!(
                    "{} · 처리 {}개 · 실패 {}개 · 제외 {}개",
                    if reply["state"] == "cancelled" {
                        "중단"
                    } else {
                        "완료"
                    },
                    result["done"].as_u64().unwrap_or(0),
                    result["failed"].as_array().map(Vec::len).unwrap_or(0),
                    result["skipped"].as_u64().unwrap_or(0)
                )
            };
            set_status(s, &summary);
            s.last_result = Some(summary);
            if s.closing {
                unsafe {
                    DestroyWindow(p);
                }
                return;
            }
            send(s, request("list", json!({})));
        } else {
            set_status(
                s,
                &format!(
                    "진행 {} / {} · {}%",
                    reply["progress"]["file"],
                    reply["progress"]["total"],
                    reply["progress"]["percent"]
                ),
            );
        }
        return;
    }
    if let Some(entries) = reply["entries"].as_array() {
        s.entries = entries.clone();
        list(
            s,
            FILES,
            s.entries.iter().map(|e| {
                format!(
                    "{}   {}",
                    e["name"].as_str().unwrap_or(""),
                    e["folder"].as_str().unwrap_or("")
                )
            }),
        );
        if r["payload"]["action"] == "list" {
            send(s, request("ruleset_list", json!({})));
        }
    }
    if r["payload"]["action"] == "preview" {
        s.preview = Some(reply.clone());
        let names: BTreeMap<_, _> = s
            .entries
            .iter()
            .filter_map(|e| Some((e["id"].as_str()?, e["name"].as_str()?)))
            .collect();
        list(
            s,
            FILES,
            reply["items"].as_array().into_iter().flatten().map(|e| {
                format!(
                    "{} → {}  [{}] {} {}",
                    names.get(e["id"].as_str().unwrap_or("")).unwrap_or(&""),
                    e["new_name"].as_str().unwrap_or(""),
                    match e["status"].as_str() {
                        Some("ok") => "변경",
                        Some("unchanged") => "유지",
                        Some("conflict") => "충돌",
                        Some("invalid") => "이름 오류",
                        Some("skipped") => "제외",
                        _ => "",
                    },
                    e["reason"].as_str().unwrap_or(""),
                    e["warnings"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|v| match v.as_str() {
                            Some("ext_changed") => Some("확장자 변경"),
                            Some("executable") => Some("실행 파일"),
                            Some("long_path") => Some("긴 경로"),
                            Some("exif_missing") => Some("촬영일 없음"),
                            Some("title_unsupported") => Some("제목 미지원"),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ? ")
                )
            }),
        );
        set_status(
            s,
            &format!(
                "미리보기 · 변경 {} · 유지 {} · 충돌 {} · 이름 오류 {} · 제외 {}",
                reply["summary"]["ok"],
                reply["summary"]["unchanged"],
                reply["summary"]["conflict"],
                reply["summary"]["invalid"],
                reply["summary"]["skipped"]
            ),
        );
        return;
    }
    if let Some(sets) = reply["rulesets"].as_array() {
        s.sets = sets.clone();
        unsafe {
            SendMessageW(s.controls[&SET_LIST], CB_RESETCONTENT, 0, 0);
            for set in &s.sets {
                SendMessageW(
                    s.controls[&SET_LIST],
                    CB_ADDSTRING,
                    0,
                    wide(set["name"].as_str().unwrap_or("")).as_ptr() as isize,
                );
            }
            SendMessageW(s.controls[&SET_LIST], CB_SETCURSEL, 0, 0);
        }
    }
    if let Some(runs) = reply["runs"].as_array() {
        s.history = runs.clone();
        unsafe {
            SendMessageW(s.controls[&HISTORY_LIST], CB_RESETCONTENT, 0, 0);
            for run in &s.history {
                SendMessageW(
                    s.controls[&HISTORY_LIST],
                    CB_ADDSTRING,
                    0,
                    wide(&format!(
                        "{} · {}개 · {}",
                        run["when"], run["count"], run["first_name"]
                    ))
                    .as_ptr() as isize,
                );
            }
            SendMessageW(s.controls[&HISTORY_LIST], CB_SETCURSEL, 0, 0);
        }
    }
    if let Some(raw) = reply["json"].as_str() {
        if let Some(folder) = pick_folder(p) {
            let path = folder.join(format!("{}.json", reply["name"].as_str().unwrap_or("규칙")));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    if let Err(e) = file.write_all(raw.as_bytes()) {
                        set_status(s, &e.to_string());
                        return;
                    }
                }
                Err(e) => {
                    set_status(s, &e.to_string());
                    return;
                }
            }
        }
    }
    if r["payload"]["action"] == "replace_mru_list" {
        unsafe {
            let menu = CreatePopupMenu();
            let pairs = reply["values"].as_array().cloned().unwrap_or_default();
            for (i, pair) in pairs.iter().enumerate() {
                AppendMenuW(
                    menu,
                    MF_STRING,
                    i + 1,
                    wide(&format!(
                        "{} → {}",
                        pair["find"].as_str().unwrap_or(""),
                        pair["to"].as_str().unwrap_or("")
                    ))
                    .as_ptr(),
                );
            }
            let mut point = POINT { x: 0, y: 0 };
            GetCursorPos(&mut point);
            let choice = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY,
                point.x,
                point.y,
                0,
                p,
                null(),
            );
            DestroyMenu(menu);
            if choice > 0 {
                if let Some(pair) = pairs.get(choice as usize - 1) {
                    for f in &s.fields {
                        if f.key == "find" || f.key == "to" {
                            SetWindowTextW(
                                f.hwnd,
                                wide(pair[&f.key].as_str().unwrap_or("")).as_ptr(),
                            );
                        }
                    }
                    s.dirty = true;
                }
            }
        }
    }
    if !s.rename {
        s.snapshot = reply.clone();
        s.dirty = false;
        let page = selection(s, PAGE);
        unsafe {
            let editable = page < 2 && reply["source"] != "hwpx" && reply["available"] == true;
            for id in [WRITE, ITEM_ADD, ITEM_DELETE, IMPORT_APP] {
                EnableWindow(s.controls[&id], i32::from(editable));
            }
            EnableWindow(
                s.controls[&OPEN_CONFIG],
                i32::from(page < 2 && reply["source"] == "hwpx"),
            );
            EnableWindow(
                s.controls[&KVF_START],
                i32::from(page == 2 && reply["running"] == true),
            );
            EnableWindow(s.controls[&KVF_ON], i32::from(page == 2));
            EnableWindow(s.controls[&KVF_OFF], i32::from(page == 2));
            EnableWindow(s.controls[&WRITE], i32::from(editable || page == 4));
            SetWindowTextW(
                s.controls[&WRITE],
                wide(if page == 4 {
                    "변경 적용"
                } else {
                    "선택 상용구 저장"
                })
                .as_ptr(),
            );
        }
        if page == 4 {
            form(s, p, &reply["values"], "");
            s.dirty = false;
            list(
                s,
                FILES,
                [
                    "항상 위에 표시: 열려 있는 Rust 창에 바로 반영합니다.".to_owned(),
                    "퀵몽땅검색: 켜기·끄기와 로그인 자동 실행을 함께 적용합니다.".to_owned(),
                ]
                .into_iter(),
            );
            if r["payload"]["action"] == "set" {
                apply_to_open_windows(reply["values"]["always_on_top"] == true);
            }
        } else if page < 2 {
            list(
                s,
                FILES,
                reply[if page == 0 { "text" } else { "body" }]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|i| {
                        format!(
                            "{} → {}",
                            i["key"].as_str().unwrap_or(""),
                            i.get("value")
                                .or(i.get("title"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                        )
                    }),
            );
        } else {
            let rows = if page == 2 {
                vec![
                    format!(
                        "가상 폴더 창: {}",
                        if reply["running"] == true {
                            "실행 중"
                        } else {
                            "꺼짐"
                        }
                    ),
                    format!(
                        "로그인 때 자동 실행: {}",
                        if reply["autostart"] == true {
                            "켬"
                        } else {
                            "끔"
                        }
                    ),
                    reply["status"]["error"].as_str().unwrap_or("").to_owned(),
                ]
            } else {
                vec![
                    format!("앱 버전: {}", reply["version"].as_str().unwrap_or("")),
                    format!("개발자: {}", reply["developer"].as_str().unwrap_or("")),
                    format!("화면: {}", reply["ui"].as_str().unwrap_or("")),
                    format!("Python: {}", reply["python"].as_str().unwrap_or("")),
                    format!(
                        "함께 쓰는 코드: {}",
                        reply["licenses"].as_str().unwrap_or("")
                    ),
                ]
            };
            list(s, FILES, rows.into_iter());
            if page == 2 {
                unsafe {
                    SendMessageW(
                        s.controls[&KVF_START],
                        BM_SETCHECK,
                        usize::from(reply["autostart"] == true),
                        0,
                    );
                }
            }
        }
    }
    if s.pending.is_some() {
        return;
    }
    if matches!(
        r["payload"]["action"].as_str(),
        Some("list" | "ruleset_list")
    ) {
        if let Some(summary) = &s.last_result {
            set_status(s, summary);
            return;
        }
    }
    set_status(
        s,
        if s.job.is_some() {
            "작업 중 · 중단 단추로 안전하게 취소합니다."
        } else {
            "완료"
        },
    );
}

unsafe extern "system" fn proc(p: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lp as *const CREATESTRUCTW);
            SetWindowLongPtrW(p, GWLP_USERDATA, cs.lpCreateParams as isize);
            return 1;
        }
        if GetWindowLongPtrW(p, GWLP_USERDATA) == 0 {
            return DefWindowProcW(p, msg, wp, lp);
        }
        match msg {
            WM_COMMAND => {
                let s = st(p);
                if let Err(e) = action(s, p, wp & 0xffff, wp >> 16) {
                    set_status(s, &e);
                }
                0
            }
            WM_DROPFILES => {
                let s = st(p);
                let drop = wp as HDROP;
                let n = DragQueryFileW(drop, u32::MAX, null_mut(), 0);
                let mut paths = vec![];
                for i in 0..n {
                    let len = DragQueryFileW(drop, i, null_mut(), 0);
                    let mut buf = vec![0; len as usize + 1];
                    DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32);
                    paths.push(String::from_utf16_lossy(&buf[..len as usize]));
                }
                DragFinish(drop);
                if s.rename {
                    s.deferred_paths.extend(paths);
                }
                0
            }
            WM_TIMER => {
                let s = st(p);
                if let Ok(reply) = s.rx.try_recv() {
                    let r = s.pending.take().unwrap_or(Value::Null);
                    busy_controls(s);
                    match reply {
                        Ok(v) => response(s, p, r, v),
                        Err(e) => {
                            set_status(s, &e);
                            s.job = None;
                            s.cancel_requested = false;
                            s.cancel_sent = false;
                            busy_controls(s);
                        }
                    }
                }
                if s.pending.is_none() {
                    if s.job.is_none() && !s.deferred_paths.is_empty() {
                        let paths = std::mem::take(&mut s.deferred_paths);
                        s.preview = None;
                        send(s, request("add_paths", json!({"paths":paths})));
                    } else if let Some(id) = s.job.clone() {
                        if s.cancel_requested {
                            s.cancel_requested = false;
                            s.cancel_sent = true;
                            send(s, json!({"op":"cancel","payload":{"job_id":id}}));
                        } else {
                            send(s, json!({"op":"job_status","payload":{"job_id":id}}));
                        }
                    }
                }
                0
            }
            WM_CLOSE => {
                let s = st(p);
                if s.job.is_some() {
                    if !s.closing
                        && confirm(
                            p,
                            "작업을 중단하고 닫을까요? 이미 처리한 파일은 이력으로 남습니다.",
                        )
                    {
                        s.closing = true;
                        s.cancel_requested = true;
                    }
                    return 0;
                }
                if s.pending.is_some() {
                    set_status(s, "요청 처리가 끝난 뒤 닫아 주세요.");
                    return 0;
                }
                if !s.dirty || confirm(p, "저장하지 않은 입력을 버리고 닫을까요?") {
                    DestroyWindow(p);
                }
                0
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lp as *mut MINMAXINFO);
                let d = st(p).dpi;
                info.ptMinTrackSize = POINT {
                    x: sc(1095, d),
                    y: sc(730, d),
                };
                0
            }
            WM_DPICHANGED => {
                let s = st(p);
                let d = (wp & 0xffff) as u32;
                let ratio = d as f64 / s.dpi as f64;
                for handle in s.controls.values() {
                    let mut r: RECT = std::mem::zeroed();
                    GetWindowRect(*handle, &mut r);
                    let mut pt = POINT {
                        x: r.left,
                        y: r.top,
                    };
                    ScreenToClient(p, &mut pt);
                    SetWindowPos(
                        *handle,
                        null_mut(),
                        (pt.x as f64 * ratio) as i32,
                        (pt.y as f64 * ratio) as i32,
                        ((r.right - r.left) as f64 * ratio) as i32,
                        ((r.bottom - r.top) as f64 * ratio) as i32,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                s.dpi = d;
                let r = &*(lp as *const RECT);
                SetWindowPos(
                    p,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                0
            }
            WM_DESTROY => {
                KillTimer(p, 1);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(p, msg, wp, lp),
        }
    }
}

pub fn run(rename: bool, data: PathBuf) -> Result<(), String> {
    let settings = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == "--settings")
        .map(|p| PathBuf::from(&p[1]))
        .unwrap_or_else(|| data.join("settings.json"));
    let (tx, requests) = mpsc::channel();
    let (answers, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut client = Client::start(&data);
        while let Ok(r) = requests.recv() {
            let reply = match &mut client {
                Ok(c) => c.call(&r),
                Err(e) => Err(e.clone()),
            };
            if answers.send(reply).is_err() {
                break;
            }
        }
    });
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = windows_sys::Win32::System::Com::CoInitializeEx(null(), 2);
        let dpi = GetDpiForSystem().max(96);
        let font = CreateFontW(
            -sc(14, dpi),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            0,
            wide("맑은 고딕").as_ptr(),
        );
        let class = wide(if rename {
            "DocFitRenameWin2"
        } else {
            "DocFitSettingsManage"
        });
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as _,
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let mut state = Box::new(State {
            rename,
            controls: BTreeMap::new(),
            fields: vec![],
            rules: vec![],
            entries: vec![],
            preview: None,
            overrides: serde_json::Map::new(),
            excluded: vec![],
            editing: None,
            sets: vec![],
            history: vec![],
            snapshot: Value::Null,
            current_page: 0,
            deferred_paths: Vec::new(),
            dirty: false,
            quiet: false,
            tx,
            rx,
            pending: None,
            job: None,
            closing: false,
            cancel_requested: false,
            cancel_sent: false,
            last_result: None,
            dpi,
            font,
        });
        let h = CreateWindowExW(
            0,
            class.as_ptr(),
            wide(if rename {
                "파일명 일괄변경"
            } else {
                "설정 관리 — 상용구·가상 폴더·앱 정보"
            })
            .as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            sc(1100, dpi),
            sc(735, dpi),
            null_mut(),
            null_mut(),
            instance,
            (&mut *state as *mut State).cast(),
        );
        if h.is_null() {
            return Err("관리 창을 만들지 못했습니다.".into());
        }
        // status는 첫 요청을 보내기 전에 있어야 한다.
        label(&mut state, h, STATUS, "준비", 16, 657, 1040);
        setup(&mut state, h);
        DragAcceptFiles(h, 1);
        SetTimer(h, 1, 150, None);
        ShowWindow(h, SW_SHOW);
        apply_saved_topmost(h, &settings);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        DeleteObject(font);
        windows_sys::Win32::System::Com::CoUninitialize();
        drop(state);
        Ok(())
    }
}

unsafe extern "system" fn set_open_topmost(h: HWND, value: LPARAM) -> BOOL {
    unsafe {
        let mut class = [0u16; 128];
        let n = GetClassNameW(h, class.as_mut_ptr(), 128);
        let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
        if matches!(
            class.as_str(),
            "SettingsWinMain"
                | "DocFitSettingsManage"
                | "DocFitRenameWin2"
                | "ConvertWinStage1"
                | "DocViewWinMain"
                | "PhotoViewWinMain"
                | "OutlinerWinMain"
        ) {
            SetWindowPos(
                h,
                if value != 0 {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        1
    }
}
fn apply_to_open_windows(value: bool) {
    unsafe {
        EnumWindows(Some(set_open_topmost), isize::from(value));
    }
}
pub fn apply_saved_topmost(h: HWND, path: &Path) {
    if let Ok(raw) = docfit_config::read_json(path) {
        let values = docfit_config::merged(&raw);
        unsafe {
            SetWindowPos(
                h,
                if values["always_on_top"] == true {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}
