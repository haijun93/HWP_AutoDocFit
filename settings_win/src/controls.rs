//! 복잡한 컨트롤의 보조 로직: 색 목록·단축키 규칙·글꼴 열거·폴더 선택 대화상자. 순수 부분은 창 없이 시험한다.

pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

/// 표시 색 목록(저장 값, 화면 이름, COLORREF). `hwp-auto-docfit.py` 의 `COLOR_OPTIONS` 와 같다.
pub const COLORS: [(&str, &str, u32); 9] = [
    ("red", "빨강", rgb(255, 0, 0)),
    ("orange", "주황", rgb(255, 128, 0)),
    ("green", "초록", rgb(0, 153, 0)),
    ("blue", "파랑", rgb(0, 0, 255)),
    ("skyblue", "하늘", rgb(0, 153, 255)),
    ("purple", "보라", rgb(128, 0, 192)),
    ("magenta", "자홍", rgb(224, 0, 160)),
    ("brown", "갈색", rgb(140, 70, 20)),
    ("gray", "회색", rgb(128, 128, 128)),
];

pub fn color_index(id: &str) -> Option<usize> {
    COLORS.iter().position(|c| c.0 == id)
}

/// 저장 값 → 화면 이름(목록에 없는 값은 그대로).
pub fn color_label(id: &str) -> String {
    color_index(id).map(|i| COLORS[i].1.to_string()).unwrap_or_else(|| id.to_string())
}

// ---------- 단축키 ----------

const MODIFIERS: [&str; 4] = ["Ctrl", "Alt", "Shift", "Win"];

/// 키 이름(문자·숫자·F1~F24·Space). 규칙은 kagent 설정 창의 단축키 입력과 같다(+ Space).
fn normalize_key(token: &str) -> Option<String> {
    let t = token.trim();
    if t.chars().count() == 1 {
        let c = t.chars().next()?;
        if c.is_ascii_alphanumeric() {
            return Some(c.to_ascii_uppercase().to_string());
        }
    }
    if t.eq_ignore_ascii_case("space") {
        return Some("Space".into());
    }
    let rest = t.strip_prefix('F').or_else(|| t.strip_prefix('f'))?;
    let n: u32 = rest.parse().ok()?;
    if (1..=24).contains(&n) && !rest.starts_with('0') {
        return Some(format!("F{n}"));
    }
    None
}

/// 단축키 글을 검사하고 `Ctrl+Alt+Shift+Win+키` 순서로 정리해 돌려준다. 빈 글은 "없음"으로 허용한다.
pub fn normalize_hotkey(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    let tokens: Vec<&str> = text.split('+').map(str::trim).collect();
    let Some((key_token, mods)) = tokens.split_last() else { return Err("단축키를 확인해 주세요.".into()) };
    let Some(key) = normalize_key(key_token) else {
        return Err("키는 문자·숫자·F1~F24·Space 중 하나여야 합니다.".into());
    };
    let mut have = [false; 4];
    for m in mods {
        let Some(i) = MODIFIERS.iter().position(|x| x.eq_ignore_ascii_case(m)) else {
            return Err(format!("'{m}'은(는) 알 수 없는 조합 키입니다. Ctrl·Alt·Shift·Win 만 쓸 수 있습니다."));
        };
        if have[i] {
            return Err(format!("{}이(가) 두 번 들어 있습니다.", MODIFIERS[i]));
        }
        have[i] = true;
    }
    if !have.iter().any(|h| *h) {
        return Err("Ctrl·Alt·Shift·Win 중 하나와 함께 지정해 주세요.".into());
    }
    let mut out: Vec<String> = MODIFIERS.iter().zip(have).filter(|(_, h)| *h).map(|(m, _)| m.to_string()).collect();
    out.push(key);
    Ok(out.join("+"))
}

/// 눌린 가상 키 코드 → 키 이름(조합 키 자체나 지원하지 않는 키는 None).
pub fn key_name_of_vk(vk: u32) -> Option<String> {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => Some((vk as u8 as char).to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x70 + 1)),
        0x20 => Some("Space".into()),
        _ => None,
    }
}

/// 키 입력 칸이 받은 키 한 번(조합 키 상태 + 가상 키)을 단축키 글로. 조합 키만 눌린 경우는 None(계속 기다린다).
pub fn hotkey_from_press(ctrl: bool, alt: bool, shift: bool, win: bool, vk: u32) -> Option<Result<String, String>> {
    if matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5) {
        return None;
    }
    let Some(key) = key_name_of_vk(vk) else {
        return Some(Err("문자·숫자·F1~F24·Space 키와 함께 눌러 주세요.".into()));
    };
    let mut parts: Vec<String> = Vec::new();
    for (on, name) in [(ctrl, "Ctrl"), (alt, "Alt"), (shift, "Shift"), (win, "Win")] {
        if on {
            parts.push(name.into());
        }
    }
    if parts.is_empty() {
        return Some(Err("Ctrl·Alt·Shift·Win 중 하나와 함께 눌러 주세요.".into()));
    }
    parts.push(key);
    Some(Ok(parts.join("+")))
}

// ---------- 글꼴 이름·폴더 경로 검사 ----------

pub fn explain_font(name: &str) -> Option<String> {
    let t = name.trim();
    if t.is_empty() {
        Some("글꼴 이름을 입력해 주세요.".into())
    } else if t.chars().count() > 64 {
        Some("글꼴 이름은 64자를 넘을 수 없습니다.".into())
    } else if t.chars().any(|c| c.is_control()) {
        Some("글꼴 이름에 쓸 수 없는 문자가 있습니다.".into())
    } else if name.chars().count() > 4096 || name.contains('\0') {
        Some("사용할 수 없는 문자가 있습니다.".into())
    } else {
        None
    }
}

/// 폴더 경로 글자 검사(존재 여부는 `folder_warning`). 비어 있으면 '지정 안 함'.
pub fn explain_folder(path: &str) -> Option<String> {
    let t = path.trim();
    if t.is_empty() {
        return None;
    }
    if t.chars().count() > 260 {
        return Some("경로는 260자를 넘을 수 없습니다.".into());
    }
    // 드라이브 글자의 ':' 와 구분자는 허용, 나머지 예약 문자는 거부.
    let body = if t.len() >= 2 && t.as_bytes()[1] == b':' { &t[2..] } else { t };
    if body.chars().any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || c.is_control()) {
        return Some("경로에 쓸 수 없는 문자(< > : \" | ? *)가 있습니다.".into());
    }
    None
}

/// 문제는 아니지만 알려 줄 것: 폴더가 지금은 없다.
pub fn folder_warning(path: &str) -> Option<String> {
    let t = path.trim();
    if t.is_empty() || explain_folder(t).is_some() {
        return None;
    }
    if std::path::Path::new(t).is_dir() { None } else { Some("지금은 이 폴더가 없습니다. 경로를 확인해 주세요.".into()) }
}

// ---------- 목록 ----------

pub const LIST_MAX_CHARS: usize = 1000;

/// 목록에 넣을 글을 다듬고 검사한다(공백 제거·빈 글·길이·NUL·중복(대소문자 무시)·개수).
pub fn check_list_entry(items: &[String], text: &str, max: usize) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("추가할 값을 입력해 주세요.".into());
    }
    if t.chars().count() > LIST_MAX_CHARS || t.contains('\0') {
        return Err(format!("{LIST_MAX_CHARS}자 이하, 사용할 수 없는 문자 없이 입력해 주세요."));
    }
    if items.iter().any(|i| i.to_lowercase() == t.to_lowercase()) {
        return Err("이미 목록에 있습니다.".into());
    }
    if items.len() >= max {
        return Err(format!("목록은 최대 {max}개까지 둘 수 있습니다."));
    }
    Ok(t.to_string())
}

// ---------- 글꼴 열거·폴더 대화상자(Win32) ----------

#[cfg(windows)]
pub use win::{installed_fonts, pick_folder};

#[cfg(windows)]
mod win {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
    use windows_sys::Win32::UI::Shell::{BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, SHBrowseForFolderW, SHGetPathFromIDListW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;

    unsafe extern "system" fn collect(lf: *const LOGFONTW, _tm: *const TEXTMETRICW, _kind: u32, lparam: LPARAM) -> i32 {
        unsafe {
            let names = &mut *(lparam as *mut Vec<String>);
            let face = &(*lf).lfFaceName;
            let len = face.iter().position(|c| *c == 0).unwrap_or(face.len());
            let name = String::from_utf16_lossy(&face[..len]);
            if !name.is_empty() && !name.starts_with('@') {
                names.push(name);
            }
            1
        }
    }

    /// Windows 에 설치된 글꼴 이름(중복 없음·가나다/알파벳 순).
    pub fn installed_fonts() -> Vec<String> {
        unsafe {
            let mut names: Vec<String> = Vec::new();
            let hdc = GetDC(null_mut());
            let mut lf: LOGFONTW = std::mem::zeroed();
            lf.lfCharSet = DEFAULT_CHARSET;
            EnumFontFamiliesExW(hdc, &lf, Some(collect), &mut names as *mut Vec<String> as LPARAM, 0);
            ReleaseDC(null_mut(), hdc);
            names.sort_by_key(|n| n.to_lowercase());
            names.dedup();
            names
        }
    }

    const BFFM_INITIALIZED: u32 = 1;
    const BFFM_SETSELECTIONW: u32 = 0x0400 + 103;

    unsafe extern "system" fn browse_cb(hwnd: HWND, msg: u32, _lp: LPARAM, data: LPARAM) -> i32 {
        if msg == BFFM_INITIALIZED && data != 0 {
            unsafe { SendMessageW(hwnd, BFFM_SETSELECTIONW, 1, data) };
        }
        0
    }

    /// 폴더 선택 대화상자(새 대화상자 스타일). 고르면 경로, 취소하면 None.
    pub fn pick_folder(owner: HWND, title: &str, initial: &str) -> Option<String> {
        unsafe {
            CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
            let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
            let title = wide(title);
            let initial = wide(initial.trim());
            let mut display = vec![0u16; 260];
            let info = BROWSEINFOW {
                hwndOwner: owner,
                pidlRoot: null_mut(),
                pszDisplayName: display.as_mut_ptr(),
                lpszTitle: title.as_ptr(),
                ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
                lpfn: Some(browse_cb),
                lParam: if initial.len() > 1 { initial.as_ptr() as LPARAM } else { 0 },
                iImage: 0,
            };
            let pidl = SHBrowseForFolderW(&info);
            if pidl.is_null() {
                return None;
            }
            let mut path = vec![0u16; 32768];
            let ok = SHGetPathFromIDListW(pidl, path.as_mut_ptr()) != 0;
            CoTaskMemFree(pidl as *const _);
            if !ok {
                return None;
            }
            let len = path.iter().position(|c| *c == 0).unwrap_or(path.len());
            Some(String::from_utf16_lossy(&path[..len]))
        }
    }
}
