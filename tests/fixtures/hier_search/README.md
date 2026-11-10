# 위계검색 회귀 자료

`regressions.json`은 개인정보 없는 합성 추출 노드 5문서·6질의다. 문서 원본이나 실제 PDF/OCR/kordoc 변환 결과가 아니다. 목차 없는 PDF, 잘못된 번호, 표 안 제목, `<hp:tab>`을 탭으로 추출한 글, 불완전 내어쓰기 메타데이터를 작은 입력으로 고정했다. XML 파싱과 실제 파일 변환 시험은 별도로 수행해야 한다.

저장소 루트에서 다음과 같이 실행한다.

```powershell
$env:PYTHONIOENCODING='utf-8'
& 'C:/Users/haiju/OneDrive/바탕 화면/HWP_AutoDocFit/mapo-agent-1/.venv/Scripts/python.exe' tools/hier_search_eval.py --output E:/docfit-tmp/HSA1.eval.json --k 5 --repeats 5
```

외부 평가 자료는 `--corpus <JSON>`으로 지정한다. 동일 스키마의 `documents[].nodes`와 `queries[]`를 준비한다. 노드 ID는 문서 안에서 고유하며 `parent_id`는 같은 문서 ID를 참조한다. 정답 ID는 `<문서id>/<노드id>`이고 `expected_context`는 정답이 가져야 할 상위 문맥의 필수 부분 문자열 목록이다. 미지원·미검증 사항은 `unsupported`, `unverified`에 기록하며 결과 파일로 그대로 옮긴다.

평가기는 모든 문서의 ID를 구별한 한 코퍼스에서 두 검색을 실행한다. 본문 전문검색 대조군은 본문 토큰만, 구조검색은 실제 `build_index/search` API를 쓴다. 둘 다 같은 입력 노드 집합에서 시작한다. Recall@k·MRR@k는 정답 ID 기준이며 질의별 평균이다. 문맥 정확도는 반환된 정답 중 필수 상위 문맥을 모두 포함한 비율이며, 정답을 못 찾으면 0이다. 문맥 주석이 없으면 해당 평균에서 제외한다. 메모리는 Python 할당량만 측정하므로 Rust/native/RSS와 다르다. 소규모 합성 자료의 결과를 실문서 품질·성능으로 일반화하지 않는다.

`legacy_2b7ec241_top5.json`은 `git show 2b7ec241:docfit_core/hierarchy_search.py`의 원본 함수를 같은 코퍼스에 실행해 얻은 ID·점수다. 평가 도구의 고정 회귀 대조군을 이 자료와 비교한다. 결과 파일은 대조군 대비 순위 변화와 원본 커밋 대비 순위 변화를 별도 기록한다. 기준값을 낮추는 자동 보정은 없다.

`kordoc_tab.xml`은 별도의 작은 합성 XML이다. defusedxml과 기존 문단 텍스트 판독기를 거쳐 탭 요소 뒤 본문이 검색되는지, 불완전 들여쓰기에서 기존 level이 유지되는지 시험한다. 실제 kordoc 실행·HWPX 패키지 변환 시험은 아니다.
