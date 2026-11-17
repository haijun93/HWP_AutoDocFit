# Rust PDF 코어 안전성 수정

[Codex 워커 · gpt-6.1-sol · medium] 페이지 편집 코어 안정성 개선

- 작업 브랜치: feature/pdf-core-safety-20261117
- 작업 폴더: E:/docfit-tmp/pdf-core-safety
- 기존 시험 2건 보존, 신규 안전성 시험 6건 추가
- 검증: MinGW bin 경로를 PATH에 추가한 Windows GNU 환경에서 cargo test 성공(8건 통과, 실패 0)
- 원본/기존 파일 덮어쓰기를 거부하고 임시 PDF 쪽 수 검증 후 persist_noclobber로 게시
- 중첩 페이지 트리의 Resources/MediaBox/CropBox/Rotate 상속값과 Parent 연결 보존
- 잘못된 삭제 번호, 빈 선택, 중복 회전, 역순/초과 분할 범위를 거부
- 분할 전 전체 범위·대상 경로 검증, 분할 시 원본 PDF는 1회만 읽음
- 암호화/전자서명 PDF는 보호 유지가 구현될 때까지 편집 거부

아직 확인하지 않은 범위: 실제 업무 PDF 렌더링 비교, 링크·책갈피·양식의 삭제 페이지 참조 보정, 합치기, UI/API 연결, PDFium 렌더러 연결.
분할은 작업 도중 디스크 오류 시 앞서 생성된 사본이 남을 수 있으며 전체 배치 트랜잭션은 제공하지 않는다.
삭제는 문서 편집 기능이며 민감 정보 완전 삭제(redaction) 기능으로 사용하면 안 된다.
