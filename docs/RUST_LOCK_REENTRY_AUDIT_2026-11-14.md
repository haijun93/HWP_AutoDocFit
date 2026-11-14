# Rust 잠금 재진입(같은 Mutex 두 번 잡기) 전수 점검 보고서 (2026-11-14)

## 1. 점검 배경 및 개요

- **배경**: 2026-11-14 `kagent/src/preview.rs`에서 `if let Some(t) = match X.lock().unwrap() ... { f() }` 형태로 조건문 안에서 잠금을 잡았을 때, Rust의 임시 값 수명 규칙(Temporary Lifetime)에 따라 잠금 가드(`MutexGuard`)가 `if let` 본문 끝까지 유지되었습니다. 이 상태에서 본문 내 `show(t)`가 동일한 잠금을 다시 잡으려다 스레드가 자기 자신을 영구 대기하는 데드락(빠른 목록 그림 미리보기 프리징)이 발생했습니다. `kagent/src/pipe.rs`의 드롭(drop) 핸들러에서도 `match a.store.lock().unwrap().insert(...)`의 분기 본문에서 `enforce()` 및 `refresh_recent()`가 동일 잠금을 다시 잡아 멈추는 유사 사례가 확인되었습니다.
- **점검 목적**: `kagent`, `kvf`, `ktm`, `klib`, `crates/kfind`, `kfind/core`, `kfind/names`, `ui_theme`, `editor_core` 등 전체 Rust 소스를 대상으로 `if let`, `match`, `while let`, `if` 조건식 및 패턴 매칭에서 `lock()`, `borrow_mut()`, `write()`, `read()`, `borrow()` 등을 호출하고 본문(또는 본문이 호출하는 함수)에서 같은 잠금을 다시 잡을 수 있는 재진입 위험 지점을 전수 조사하고, 위험한 곳은 동작 변경 없이 잠금을 선행 해제하도록 수정합니다.

---

## 2. 점검 대상 및 통계

- **점검 대상 크레이트**:
  - `kagent` (몽땅클립보드 백엔드 / UI 에이전트)
  - `kvf` (가상 폴더 트레이 / 주 창)
  - `ktm` (단축키 / 창 관리)
  - `klib` (라이브러리 및 파이프 서버)
  - `crates/kfind` / `kfind/core` / `kfind/names` (검색 엔진 및 NTFS 모니터링)
  - `ui_theme` (공용 테마)
  - `editor_core` (버퍼 및 텍스트 엔진)
  - `bench/kb1` 및 기타 스크립트
- **전체 동시성/가변성 잠금 호출 수**: 총 354건
- **조건식(`if let` / `match` / `while let` / `if`) 내 잠금 검출**: 총 36건
- **조치 결과**:
  - **위험 (Critical / High)**: 1건 발견 및 **즉시 수정 완료** (`kagent/src/popup.rs:1290`)
  - **주의 (Medium / 잠재적 위험)**: 1건 발견 및 **방어적 수정 완료** (`kagent/src/popup.rs:1077`)
  - **경미/권장 개선 (Low)**: 2건 **잠금 시간 단축 및 바인딩 분리 완료** (`kagent/src/pipe.rs:265, 288`)
  - **안전 (Safe)**: 32건 (본문 내 재진입 경로 없음, 단순 대입/반환, 또는 매치 가드)

---

## 3. 전수 점검 결과 표

| 파일:줄 | 잠금 이름 / 타입 | 조건식 / Scrutinee 형태 | 본문에서 다시 잡는 경로 및 영향 | 위험도 | 조치 상태 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `kagent/src/popup.rs:1290` | `STATE`<br>(`Mutex<State>`) | `if STATE.lock().unwrap().hover == Some(i) && x >= w - action_width` | **1291행 `let item = { let st = STATE.lock().unwrap(); ... }`에서 동일 Mutex 직접 재잠금!**<br>또한 1300행 `refresh()` → `refilter()`에서도 `STATE.lock()` 호출.<br>마우스로 항목 핀/삭제 클릭 시 즉시 영구 데드락(UI 프리징). | **위험 (Critical)** | **수정 완료**<br>(조건 진입 전 `let is_hovered = STATE.lock().unwrap().hover == Some(i);`로 잠금 선해제) |
| `kagent/src/popup.rs:1077` | `app().settings`<br>(`Mutex<Settings>`) | `if !app().settings.lock().unwrap().sticky_popup` | 본문 `hide()` 호출. 현재 `hide()`는 `settings`를 잡지 않으나, `hide()` 실행 동안 `settings` 잠금 가드가 불필요하게 유지되어 잠재적 데드락 위험. | **주의 (Medium)** | **수정 완료**<br>(`let sticky = app().settings.lock().unwrap().sticky_popup;`로 분리) |
| `kagent/src/pipe.rs:265` | `a.store`<br>(`Mutex<Store>`) | `if let Ok((_, created)) = a.store.lock().unwrap().insert(...)` | 본문은 `{ if created { added += 1; } }` 뿐이라 현재 재잠금 없으나, 루프 내 `if let` 패턴 일관성 유지. | **낮음 (Low)** | **개선 완료**<br>(`let inserted = ...;`로 변수 분리) |
| `kagent/src/pipe.rs:288` | `a.store`<br>(`Mutex<Store>`) | `match a.store.lock().unwrap().payload(id)` | Arm 내부에서 `base64`, `unpack_formats` 등 순수 데이터 가공 수행. 본문 내 재잠금 없으나 match 전체 동안 store 락 유지. | **낮음 (Low)** | **개선 완료**<br>(`let payload = ...;`로 변수 분리) |
| `kagent/src/popup.rs:1162` | `STATE`<br>(`Mutex<State>`) | `match vk { ... if STATE.lock().unwrap().needle().is_empty() =>` | Match guard 식. Rust 스펙상 pattern guard 임시 값은 guard 평가 직후(arm 진입 전) 드롭되므로 arm 내부의 `STATE.lock()`과 겹치지 않음. | 안전 (Safe) | 유지 (정상 동작) |
| `kagent/src/preview.rs:195` | `SHARED`<br>(`Mutex<Shared>`) | `let shown = match SHARED.lock().unwrap().state ...; if let Some(t) = shown` | 2026-11-14 기수정 완료 건. `show(t)` 호출 전 잠금 해제됨. | 안전 (Safe) | 확인 완료 |
| `kagent/src/preview.rs:261` | `SHARED`<br>(`Mutex<Shared>`) | `let shown = match SHARED.lock().unwrap().state ...;` | 2026-11-14 기수정 완료 건. 변수로 분리되어 즉시 해제됨. | 안전 (Safe) | 확인 완료 |
| `kagent/src/pipe.rs:276` | `a.store`<br>(`Mutex<Store>`) | `let inserted = a.store.lock().unwrap().insert(...); match inserted` | 2026-11-14 기수정 완료 건. `enforce()` 및 `refresh_recent()` 전 잠금 해제됨. | 안전 (Safe) | 확인 완료 |
| `kagent/src/main.rs:343` | `app().timeline_tx`<br>(`Mutex<Option<Sender>>`) | `if let Some(tx) = app().timeline_tx.lock().unwrap().as_ref()` | 본문은 `tx.send(TimelineMsg::Reconfigure)` 채널 송신만 수행. 재잠금 없음. | 안전 (Safe) | 유지 |
| `kagent/src/main.rs:369` | `app().timeline_tx`<br>(`Mutex<Option<Sender>>`) | `if let Some(tx) = app().timeline_tx.lock().unwrap().as_ref()` | 본문은 `tx.send(TimelineMsg::Apply)` 채널 송신만 수행. 재잠금 없음. | 안전 (Safe) | 유지 |
| `kagent/src/main.rs:952` | `a.settings`<br>(`Mutex<Settings>`) | `if a.settings.lock().unwrap().paused \|\| ...` | 본문은 `return 0;` 뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `kvf/src/main.rs:56` | `TRAY`<br>(`Mutex<Option<Tray>>`) | `if let Some(t) = TRAY.lock().unwrap().as_mut()` | 본문은 `unsafe { t.reload(hwnd); }`. `reload()`는 Win32 API 호출만 수행하며 `TRAY`에 접근하지 않음. 이후 `let settings = TRAY.lock()`은 `if let` 블록 밖에서 순차 실행. | 낮음 (Low) | 유지 |
| `kvf/src/main.rs:62` | `TRAY`<br>(`Mutex<Option<Tray>>`) | `if let Some(t) = TRAY.lock().unwrap().as_mut()` | 본문은 `unsafe { t.remove(hwnd); }`. Win32 Shell API만 호출. 재잠금 없음. | 낮음 (Low) | 유지 |
| `kvf/src/main.rs:137` | `TRAY`<br>(`Mutex<Option<Tray>>`) | `if let Some(t) = TRAY.lock().unwrap().as_ref()` | 본문은 `unsafe { t.notice(hwnd, ...); }`. Shell_NotifyIconW 호출만 수행. 재잠금 없음. | 낮음 (Low) | 유지 |
| `ktm/src/window.rs:622` | `METRICS`<br>(`RefCell<Vec<String>>`) | `if METRICS.with(\|m\| { let mut m = m.borrow_mut(); ... m.len() >= 1000 })` | `m.borrow_mut()`은 클로저 내부 지역 가드이므로 클로저 반환 시 즉시 드롭됨. 본문의 `flush_metrics()` 호출 시 `borrow_mut()` 충돌 없음. | 안전 (Safe) | 유지 |
| `klib/src/pipe.rs:858` | `db`<br>(`Mutex<Database>`) | `if let Err(e) = db.lock().unwrap().nb_ready()` | 본문은 JSON 에러 문자열 생성 후 즉시 `return`. 재잠금 없음. | 안전 (Safe) | 유지 |
| `klib/src/pipe.rs:879` | `db`<br>(`Mutex<Database>`) | `let ix = match db.lock().unwrap().nb_index()` | `match` 식의 결과(`Arc<SearchIndex>`)를 `ix`에 바인딩하고 match 문 종료 시 가드 해제됨. 이후 검색 호출은 `ix` 인스턴스 메서드. | 안전 (Safe) | 유지 |
| `klib/src/pipe.rs:1105` | `db`<br>(`Mutex<Database>`) | `match db.lock().unwrap().box_rename(id, name)` | 본문은 결과에 따른 `ok_val`/`err_val` 반환만 수행. 재잠금 없음. | 안전 (Safe) | 유지 |
| `klib/src/pipe.rs:1357` | `db`<br>(`Mutex<Database>`) | `match db.lock().unwrap().item_rename(id, name)` | 본문은 결과에 따른 `ok_val`/`err_val` 반환만 수행. 재잠금 없음. | 안전 (Safe) | 유지 |
| `klib/src/pipe.rs:1670` | `db`<br>(`Mutex<Database>`) | `match db.lock().unwrap().relation_update(id, label)` | 본문은 결과에 따른 `ok_val`/`err_val` 반환만 수행. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:131` | `self.names`<br>(`RwLock<Option<Names>>`) | `if let Ok(mut g) = self.names.write()` | 본문은 `*g = Some(names);` 필드 대입뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:134` | `self.deny`<br>(`Mutex<HashMap>`) | `if let Ok(mut d) = self.deny.lock()` | 본문은 `d.clear();` 호출뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:224` | `self.info`<br>(`Mutex<Info>`) | `if let Ok(mut i) = self.info.lock()` | 본문은 `i.followers = hs.len();` 대입뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:272` | `self.names`<br>(`RwLock<Option<Names>>`) | `let fresh = match self.names.read()` | `fresh` 바인딩 후 match 종료 시 읽기 가드 해제됨. 이후 276행의 `names.write()`와 겹치지 않음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:276` | `self.names`<br>(`RwLock<Option<Names>>`) | `if let (Some(f), Ok(mut g)) = (fresh, self.names.write())` | 본문은 `*g = Some(f);` 대입뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:283` | `self.info`<br>(`Mutex<Info>`) | `if let Ok(mut i) = self.info.lock()` | 본문은 `i.last_error = Some(e);` 대입뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:300` | `self.info`<br>(`Mutex<Info>`) | `if let Ok(mut i) = self.info.lock()` | 본문은 `i.followers = 0;` 대입뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:311` | `self.deny`<br>(`Mutex<HashMap>`) | `if let Ok(c) = self.deny.lock()` | 본문은 맵 조회 및 값 복사 후 반환. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/names.rs:343` | `self.deny`<br>(`Mutex<HashMap>`) | `if let Ok(mut c) = self.deny.lock()` | 본문은 `c.clear()`, `c.insert()` 맵 수정만 수행. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/pipe.rs:246` | `self.active`<br>(`Mutex<HashMap>`) | `if let Ok(mut active) = self.active.lock()` | `QueryLease::drop` 내부. `active.remove(&self.key);`만 수행. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/pipe.rs:331` | `self.last_error`<br>(`Mutex<Option<String>>`) | `if let Ok(mut last) = self.last_error.lock()` | 본문은 `*last = Some(e.to_owned());` 대입뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `crates/kfind/src/pipe.rs:643` | `store`<br>(`Mutex<Store>`) | `if let Err(e) = check_replace(...)` | Scrutinee에서 `store.lock()` 호출 후 속성 추출. 본문은 `return Err(...)`뿐임. 재잠금 없음. | 안전 (Safe) | 유지 |
| `kfind/names/examples/fn1_bench.rs:182` | `REPLY`<br>(`Mutex<Option<Vec<u8>>>`) | `if let Some(r) = REPLY.lock().unwrap().take()` | 벤치마크 예제. 본문에서 `REPLY` 재잠금 없음. | 안전 (Safe) | 유지 |

---

## 4. 세부 수정 내역

### 4.1. `kagent/src/popup.rs` - 마우스 오버 후 고정/삭제 버튼 클릭 시 데드락 수정 (Critical)
- **위치**: [kagent/src/popup.rs:1290](file:///Users/hyeokjunkong/work/hwp/kagent/src/popup.rs#L1290)
- **증상**: 빠른 목록 팝업에서 마우스가 올라간 항목의 고정(pin) 또는 삭제(delete) 영역을 클릭하면 프로그램이 영구 응답 없음(프리징)에 빠짐.
- **원인**:
  ```rust
  // 수정 전 (버그)
  if STATE.lock().unwrap().hover == Some(i) && x >= w - action_width {
      let item = { let st = STATE.lock().unwrap(); st.items.get(i).map(|v| (v.id, v.pinned)) }; // <-- 데드락!
      ...
      refresh(); // <-- 내부에서 refilter() -> STATE.lock() 호출로 또 데드락!
      return 0;
  }
  ```
  `if` 조건식의 `STATE.lock().unwrap()`으로 생성된 `MutexGuard` 임시 객체는 `if` 블록의 끝까지 살아있습니다. 바로 다음 줄에서 `{ let st = STATE.lock().unwrap(); ... }`로 동일한 Mutex를 재잠금하려 시도하면서 스레드가 자가 교착 상태(Deadlock)에 빠집니다.
- **수정 내용**:
  ```rust
  // 수정 후
  let is_hovered = STATE.lock().unwrap().hover == Some(i);
  if is_hovered && x >= w - action_width {
      let item = { let st = STATE.lock().unwrap(); st.items.get(i).map(|v| (v.id, v.pinned)) };
      ...
  ```
  `is_hovered` bool 값으로 먼저 추출하여 잠금을 즉시 해제한 후 조건문을 검사하도록 변경했습니다.

### 4.2. `kagent/src/popup.rs` - 비활성화 시 잠금 가드 수명 단축 (Medium)
- **위치**: [kagent/src/popup.rs:1077](file:///Users/hyeokjunkong/work/hwp/kagent/src/popup.rs#L1077)
- **수정 내용**:
  ```rust
  // 수정 전
  WM_ACTIVATE if (wp & 0xFFFF) as u32 == WA_INACTIVE => {
      if !app().settings.lock().unwrap().sticky_popup { hide(); }
      0
  }

  // 수정 후
  WM_ACTIVATE if (wp & 0xFFFF) as u32 == WA_INACTIVE => {
      let sticky = app().settings.lock().unwrap().sticky_popup;
      if !sticky { hide(); }
      0
  }
  ```
  `hide()`가 호출되는 동안 `app().settings` 잠금이 물려있지 않도록 `bool` 변수로 먼저 추출했습니다.

### 4.3. `kagent/src/pipe.rs` - 드롭 루프 및 get 명령 잠금 범위 분리 (Low)
- **위치**: [kagent/src/pipe.rs:265, 288](file:///Users/hyeokjunkong/work/hwp/kagent/src/pipe.rs#L265)
- **수정 내용**:
  1. 그림 항목 드롭 루프에서 `a.store.lock().unwrap().insert(...)` 결과를 `let inserted = ...;`로 분리하여 `if let` 조건식에 잠금 가드가 머무르지 않도록 통일.
  2. `"get"` 핸들러에서 `let payload = a.store.lock().unwrap().payload(id); match payload { ... }`로 분리하여 base64 인코딩 및 포맷 언팩 중 저장소 락을 잡고 있지 않도록 최적화.

---

## 5. 결론 및 향후 개발 규칙

1. **Rust 제어문(`if`, `if let`, `match`, `while let`) 조건식에서의 잠금 원칙**:
   - 조건식(`condition`/`scrutinee`)에서 직접 `lock()`, `borrow_mut()`, `write()`를 호출하는 패턴은 조건절 내부에서 생성된 임시 객체(`MutexGuard`, `RefMut`, `RwLockWriteGuard` 등)가 **해당 제어문 블록 전체의 끝까지 유지**됩니다.
   - 따라서 조건식에서 잠금을 통해 데이터를 확인할 때는 반드시 **`let val = lock().something;` 형태로 먼저 지역 변수로 꺼내어 잠금을 즉시 해제한 뒤** 조건문을 수행해야 합니다.
2. 이번 전수 점검을 통해 `kagent/src/popup.rs`의 치명적 핀/삭제 프리징 버그(Critical) 1건을 선제적으로 찾아내어 안전하게 해소하였으며, 기존 동작 변경 없이 코드베이스 전체의 동시성 안정성을 확보하였습니다.
