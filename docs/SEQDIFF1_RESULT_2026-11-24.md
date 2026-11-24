# SEQDIFF1 결과 — 순차 문서 처리의 결과 재현성

작성일: 2026-11-24
브랜치: `feature/SEQDIFF1`
기준: PARALLEL1 커밋 `b67ba57a`, ALLQA 합성 문서 모음

## 결론

확인한 결함은 앞 문서가 남긴 한/글 프로세스 상태가 다음 문서의 산출물 메타데이터에 영향을 주는 순차 경로 의존성이다. `FileClose`만으로는 초기화되지 않았고, 다음 문서를 별도의 한/글 COM 프로세스에서 처리하면 단독 실행 결과와 같아졌다. 운영 파이프라인에 문서 사이 COM 인스턴스 재시작을 적용했다.

기존 비교에서 보인 `hwp_reference`의 차이는 `borderFillIDRef` 번호 재배치뿐이었다. 실제 borderFill 정의, 문단/쪽 서식 값은 같았다. `hwpx_continuation`에서는 실제로 XML 메타데이터 값 차이가 확인됐다. `endNotePr/noteLine/@length`가 단독 실행에서 `14692344`, 앞서 `hwp_reference`를 처리한 같은 프로세스에서는 `0`이었다. 이 합성 문서에는 각주/미주 참조가 없고 페이지·문단의 실제 서식 값은 같아 렌더링 차이는 관찰하지 못했지만, 산출물은 순서에 따라 달랐다. 문단 `p/@id`의 생성 ID도 달라졌으나 서식 속성은 아니다.

## 재현 및 영향 문서

ALLQA 합성 문서 사본으로 정방향·역방향·고정 시드 무작위 순서와 문서별 단독 실행을 비교했다. 이전 동작에서 모두 저장 성공, 오류 0건이었다.

- `txt_long → hwp_reference`에서 `hwp_reference`의 정규화 서식 해시가 단독 결과와 달랐다. `txt_short` 또는 `hwpx_continuation`을 앞에 둔 쌍에서는 이 변화가 재현되지 않았다.
- `hwp_reference → hwpx_continuation`에서 미주 선 길이 메타데이터가 단독 실행과 달랐다.
- `hwpx_shapes → hwpx_continuation`에서는 같은 차이가 재현되지 않았다.
- `FileClose`를 문서마다 수행한 `txt_long → hwp_reference`도 영향을 받았다. 닫기만으로 한/글의 문서 간 상태가 초기화되지 않는 것을 확인했다.
- 각 문서 뒤 한/글 프로세스를 종료하고 새 COM 인스턴스를 만든 시험에서는 `txt_long → hwp_reference`와 `hwp_reference → hwpx_continuation` 모두 단독 결과와 일치했다. 프로세스 PID 교체도 기록했다.

### 서식 ID 차이 판정

`allqa_compare`의 정규화에 borderFill 정의를 이용한 참조 해석을 추가했다. 참조 ID와 정의의 순서가 달라도 같은 실제 정의를 가리키면 같은 서식 해시를 내며, 참조된 borderFill의 선 종류가 바뀌면 다른 해시를 낸다. 기존 `hwp_reference` 결과들을 이 방식으로 비교했을 때 정규화 해시는 모두 `dea4e5c2062bbf7ce47260cc77011a3c2079f681d3b4350324f4f072c52d6706`으로 같았다. 따라서 기존 `borderFillIDRef` 번호 차이는 모양 차이가 아니다.

## 수정

`docfit_engine/pipeline.py`는 처리할 문서가 더 남아 있고 사용자가 중단하지 않은 경우, 현재 자동 닫기 설정에 맞춰 작업창을 닫거나 분리한 뒤 독립 COM 인스턴스를 새로 만든다. 마지막 문서 뒤에는 재시작하지 않는다. `FileClose` 단독 시험 결과로 충분하지 않았기 때문에 프로세스 재생성을 선택했다.

재현 스크립트 3개를 추가했다. `seqdiff1_probe.py`는 순서별 일괄 처리와 단독 처리를 비교하고, `seqdiff1_pair_probe.py`는 의심 쌍을 분리하며, `seqdiff1_driver.py`는 운영 처리 진입점을 호출하고 실험 옵션으로 닫기/재시작을 기록한다. 입력 문서의 사본만 사용하고 원본 fixture 해시를 확인한다. 결과 파일과 경로는 실행 시 지정하며 PC 전용 경로를 저장소에 넣지 않았다.

수정 후 `txt_long`, `hwp_reference`, `hwpx_continuation` 세 문서로 정방향·역방향·고정 시드 순서를 각각 단독 실행 결과와 비교했다. 일괄 9건과 단독 3건 모두 저장 성공, 오류 0건이었고, 텍스트 및 정규화 서식 해시 차이는 0건이었다. 입력 fixture도 변경되지 않았다.

## 검증

- `python -m unittest tests.test_allqa_tools`: 8개 통과. ID 재배치/정의 순서 차이는 같게, 실제 borderFill 값 변경은 다르게 판정하는 회귀 사례를 추가했다.
- 지정된 Windows Python 환경에서 `python -m unittest discover -s tests`: 3580개 통과, 28개 건너뜀. 종료 후 테스트 샌드박스의 SQLite 임시 파일 정리에서 파일 잠금 `PermissionError`가 무시 경고로 출력됐으나 unittest 결과는 `OK`였다. 테스트가 바꾼 추적 fixture 4개는 기준 버전으로 복구했다.
- COM 측정은 공용 잠금 아래 순차적으로 실행했다. 완료 후 잠금 파일은 남아 있지 않다.
- `hwp-auto-docfit.py`는 수정하지 않았다.

## 한계

확인 범위는 ALLQA의 세 합성 문서와 앞서 조사한 ALLQA 쌍이다. 실제 사용자의 문서 원본은 사용하지 않았다. 한/글 GUI에서 페이지를 육안 대조하지 않았으며, 미주가 실제로 있는 문서에서 선 길이 변화가 어떻게 보이는지도 확인하지 못했다. 자동 닫기를 끈 상태에서 여러 문서를 처리하는 GUI 동작은 별도로 검증하지 않았다. 따라서 이 결과는 관찰한 순서 의존성을 수정하고 합성 회귀군으로 확인한 것이며, 모든 문서 종류의 시각 결과를 보증하지는 않는다.
