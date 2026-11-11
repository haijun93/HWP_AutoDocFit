# T1111-NDET 조사 결과

고정 입력과 실행: `tests/fixtures/T1111_NDET.json`, `C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 scripts/investigate_hierarchy_ndet.py`.
최초 스크립트에는 혼합 입력이 남아 있지 않아 MISC 보고서의 문자열과 측정 스크립트의 indent 0/10/20/30, font_size 15/13/13/13을 결합해 재구성했다. 현재 Python 분석은 size_pt를 사용한다는 점도 포함해 입력을 그대로 보존했다. 결과 비교는 JSON 객체 키 순서를 정규화하며 배열 순서는 보존한다. 오류 None도 별도 결과로 집계한다(이번에는 없음).

| 경로 | 호출 | 가짓수 | Python과 같은 역할 | 다른 역할 |
|---|---:|---:|---:|---:|
| 현재 run_library | 1000 | 2 | 586 | 414 |
| 현재 run_executable | 1000 | 2 | 549 | 451 |
| Python analyze_hierarchy | 1000 | 1 | 1000 | 0 |
| e60fed03 직전 run_library | 1000 | 2 | 530 | 470 |

각 결과 원문과 빈도는 T1111_NDET_MEASUREMENTS.json에 저장했다. 두 번째 문단 역할이 내용/중제목으로 갈리며 styles, variants, warnings, 문서 유형도 함께 달라진다.

## 원인

`hierarchy/src/compat/marker_scheme.rs`의 detect_scheme에서 ordinals는 HashMap이다. `ranks=ordinals.keys().map(...)` 결과에 windows(2)의 증가 순서 검사를 적용해 official_chain을 판정한다. 1. 및 가.의 첫 등장 순서가 보장되지 않아 official_chain/support/eligible/levels가 달라진다. Python marker_scheme.py:179는 삽입 순서를 보존하는 dict를 순회한다. Rust는 호출마다 새 HashMap 무작위 시드로 같은 프로세스에서도 갈린다.

DLL ABI는 compat::analyze_hierarchy로 연결된다. e60fed03에는 hierarchy 소스 변경이 없으며 직전 브리지도 동일 DLL에서 재현됐다. 캐시 도입이 원인이 아니다. 독립 프로세스 EXE에서도 재현되므로 DLL 핸들 공유나 스레드 경합 없이 발생한다. 관련 전역은 정규식 OnceLock이며 이 판정의 입력 맵은 호출 지역 변수다. 정렬 동점보다 앞선 체계 판정에서 이미 결과가 갈리는 원인을 확인했다.

## 조치와 제안

사용자 규칙의 '결과가 바뀌면 고치지 말고 보고'에 따라 엔진 수정 및 동일성 통과를 요구하는 회귀시험은 추가하지 않았다. 수정 제안은 ordinals 조회용 HashMap을 유지하면서 별도 Vec에 번호 계열의 최초 등장 순서를 기록하고 ranks를 그 순서로 생성하는 것이다. BTreeMap이나 키 정렬만으로 대체하면 Python의 문서 등장 순서와 의미가 달라져 역순 문서가 잘못 승인될 수 있다. 후속 수정 시 정방향/역방향 번호 문서와 1000회 동일성 검사를 추가해야 한다.

추가 검증: `tools/hier_regress.py --no-build` 실제 결과 documents=29 paragraphs=549 differences=0, 1.060초. 기존 corpus의 차이 0은 이번 혼합 입력의 결정성을 보장하지 않는다. 제품 코드 변경 없음, cargo 실행 없음, COM 실행 없음. content_search.py 및 기존 파일 줄바꿈 변경 없음. 푸시·태그·fetch 없음.

재현 자료 커밋: f0bf4403. 남은 문제: 엔진의 비결정성이 그대로 남아 있으며 수정 승인 후 의미 보존 방식으로 해결해야 한다.
