# KCLIPSLOW1 결과 — 몽땅클립보드 '켜기' 지연·`enable timeout` 개선 (2026-11-22)

## 증상

설정 창에서 '몽땅클립보드 켜기'를 누르면 오래 걸리다 15초 뒤 "켜지 못했습니다. 진단: enable timeout"이 뜬다.
같은 시점에 kagent 는 실제로 떠 있었다(`status.json` 에 pid, `kagent --status` 약 90ms 응답).
즉 **에이전트는 떴는데 화면이 15초 안에 성공 응답을 받지 못해 '실패'로 단정**한 것이다.

## 1. 원인 후보 표 (코드 근거)

이 PC 에서 실행 중인 kagent 를 건드리지 않는 조건이라 실측은 하지 못했다. 아래는 코드를 따라가 얻은 **후보**이며,
어느 것이 실제 원인인지는 3절의 구간 로그로 다음 재현 때 확정한다.

| # | 구간 | 후보 원인 | 근거(파일) | 15초 벽과의 관계 |
|---|---|---|---|---|
| A | 클라이언트 | Flutter 가 `enable` 호출 하나에 15초를 걸고, **그 안에 파이썬 동기 체인 전체**를 기다림 | `kclip_dialog.dart` `_act`(수정 전 `Duration(seconds: 15)`) | 아래 B~H 가 합쳐 15초를 넘으면 곧바로 실패 표시 |
| B | 서버 | 파이썬 `_lifecycle` 서브프로세스 제한도 15초(`lifecycle_timeout=15`). Flutter 15초와 **같은 값**이라 어느 쪽이 먼저 끊을지 불확정, 어느 쪽이든 에이전트는 뜬 채 실패 보고 | `kclip_service.py` `_lifecycle` | 두 제한이 겹쳐 실제 여유가 더 짧음 |
| C | kagent | 켜기 때 번들 SHA-256 계산(직접 구현, 파일 전체 읽기) + 번들·설치본 전체 읽기. 백신 실시간 검사가 붙으면 수 초 | `main.rs` `lifecycle_command`, `sha256_file` | 파일 크기·백신에 비례 |
| D | kagent | 실행 중이지만 **구버전/파일 다름(stale)** 이면 종료(파이프 응답 최대 2초 + 종료 대기 최대 3초) → 설치(원자 교체, 권한 거부 시 0.1초×20 재시도 = 최대 2초) → 재시작 대기(최대 3초) | `main.rs` `quit_and_wait`/`atomic_agent_write`/`start_and_wait`, `lifecycle.rs` `update` | 최악 합계가 8~10초 이상. 이미 같은 버전이면 건너뛰지만 **파일 바이트가 다르면 같은 버전이어도 stale** |
| E | kagent | 새 프로세스가 `status.json`(pid 일치)을 쓸 때까지 **3초만** 기다림. 첫 실행(암호화 DB 생성·prewarm)이 3초를 넘으면 `agent start timeout` → 복구(종료 요청 2초+3초) 시도. 이때 에이전트는 이미 떠 있는 상태로 실패 보고 | `main.rs` `start_and_wait`, `lifecycle.rs` 실패 복구 | **'에이전트는 떴는데 UI 는 실패' 와 가장 부합** |
| F | kagent | 업데이트 뮤텍스를 최대 15초 기다림(`LifecycleLock`). 앱 시작 시 `kclip-upgrade` 스레드의 `--upgrade-if-stale` 이 잠금을 쥐고 있으면 켜기 명령이 줄을 섬. **`--status` 도 같은 잠금을 잡아** 폴링이 같이 막힘 | `main.rs` `LifecycleLock::acquire`, `desktop_web_ui.py` `upgrade_kagent` 스레드 | 잠금 보유 시간만큼 지연 |
| G | 서버 | 켜기 성공 뒤 파이썬이 `--status` 서브프로세스를 **매번 새로 띄워** 최대 75회 폴링(회당 약 90ms + 0.02초, 최악 약 8초). 보통 1회에 끝나지만 F 와 겹치면 회당 수 초 | `kclip_service.py` `enable` | 최악 +8초 |
| H | kagent | 자동 실행 등록(HKCU Run 쓰기), `pause_until()` 이 `--status` 마다 임시 파일을 읽고 필요 시 `--resume` 프로세스를 띄움 | `main.rs` `set_autostart`, `pause_until` | 보통 수 ms. 백신·레지스트리 감시 시 지연 가능 |
| I | 브리지 | 느린 호출 뒤에 줄을 서는가 → **서는 구조가 아님**. `desktop_web_ui._BrowserApi` 는 메서드를 그대로 노출하고 pywebview 가 호출마다 별도 스레드를 쓴다. `kclip()` 에는 전역 잠금이 없다. 다만 문서뷰·사진뷰 전체 드라이브 스캔이 같은 프로세스에서 GIL·디스크를 많이 쓰면 상대적으로 느려질 수는 있음(후보, 미확인) | `desktop_web_ui.py` `kclip`, `_BrowserApi` | 줄서기는 기각, 자원 경쟁은 미확인 |
| J | 클라이언트 | 켜기 성공 뒤 `_reload` 가 **50ms 간격 최대 200회** `status` 를 호출(각각 서브프로세스). 이 구간은 15초 제한 밖이지만 UI 를 오래 '켜는 중'으로 보이게 함 | `kclip_dialog.dart` `_act` | 체감 지연 |

가장 그럴듯한 시나리오: **E(또는 D)로 kagent 가 3초 안에 준비 신호를 못 줘 복구 경로를 타거나 F 로 줄을 서고, 그동안 Flutter 15초가 먼저 끊음.**
이것은 추정이며 3절의 구간 로그로 확인해야 한다.

## 2. 개선 내용

| 항목 | 변경 |
|---|---|
| (가) 즉시 응답 + 상태 조회 | `command("enable", {"async": true})` 가 **작업을 백그라운드 스레드로 시작하고 바로** `{ok, starting:true, state:"running", phase}` 를 돌려준다. 새 명령 `enable_progress` 가 `running/done/failed` 와 현재 단계·경과 ms 를 돌려준다. 같은 시점의 중복 요청은 같은 작업으로 합친다. Flutter 는 요청 10초 + 조회(0.3초 간격 최대 200회=60초)로 바뀌어 **15초 벽이 사라졌다**. `async` 를 주지 않으면 기존 동기 동작 그대로(호환) |
| (나) 불필요한 해시·재설치·재시작 생략 | 이미 실행 중이고 **실행 중 버전 = 번들 버전, 설치본·번들 파일 크기 동일**이면 `kagent --enable` 을 부르지 않고 자동 실행 등록과 상태 조회만 한다(`reason:"already_running"`). 크기가 다르거나 버전이 다르면 기존 정식 경로(안전 우선). 앱 시작 시 `--upgrade-if-stale`(바이트 비교)은 그대로라 같은 크기 재빌드는 거기서 걸러진다 |
| (다) 타임아웃 뒤 실제 상태 재확인 | 파이썬: `--enable` 이 `KCLIP_TIMEOUT` 이면 `client.running()` 으로 한 번 더 확인해 실행 중이면 성공(`reason:"recovered_after_timeout"`). Flutter: 조회 제한(60초)·요청 제한(10초)·조회 중 실패 때 `status` 를 한 번 더 불러 `running` 이면 성공 처리 |
| (라) 진행 표시·구체적 사유 | 단추가 '켜는 중… (단계 이름)' 으로 바뀐다. 실패 문구에 사유 + `(단계: …)` + `진단: …` 를 붙인다. kagent 가 `reason` 만 주던 실패(`update busy`, `still_running`, `start timeout`, `install…`, `integrity` 등)에는 한국어 설명(`explain_failure`)을 붙인다 |
| kagent `--status` 무잠금 | `--status` 는 파일 읽기뿐이므로 업데이트 뮤텍스를 잡지 않게 했다(F·G 의 줄서기 제거). 다른 명령의 잠금 동작은 그대로 |
| 파이썬 enable 제한 | `--enable` 서브프로세스 제한을 15초 → 40초(`enable_timeout`)로 늘림. 이제 UI 가 막히지 않으므로 느린 설치를 끊지 않고 기다림 |

불변: 동의 확인(`KCLIP_CONSENT`, 비동기 요청도 동기로 거부), 무결성(`.sha256`) 검사(빠른 경로도 파이썬에서 먼저 검사), 설정 키·기본값, kagent 의 종료·설치·시작·복구 순서(`lifecycle.rs` 미변경).

## 3. 구간별 계측 (enable 경로)

`enable()` 결과에 `timings: [{step, ms}]`, `elapsed_ms` 를 넣고 `docfit_app.kclip_service` 로거 INFO 로
`몽땅클립보드 켜기 구간: 무결성 확인 12ms, 실행 중 버전 확인 85ms, … (합계 NNNms)` 를 남긴다.
(성공·실패·시간 초과 모두.) 비동기 조회 결과에도 같은 `timings` 가 들어 있다.

| 경로 | 구간 이름 |
|---|---|
| 정식(제품) | 무결성 확인 → 실행 중 버전 확인 → [빠른 경로: 자동 실행 등록 → 상태 조회] 또는 [kagent --enable → (시간 초과 시) 시간 초과 뒤 상태 재확인 → 시작 확인] |
| 주입 클라이언트(시험·내장) | 무결성 확인 → 설치 → 시작 → 시작 확인 → 자동 실행 등록 → 상태 조회 (실행 중이면 버전 확인·필요 시 갱신) |

`kagent --enable` 내부의 해시·종료·설치·재시작 시간은 Rust 쪽에서 쪼개 재지 못했다(`upgrade.log` 는 실패 때만 기록). 아래 4절에서 이 구간이 길면 Rust 계측을 다음 단계로 한다.

## 4. 가짜 러너 재현 (단위 시험, 이 PC 의 실제 에이전트 미접촉)

`tests/test_kclip_service.py` — 모든 시험은 가짜 러너·가짜 클라이언트·임시 폴더만 쓴다.

| 상황 | 수정 전 | 수정 후 (시험) |
|---|---|---|
| `kagent --enable` 이 시간 초과, 에이전트는 떠 있음 | 실패(`KCLIP_TIMEOUT`) | 성공, `recovered_after_timeout` (`test_timeout_rechecks_real_state…`) |
| 시간 초과, 에이전트 없음 | 실패 | 실패 유지(`test_timeout_with_agent_not_running…`) |
| 이미 같은 버전이 실행 중 | `--enable` 호출(해시·파일 읽기) | `--enable` 호출 없음, `already_running` (`test_already_running_same_version…`) |
| 실행 중 버전이 다름 | 정식 경로 | 정식 경로 유지 (`test_running_but_different_version…`) |
| 빠른 경로에서 무결성 불일치 | (해당 없음) | `KCLIP_INTEGRITY`, 서브프로세스 호출 0회 (`test_fast_path_still_checks_integrity`) |
| 각 구간이 느림(구간마다 5초) | 합계만 | 구간별 ms 가 결과에 기록(`test_slow_lifecycle_each_step_slow…`) |
| 비동기 켜기 | 응답이 끝날 때까지 대기 | 즉시 `starting`, 단계 `kagent --enable` 표시, 완료 시 `done`, 중복 요청은 같은 job (`test_async_enable_returns_immediately…`) |
| 비동기 실패 | 사유 없음 | `failed`, 한국어 사유, `failed_phase`, `diagnostic` |
| 동의 없이 비동기 요청 | — | 즉시 `KCLIP_CONSENT`, 작업 시작 안 함 |

Flutter 위젯 시험(`flutter_ui/test/kclip_dialog_test.dart`): async 즉시 응답+단계 표시→완료 전환, 실패 사유·단계·진단 표시와 재시도 가능, 60초 조회 뒤에도 실제 실행 중이면 성공, 60초 뒤 꺼져 있으면 마지막 단계·진단 표시.

## 5. 전후 비교

| | 수정 전 | 수정 후 |
|---|---|---|
| `enable` 요청 응답 | 전체 체인이 끝날 때까지(수 초~15초+) | 즉시(스레드 시작만) |
| 화면 대기 상한 | 15초(그 뒤 실패 단정) | 최대 60초 + 마지막 실제 상태 확인 |
| 이미 켜져 있는데 켜기 누름 | 해시+파일 읽기+`--enable` 서브프로세스 | 서브프로세스 2회(`--version`, `--status`) + 레지스트리 쓰기, 재시작 없음 |
| `--status` | 업데이트 뮤텍스 대기 가능(최대 15초) | 대기 없음 |
| 실패 문구 | "…진단: enable timeout" 한 가지 | 사유(한국어)+단계+진단 |
| 진행 표시 | '켜는 중…' | '켜는 중… (단계 이름)' |

실제 ms 의 전후 수치는 이 PC 에서 실에이전트를 돌릴 수 없어 **측정하지 못했다**. 위 표의 '수정 후' 는 구조상 보장(즉시 응답·상한 변경)이며, 시간 단축 효과는 아래 확인 항목으로 검증해야 한다.

## 6. 실제 PC 에서 확인할 점

1. 앱을 새로 빌드해 설정 창에서 켜기를 눌러, 단추에 단계 이름이 바뀌어 보이는지와 성공까지 걸린 시간을 확인한다.
2. 앱 로그에서 `몽땅클립보드 켜기 구간:` 줄을 찾아 어느 구간이 긴지 본다. `kagent --enable` 이 길면 후보 C~F, `시작 확인` 이 길면 G, `실행 중 버전 확인` 이 길면 `--version` 지연이다.
3. 이미 켜진 상태에서 다시 켜기를 눌러 `already_running`(재시작 없음, 1초 안 응답)인지 확인한다. 같은 버전 재빌드 파일(크기만 같음)은 이 경로를 타므로 의도한 동작인지 본다.
4. 켜기 직후 앱 시작 업그레이드 스레드와 겹치는 상황(앱 재시작 직후 바로 켜기)에서 지연이 줄었는지 확인한다.
5. 구간 로그로 원인이 E(3초 준비 신호 제한)로 확정되면, kagent `start_and_wait` 의 3초를 늘리거나 Rust 구간 계측을 추가한다(이번에는 `lifecycle.rs` 순서를 바꾸지 않았다).

## 7. 남은 문제

- **Win32 설정 창(`settings_window.rs`)의 켜기는 여전히 15초 제한**(`enable_timeout()`=15초, 시험 `enable_deadline_is_bounded`). 이번 신고의 화면은 Flutter 대화상자(문구가 Dart 코드와 일치)라 이번에는 바꾸지 않았다. 같은 벽이 있으므로 후속 작업 대상이다.
- kagent 내부(`--enable` 의 해시·종료·설치·시작) 구간 시간은 계측하지 못했다.
- 근본 원인은 코드상 후보까지만 좁혔고 실제 PC 실측은 없다.
- 빠른 경로의 '같은 버전 + 같은 크기' 판정은 바이트 비교를 하지 않는다(앱 시작 시 `--upgrade-if-stale` 이 보완).
- Rust 시험은 이 PC 에서 휴대용 MinGW PATH 설정 후에만 링크된다(HANDOVER 기재 사항).

## 8. 시험 결과

| 시험 | 결과 |
|---|---|
| Python `tests.test_kclip_service` | Ran 53 tests, OK (신규 11건 포함, 3회 반복 OK) |
| Python 전체 `unittest discover -s tests` | Ran 3479 tests in 1247s, OK (skipped=21) |
| kagent `cargo test --release --locked` (휴대용 MinGW PATH) | test result: ok. 16 passed / ok. 160 passed, 2 ignored / ok. 7 passed / ok. 0 passed — 실패 0 |
| `flutter analyze` | No issues found |
| `flutter test test/kclip_dialog_test.dart` | 신규 4건 포함 전부 통과(수정 후 재실행) |
| `flutter test` 전체 | 796 passed, 2 skipped, **1 failed**: `library_explorer_test.dart` '탐색기 형식 구성 트리·주소 표시줄·뒤로/앞으로/위로…'. 같은 파일 단독 실행은 17개 전부 통과(All tests passed) → 전체 실행 부하에 따른 시간 의존 실패로 보이나, 전체 실행에서 통과한 것은 아니다. kclip 와 무관한 파일 |
| `flutter build web` | Built build\web 성공 |
