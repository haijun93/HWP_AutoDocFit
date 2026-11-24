# SRCH6 진행 기록

- 2026-11-24 21:10 KST: 기준을 3f65771d로 맞춤. 결과물은 SRCH6_RESULT·MAC_FEASIBILITY 및 합성 측정 JSON. 합격 기준은 warm 1천/5천 중앙값 30ms 이하, 12개 이상 질의 품질 비교·전체 집합 일치 확인, SQLite 계약 보존, Python 전체·cargo 시험. 푸시·COM·사용자 원본 금지. 검색 상태는 reader 재사용 → 후보 검색 → 문서 집계 → 범위/페이지 선택 → 위치 복원 → JSONL → Python 변환이며, 쓰기 커밋 뒤 reader 세대가 바뀌면 캐시를 재구성한다.
- 2026-11-24 21:22 KST: 중간 커밋 준비. opt-in 검색만 페이지 선택 후 복원·reader 세대별 경로/문단 캐시·위계 가중 term_freq 순위를 적용. Windows release 성공(첫 trait import 오류 수정), 관련 Python 24개 OK. 프로파일 warm 1천 4.54~13.69ms·5천 8.11~24.78ms, 405문서/18질의의 Recall@10 0.875·MRR/순서 1.00으로 SQLite와 같고 전체 집합 18/18 일치. 1천 집합 대조·정식 벤치 실행 중; cargo·전체 Python은 대기. 기본 SQLite·공용 자료함 순위 유지, 푸시 없음.
