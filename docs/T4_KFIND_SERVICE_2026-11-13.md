# T4 색인 서비스 1단계 구현·검증 기록 (2026-11-13)

기준: feature/kfind-service, KB4 f2355aca. 사용자 지정 범위만 구현한다.
실제 코어 위치는 kfind/core이며 이동하지 않고 crates/kfind에서 경로 의존성으로 쓴다.

## 구현 경계
- 실행 파일 kfind.exe. SCM 등록 이름 HwpAutoDocFit.Index, 표시 이름 HwpAutoDocFit 색인 서비스.
- --install / --start / --stop / --uninstall. SYSTEM 독립 프로세스, 지연 자동 시작.
- 서비스 수명: StartPending → 파이프 바인딩 성공 → Running → 중지 요청 → Stopped.
  바인딩 실패 시 Running으로 가지 않는다. 중지 확인 실패 시 공개 색인을 지우지 않는다.
- --user --index-root DIR은 SCM 등록 없이 같은 파이프/검색 코드를 실행한다.
- 파이프는 \\.\pipe\HwpAutoDocFit.Index. 보호 DACL은 SYSTEM·대화형 사용자만 허용하고 원격 연결은 거부한다.
- 접속 프로세스 PID → 프로세스 토큰 SID 확인. 요청에서 SID를 받지 않는다.
  DIR/<SID>의 kfind-core 색인만 연다. SID별 인덱스와 KB4 캐시를 유지하고 검색 전에 reload한다.
- 질의 상태: 활성 등록 → 검색 → 완료/오류 시 등록 해제. cancel은 같은 SID·id에만 작용한다.
  검색 2초, 연결 작업 10초, 연결 32개, 활성 질의·사용자 색인 각 64개 상한.
- 설치가 권한/정책 오류(5·1314·1260)로 막힌 경우만 예약 작업/사용자 권한을 질문한다.
  %LOCALAPPDATA%/HwpAutoDocFit/kfind/settings.json의 fallback에 task 또는 user 저장.
  이미 선택했으면 다시 묻지 않는다. 깨진 설정은 덮어쓰지 않고 오류를 낸다.
- --uninstall은 SCM 중지·제거 성공 뒤 고정 data-root/index만 삭제한다.
  하위 재분석 지점이 있으면 삭제를 거부한다. 사용자 원본 문서는 열거나 고치지 않는다.

## 파이프 계약
--user의 --pipe-name NAME은 격리 시험용 파이프 이름을 지정한다. 생략하면 확정된 기본 이름을 쓴다.

한 연결에 요청 1개, 응답 1개. 양쪽 모두 u32 little endian 바이트 길이 뒤 UTF-8 JSON.
요청 본문 65,536바이트까지 허용하고, 길이 초과는 본문 할당 전에 연결을 닫는다.

```json
{"command":"query","id":1,"text":"예산 집행","top":20}
{"command":"cancel","id":1}
{"command":"status"}
```

query의 top 기본 20, 허용 1..1000. 알 수 없는 필드·명령·잘못된 UTF-8/JSON은 거부한다.
정상 해석 후 응답은 {"ok":true,"result":...} 또는 {"ok":false,"error":...}.
전송/해석 오류는 연결 종료로 표시한다. cancel은 별도 연결에서 보내야 한다.
검색 결과는 코어 문서 ID·위계·일치 문장 위치를 돌려준다. 이름 색인 전용 검색은 이 단계에 포함하지 않는다.

## 검증
실제 SCM/예약 작업 등록·시작·제거는 시험에서 실행하지 않는다.
가짜 SCM, 임시 합성 색인, 현재 사용자 권한의 실제 named pipe로 검증한다.
시험 코드에는 이 PC의 계정명·절대 경로를 넣지 않는다.
단위 시험 17개(가짜 SCM·설정·제거 9개, 파이프·검색 8개)와 실행 파일 --user 통합 시험 1개가 통과했다.
초기 실행은 14개 중 13개 통과: serde의 단위 enum 변형에서 status 추가 필드가 무시되는 문제를 발견했다.
status를 빈 구조체 변형으로 바꾸어 알 수 없는 필드를 거부했고, 기존 시험을 낮추지 않고 통과시켰다.
GNU 도구 체인의 rustfmt 미설치로 첫 포맷 명령은 실패했다. 설치된 MSVC 도구 체인의 cargo fmt로 정리·검사했다.
SCM 수명·제거 커밋 0da18c85, 검색 파이프 커밋 19999a16. 나머지 실행 파일 연결·검증 기록은 다음 커밋에 포함한다.
Python 전체 시험·기존 코어 전체 시험은 실행하지 않았다. 기존 코드 변경은 없으며 새 크레이트의 합성 색인 통합 시험으로 연결을 검증했다.

## 후속 단계와 미확인
사용자별 게시기·NTFS ACL 게시·MFT/USN·작업 프로세스·업데이트/되돌리기는 후속 T4~T6 범위다.
실제 SCM 수명·조직 정책 차단·예약 작업 등록/로그온 실행과 다른 사용자 로그온은 VM 검증이 필요하다.
예약 작업 대안은 현재 사용자 LIMITED ONLOGON으로 등록하며 전체 드라이브 관리자 색인을 제공하지 않는다.
현재 --uninstall은 서비스 공개 색인만 제거한다. 대체 모드 예약 작업 제거·로컬 색인 정리는 후속 설치 확장 범위다.

최종 잠금 파일 기준 명령: cargo test --locked --manifest-path crates/kfind/Cargo.toml -j 1.
결과: 17개 단위 시험 통과(0.64초), 실행 파일 사용자 모드 통합 시험 1개 통과(4.62초), 실패·무시 0개. 빌드 포함 2분 27초.
포맷 검사와 git diff --check 통과. 실제 SCM 등록·예약 작업 생성·COM 호출·사용자 문서 변경·푸시·태그·GitLab 접근은 수행하지 않았다.
