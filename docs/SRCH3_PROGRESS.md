2026-11-24: 요청 기준 커밋 2cb65faf로 feature/SRCH3를 맞추고 원래 HEAD는 backup/SRCH3-before-card-bf624ca5에 보존; JSONL klib 연결·모드 라우터 구현, Rust release 빌드 진행 중.
2026-11-24: Rust release 빌드 성공; 전체 unittest 실행 중(중간 실패 2건 표시, 종료 후 원인 확인 예정), 성능·결과 비교는 대기.
2026-11-24: backend 7건 및 전체 3,590건 시험 통과(28 skip); Rust 1천 문서 벤치 완료, SRCH2 동일 질의의 top-20 JSON 비교 저장 작업 중.
2026-11-24: 동일 질의 top-20 비교와 전체 누락·추가 JSON 저장 완료; 초기 색인은 SQLite보다 5.1배 느리고 결과 일치율 0~10%라 기본 모드 유지 결론.
