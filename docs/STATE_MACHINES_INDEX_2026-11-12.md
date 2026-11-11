# SM1 — 타임머신 본문 색인의 암묵적 상태 기계

- 기준: 2026-11-12, `feature/hs2-hsa`. 코드 변경 없이 조사 문서 1개만 작성한다.
- 선행 확인: HANDOVER.md와 docs/TEAM_OPERATING_RULES.md를 읽었다. 현재 팀 규칙에는 요청한 **0절이 없고 1~8절만 있다**. 누락된 대원칙의 원문·취지는 미확인이다.
- 구현 조사 범위: `docfit_core/timemachine/content_search.py`, `git grep ContentSearchIndex`로 찾은 `docfit_app/timemachine_service.py`, Flutter 타임머신의 색인 상태 표시 부분이다.
- 아래 C=`docfit_core/timemachine/content_search.py`, S=`docfit_app/timemachine_service.py`, U=`flutter_ui/lib/features/timemachine/timemachine_screen.dart`. `C:992` 등은 실제 파일의 줄 번호다.
- 결론: 전체 작업을 나타내는 enum/상태 문자열은 없다. 서비스의 `running/cancelled/seen/indexed/errors/started_at/finished_at`, Event·Thread, DB 행·버전을 조합한다. Rust 출력의 `indexed/skipped/error` 문자열(S:130)은 파일별 이벤트이며 작업 상태가 아니다.
- 저장 상태(파일별)와 작업 상태(전체)는 직교한다. 아래 이름은 조사자가 붙였으며 상호 배타적인 기존 상태 값이 아니다.

## 1. 상태 목록

| 상태 이름 | 지금 코드에서 알아내는 조건(파일:줄) | 읽는 곳(서비스·화면) |
|---|---|---|
| 색인 없음 | 활성 루트의 `indexed_paths()`가 빈 목록(C:1188); 오류 행만 있는 경우도 포함 | S:528에서 자동 갱신 생략. U:1473은 일반 시작 버튼만 표시, 없음 전용 표시는 없음 |
| 작업 이력 없음/대기 | `running=False`, `started_at/finished_at=None`(S:60) | 상태 API S:590; 기존 DB 유무와 별개. U:56의 초기 false와 구별되지 않음 |
| 최초/일반 색인 중 | `running=True`, 시작 시각 있음, 종료 시각 없음(S:248); 최초 여부를 저장하지 않음 | S:243 중복 시작 차단; U:529·538→1469 진행률 |
| 재색인 필요 | 루트 내 meta의 버전이 현재보다 낮거나 문단 map 수=0(C:992·1011) | S:66 자동 갱신, S:593 상태 API. U:346·894가 별도 필드로 안내하려 함 |
| 재색인/증분 갱신 중 | 위 필요 조건과 `running=True`의 조합 또는 기존 색인에 대한 갱신 호출(S:523·534); 전용 플래그 없음 | U는 일반 색인 중과 같은 표시. 자동 갱신 여부·작업 종류는 모름 |
| 파일 최신/건너뜀 | size·mtime 동일, 현재 버전 이상, 문단 있음(C:1116·1124), 반환 `indexed=False,error=None` | S:213에서 seen만 증가. U에는 확인 건수로만 반영 |
| 작업 종료(완료 후보) | `running=False,cancelled=False,finished_at!=None`(S:160) | U:538→1473 시작 버튼. 성공 완료 전용 문구 없음; 예외 종료도 같은 조합 가능 |
| 파일 오류/오류 포함 종료 | `content_errors` 행(C:1174·2009), 또는 작업 `errors>0`(S:215) | S:668 오류 목록 API; U:1479는 작업 오류 건수만 표시. 영속 오류와 작업 건수는 별개 |
| 취소 요청 중 | Event set, 아직 running=True(S:266); `cancelled`는 아직 false일 수 있음 | 취소 응답 S:276, U:545에서 계속 진행 중으로 표시 |
| 취소 완료(부분 색인 보존) | `running=False,cancelled=True,finished_at!=None`(S:162) | U:545·538은 cancelled를 읽지 않고 시작 버튼 표시 |
| 일시정지 | pause/resume 상태·명령 없음. 취소 후 재시작은 새 작업(S:245·248), 최신 파일 건너뜀(C:1116) | 화면에도 일시정지 표시는 없음. 이어하기 위치 보장 없음 |
| DB 이전 실패 | `migration_error` 설정 후 RuntimeError(C:805·806) | S:42의 생성 자체가 실패할 수 있음. 상위 생성 호출부·화면 전달은 범위 밖 미확인 |

## 2. 전이

| 어디에서 → 어디로 | 계기(파일:줄) | 실패하면 |
|---|---|---|
| 대기/종료 → 색인 중 | 수동 시작 S:241·248; 이미 실행/스레드 생존이면 상태만 반환(S:243) | start 예외는 running=False와 종료 시각 설정, TM_INDEX_FAILED(S:255). 상태 자체에는 실패 원인 없음 |
| 기존 색인 → 자동 갱신 중 | 서비스 생성 시 필요 건수>0(S:66), 스캔 뒤 갱신(S:486); 기존 경로가 있어야 함(S:528) | 자동 경로의 thread.start에는 수동 경로의 복구 처리 없음(S:539); running=True 잔류 가능 |
| 최신 파일 → 최신 파일 | 메타·문단 검사 통과(C:1116) | 검사/추출의 try 안 예외는 파일 오류 처리(C:1165) |
| 없음/구버전/변경 파일 → 최신 파일 | 추출 후 문단·meta·현재 버전 저장, 오류 제거(C:1126·1136·1160·1163) | try 안 실패는 기존 검색 행·meta 삭제 후 오류 행 저장(C:1169). 오류 저장 자체 실패는 외부로 전파 가능 |
| 색인 중 → 계속 색인 중 | Rust 실패 파일을 Python에 넘김(S:141·207), 각 파일 결과로 카운터 갱신(S:212) | Python의 반환 오류는 errors 증가 후 계속. Rust 내부 구현은 미확인 |
| 색인 중 → 작업 종료 | 순회·색인·삭제 정리 후 finally(S:218·238) | 미포착 예외도 동일 finally로 종료. 예: `os.stat`은 try 밖(C:1103)이라 파일 오류 카운터 없이 종료 가능 |
| 실행 → 취소 요청 → 취소 완료 | Event 설정(S:266), 각 루프 검사(S:172·211·230), finally(S:162) | wait 시간 초과는 TM_CANCEL_TIMEOUT(S:274), 작업이 즉시 끝났다는 뜻 아님 |
| 취소 완료 → 새 색인 중 | 재시작 시 Event·카운터 새로 생성(S:245·248) | 시작 실패는 위 수동 시작 행과 동일. 남은 작업 위치는 저장하지 않음 |
| 색인 있음 → 없음/부분 보존 | 제외·삭제 정리(S:222·237), remove_file(C:1177) | 연결 끊긴 루트는 삭제하지 않음(S:223). 정리 예외는 finally 종료로 합류 |

## 3. 문제와 영향

1. **성공 종료와 예외 종료가 같은 조합으로 표현됨.** S:238은 무조건 완료 처리하고 전체 실패 사유를 기록하지 않는다. C:1103의 stat 예외, 파일 루프 밖 예외는 `errors=0,cancelled=False,finished_at!=None`로도 끝날 수 있다. 자동 시작 실패는 반대로 `running=True`인데 작업 스레드가 없는 조합이 가능하다(S:534·539). 실제 장애 발생 빈도는 미확인이다.
   - UX: 중단을 완료로 오해하거나 진행 중 표시가 끝나지 않을 수 있다.
   - DX: 카운터만으로 작업 실패와 정상 종료를 구별할 수 없다.
   - AX: 상태 API만 읽어 재시작·복구 여부를 결정할 근거가 부족하다.
2. **재색인 안내의 생산자와 소비자가 어긋남.** S:590만 필요 필드를 반환하지만 U:346은 검색 응답에서 읽는다. 본문 검색 응답은 `ok/items`뿐(S:606)이며 U:538의 상태 조회는 안내용 필드를 갱신하지 않는다. 따라서 이 경로로는 재색인 배너(U:894)의 조건이 참이 되지 않는다.
   - UX: 재색인이 필요해도 안내·재색인 버튼을 보지 못한다.
   - DX: 서버 상태 필드가 존재해도 화면 연결이 정상이라는 보장이 없다.
   - AX: API 계약과 화면 계약을 각각 추론해야 하며 잘못된 완료 판단을 하기 쉽다.
3. **자동 갱신과 화면 진행 상태가 분리됨.** S:66·486은 자동 실행하나 U:530의 폴링 진입은 수동 시작 응답에 달렸다. U:300·303은 폴링을 끊고 화면 실행 플래그를 내린다. U:540은 통신 예외에도 플래그만 내린다.
   - UX: 실제 재색인 중인데 진행 표시가 없거나 통신 실패를 대기로 보게 된다.
   - DX: 서비스 Event·Thread와 화면 bool의 불일치를 재현해야 한다.
   - AX: 화면 관측만으로 작업이 멈췄다고 단정할 수 없다.
4. **재색인 필요와 최신 판정의 의미가 다름.** C:1011은 버전·문단만 검사하고 C:1118은 size·mtime까지 검사한다. 파일 내용 변경은 `needs_reindex=0`이어도 재처리 대상이다. 구버전 파일 추출 실패는 meta 삭제(C:1172)로 필요 건수까지 줄지만 오류는 남는다.
   - UX: 필요 건수 감소·0건을 검색 최신성이나 성공으로 오해할 수 있다.
   - DX: 같은 '재색인' 용어가 스키마 보수와 파일 갱신을 함께 가리킨다.
   - AX: 필요 건수만으로 색인의 완전성·최신성을 증명할 수 없다.
5. **취소·완료·영속 오류가 화면에서 축약됨.** U:1473은 running=false를 모두 같은 버튼으로 표시하며 S:668의 영속 오류와 S:215의 이번 작업 오류는 다르다. U:1479는 오류가 0이어도 키가 있으면 빨간 실패 문구를 표시한다.
   - UX: 부분 완료·취소 완료·과거 실패를 구분하기 어렵다.
   - DX: 상태 복원 시 어떤 오류 수치를 보여야 하는지 별도 규칙이 필요하다.
   - AX: 취소 후 남은 데이터와 실패 이력을 한 응답에서 파악할 수 없다.

## 4. 기존 동작을 유지하는 최소 제안(구현하지 않음)

| 변경 | 보존할 동작·주의점 | 깨질 수 있는 기존 시험 이름(이름만 확인, 세부 보장 범위 미확인) |
|---|---|---|
| S에 `content_status()` 하나를 두고 기존 필드와 추가 `phase` 반환 | 기존 시작/취소/상태 명령이 같은 투영 함수를 호출. idle/running/cancel_requested/cancelled/finished 정도만 확실히 구분하고 finished를 성공으로 단정하지 않는다. DB 없음·재색인 필요는 별도 축으로 유지; 카운터·명령·추출 순서는 유지 | `test_background_content_index_progress_and_cancel`, `test_content_index_refresh_is_single_flight`(tests/test_timemachine_service.py) |
| C의 기존 판정 결과를 상태 요약에 모아 기존 필드와 함께 반환 | `needs_reindex`의 기존 의미를 바꾸지 않고 스키마 필요 건수·색인 유무·영속 오류 수를 구분. 조회에 파일 전체 stat 순회를 새로 넣지 않음. U의 소비 위치 수정은 별도 승인할 동작 변경으로 남김 | `test_needs_reindex_count_drops_after_background_index`(tests/test_timemachine_service.py), `test_old_rule_search_survives_reopen_and_reindex`(tests/test_hs2_p_title_reindex.py) |
| U에 응답→화면 상태 해석 함수 1개 도입 | 기존 폴링 주기·시작 지점·표시 문구는 유지하고 running/카운터 파싱만 한곳으로 이동. 자동 폴링·배너 복구는 이번 제안의 기계적 통합과 분리 | Flutter 시험 이름은 범위 밖으로 미확인. Python 후보 `test_service_open_reindexes_in_existing_background_worker`(tests/test_hs2_p_title_reindex.py)는 서비스 자동 갱신 회귀 확인용 |

## 5. 검증과 한계

- 지정 Python으로 `tests.test_hs2_p_title_reindex tests.test_timemachine_service tests.test_timemachine_content_search` 실행: 148개, 98.908초, 실패 0·오류 0, OK(워커 실행 결과). 문서 외 코드·시험·원본 문서는 변경하지 않았다.
- 단, 출력에 `Cannot operate on a closed database` 자동 갱신 오류 로그가 2회 있었다(S:486→528→C:1191). unittest 실패에는 반영되지 않았다. 발생 시험의 정확한 이름·기존 발생 여부는 미확인으로 남기며 코드 수정·재시험은 하지 않았다.
- 관련 Python 시험의 이름을 확인하고 실행했다. 조사 범위 밖 구현, Rust 내부, 실제 Flutter 실행·통신 장애·스레드 시작 실패 주입은 미확인이다.
- 작업 상태의 성공/실패를 새로 정의하거나 화면 표시를 고치는 작업은 수행하지 않았다. 푸시·태그·원격 접근 없음.