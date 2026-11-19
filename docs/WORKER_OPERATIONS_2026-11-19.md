# 3기기·8워커 개발 체계 (2026-11-19, PM 설계)

워커: Claude 3(haijun93=PM, ngaytot9, haijun2be), Codex 2, Gemini 3. 기기: 이 PC, 맥 스튜디오, 맥북 에어. 기기별 사양·도구는 `WORKER_PLACEMENT_2026-11-19.md`.

## 1. 원칙
1. **일은 기기 제약으로 먼저 나눈다.** Win32·한/글 COM·exe·설치·CI는 이 PC. Flutter 위젯·이식 가능한 Rust·순수 Python은 맥 스튜디오. 문서·가벼운 점검은 맥북 에어.
2. **워커 종류는 일의 난이도로 나눈다.** Claude=설계·어려운 판단·재시도 불가 작업, Codex=코드 구현·검수, Gemini=반복 구현·수정(Flash 우선, 한도 시 Argon low→Sonnet low→GPT-OSS).
3. **병합은 이 PC에서 재시험 후에만.** 워커 보고를 믿지 않는다.
4. **이 PC 부하**: 동시 컴파일·시험 2개까지, 릴리스 후보 전체 시험은 단독.
5. 한 카드는 한 워커. 같은 카드 재실행은 1회까지(추론 강도 low→medium→high).

## 2. 배치표
| 워커 | 기기 | 맡길 일 | 상태(11-19 22:55 확인) |
|---|---|---|---|
| Claude haijun93 (PM) | 이 PC | 지휘·판정·병합·릴리스·이 PC 전용 어려운 과제 | 정상 |
| Claude ngaytot9 | 맥 스튜디오 | Rust 전환 중 이식 가능 로직(kfind-core·outliner_core 등), Flutter 화면 설계 | **11-19 재로그인, 작은 카드 시험 통과(exit=0)** |
| Claude haijun2be | **이 PC** | Rust+Win32 전환 카드(Win32 UI·COM PoC), 시험까지 한 자리에서 | **로그인 필요(미로그인)** |
| Codex 1 | 이 PC | Win32 Rust 구현·검수(ktm·kvf·kagent·dock) | 한도, 11-20 05:20 이후 |
| Codex 2 | 맥 스튜디오 | 이식 가능 Rust 구현·검수 | 로그인 완료 |
| Gemini 1 | 이 PC | 수정·반복 구현 | 미점검 |
| Gemini 2 | 맥 스튜디오 | Flutter 위젯·Python 서비스 반복 구현 | 미점검 |
| Gemini 3 | 맥북 에어 | 문서·가벼운 Python·작은 Rust 이식 | 미점검 |
맥북 에어에는 Claude를 두지 않는다(Claude는 어려운 과제용이라 기기 제약이 적은 스튜디오·PC에 둔다).

## 3. 운영 순서
- 카드 흐름: PM 카드 작성 → 기기별 큐(맥 스튜디오 `~/work/cjobs/*.job`, 맥 Codex·Gemini `tools/run_mac.sh`, 이 PC `run_codex.sh`·`run_gemini.sh`) → 결과 브랜치(`mac/<이름>`·`feature/<이름>`) → PM이 이 PC에서 재시험 → 병합.
- 감시: 30분마다 로그 0바이트·프로세스 없음이면 '멈춤' 보고, 쉬는 워커는 다음 카드 배정.
- 우선순위: 사용자 신규 TODO > 릴리스 진행 > Rust+Win32 전환(RUSTALL3 로드맵) > 기존 개선.
- 맥 스튜디오 Claude 워커는 키체인 때문에 맥 터미널 창에서 실행(`zsh ~/work/claude_worker.sh`). 창이 닫히면 멈춘다.

## 4. 즉시 배정(첫 주기)
- ngaytot9(스튜디오): 전환 로드맵 R-0 중 이식 가능 부분 — 브리지 메서드 호출 표 자동 추출.
- Codex 2(스튜디오): `klib rename`과 Python 규칙 결과 비교 도구.
- haijun2be(이 PC, 로그인 후): 한/글 COM Rust PoC(COM 잠금 필수).
- 이 PC Codex(05:20 이후): 몽땅검색(메뉴 7)을 ktm 창으로 연결하는 플래그.
- Gemini: 앱 기준선 측정 스크립트(에어), 문서 정리(에어), Flutter 위젯 시험 보강(스튜디오).
