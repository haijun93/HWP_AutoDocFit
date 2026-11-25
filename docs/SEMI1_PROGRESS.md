# SEMI1 진행 기록

- 2026-11-24: 기본 브랜치에는 SRCH backend 파일·시드 생성기가 없어 독립 옵션 경로를 구현. 파서·문장 AND/문서 교집합·근거 응답·ktm 연결 완료.
- 2026-11-25: SEMI1 검증·완성. 문장 경계에서 AND 누출 수정, Python 3620개 OK(26 skipped), kfind-core 103개·kfind-names 32개·klib 19개+연관 검색 1개·ktm 260개 통과. 합성 seed 20261124의 24개 본문/파일명 조합에서 SQLite/Rust 집합·순서·근거 차이 0. 세부 시험 결과·종료 경고·SRCH6 통합 제안은 `SEMI1_RESULT_2026-11-24.md` 참고.
- 2026-11-25: 추가 검토에서 파일명 확장자 점은 문장 경계가 아니므로 파일명 전체를 단일 검색 단위로 별도 처리. 집중 Python 18개·kfind-core 파서 2개·kfind-names 실제 그룹 검색 1개·klib 연관 검색 1개 통과. 이 보정 후 전체 Python 및 1,000문서 SQLite/Rust parity 재실행 대기.
