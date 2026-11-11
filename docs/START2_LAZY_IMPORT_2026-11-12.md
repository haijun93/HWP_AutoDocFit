# START2 시작 때 필요 없는 모듈의 지연 불러오기 (2026-11-12)

브랜치 `perf/startup-breakdown`. 측정 도구는 `scripts/startup_breakdown.py`(소스 모듈 import만, `main()`·UI 미실행).
Python: 지정 venv(3.14.0). 한/글 COM은 쓰지 않았다.

## 1. 기준선 (변경 전, 커밋 0611c35e)

### 1-1. `scripts/startup_breakdown.py` 3회

| 회차 | 인터프리터 기동~import 종료(ms) | importtime 항목 수 |
|---:|---:|---:|
| 1 | 1922.26 | 294 |
| 2 | 1140.30 | 294 |
| 3 | 1553.31 | 294 |

- 중앙값 **1553.31 ms**, 최소 1140.30, 최대 1922.26 → **편차(최대−최소) 782 ms**. START1의 1회 값(1449 ms)도 이 범위 안이다.
- 같은 PC에서 다른 워커가 함께 돌고 있어 3회 중앙값만으로는 100 ms 안팎의 변화를 가려낼 수 없다. 그래서 아래 9회 반복 값을 함께 쓴다.

### 1-2. 9회 반복(같은 `measure()` 함수 + importtime 없는 같은 import)

| 측정 | 중앙값 | 최소 | 최대 |
|---|---:|---:|---:|
| `-X importtime` 벽시계(ms) | 1492.6 | 1236.1 | 3399.8 |
| importtime 없이 벽시계(ms) | 1201.8 | 1125.8 | 1633.8 |

importtime self 시간 합(5회): 922.3 / 1151.5 / 1066.8 / 566.5 / 952.3 ms (중앙값 952.3).

### 1-3. 누적 시간 상위 30개 (3회 중 2회차)

| 누적(ms) | 자체(ms) | 모듈 |
|---:|---:|---|
| 75.6 | 4.7 | site |
| 62.1 | 3.4 | http.client |
| 57.4 | 3.6 | __editable___govflow_python_worker_0_1_0_finder |
| 45.5 | 3.4 | traceback |
| 44.5 | 5.3 | docfit_core.batch_format |
| 42.0 | 2.7 | pathlib |
| 41.6 | 2.0 | docfit_engine.runtime |
| 41.1 | 2.9 | win32com.client |
| 40.5 | 6.1 | tkinter |
| 33.3 | 33.3 | docfit_core.korean_proofread |
| 32.5 | 2.0 | docfit_engine.context |
| 32.3 | 32.3 | _tkinter |
| 32.1 | 10.9 | _colorize |
| 31.8 | 2.0 | glob |
| 29.0 | 4.2 | defusedxml.ElementTree |
| 28.8 | 2.5 | email.parser |
| 24.2 | 2.5 | email.feedparser |
| 23.8 | 15.7 | pythoncom |
| 22.3 | 2.9 | shutil |
| 21.1 | 2.5 | dataclasses |
| 19.7 | 1.9 | email._policybase |
| 18.7 | 5.1 | inspect |
| 17.6 | 2.4 | win32com.client.gencache |
| 17.4 | 1.4 | docfit_core |
| 16.8 | 0.0 | docfit_core.fidelity.package |
| 16.8 | 1.4 | docfit_core.fidelity |
| 15.3 | 2.4 | re |
| 13.7 | 3.1 | docfit_core.document_review |
| 13.6 | 3.2 | win32com.client.dynamic |
| 12.9 | 3.9 | docfit_core.writing_aids |
