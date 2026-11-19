# 작업자 3기기 분산 배치 (2026-11-19)

목표: 이 PC의 컴파일·시험 부하를 줄이고(여러 작업이 겹치면 전체 시험이 25분→60분으로 늘고 속도 시험이 거짓 실패한 사례), 이식 가능한 일은 맥 두 대로 보낸다.

## 기기별 역할

| 기기 | 사양·도구 | 맡길 일 | 맡기지 않는 일 |
|---|---|---|---|
| **이 PC** (Windows, PM) | 릴리스 파이프라인, 한/글 COM, Win32 Rust(ktm·kvf·kagent·dock·ui_theme·qms_bench의 win 모듈), CI와 설치 파일 | 최종 판정(맥 결과 재시험), 전체 Python 시험, `flutter analyze`·`flutter test`·웹 빌드 최종 확인, Win32 UI 작업(캡처 모드 포함), 설치·태그·병합 | 동시에 무거운 컴파일 3개 이상 |
| **맥 스튜디오** (M1 Max 10코어·32GB, 외장 2TB `~/docfit-ext`) | flutter 3.50·dart·node·cargo·python 3.14·codex(haijun93)·agy(Gemini haijun2be) | **Flutter 화면 작업과 위젯 시험**(CI는 3.47.9라 최종 확인은 이 PC), 순수 Python 서비스·시험, 이식 가능한 Rust(kfind-core·outliner_core·kfind/annual·hierarchy·pdf_edit_core·qms_bench 독립부), 문서·분석 스크립트 | Win32·COM·설치 파일·exe |
| **맥북 에어** (M3 8코어·16GB) | cargo·python 3.14·agy(Gemini) | 문서·가벼운 Python 카드, 작은 Rust 이식 크레이트, 코드 읽기 위주 점검 | flutter 필요 작업(미설치), 장시간 컴파일 |

## 실행 방법
- 맥 Codex·Gemini: `tools/run_mac.sh <studio|air> <codex|gemini> <카드.md> <이름> [effort]` (코드는 git bundle로 보내고 결과는 `mac/<이름>` 브랜치로 받는다). Gemini는 맥 화면 터미널의 `agy_worker.sh`가 켜져 있어야 한다.
- 이 PC Gemini: `tools/run_gemini.sh <폴더> <카드> <이름>`. 이 PC Codex: `tools/run_codex.sh`(한도 5시간).
- Claude: 이 세션의 하위 작업자(Agent), 또는 로그인된 계정 폴더로 `tools/run_claude2.sh`(ngaytot9)·`tools/run_claude_h2b.sh`(haijun2be).
- 어떤 결과든 **이 PC에서 다시 시험한 뒤 병합**(워커 보고를 믿지 않는다). 푸시는 시험이 통과할 때만.

## 부하 규칙
- 이 PC에서 동시에 도는 컴파일·시험은 2개까지. 전체 Python 시험(릴리스 후보)은 다른 컴파일을 멈춘 상태에서 단독으로.
- 시험이 도는 작업 폴더에서 브랜치를 바꾸거나 파일을 건드리지 않는다.
