# C# 런처(`launcher/smart_launcher.cs`)와 Rust 런처(`launcher_rs/src/main.rs`) 자식 연동 차이 분석표

## 1. 개요
Rust 런처로 자식 프로세스를 기동했을 때 발생하는 약 6.6초 대기 현상(child-exit 대기)의 근본 원인을 파악하기 위해, C# 런처와 Rust 런처가 자식 프로세스에 전달하는 **인자(Arguments)**, **환경변수(Environment Variables)**, **뮤텍스 이름(Mutex Name)**, **준비 신호(Ready Signal: Pipe/Event/Port)**의 차이를 전수 비교 분석한다.

## 2. 자식 연동 상세 비교표

| 항목 구분 | C# 런처 (`launcher/smart_launcher.cs`) | Rust 런처 (`launcher_rs/src/main.rs`) | 대기 영향 및 분석 |
|---|---|---|---|
| **1. 인자 (Arguments)** | • `Arguments = string.Join(" ", args.Select(QuoteArgument).ToArray())`<br>• 커맨드라인에서 전달된 모든 인자(`--self-test`, `--self-test-report <path>` 등 포함)를 **그대로 자식 프로세스에 누락 없이 전달**.<br>• 비CLI 시 경로 인자는 `Path.GetFullPath`로 변환. | • `let child_args = if self_test { args.iter().skip(1).cloned().collect::<Vec<_>>() } else { args.clone() };`<br>• **`self_test`일 때 첫 번째 인자(`--self-test`)를 임의로 건너뛰고 자식에 전달하지 않음**.<br>• 자식(`rust_launcher_args_child.cs` 또는 `hwp-auto-docfit.py`)이 `--self-test`를 기대할 때 첫 인자가 `--self-test-report`가 되어 플래그 판별 실패 → fixture는 null 환경변수 파일쓰기로 `ArgumentNullException` 크래시 발생 후 WerFault 6.6초 대기, 실 앱은 GUI 루프로 진입. | **★ 6.6초 대기 직접 원인** (자식 인자에서 `--self-test` 탈락으로 인한 미처리 예외/충돌 후 Windows WerFault 6.6초 대기) |
| **2. 환경변수: `DOCFIT_LAUNCHER_PATH`** | • `Assembly.GetExecutingAssembly().Location`을 읽어 자식 환경변수에 **무조건 설정**. | • `if let Some(path) = env::var_os("DOCFIT_LAUNCHER_PATH") { command.env(...); }`<br>부모 환경에 이미 있을 때만 전달하고, 없을 때는 설정하지 않음. | C#과 차이 있으나 self-test 크래시의 직접 원인은 아님. (단, 일반 실행 시 update_service 등에서 필요) |
| **3. 환경변수: `DOCFIT_LAUNCHER_HANDOFF_EVENT`** | • `!selfTest`일 때 `Local\HWP_AutoDocFit_Handoff_<GUID>` 생성 후 환경변수로 전달.<br>• `selfTest`일 때는 환경변수 명시적 제거. | • `instance.is_some() && !self_test`일 때 `Local\HWP_AutoDocFit_Handoff_<PID>` 전달.<br>• 단, `DOCFIT_LAUNCHER_TEST_ROOT`가 있거나 첫 인자가 `-`로 시작하지 않으면 `instance=None`이 되어 전달되지 않음. | C#과 이벤트 이름 형식(GUID vs PID) 차이 및 설정 조건 차이 존재. |
| **4. 환경변수: `DOCFIT_LAUNCHER_MUTEX_NAME`** | • `!selfTest`일 때 `Global\HWP_AutoDocFit_SingleInstance_Mutex[_Test]` (또는 Local) 전달.<br>• `selfTest`일 때 환경변수 명시적 제거. | • `instance.is_some() && !self_test`일 때 `instance.name` 전달.<br>• `TEST_ROOT` 있을 때 C#은 `_Test` 뮤텍스를 전달하지만 Rust는 `instance=None`으로 전달 생략. | C#과 차이 존재. |
| **5. 뮤텍스 이름 및 획득 정책** | • `Global\HWP_AutoDocFit_SingleInstance_Mutex`<br>• `DOCFIT_LAUNCHER_TEST_ROOT` 있으면 `..._Mutex_Test`<br>• `!selfTest && !cliCommand`이면 획득 시도. 소유 실패 시 기존 창 활성화 후 인자 있으면 앱 전달 실행, 없으면 0 종료. | • `Global\HWP_AutoDocFit_SingleInstance_Mutex`<br>• `DOCFIT_LAUNCHER_TEST_ROOT` 있으면 `..._Mutex_Test`<br>• 단, `self_test || TEST_ROOT.is_some() || args[0]이 '-'로 시작하지 않음`이면 `instance=None`으로 뮤텍스 미획득. | C#은 TEST_ROOT일 때도 `_Test` 뮤텍스를 획득하지만, Rust는 건너뜀. |
| **6. 준비 신호 (파이프/이벤트/포트) 핸드오프 메커니즘** | • 자식 프로세스 시작(`Process.Start`) **이전에** `CreateEventW`(`EventWaitHandle`)를 생성하여 자식이 즉시 `OpenEventW` 할 수 있도록 보장.<br>• 자식이 `SetEvent` 하면 런처는 `mutex.ReleaseMutex()` 후 즉시 0 리턴 종료. | • 자식 프로세스를 `command.spawn()`으로 시작한 **이후에** `handoff_mutex` 안에서 `CreateEventW` 호출.<br>• 자식이 매우 빠르면 Event 생성 전 `OpenEventW` 실패 위험.<br>• 자식 `SetEvent` 수신 시 `ReleaseMutex` 후 0 리턴 종료. | 신호 시점(프로세스 시작 전 vs 시작 후) 차이 존재. |

## 3. 결론 및 조치 계획
1. **6.6초 대기 직접 원인**:
   Rust 런처가 `self_test` 플래그 판별 후 `child_args`를 만들 때 첫 번째 인자(`--self-test`)를 `skip(1)`하여 자식에 전달하지 않았음.
   이로 인해 fixture 프로세스는 `args[0] == "--self-test"` 조건 분기에 진입하지 못하고 `File.WriteAllLines(null, args)`를 호출하여 `ArgumentNullException`으로 비정상 종료됨.
   Windows 오류 보고(WerFault.exe)가 비정상 종료된 프로세스를 약 6.6초(6,634ms ~ 6,644ms) 동안 붙잡고 대기한 뒤 종료시켰으며, Rust 런처의 `command.status()`가 이 6.6초 동안 멈춰 있었음.
2. **단일 수정 방안**:
   Rust 런처의 자식 인자 전달부를 C#과 동일하게 커맨드라인 인자 전체를 그대로 전달하도록 `let child_args = args.clone();`으로 수정함.
