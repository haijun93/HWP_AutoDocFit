# USAGEDASH 구현 결과

실행은 저장소 루트에서 `python tools/usage_dash/serve.py`를 실행한 뒤 브라우저에서 `http://127.0.0.1:8765`에 접속합니다. 서버는 loopback 인터페이스에만 바인딩합니다. `collect.py`는 30초마다 `tools/usage_dash/usage.json`을 원자적으로 갱신하고 페이지는 5초마다 다시 읽습니다. 수집 데이터는 외부 서비스로 전송하지 않습니다.

## 값의 정확도

- **Codex PC 세 계정**: `.codex`, `.codex-ngaytot9`, `.codex-2be`의 세션 JSONL에서 가장 최신 `event_msg` / `token_count` 이벤트의 `rate_limits.primary/secondary.used_percent`, `resets_at`, 창 길이를 표시합니다. 값은 해당 계정 로그가 기록한 정확한 스냅샷입니다. 로그가 없으면 비어 있는 상태로 표시합니다.
- **Codex Mac**: `hyeokjunkong@192.168.0.4`의 `~/.codex/sessions`를 SSH로 읽습니다. SSH 실패·권한 거부 시 읽기 불가로 표시됩니다. 토큰 이벤트의 한도 값은 로그 기준 정확한 스냅샷입니다.
- **Claude PC 두 설정 폴더**: `~/.claude`와 `~/.claude-ngaytot9`의 JSONL `message.usage`에서 타임스탬프가 지난 5시간·7일인 입력/출력 및 cache token 필드를 더합니다. 이는 로컬 세션 로그에 기록된 토큰 합계이며 실제 공급자 한도 사용률은 아닙니다. 표시하는 한도 %는 **추정/제공 안 됨**입니다. 세션 로그 안에서 `rate limit`, `usage limit`, `limit reached`, `overloaded_error` 문자열이 발견되면 빨강으로 표시합니다.
- **Gemini (agy) 세 계정**: PC의 `C:\HWP_AUTODOCFIT\v1803\reports`에서 파일명이 `agy` 또는 `maccoder`인 로그를 검색합니다. 한도/429/quota 문구의 타임스탬프(없으면 파일 수정 시각)와 최근 7일의 `agy`/`maccoder` 문자열 기반 실행 건수를 표시합니다. 로그마다 기록 방식이 다르므로 실행 수는 로그 내용에 따른 근사치입니다. 할당량 %는 **제공 안 됨**입니다. 최근 worker 로그 파일 이름과 수정 시각도 표시합니다.

현재 맥 Gemini 계정의 `reports` 경로는 요청에 구체적으로 지정되지 않아 자동 SSH 조회는 구성하지 않았으며, UI에 설정 미완료 상태로 표시됩니다. 정확한 로그 위치는 추후 계정별 경로로 설정할 수 있습니다.

## 시험

`python -m unittest tools.usage_dash.tests.test_collect`로 가짜 JSONL 및 로그 파서를 확인할 수 있습니다. 저장소 기본 Python 가상환경은 이 작업 디렉터리에 없어 시스템 `python`을 사용합니다.
