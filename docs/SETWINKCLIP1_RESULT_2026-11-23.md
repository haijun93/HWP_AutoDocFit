# SETWINKCLIP1 Result (2026-11-23)

## Changes

- The Win32 settings window starts kagent on a worker thread and shows the current lifecycle phase: integrity/hash, shutdown, install, or startup wait.
- The UI wait changed from 15 to 60 seconds. At timeout it does not kill the child; it checks the status PID against the actual process state and treats a live agent as success.
- The autostart checkbox value is passed as `--autostart`. The existing lifecycle fast path skips restart for a current running version and applies autostart registration.
- `KAGENT_DATA/lifecycle-timings.jsonl` records hash/integrity, bundle and installed-file reads, shutdown wait, install, startup wait, and autostart registration durations.
- `start_and_wait` now waits up to 60 seconds (previously 3). It previously entered the existing quit/install/rollback recovery sequence when status PID readiness was late, even if the child was still alive. Waiting longer lets slow first-start initialization report readiness before recovery. The recovery order is unchanged. A hung startup may remain pending for up to 60 seconds; a live agent after the limit is treated as success to avoid rolling it back.

## Verification

- `cargo test --release --locked`: passed. Test result lines: 16 passed; 160 passed, 2 ignored; 7 passed; doctests 0 passed.
- `python -m unittest tests.test_kclip_service`: Ran 53 tests, OK.
- `git diff --check`: passed.
- No running app or kagent process was started or restarted. HWP COM tests were not run as requested. Actual UI display and registry state were not verified on the user PC.
- Unit coverage checks success/failure/timeout classification and the post-timeout live-process recovery decision. A delayed fake-runner startup scenario was not implemented/run.
