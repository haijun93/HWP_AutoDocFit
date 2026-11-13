//! 접근성(UI Automation, Q17): 직접 그린 요소마다 UIA 제공자(이름·역할·상태·위치 + 단추 호출·항목 선택 패턴).
//!
//! 흐름
//! 1. 창은 그릴 때마다 지금 보이는 요소 목록을 [`publish`]로 올린다(창 하나당 목록 하나, Mutex).
//! 2. `WM_GETOBJECT`는 [`on_getobject`]에 맡긴다 → 뿌리 제공자(IRawElementProviderFragmentRoot)를 돌려준다.
//!    창 자체의 이름·역할·위치는 OS 기본 제공자(UiaHostProviderFromHwnd)가 맡는다.
//! 3. 화면 읽기 프로그램이 단추 호출·항목 선택을 하면 UI 스레드로 [`action_message`]를 `PostMessageW`한다
//!    (UIA 호출이 다른 스레드에서 와도 창 상태는 UI 스레드에서만 바꾼다). 창은 [`decode`]로 풀어 같은 일을 한다.
//! 4. `WM_DESTROY`에서 [`on_destroy`].
//!
//! COM 개체는 windows-sys 위에 손으로 만든 vtable(IUnknown + Simple·Fragment·FragmentRoot·Invoke·SelectionItem).
//! 개체는 (창, 요소 id)만 들고 있고 속성은 부를 때마다 목록에서 찾는다 — 목록에서 사라진 요소는
//! UIA_E_ELEMENTNOTAVAILABLE.

#![allow(non_upper_case_globals)] // windows-sys UIA 상수 이름 그대로 match에 쓴다.

use std::ffi::c_void;
use std::mem::offset_of;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SysAllocString, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, ScreenToClient};
use windows_sys::Win32::System::Com::SAFEARRAY;
use windows_sys::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows_sys::Win32::System::Variant::{VARIANT, VT_BOOL, VT_BSTR, VT_I4};
use windows_sys::Win32::UI::Accessibility::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
use windows_sys::core::{GUID, HRESULT};

use crate::geom::Rect;

/// 요소 역할.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Button,
    CheckBox,
    RadioButton,
    ComboBox,
    Edit,
    Text,
    List,
    ListItem,
    Tab,
    TabItem,
    Table,
    DataItem,
    Header,
    HeaderItem,
    Group,
    Pane,
    ScrollBar,
    Image,
    MenuItem,
}

impl Role {
    pub fn control_type(self) -> i32 {
        match self {
            Role::Button => UIA_ButtonControlTypeId,
            Role::CheckBox => UIA_CheckBoxControlTypeId,
            Role::RadioButton => UIA_RadioButtonControlTypeId,
            Role::ComboBox => UIA_ComboBoxControlTypeId,
            Role::Edit => UIA_EditControlTypeId,
            Role::Text => UIA_TextControlTypeId,
            Role::List => UIA_ListControlTypeId,
            Role::ListItem => UIA_ListItemControlTypeId,
            Role::Tab => UIA_TabControlTypeId,
            Role::TabItem => UIA_TabItemControlTypeId,
            Role::Table => UIA_TableControlTypeId,
            Role::DataItem => UIA_DataItemControlTypeId,
            Role::Header => UIA_HeaderControlTypeId,
            Role::HeaderItem => UIA_HeaderItemControlTypeId,
            Role::Group => UIA_GroupControlTypeId,
            Role::Pane => UIA_PaneControlTypeId,
            Role::ScrollBar => UIA_ScrollBarControlTypeId,
            Role::Image => UIA_ImageControlTypeId,
            Role::MenuItem => UIA_MenuItemControlTypeId,
        }
    }
    /// 키보드 포커스를 받을 수 있는 역할.
    pub fn focusable(self) -> bool {
        !matches!(self, Role::Text | Role::Group | Role::Pane | Role::Image | Role::Header)
    }
}

/// 요소 하나. id는 창 안에서 고유한 0이 아닌 수, parent 0 = 뿌리(창).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: u32,
    pub parent: u32,
    pub name: String,
    pub role: Role,
    /// 창 안 좌표.
    pub rect: Rect,
    pub enabled: bool,
    /// 선택 가능한 항목이면 Some(선택 여부) → SelectionItem 패턴.
    pub selected: Option<bool>,
    pub focused: bool,
    /// 호출(단추 누름) 가능 → Invoke 패턴.
    pub invoke: bool,
}

impl Node {
    pub fn new(id: u32, parent: u32, role: Role, name: impl Into<String>, rect: Rect) -> Self {
        Self { id, parent, name: name.into(), role, rect, enabled: true, selected: None, focused: false, invoke: false }
    }
    pub fn button(id: u32, parent: u32, name: impl Into<String>, rect: Rect) -> Self {
        Self { invoke: true, ..Self::new(id, parent, Role::Button, name, rect) }
    }
    pub fn item(id: u32, parent: u32, role: Role, name: impl Into<String>, rect: Rect, selected: bool) -> Self {
        Self { selected: Some(selected), ..Self::new(id, parent, role, name, rect) }
    }
}

/// 빈 이름 검사(시험·자동 검사용): 이름이 빈 요소 id.
pub fn unnamed(nodes: &[Node]) -> Vec<u32> {
    nodes.iter().filter(|n| n.name.trim().is_empty()).map(|n| n.id).collect()
}

// ---- 창별 요소 목록 ----

struct Tree {
    hwnd: isize,
    nodes: Vec<Node>,
}

static TREES: Mutex<Vec<Tree>> = Mutex::new(Vec::new());

/// 창의 요소 목록을 바꾼다(그릴 때마다). 선택·포커스가 바뀌었으면 듣는 쪽이 있을 때 UIA 사건을 알린다.
pub fn publish(hwnd: HWND, nodes: Vec<Node>) {
    let changed: Vec<(u32, i32)> = {
        let mut trees = TREES.lock().unwrap();
        let key = hwnd as isize;
        let old = trees.iter().position(|t| t.hwnd == key);
        let mut events = Vec::new();
        if let Some(i) = old {
            let before = &trees[i].nodes;
            let was = |id: u32, f: &dyn Fn(&Node) -> bool| before.iter().any(|n| n.id == id && f(n));
            for n in &nodes {
                if n.focused && !was(n.id, &|o| o.focused) {
                    events.push((n.id, UIA_AutomationFocusChangedEventId));
                }
                if n.selected == Some(true) && !was(n.id, &|o| o.selected == Some(true)) {
                    events.push((n.id, UIA_SelectionItem_ElementSelectedEventId));
                }
            }
            trees[i].nodes = nodes;
        } else {
            trees.push(Tree { hwnd: key, nodes });
        }
        events
    };
    if !changed.is_empty() && unsafe { UiaClientsAreListening() } != 0 {
        for (id, event) in changed {
            let e = Element::new(hwnd, id);
            unsafe {
                UiaRaiseAutomationEvent(simple_ptr(e), event);
                release(e);
            }
        }
    }
}

/// 창의 지금 요소 목록(복사).
pub fn nodes(hwnd: HWND) -> Vec<Node> {
    TREES.lock().unwrap().iter().find(|t| t.hwnd == hwnd as isize).map(|t| t.nodes.clone()).unwrap_or_default()
}

fn with_node<R>(hwnd: isize, id: u32, f: impl FnOnce(&Node, &[Node]) -> R) -> Option<R> {
    let trees = TREES.lock().unwrap();
    let t = trees.iter().find(|t| t.hwnd == hwnd)?;
    let n = t.nodes.iter().find(|n| n.id == id)?;
    Some(f(n, &t.nodes))
}

// ---- 창 메시지 ----

/// 창으로 보내는 접근성 동작.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Invoke,
    Select,
    Focus,
}

/// 동작 알림 메시지 번호(프로세스 안 고유, RegisterWindowMessageW).
pub fn action_message() -> u32 {
    static MSG: OnceLock<u32> = OnceLock::new();
    *MSG.get_or_init(|| {
        let name: Vec<u16> = "HwpAutoDocFit.UiCore.A11yAction\0".encode_utf16().collect();
        unsafe { RegisterWindowMessageW(name.as_ptr()) }
    })
}

/// 동작 메시지 풀기: (동작, 요소 id).
pub fn decode(wp: WPARAM, lp: LPARAM) -> Option<(Action, u32)> {
    let a = match wp {
        1 => Action::Invoke,
        2 => Action::Select,
        3 => Action::Focus,
        _ => return None,
    };
    Some((a, lp as u32))
}

fn post(hwnd: isize, a: Action, id: u32) {
    let wp = match a {
        Action::Invoke => 1,
        Action::Select => 2,
        Action::Focus => 3,
    };
    unsafe { PostMessageW(hwnd as HWND, action_message(), wp, id as LPARAM) };
}

/// WM_GETOBJECT 처리. UIA 요청이면 Some(결과), 아니면 None(DefWindowProc로).
///
/// # Safety
/// 창 프로시저 인자를 그대로 넘겨야 한다.
pub unsafe fn on_getobject(hwnd: HWND, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    if lp as i32 != UiaRootObjectId {
        return None;
    }
    unsafe {
        let root = Element::new(hwnd, 0);
        let r = UiaReturnRawElementProvider(hwnd, wp, lp, simple_ptr(root));
        release(root);
        Some(r)
    }
}

/// WM_DESTROY: 목록을 지우고 UIA에 연결을 끊으라고 알린다.
pub fn on_destroy(hwnd: HWND) {
    TREES.lock().unwrap().retain(|t| t.hwnd != hwnd as isize);
    unsafe { UiaReturnRawElementProvider(hwnd, 0, 0, std::ptr::null_mut()) };
}

// ---- COM 개체 ----

type P = *mut c_void;
const S_OK: HRESULT = 0;
const E_NOINTERFACE: HRESULT = 0x80004002u32 as i32;
const E_INVALIDARG: HRESULT = 0x80070057u32 as i32;
const UIA_E_ELEMENTNOTAVAILABLE: HRESULT = 0x80040201u32 as i32;
const UIA_E_ELEMENTNOTENABLED: HRESULT = 0x80040200u32 as i32;
const UIA_E_INVALIDOPERATION: HRESULT = 0x80131509u32 as i32;

const IID_IUNKNOWN: u128 = 0x00000000_0000_0000_c000_000000000046;
const IID_SIMPLE: u128 = 0xd6dd68d1_86fd_4332_8666_9abedea2d24c;
const IID_FRAGMENT: u128 = 0xf7063da8_8359_439c_9297_bbc5299a7d87;
const IID_ROOT: u128 = 0x620ce2a5_ab8f_40a9_86cb_de3c75599b58;
const IID_INVOKE: u128 = 0x54fcb24b_e18e_47a2_b4d3_eccbe77599a2;
const IID_SELECTION_ITEM: u128 = 0x2acad808_b2d4_452d_a407_91ff1ad167b2;

fn is(g: &GUID, v: u128) -> bool {
    let w = GUID::from_u128(v);
    g.data1 == w.data1 && g.data2 == w.data2 && g.data3 == w.data3 && g.data4 == w.data4
}

#[repr(C)]
struct Element {
    simple: *const SimpleVtbl,
    fragment: *const FragmentVtbl,
    root: *const RootVtbl,
    invoke: *const InvokeVtbl,
    selection: *const SelectionVtbl,
    refs: AtomicU32,
    hwnd: isize,
    id: u32,
}

/// 살아 있는 개체 수(시험에서 새는지 확인).
static LIVE: AtomicU32 = AtomicU32::new(0);

impl Element {
    fn new(hwnd: HWND, id: u32) -> *mut Element {
        LIVE.fetch_add(1, Ordering::SeqCst);
        Box::into_raw(Box::new(Element {
            simple: &SIMPLE,
            fragment: &FRAGMENT,
            root: &ROOT,
            invoke: &INVOKE,
            selection: &SELECTION,
            refs: AtomicU32::new(1),
            hwnd: hwnd as isize,
            id,
        }))
    }
}

fn simple_ptr(e: *mut Element) -> P {
    unsafe { std::ptr::addr_of_mut!((*e).simple) as P }
}
fn fragment_ptr(e: *mut Element) -> P {
    unsafe { std::ptr::addr_of_mut!((*e).fragment) as P }
}

unsafe fn base(this: P, off: usize) -> *mut Element {
    unsafe { (this as *mut u8).sub(off) as *mut Element }
}

unsafe fn add_ref(e: *mut Element) -> u32 {
    unsafe { (*e).refs.fetch_add(1, Ordering::SeqCst) + 1 }
}

unsafe fn release(e: *mut Element) -> u32 {
    unsafe {
        let n = (*e).refs.fetch_sub(1, Ordering::SeqCst) - 1;
        if n == 0 {
            drop(Box::from_raw(e));
            LIVE.fetch_sub(1, Ordering::SeqCst);
        }
        n
    }
}

unsafe fn query(e: *mut Element, riid: *const GUID, out: *mut P) -> HRESULT {
    unsafe {
        if out.is_null() || riid.is_null() {
            return E_INVALIDARG;
        }
        *out = std::ptr::null_mut();
        let g = &*riid;
        let (hwnd, id) = ((*e).hwnd, (*e).id);
        let patterns = || with_node(hwnd, id, |n, _| (n.invoke, n.selected.is_some())).unwrap_or((false, false));
        let p: P = if is(g, IID_IUNKNOWN) || is(g, IID_SIMPLE) {
            simple_ptr(e)
        } else if is(g, IID_FRAGMENT) {
            fragment_ptr(e)
        } else if is(g, IID_ROOT) && id == 0 {
            std::ptr::addr_of_mut!((*e).root) as P
        } else if is(g, IID_INVOKE) && id != 0 && patterns().0 {
            std::ptr::addr_of_mut!((*e).invoke) as P
        } else if is(g, IID_SELECTION_ITEM) && id != 0 && patterns().1 {
            std::ptr::addr_of_mut!((*e).selection) as P
        } else {
            return E_NOINTERFACE;
        };
        add_ref(e);
        *out = p;
        S_OK
    }
}

macro_rules! unknown {
    ($field:ident, $qi:ident, $ar:ident, $rel:ident) => {
        unsafe extern "system" fn $qi(this: P, riid: *const GUID, out: *mut P) -> HRESULT {
            unsafe { query(base(this, offset_of!(Element, $field)), riid, out) }
        }
        unsafe extern "system" fn $ar(this: P) -> u32 {
            unsafe { add_ref(base(this, offset_of!(Element, $field))) }
        }
        unsafe extern "system" fn $rel(this: P) -> u32 {
            unsafe { release(base(this, offset_of!(Element, $field))) }
        }
    };
}

unknown!(simple, s_qi, s_addref, s_release);
unknown!(fragment, f_qi, f_addref, f_release);
unknown!(root, r_qi, r_addref, r_release);
unknown!(invoke, i_qi, i_addref, i_release);
unknown!(selection, l_qi, l_addref, l_release);

type Qi = unsafe extern "system" fn(P, *const GUID, *mut P) -> HRESULT;
type Ref = unsafe extern "system" fn(P) -> u32;

#[repr(C)]
struct SimpleVtbl {
    qi: Qi,
    add_ref: Ref,
    release: Ref,
    provider_options: unsafe extern "system" fn(P, *mut i32) -> HRESULT,
    pattern: unsafe extern "system" fn(P, i32, *mut P) -> HRESULT,
    property: unsafe extern "system" fn(P, i32, *mut VARIANT) -> HRESULT,
    host: unsafe extern "system" fn(P, *mut P) -> HRESULT,
}

#[repr(C)]
struct FragmentVtbl {
    qi: Qi,
    add_ref: Ref,
    release: Ref,
    navigate: unsafe extern "system" fn(P, i32, *mut P) -> HRESULT,
    runtime_id: unsafe extern "system" fn(P, *mut *mut SAFEARRAY) -> HRESULT,
    bounds: unsafe extern "system" fn(P, *mut UiaRect) -> HRESULT,
    embedded: unsafe extern "system" fn(P, *mut *mut SAFEARRAY) -> HRESULT,
    set_focus: unsafe extern "system" fn(P) -> HRESULT,
    fragment_root: unsafe extern "system" fn(P, *mut P) -> HRESULT,
}

#[repr(C)]
struct RootVtbl {
    qi: Qi,
    add_ref: Ref,
    release: Ref,
    from_point: unsafe extern "system" fn(P, f64, f64, *mut P) -> HRESULT,
    focus: unsafe extern "system" fn(P, *mut P) -> HRESULT,
}

#[repr(C)]
struct InvokeVtbl {
    qi: Qi,
    add_ref: Ref,
    release: Ref,
    invoke: unsafe extern "system" fn(P) -> HRESULT,
}

#[repr(C)]
struct SelectionVtbl {
    qi: Qi,
    add_ref: Ref,
    release: Ref,
    select: unsafe extern "system" fn(P) -> HRESULT,
    add: unsafe extern "system" fn(P) -> HRESULT,
    remove: unsafe extern "system" fn(P) -> HRESULT,
    is_selected: unsafe extern "system" fn(P, *mut i32) -> HRESULT,
    container: unsafe extern "system" fn(P, *mut P) -> HRESULT,
}

static SIMPLE: SimpleVtbl = SimpleVtbl {
    qi: s_qi, add_ref: s_addref, release: s_release,
    provider_options: s_options, pattern: s_pattern, property: s_property, host: s_host,
};
static FRAGMENT: FragmentVtbl = FragmentVtbl {
    qi: f_qi, add_ref: f_addref, release: f_release,
    navigate: f_navigate, runtime_id: f_runtime_id, bounds: f_bounds, embedded: f_embedded, set_focus: f_set_focus,
    fragment_root: f_root,
};
static ROOT: RootVtbl = RootVtbl { qi: r_qi, add_ref: r_addref, release: r_release, from_point: r_from_point, focus: r_focus };
static INVOKE: InvokeVtbl = InvokeVtbl { qi: i_qi, add_ref: i_addref, release: i_release, invoke: i_invoke };
static SELECTION: SelectionVtbl = SelectionVtbl {
    qi: l_qi, add_ref: l_addref, release: l_release,
    select: l_select, add: l_select, remove: l_remove, is_selected: l_is_selected, container: l_container,
};

unsafe fn me(this: P, off: usize) -> (isize, u32) {
    unsafe {
        let e = base(this, off);
        ((*e).hwnd, (*e).id)
    }
}

unsafe extern "system" fn s_options(_this: P, out: *mut i32) -> HRESULT {
    if out.is_null() {
        return E_INVALIDARG;
    }
    unsafe { *out = ProviderOptions_ServerSideProvider };
    S_OK
}

unsafe extern "system" fn s_pattern(this: P, pattern: i32, out: *mut P) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        *out = std::ptr::null_mut();
        let e = base(this, offset_of!(Element, simple));
        let iid = match pattern {
            UIA_InvokePatternId => IID_INVOKE,
            UIA_SelectionItemPatternId => IID_SELECTION_ITEM,
            _ => return S_OK,
        };
        let g = GUID::from_u128(iid);
        if query(e, &g, out) != S_OK {
            *out = std::ptr::null_mut();
        }
        S_OK
    }
}

fn bstr(s: &str) -> VARIANT {
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let mut v: VARIANT = unsafe { std::mem::zeroed() };
    unsafe {
        v.Anonymous.Anonymous.vt = VT_BSTR;
        v.Anonymous.Anonymous.Anonymous.bstrVal = SysAllocString(w.as_ptr());
    }
    v
}

fn i4(n: i32) -> VARIANT {
    let mut v: VARIANT = unsafe { std::mem::zeroed() };
    v.Anonymous.Anonymous.vt = VT_I4;
    v.Anonymous.Anonymous.Anonymous.lVal = n;
    v
}

fn boolean(b: bool) -> VARIANT {
    let mut v: VARIANT = unsafe { std::mem::zeroed() };
    v.Anonymous.Anonymous.vt = VT_BOOL;
    v.Anonymous.Anonymous.Anonymous.boolVal = if b { -1 } else { 0 };
    v
}

unsafe extern "system" fn s_property(this: P, prop: i32, out: *mut VARIANT) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        *out = std::mem::zeroed();
        let (hwnd, id) = me(this, offset_of!(Element, simple));
        if id == 0 {
            return S_OK; // 창 자체는 OS 기본 제공자가 맡는다.
        }
        let v = with_node(hwnd, id, |n, _| match prop {
            UIA_NamePropertyId => Some(bstr(&n.name)),
            UIA_ControlTypePropertyId => Some(i4(n.role.control_type())),
            UIA_IsEnabledPropertyId => Some(boolean(n.enabled)),
            UIA_HasKeyboardFocusPropertyId => Some(boolean(n.focused)),
            UIA_IsKeyboardFocusablePropertyId => Some(boolean(n.enabled && n.role.focusable())),
            UIA_AutomationIdPropertyId => Some(bstr(&format!("e{}", n.id))),
            UIA_FrameworkIdPropertyId => Some(bstr("ui-core")),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => Some(boolean(true)),
            UIA_IsOffscreenPropertyId => Some(boolean(n.rect.is_empty())),
            _ => None,
        });
        match v {
            None => UIA_E_ELEMENTNOTAVAILABLE,
            Some(Some(v)) => {
                *out = v;
                S_OK
            }
            Some(None) => S_OK,
        }
    }
}

unsafe extern "system" fn s_host(this: P, out: *mut P) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        *out = std::ptr::null_mut();
        let (hwnd, id) = me(this, offset_of!(Element, simple));
        if id == 0 { UiaHostProviderFromHwnd(hwnd as HWND, out) } else { S_OK }
    }
}

/// 같은 부모의 형제 순서에서 이웃·자식 찾기(순수 함수).
pub fn navigate(nodes: &[Node], id: u32, dir: i32) -> Option<u32> {
    let parent_of = |id: u32| nodes.iter().find(|n| n.id == id).map(|n| n.parent);
    let children = |p: u32| nodes.iter().filter(move |n| n.parent == p && n.id != 0).map(|n| n.id);
    match dir {
        NavigateDirection_Parent => if id == 0 { None } else { parent_of(id) },
        NavigateDirection_FirstChild => children(id).next(),
        NavigateDirection_LastChild => children(id).last(),
        NavigateDirection_NextSibling | NavigateDirection_PreviousSibling if id != 0 => {
            let p = parent_of(id)?;
            let sibs: Vec<u32> = children(p).collect();
            let i = sibs.iter().position(|s| *s == id)?;
            if dir == NavigateDirection_NextSibling { sibs.get(i + 1).copied() } else { i.checked_sub(1).map(|j| sibs[j]) }
        }
        _ => None,
    }
}

unsafe extern "system" fn f_navigate(this: P, dir: i32, out: *mut P) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        *out = std::ptr::null_mut();
        let (hwnd, id) = me(this, offset_of!(Element, fragment));
        if id == 0 && dir == NavigateDirection_Parent {
            return S_OK; // 창의 부모는 OS가 안다.
        }
        let all = nodes(hwnd as HWND);
        if id != 0 && !all.iter().any(|n| n.id == id) {
            return UIA_E_ELEMENTNOTAVAILABLE;
        }
        if let Some(t) = navigate(&all, id, dir) {
            *out = fragment_ptr(Element::new(hwnd as HWND, t));
        }
        S_OK
    }
}

unsafe extern "system" fn f_runtime_id(this: P, out: *mut *mut SAFEARRAY) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        *out = std::ptr::null_mut();
        let (_, id) = me(this, offset_of!(Element, fragment));
        if id == 0 {
            return S_OK;
        }
        let sa = SafeArrayCreateVector(VT_I4, 0, 2);
        if sa.is_null() {
            return E_INVALIDARG;
        }
        for (i, v) in [UiaAppendRuntimeId as i32, id as i32].iter().enumerate() {
            let idx = i as i32;
            SafeArrayPutElement(sa, &idx, v as *const i32 as *const c_void);
        }
        *out = sa;
        S_OK
    }
}

unsafe extern "system" fn f_bounds(this: P, out: *mut UiaRect) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        *out = std::mem::zeroed();
        let (hwnd, id) = me(this, offset_of!(Element, fragment));
        if id == 0 {
            return S_OK;
        }
        match with_node(hwnd, id, |n, _| n.rect) {
            None => UIA_E_ELEMENTNOTAVAILABLE,
            Some(r) => {
                let mut pt = POINT { x: r.left, y: r.top };
                ClientToScreen(hwnd as HWND, &mut pt);
                *out = UiaRect { left: pt.x as f64, top: pt.y as f64, width: r.width() as f64, height: r.height() as f64 };
                S_OK
            }
        }
    }
}

unsafe extern "system" fn f_embedded(_this: P, out: *mut *mut SAFEARRAY) -> HRESULT {
    if !out.is_null() {
        unsafe { *out = std::ptr::null_mut() };
    }
    S_OK
}

unsafe extern "system" fn f_set_focus(this: P) -> HRESULT {
    let (hwnd, id) = unsafe { me(this, offset_of!(Element, fragment)) };
    if id != 0 {
        post(hwnd, Action::Focus, id);
    }
    S_OK
}

unsafe extern "system" fn f_root(this: P, out: *mut P) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let (hwnd, _) = me(this, offset_of!(Element, fragment));
        let r = Element::new(hwnd as HWND, 0);
        *out = std::ptr::addr_of_mut!((*r).root) as P;
        S_OK
    }
}

/// 화면 점(창 안 좌표)에 있는 가장 안쪽 요소(목록 뒤쪽이 위, 순수 함수).
pub fn hit(nodes: &[Node], x: i32, y: i32) -> Option<u32> {
    nodes.iter().rev().find(|n| n.rect.contains(x, y) && !nodes.iter().any(|c| c.parent == n.id && c.rect.contains(x, y)))
        .map(|n| n.id)
}

unsafe extern "system" fn r_from_point(this: P, x: f64, y: f64, out: *mut P) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let (hwnd, _) = me(this, offset_of!(Element, root));
        let mut pt = POINT { x: x as i32, y: y as i32 };
        ScreenToClient(hwnd as HWND, &mut pt);
        let id = hit(&nodes(hwnd as HWND), pt.x, pt.y).unwrap_or(0);
        *out = fragment_ptr(Element::new(hwnd as HWND, id));
        S_OK
    }
}

unsafe extern "system" fn r_focus(this: P, out: *mut P) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        *out = std::ptr::null_mut();
        let (hwnd, _) = me(this, offset_of!(Element, root));
        if let Some(n) = nodes(hwnd as HWND).iter().find(|n| n.focused) {
            *out = fragment_ptr(Element::new(hwnd as HWND, n.id));
        }
        S_OK
    }
}

unsafe extern "system" fn i_invoke(this: P) -> HRESULT {
    let (hwnd, id) = unsafe { me(this, offset_of!(Element, invoke)) };
    match with_node(hwnd, id, |n, _| n.enabled) {
        None => UIA_E_ELEMENTNOTAVAILABLE,
        Some(false) => UIA_E_ELEMENTNOTENABLED,
        Some(true) => {
            post(hwnd, Action::Invoke, id);
            S_OK
        }
    }
}

unsafe extern "system" fn l_select(this: P) -> HRESULT {
    let (hwnd, id) = unsafe { me(this, offset_of!(Element, selection)) };
    match with_node(hwnd, id, |n, _| n.enabled) {
        None => UIA_E_ELEMENTNOTAVAILABLE,
        Some(false) => UIA_E_ELEMENTNOTENABLED,
        Some(true) => {
            post(hwnd, Action::Select, id);
            S_OK
        }
    }
}

unsafe extern "system" fn l_remove(_this: P) -> HRESULT {
    UIA_E_INVALIDOPERATION
}

unsafe extern "system" fn l_is_selected(this: P, out: *mut i32) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let (hwnd, id) = me(this, offset_of!(Element, selection));
        match with_node(hwnd, id, |n, _| n.selected == Some(true)) {
            None => UIA_E_ELEMENTNOTAVAILABLE,
            Some(b) => {
                *out = b as i32;
                S_OK
            }
        }
    }
}

unsafe extern "system" fn l_container(this: P, out: *mut P) -> HRESULT {
    unsafe {
        if out.is_null() {
            return E_INVALIDARG;
        }
        *out = std::ptr::null_mut();
        let (hwnd, id) = me(this, offset_of!(Element, selection));
        match with_node(hwnd, id, |n, _| n.parent) {
            None => UIA_E_ELEMENTNOTAVAILABLE,
            Some(p) => {
                *out = simple_ptr(Element::new(hwnd as HWND, p));
                S_OK
            }
        }
    }
}

/// 시험용: 뿌리 제공자(IRawElementProviderSimple 포인터, 참조 1). 다 쓰면 vtable Release.
#[doc(hidden)]
pub fn root_provider(hwnd: HWND) -> *mut c_void {
    simple_ptr(Element::new(hwnd, 0))
}

#[doc(hidden)]
pub fn live_objects() -> u32 {
    LIVE.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::SysFreeString;
    use windows_sys::Win32::System::Ole::SafeArrayDestroy;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DestroyWindow, MSG, PM_REMOVE, PeekMessageW, WM_GETOBJECT,
    };

    fn sample() -> Vec<Node> {
        let mut nodes = vec![
            Node::button(1, 0, "닫기", Rect::xywh(200, 0, 40, 30)),
            Node::new(10, 0, Role::List, "목록", Rect::xywh(0, 40, 240, 120)),
            Node::item(11, 10, Role::ListItem, "첫 항목", Rect::xywh(0, 40, 240, 30), true),
            Node::item(12, 10, Role::ListItem, "둘째 항목", Rect::xywh(0, 70, 240, 30), false),
        ];
        nodes[3].focused = true;
        nodes.push(Node { enabled: false, ..Node::button(2, 0, "막힌 단추", Rect::xywh(160, 0, 40, 30)) });
        nodes
    }

    #[test]
    fn navigation_and_hit_are_pure() {
        let n = sample();
        assert_eq!(navigate(&n, 0, NavigateDirection_FirstChild), Some(1));
        assert_eq!(navigate(&n, 0, NavigateDirection_LastChild), Some(2));
        assert_eq!(navigate(&n, 1, NavigateDirection_NextSibling), Some(10));
        assert_eq!(navigate(&n, 1, NavigateDirection_PreviousSibling), None);
        assert_eq!(navigate(&n, 12, NavigateDirection_Parent), Some(10));
        assert_eq!(navigate(&n, 10, NavigateDirection_FirstChild), Some(11));
        assert_eq!(navigate(&n, 11, NavigateDirection_NextSibling), Some(12));
        assert_eq!(navigate(&n, 99, NavigateDirection_NextSibling), None);
        assert_eq!(hit(&n, 10, 80), Some(12), "목록 안 항목이 이김");
        assert_eq!(hit(&n, 10, 150), Some(10));
        assert_eq!(hit(&n, 10, 5), None);
        assert!(unnamed(&n).is_empty());
        assert_eq!(unnamed(&[Node::new(5, 0, Role::Button, " ", Rect::default())]), vec![5]);
        assert_eq!(decode(1, 7), Some((Action::Invoke, 7)));
        assert_eq!(decode(9, 7), None);
    }

    unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe {
            if m == WM_GETOBJECT {
                if let Some(r) = on_getobject(h, w, l) {
                    return r;
                }
            }
            DefWindowProcW(h, m, w, l)
        }
    }

    unsafe fn vt<T>(p: P) -> &'static T {
        unsafe { &**(p as *const *const T) }
    }

    #[test]
    fn com_provider_exposes_names_roles_patterns_and_frees_objects() {
        crate::window::register_class("UiCore.Test.A11y", Some(proc_), false).unwrap();
        let h = crate::window::create(crate::window::Kind::Floating { topmost: false, noactivate: true }, "UiCore.Test.A11y",
                                      "접근성 시험", Rect::xywh(0, 0, 240, 200), std::ptr::null_mut()).unwrap();
        publish(h, sample());
        let before = live_objects();
        unsafe {
            let root = root_provider(h);
            let s = vt::<SimpleVtbl>(root);
            let mut opts = 0;
            assert_eq!((s.provider_options)(root, &mut opts), S_OK);
            assert_eq!(opts, ProviderOptions_ServerSideProvider);
            let mut host: P = std::ptr::null_mut();
            assert_eq!((s.host)(root, &mut host), S_OK);
            assert!(!host.is_null(), "창 기본 제공자");
            (vt::<SimpleVtbl>(host).release)(host);
            // 뿌리 → Fragment·FragmentRoot QI.
            let mut frag: P = std::ptr::null_mut();
            assert_eq!((s.qi)(root, &GUID::from_u128(IID_FRAGMENT), &mut frag), S_OK);
            let mut rootif: P = std::ptr::null_mut();
            assert_eq!((s.qi)(root, &GUID::from_u128(IID_ROOT), &mut rootif), S_OK);
            let mut none: P = std::ptr::null_mut();
            assert_eq!((s.qi)(root, &GUID::from_u128(IID_INVOKE), &mut none), E_NOINTERFACE);
            // 첫 자식 = 닫기 단추.
            let mut child: P = std::ptr::null_mut();
            assert_eq!((vt::<FragmentVtbl>(frag).navigate)(frag, NavigateDirection_FirstChild, &mut child), S_OK);
            let mut cs: P = std::ptr::null_mut();
            (vt::<FragmentVtbl>(child).qi)(child, &GUID::from_u128(IID_SIMPLE), &mut cs);
            let mut v: VARIANT = std::mem::zeroed();
            assert_eq!((vt::<SimpleVtbl>(cs).property)(cs, UIA_NamePropertyId, &mut v), S_OK);
            assert_eq!(v.Anonymous.Anonymous.vt, VT_BSTR);
            let b = v.Anonymous.Anonymous.Anonymous.bstrVal;
            let mut n = 0;
            while *b.add(n) != 0 {
                n += 1;
            }
            assert_eq!(String::from_utf16_lossy(std::slice::from_raw_parts(b, n)), "닫기");
            SysFreeString(b);
            (vt::<SimpleVtbl>(cs).property)(cs, UIA_ControlTypePropertyId, &mut v);
            assert_eq!(v.Anonymous.Anonymous.Anonymous.lVal, UIA_ButtonControlTypeId);
            // 호출 패턴 → 창에 동작 메시지.
            let mut inv: P = std::ptr::null_mut();
            assert_eq!((vt::<SimpleVtbl>(cs).pattern)(cs, UIA_InvokePatternId, &mut inv), S_OK);
            assert!(!inv.is_null());
            assert_eq!((vt::<InvokeVtbl>(inv).invoke)(inv), S_OK);
            let mut msg: MSG = std::mem::zeroed();
            assert_ne!(PeekMessageW(&mut msg, h, action_message(), action_message(), PM_REMOVE), 0);
            assert_eq!(decode(msg.wParam, msg.lParam), Some((Action::Invoke, 1)));
            let mut sel: P = std::ptr::null_mut();
            (vt::<SimpleVtbl>(cs).pattern)(cs, UIA_SelectionItemPatternId, &mut sel);
            assert!(sel.is_null(), "단추에는 선택 패턴 없음");
            // 위치(화면 좌표, 창이 0,0이므로 같음).
            let mut r: UiaRect = std::mem::zeroed();
            assert_eq!((vt::<FragmentVtbl>(child).bounds)(child, &mut r), S_OK);
            assert_eq!((r.width, r.height), (40.0, 30.0));
            let mut sa: *mut SAFEARRAY = std::ptr::null_mut();
            assert_eq!((vt::<FragmentVtbl>(child).runtime_id)(child, &mut sa), S_OK);
            assert!(!sa.is_null());
            SafeArrayDestroy(sa);
            // 점 → 둘째 항목, 선택 패턴.
            let mut at: P = std::ptr::null_mut();
            let mut wr: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(h, &mut wr);
            assert_eq!((vt::<RootVtbl>(rootif).from_point)(rootif, (wr.left + 10) as f64, (wr.top + 80) as f64, &mut at), S_OK);
            let mut ats: P = std::ptr::null_mut();
            (vt::<FragmentVtbl>(at).qi)(at, &GUID::from_u128(IID_SIMPLE), &mut ats);
            let mut item: P = std::ptr::null_mut();
            (vt::<SimpleVtbl>(ats).pattern)(ats, UIA_SelectionItemPatternId, &mut item);
            assert!(!item.is_null());
            let mut on = 1;
            assert_eq!((vt::<SelectionVtbl>(item).is_selected)(item, &mut on), S_OK);
            assert_eq!(on, 0);
            assert_eq!((vt::<SelectionVtbl>(item).select)(item), S_OK);
            assert_ne!(PeekMessageW(&mut msg, h, action_message(), action_message(), PM_REMOVE), 0);
            assert_eq!(decode(msg.wParam, msg.lParam), Some((Action::Select, 12)));
            let mut focus: P = std::ptr::null_mut();
            (vt::<RootVtbl>(rootif).focus)(rootif, &mut focus);
            assert!(!focus.is_null());
            // 사라진 요소.
            publish(h, vec![]);
            assert_eq!((vt::<SimpleVtbl>(cs).property)(cs, UIA_NamePropertyId, &mut v), UIA_E_ELEMENTNOTAVAILABLE);
            assert_eq!((vt::<InvokeVtbl>(inv).invoke)(inv), UIA_E_ELEMENTNOTAVAILABLE);
            for p in [focus, item, ats, at, inv, cs, child, rootif, frag, root] {
                (vt::<SimpleVtbl>(p).release)(p);
            }
            assert_eq!(live_objects(), before, "우리가 만든 개체는 모두 풀림");
            // 막힌 단추 호출은 거절. (다시 올리면 선택·포커스 사건을 알리고, 듣는 쪽이 있으면 UIA가 개체를 잠시 들고 있다.)
            publish(h, sample());
            let mut c2: P = std::ptr::null_mut();
            let r2 = root_provider(h);
            let mut f2: P = std::ptr::null_mut();
            (vt::<SimpleVtbl>(r2).qi)(r2, &GUID::from_u128(IID_FRAGMENT), &mut f2);
            (vt::<FragmentVtbl>(f2).navigate)(f2, NavigateDirection_LastChild, &mut c2);
            let mut c2s: P = std::ptr::null_mut();
            (vt::<FragmentVtbl>(c2).qi)(c2, &GUID::from_u128(IID_SIMPLE), &mut c2s);
            let mut inv2: P = std::ptr::null_mut();
            (vt::<SimpleVtbl>(c2s).pattern)(c2s, UIA_InvokePatternId, &mut inv2);
            assert_eq!((vt::<InvokeVtbl>(inv2).invoke)(inv2), UIA_E_ELEMENTNOTENABLED);
            for p in [inv2, c2s, c2, f2, r2] {
                (vt::<SimpleVtbl>(p).release)(p);
            }
            // WM_GETOBJECT는 UIA 뿌리 요청만 처리(뿌리 하나는 UIA가 창이 없어질 때까지 들고 있을 수 있다).
            assert_eq!(on_getobject(h, 0, 0), None);
            assert!(on_getobject(h, 0, UiaRootObjectId as LPARAM).is_some());
            on_destroy(h);
            assert!(nodes(h).is_empty());
            DestroyWindow(h);
        }
    }
}
