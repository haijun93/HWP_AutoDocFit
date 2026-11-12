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

## 2. 옮기거나 뺀 import (4건)

| 커밋 | 내용 | 확인한 것 |
|---|---|---|
| 12a1f382 | `docfit_core.korean_proofread`(4개 이름)·`document_review`(3개)·`pasted_text`(2개) import 제거. `docfit_core.ai_answer` 3개 이름은 `formatter_text`의 `AI답변_개조식_변환` 안으로 이동 | 뺀 9개 이름은 `git grep -w`로 엔진·화면·앱·시험 어디에서도 이름 공간 전역으로 읽지 않는다(`PURPOSES`는 `native_file_dialog_service`의 다른 상수, `pasted_text`는 `document_tools_service`가 자체 import). 모듈 자체와 그 시험은 그대로 |
| 10aec3ff | `docfit_core.batch_format`(3개 이름)·`fidelity.package.UnsupportedPackage`를 `formatter_apply`의 `표준서식_hwpx_처리` 안으로 이동 | 네 이름은 이 함수(1393~1613행) 안에서만 쓰이고, import 줄이 첫 사용보다 앞에 있다. 이 이름을 이름 공간에 넣는 시험 4개는 같은 객체를 넣으므로 결과가 같다 |
| 04ef855e | `writing_aids.refresh_dates`(`날짜_현행화`)를 `formatter_titles`의 `_칸_날짜_현행화` 안으로 이동, 쓰이지 않는 `writing_aids` 모듈 이름 import 제거 | `날짜_현행화`는 이 함수에서만, `writing_aids` 이름은 `document_tools_service`(자체 import)에서만 쓰인다 |
| 31c5dd2e | `http.client`·`ssl`을 `com_driver`의 `_업데이트_HTTP_GET` 안으로 이동 | `http`·`ssl` 이름은 이 함수에서만 쓰인다(`update_service`는 자체 import). 가짜 연결로 함수를 직접 불러 호스트·포트·요청 경로가 같고, 허용되지 않은 주소는 전과 같이 막히는 것을 확인 |

- 최상위(클래스 상속·데코레이터·전역 초기화)에서 쓰이던 이름은 없다.
- 패키징: `HWP_AutoDocFit.spec`이 `collect_submodules('docfit_core')`로 모든 하위 모듈을 넣으므로, 시작 때 import하지 않게 된 모듈도 실행 파일에 그대로 들어간다(실제 빌드는 하지 않았다).
- 스레드: 함수 안 import는 작업 스레드에서 처음 실행될 수 있다. 같은 모듈을 두 스레드가 동시에 처음 불러도 Python의 모듈별 import 잠금이 한 번만 실행하게 하고, 옮긴 모듈들은 import 때 COM·화면·전역 상태를 건드리지 않는 순수 모듈(또는 표준 라이브러리)이다.
- `hwp-auto-docfit.py`는 CRLF 유지(3,506줄 모두 CRLF). `_engine_resolve`·`bind_function` 구조는 바꾸지 않았다.

## 3. 전후 측정

같은 PC에서 세 상태(변경 전 873515df / 3건 뒤 04ef855e / 4건 뒤 31c5dd2e)를 번갈아 9회씩 돌렸다(전체 시험이 끝난 뒤, 다른 시험·빌드 없이). 단위 ms.

| 상태 | 시작 import 모듈 수 | 벽시계 중앙값(최소~최대) | importtime 켠 벽시계 중앙값 | importtime self 합 중앙값 |
|---|---:|---:|---:|---:|
| 변경 전 | 294 | 1010.4 (877.1~1252.8) | 975.2 | 592.2 |
| 3건 뒤 | 271 | 901.8 (870.5~1043.7) | 919.9 | 510.7 |
| 4건 뒤(현재) | 249 | **894.7** (829.1~1049.0) | 864.1 | **486.7** |

- 변경 전 → 현재: 벽시계 중앙값 **1010.4 → 894.7 ms(−115.7 ms, −11%)**, self 합 592.2 → 486.7 ms(−105.5 ms), 모듈 294 → 249개(−45).
- `scripts/startup_breakdown.py` 3회(현재): 863.57 / 957.33 / 991.09 ms, 중앙값 **957.33 ms**(항목 249개). 1절의 기준선 3회(중앙값 1553.31 ms)는 다른 워커가 함께 돌던 때 잰 값이라 이 값과 바로 비교하면 개선이 부풀려진다. 비교는 위 표(같은 시각에 번갈아 잰 값)로 한다.
- 편차: 같은 상태 안에서도 최대−최소가 170~375 ms다. 4번째(http.client) 변경의 벽시계 효과(901.8 → 894.7)는 편차 안이라 벽시계만으로는 입증되지 않고, self 합(−24 ms)·importtime 켠 벽시계(−56 ms)·모듈 수(−22)가 줄어든 것으로 판단해 남겼다. 시험 중(다른 작업이 돌던 때) 잰 앞선 9회에서는 변경 전 1396.4 → 현재 1082.1 ms였고, 같은 코드 두 벌 사이에도 160 ms 차이가 났다.
- 절대값이 START1(1,449 ms)보다 작은 것은 PC 부하 차이다.

## 4. 건드리지 않은 후보와 이유

| 후보(현재 누적 ms) | 이유 |
|---|---|
| `win32com.client`(36)·`pythoncom`(23)·`win32gui`·`win32con` | 한/글 COM. `pythoncom`은 처음 import하는 스레드에서 COM을 초기화하므로 미루면 초기화되는 스레드·순서가 바뀔 수 있다. 엔진 여러 모듈이 이름 공간 전역으로 쓴다. 확신이 없어 미루지 않았다 |
| `tkinter`(39, 그중 `_tkinter` 32)·`tkinterdnd2`·`ttk`·`simpledialog` | 화면 자체. 첫 화면 전에 반드시 필요하다 |
| `traceback`(50, 하위 `_colorize` 35) | 엔진·화면 곳곳의 예외 처리에서 전역 이름으로 쓰여 한 함수로 옮길 수 없다. 표준 라이브러리 내부 비용 |
| `site`(76)·`__editable___govflow_python_worker…finder`(56) | 앱 코드가 아니라 이 개발용 venv의 `.pth` 처리. 배포 실행 파일에는 없다 |
| `docfit_engine.runtime`(40)·`context`(32) | 엔진 뼈대(`_engine_resolve`가 쓰는 구조)라 손대지 않았다 |
| `docfit_core/__init__`의 `hwpx`·`kordoc_bridge`(13) | 패키지 공개 이름이라 미루려면 패키지 구조를 바꿔야 한다. 범위 밖 |
| `shutil`(23)·`hashlib`(14)·`zipfile`(11)·`json`·`subprocess`·`tempfile`·`uuid`·`urllib.parse` 등 | 각각 15 ms 안팎 이하이고 이름 공간 전역으로 여러 엔진 모듈이 쓴다. 옮길 곳이 많아 얻는 것보다 위험이 크다 |
| `docfit_core.updater`(15) | 자체 시간은 1.8 ms이고 나머지는 `docfit_core` 패키지 초기화(어차피 필요)다 |
| `docfit_core`의 나머지 서식 모듈(각 10 ms 이하) | 전역 초기화·여러 함수에서 쓰이는 것이 섞여 있고 하나하나의 몫이 측정 편차보다 훨씬 작다 |

## 5. 시험

- `python -m unittest discover -s tests`(4건 뒤, 31c5dd2e): **2,788건 실행, 통과, 건너뜀 8, 796.5초**. 실패·오류 0.
- 관련 시험 `tests.test_startup`·`tests.test_update_service`: 47건 통과(건너뜀 1).
- 전체 시험이 `tests/fixtures/timemachine_extract/`의 파일 7개를 다시 써서 작업 폴더가 바뀐 것으로 나왔다(이번 변경과 무관한 기존 시험의 동작). 커밋하지 않고 되돌렸다.

## 6. 확인하지 못한 것

- 실제 앱 실행(첫 화면까지 시간)·PyInstaller 빌드·한/글 COM 동작은 확인하지 않았다. 측정은 소스 import만이다.
- 실제 네트워크로 업데이트 조회를 해 보지 않았다(가짜 연결로만 확인).
- 첫 업데이트 조회·첫 일괄 서식·첫 AI 답변 변환·첫 날짜 현행화 때 미룬 import 시간(합쳐 약 100 ms)이 그 시점으로 옮겨 간다. 그 시점의 지연은 따로 재지 않았다.
