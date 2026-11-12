# 에이전트 역할 정의 (2026-11-12)

GitHub Skills "agent-orchestration-build-your-ai-dream-team"의 방식(역할별 정의 파일, 결과 중심 위임, 파일 범위 지정, 겹치지 않을 때만 병렬, 단계마다 요약)을 우리 팀에 맞춘 것이다. 상위 규칙은 `docs/TEAM_OPERATING_RULES.md`(0절 대원칙 포함).

| 역할 | 파일 | 맡는 워커 |
|---|---|---|
| 총괄 | `orchestrator.md` | Claude 총괄 세션 |
| 설계 | `planner.md` | Claude 워커(haijun93·ngaytot9), 한도면 Codex 상위 모델 |
| 구현 | `coder.md` | Codex ngaytot9(상위 모델), Codex haijun93(작은 과제) |
| 화면 | `designer.md` | Codex haijun93 |
| 검증 | `verifier.md` | 총괄이 직접(판정), 사람에게 나가는 글은 다른 워커 1곳 |

카드는 `card-template.md` 형식으로 쓴다.
