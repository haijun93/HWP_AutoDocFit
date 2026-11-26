//! 두 네이티브 창의 메뉴·대화상자·작업 상태. 모달 호출은 창 State 참조를 놓은 뒤 사용한다.
use crate::{
    file_ops::{FileResult, Operation, Request, Task},
    items::Doc,
};
use std::path::PathBuf;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Preparing,
    Running,
    Complete,
}

#[derive(Default)]
pub struct NativeOps {
    pub phase: Phase,
    pub task: Option<Task>,
    pub last_results: Vec<FileResult>,
}
impl NativeOps {
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Preparing | Phase::Running)
    }
    pub fn start(&mut self, owner: HWND, request: Request) -> Result<(), String> {
        if self.task.is_some() {
            return Err("이전 파일 작업이 끝난 뒤 다시 시도해 주세요".into());
        }
        match Task::start(
            request,
            crate::file_ops_windows::SystemBackend {
                owner: owner as isize,
            },
        ) {
            Ok(task) => {
                self.task = Some(task);
                self.last_results.clear();
                self.phase = Phase::Running;
                Ok(())
            }
            Err(e) => {
                self.phase = Phase::Complete;
                Err(e.to_string())
            }
        }
    }
    pub fn poll(&mut self) -> Option<(Operation, Vec<FileResult>)> {
        if !self.task.as_mut()?.poll() {
            return None;
        }
        let task = self.task.take().unwrap();
        self.last_results = task.results;
        self.phase = Phase::Complete;
        Some((task.op, self.last_results.clone()))
    }
    pub fn progress(&self) -> Option<String> {
        self.task
            .as_ref()
            .map(|t| format!("파일 작업 중 {}/{}", t.results.len(), t.total))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Clipboard,
    Copy,
    Move,
    Rename,
    Delete,
}
impl Action {
    pub fn enabled(self, count: usize, busy: bool) -> bool {
        !busy
            && count > 0
            && count <= 5000
            && (count == 1 || !matches!(self, Self::Open | Self::Rename))
    }
}

pub fn shortcut(vk: u32, ctrl: bool, shift: bool) -> Option<Action> {
    // Shift+Delete도 반드시 같은 휴지통 확인 경로로 간다.
    match vk as u16 {
        VK_DELETE => Some(Action::Delete),
        VK_F2 if !ctrl && !shift => Some(Action::Rename),
        0x43 if ctrl && !shift => Some(Action::Clipboard),
        _ => None,
    }
}

/// 제목 표시줄의 시스템 메뉴는 기본 창 처리에 맡긴다.
pub fn client_context(owner: HWND, x: i32, y: i32) -> bool {
    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::ScreenToClient,
    };
    if x == -1 && y == -1 {
        return true;
    }
    unsafe {
        let mut point = POINT { x, y };
        let mut rect: RECT = std::mem::zeroed();
        ScreenToClient(owner, &mut point);
        GetClientRect(owner, &mut rect);
        point.x >= 0 && point.y >= 0 && point.x < rect.right && point.y < rect.bottom
    }
}

/// 오른쪽 메뉴에서 고른 것: 파일 작업 또는 다른 창(메뉴 번호)으로 보내기.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    File(Action),
    Send(u8),
    Extra(u8),
}

/// 창마다 다른 메뉴 줄(파일 작업 위에 붙는다).
pub struct Extra {
    pub id: u8,
    pub label: String,
    pub enabled: bool,
}

/// 창 고유 줄의 명령 번호.
const EXTRA_ID: usize = 400;

/// 보내기 줄의 명령 번호는 파일 작업(1부터)과 겹치지 않게 이 값부터다.
const SEND_ID: usize = 100;

fn choice(id: usize, actions: &[Action]) -> Option<Choice> {
    match id {
        0 => None,
        id if id >= EXTRA_ID => u8::try_from(id - EXTRA_ID).ok().map(Choice::Extra),
        id if id >= SEND_ID => u8::try_from(id - SEND_ID).ok().map(Choice::Send),
        id => actions.get(id - 1).copied().map(Choice::File),
    }
}

/// 파일 작업 아래에 `send` 줄을 붙인 메뉴 하나. 메시지 루프를 돌므로 창 상태 참조를 쥐지 않고 부른다.
pub fn menu(
    owner: HWND,
    x: i32,
    y: i32,
    count: usize,
    busy: bool,
    send: &[crate::send_menu::Entry],
    extra: &[Extra],
) -> Option<Choice> {
    let actions = [
        Action::Open,
        Action::Clipboard,
        Action::Copy,
        Action::Move,
        Action::Rename,
        Action::Delete,
    ];
    let labels = [
        "열기",
        "복사\tCtrl+C",
        "폴더로 복사…",
        "이동…",
        "이름 변경…\tF2",
        "삭제(휴지통)…\tDelete",
    ];
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return None;
        }
        for entry in extra {
            AppendMenuW(
                menu,
                MF_STRING | if entry.enabled && !busy { 0 } else { MF_GRAYED },
                EXTRA_ID + entry.id as usize,
                wide(&entry.label).as_ptr(),
            );
        }
        if !extra.is_empty() {
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        }
        for (i, (action, label)) in actions.iter().zip(labels).enumerate() {
            let label = if !action.enabled(count, busy) {
                format!(
                    "{label} ({})",
                    if busy {
                        "작업 중"
                    } else if count == 0 {
                        "선택 필요"
                    } else if count > 5000 {
                        "5000개까지"
                    } else {
                        "1개 선택 필요"
                    }
                )
            } else {
                label.into()
            };
            AppendMenuW(
                menu,
                MF_STRING
                    | if action.enabled(count, busy) {
                        0
                    } else {
                        MF_GRAYED
                    },
                i + 1,
                wide(&label).as_ptr(),
            );
        }
        if !send.is_empty() {
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        }
        for entry in send {
            AppendMenuW(
                menu,
                MF_STRING | if entry.enabled && !busy { 0 } else { MF_GRAYED },
                SEND_ID + entry.menu as usize,
                wide(&entry.label).as_ptr(),
            );
        }
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            x,
            y,
            0,
            owner,
            std::ptr::null(),
        );
        DestroyMenu(menu);
        choice(usize::try_from(chosen).unwrap_or(0), &actions)
    }
}

pub fn prepare(
    owner: HWND,
    action: Action,
    docs: Vec<Doc>,
    extensions: &[&str],
) -> Option<Request> {
    if !action.enabled(docs.len(), false) {
        return None;
    }
    let op = match action {
        Action::Clipboard => Operation::Clipboard,
        Action::Copy => Operation::Copy,
        Action::Move => Operation::Move,
        Action::Rename => Operation::Rename,
        Action::Delete => Operation::Delete,
        Action::Open => return None,
    };
    let mut request = Request {
        op,
        docs,
        extensions: extensions.iter().map(|s| s.to_string()).collect(),
        destination: None,
        name: String::new(),
    };
    match op {
        Operation::Copy | Operation::Move => {
            request.destination = Some(pick_folder(owner)?);
        }
        Operation::Rename => {
            let source = std::path::Path::new(&request.docs[0].path);
            let stem = source.file_stem()?.to_string_lossy();
            let ext = source.extension()?.to_string_lossy();
            request.name = text_dialog(
                owner,
                "이름 변경",
                &format!("확장자 .{ext}는 고정됩니다. 새 이름을 입력하세요."),
                &stem,
                true,
            )?;
        }
        Operation::Delete => {
            let names = request
                .docs
                .iter()
                .take(12)
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let rest = if request.docs.len() > 12 {
                format!("\n외 {}개", request.docs.len() - 12)
            } else {
                String::new()
            };
            let prompt = format!(
                "선택한 {}개 파일을 휴지통으로 보낼까요?\n휴지통을 쓸 수 없으면 삭제하지 않습니다.\n\n{names}{rest}",
                request.docs.len()
            );
            if unsafe {
                MessageBoxW(
                    owner,
                    wide(&prompt).as_ptr(),
                    wide("삭제 확인").as_ptr(),
                    MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2,
                )
            } != IDOK
            {
                return None;
            }
        }
        _ => {}
    }
    Some(request)
}

pub fn show_results(owner: HWND, results: &[FileResult]) {
    let failures: Vec<_> = results.iter().filter(|r| r.error.is_some()).collect();
    if failures.is_empty() {
        return;
    }
    let text = failures
        .iter()
        .map(|r| {
            format!(
                "{}\r\n{}",
                r.source.path,
                r.error.as_deref().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\r\n\r\n");
    text_dialog(
        owner,
        "파일별 작업 결과",
        &format!(
            "성공 {}개 · 실패 {}개",
            results.len() - failures.len(),
            failures.len()
        ),
        &text,
        false,
    );
}

pub fn pick_folder(owner: HWND) -> Option<PathBuf> {
    use windows::{
        Win32::{Foundation::HWND as Window, System::Com::*, UI::Shell::*},
        core::PCWSTR,
    };
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| -> windows::core::Result<PathBuf> {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(
                FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR,
            )?;
            dialog.SetTitle(PCWSTR(wide("대상 폴더를 선택하세요").as_ptr()))?;
            dialog.Show(Some(Window(owner)))?;
            let item = dialog.GetResult()?;
            let name = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let text = name.to_string();
            CoTaskMemFree(Some(name.0.cast()));
            Ok(PathBuf::from(text?))
        })();
        if initialized {
            CoUninitialize();
        }
        result.ok()
    }
}

struct TextDialog {
    description: String,
    value: String,
    editable: bool,
}
const DIALOG_USER: i32 = (2 * std::mem::size_of::<isize>()) as i32;
unsafe extern "system" fn dialog_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
    unsafe {
        if msg == WM_INITDIALOG {
            SetWindowLongPtrW(hwnd, DIALOG_USER, lp);
            let data = &*(lp as *const TextDialog);
            SetDlgItemTextW(hwnd, 101, wide(&data.description).as_ptr());
            SendDlgItemMessageW(
                hwnd,
                100,
                0x00C5, /* EM_SETLIMITTEXT */
                data.value.encode_utf16().count().max(1024) + 1,
                0,
            );
            SetDlgItemTextW(hwnd, 100, wide(&data.value).as_ptr());
            if data.editable {
                SendDlgItemMessageW(hwnd, 100, 0x00B1, 0, -1);
                SetFocus(GetDlgItem(hwnd, 100));
                return 0;
            }
            return 1;
        }
        if msg == WM_CLOSE {
            EndDialog(hwnd, IDCANCEL as isize);
            return 1;
        }
        if msg == WM_COMMAND {
            let id = (wp & 0xffff) as i32;
            if id == IDOK || id == IDCANCEL {
                let data = &mut *(GetWindowLongPtrW(hwnd, DIALOG_USER) as *mut TextDialog);
                if id == IDOK && data.editable {
                    let mut text = vec![0u16; 1024];
                    let n = GetDlgItemTextW(hwnd, 100, text.as_mut_ptr(), text.len() as i32);
                    data.value = String::from_utf16_lossy(&text[..n as usize]);
                }
                EndDialog(hwnd, id as isize);
                return 1;
            }
        }
        0
    }
}

/// 메모리 대화상자: 이름 입력 또는 모든 실패를 스크롤할 수 있는 읽기 전용 결과창.
pub fn text_dialog(
    owner: HWND,
    title: &str,
    description: &str,
    value: &str,
    editable: bool,
) -> Option<String> {
    let mut words: Vec<u16> = Vec::new();
    fn dword(v: &mut Vec<u16>, n: u32) {
        v.extend([n as u16, (n >> 16) as u16]);
    }
    fn item(v: &mut Vec<u16>, style: u32, rect: [u16; 4], id: u16, class: u16, text: &str) {
        if v.len() % 2 != 0 {
            v.push(0);
        }
        dword(v, style | WS_CHILD | WS_VISIBLE);
        dword(v, 0);
        v.extend(rect);
        v.extend([id, 0xffff, class]);
        v.extend(wide(text));
        v.push(0);
    }
    dword(
        &mut words,
        WS_CAPTION | WS_SYSMENU | DS_MODALFRAME as u32 | DS_SETFONT as u32,
    );
    dword(&mut words, 0);
    let height = if editable { 94 } else { 250 };
    words.extend([4, 0, 0, 310, height, 0, 0]);
    words.extend(wide(title));
    words.push(9);
    words.extend(wide("Segoe UI"));
    item(&mut words, 0, [10, 8, 290, 24], 101, 0x82, "");
    let edit_height = if editable { 16 } else { 180 };
    let edit_style = if editable {
        ES_AUTOHSCROLL as u32
    } else {
        ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32 | WS_VSCROLL
    };
    item(
        &mut words,
        WS_BORDER | WS_TABSTOP | edit_style,
        [10, 34, 290, edit_height],
        100,
        0x81,
        "",
    );
    item(
        &mut words,
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        [192, height - 24, 50, 14],
        IDOK as u16,
        0x80,
        "확인",
    );
    item(
        &mut words,
        WS_TABSTOP,
        [248, height - 24, 50, 14],
        IDCANCEL as u16,
        0x80,
        "취소",
    );
    if words.len() % 2 != 0 {
        words.push(0);
    }
    let template: Vec<u32> = words
        .chunks_exact(2)
        .map(|p| p[0] as u32 | ((p[1] as u32) << 16))
        .collect();
    let mut data = TextDialog {
        description: description.into(),
        value: value.into(),
        editable,
    };
    let result = unsafe {
        DialogBoxIndirectParamW(
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()),
            template.as_ptr().cast(),
            owner,
            Some(dialog_proc),
            &mut data as *mut _ as LPARAM,
        )
    };
    if result == IDOK as isize {
        Some(data.value)
    } else {
        None
    }
}

pub fn hdrop_payload(paths: &[PathBuf]) -> Vec<u8> {
    let mut data = Vec::new();
    for n in [20u32, 0, 0, 0, 1] {
        data.extend(n.to_le_bytes());
    }
    for p in paths {
        for n in p.to_string_lossy().encode_utf16().chain(Some(0)) {
            data.extend(n.to_le_bytes());
        }
    }
    data.extend(0u16.to_le_bytes());
    data
}

pub fn listed_destination(path: &str, excludes: &[String]) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    !excludes.iter().any(|p| {
        let p = p.replace('\\', "/").to_lowercase();
        let p = p.trim_end_matches('/');
        normalized == p
            || normalized
                .strip_prefix(p)
                .is_some_and(|tail| tail.starts_with('/'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_numbers_separate_file_actions_from_send_targets() {
        let actions = [Action::Open, Action::Delete];
        assert_eq!(choice(0, &actions), None);
        assert_eq!(choice(2, &actions), Some(Choice::File(Action::Delete)));
        assert_eq!(choice(3, &actions), None);
        // 문서편집은 메뉴 번호 0 이다(취소의 0 과 섞이면 안 된다).
        assert_eq!(choice(SEND_ID, &actions), Some(Choice::Send(0)));
        assert_eq!(choice(SEND_ID + 13, &actions), Some(Choice::Send(13)));
        assert_eq!(choice(EXTRA_ID + 1, &actions), Some(Choice::Extra(1)));
    }
}
