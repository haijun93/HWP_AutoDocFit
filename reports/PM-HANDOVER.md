# APP1-G·I·J·H 인계 (2026-11-14)

`feature/kfind-app`, `2335d18c` 이후 Gemini 미커밋 변경을 검토·보완해 채택. github/main은 이미 병합되어 있었으며 fetch 후 추가 커밋 0건. 상자/NBIDX1 kfind 동기화와 legacy 대체, 공통 search_engine 설정·상태 표시, 실제 시험 파이프 동등성 도구, 서버 상자 범위 검색·count 배지·자료 ID로 메모 열기 구현. Python 관련 104개 및 상자 삭제 보강 55개 통과. Flutter 관련 37개, 같은 이름 메모 보강 4개, ISUI/타임머신/업무자료함 연결 109개 통과, 최신 analyze 0건. 합성 문서 2개·86질의 실제 서비스 비교 A 차이 0, 두 번 독립 실행 JSON 바이트 동일. 비교 B 미설명 15건·Tantivy·실사용 지연·실문서군·되돌리기 관문은 APP1-K 후속. E:도 exFAT라 전체 시험은 NTFS C: 작업 트리와 TEMP에서 실행(잠금 2개). 전체 시작 시점 3,095개(1,188.755초): 기존 조회 속도 기준 실패 1·제외 13, 같은 실패 1개 단독 재확인 통과. 최신 보강분 NTFS 관련 106개 통과. 최종 discovery는 3,101개이며 최신 전체 재실행은 하지 않았다. 생성 fixture 4개 복원·잠금 2개 해제 확인. `reports/APP1_GIJH_2026-11-14.md`, `reports/APP1-equiv-2026-11-14.md`. main/GitLab/태그/릴리스 변경 금지 유지.

# APP1-D·E·F 인계 (2026-11-14)

`feature/kfind-app`, `8c0eaefd` 위에 `github/main` 먼저 병합(`553ff982`, 충돌 없음). 결과 4종 변환·문장/조각 강조, IPC 잠금 포함 가짜 서버 지연 1회(count p95 4.2449ms/query 4.1488ms/별도 연결 취소 3.9917ms), 기본 kfind·오류 시 legacy와 사유 표시·seeding phase 및 변경 훅 구현. 관련 254개 통과 뒤 보강한 최종 관련 41개 통과. 전체 3,057개는 E: exFAT TEMP 탓에 실패 3·오류 19·제외 15, 같은 실패 22개 NTFS TEMP에서 전부 통과(단일 전체 통과는 아님). 생성 fixture 4개 원복, 전체/COM 잠금 해제. 실제 Flutter/서비스 속도와 동등성 정밀 대조는 미측정(APP1-J/K 후속). ktm.exe는 isearch 읽기 전용 범위라 수정하지 않음. 보고서 `reports/APP1_DEF_2026-11-14.md`. GitHub 기능 브랜치에만 푸시, main/GitLab 금지 유지.

# APP1-A·C 인계 (2026-11-13)

`feature/kfind-app`: APP1-A `4f3b7ee1`, APP1-C `8e9b9a63` 구현. 기존 시험 2,941개 유지·새 시험 6개 추가. 지정 명령의 최종 전체 2,947개 OK(제외 10), 실패·오류 0, 1,830.883초. 래퍼 실행의 문서 진단 1건 실패와 재검증 이력은 `reports/APP1_AC_2026-11-13.md` 참조. 생성 fixture 4개 복원, GitHub `feature/kfind-app`에만 푸시. main 병합·푸시·GitLab 접근 금지 유지. 후속 APP1-B·D 이후 연결 작업은 이 카드 범위 밖이다.
# FIX13 인계 (2026-11-13)

`fix/todo-1113`에서 타임머신 open/reveal에 path 요청을 지원했다. ID가 있으면 기존 색인 항목을 우선하며 잘못된 ID를 path로 우회하지 않는다. 활성 조사 폴더·실제 연결 경로·지원 형식·제외 이름/폴더·파일 존재를 검사하고, 실패 이유를 화면에 표시한다. 파일명 열 보기의 단일 클릭은 미리보기, 더블클릭과 미리보기 문서 열기는 공통 명령 경로를 사용한다. 본문/위계검색도 ID 유무에 따라 같은 열기 경로를 사용한다.

문서 선택 화면의 표 칸 그림 배경 버튼·전용 처리·안내를 제거했다. 파일변환 화면의 동일 기능과 시험은 유지했다. TODO에서 해결한 두 절·HLAT1·ATTP1을 제거하고 사무실 클립보드는 수정 완료, 다음 릴리스 후 실기 확인 한 줄로 줄였다. 접힘 제거 `0f6a6896` 포함 확인. Rust 132통과·1제외, Python 타임머신 79통과 및 마지막 오류 이유 보강 후 path 시험 3통과, Flutter 집중 81통과·analyze 0건. Flutter/Python 전체 최종 결과는 검증 후 아래에 기록한다.

검증 환경 문제: E: 공간 부족으로 첫 Rust 빌드·Flutter 컴파일 실패. C:/HWP_AUTODOCFIT/fix13-* 아래 프로세스별 빌드/임시 경로로 분리했다(시험 코드에 PC 경로 추가 없음). 첫 Flutter 전체는 실행 중 소스 수정과 부하로 실패했고 최신 소스 동결 후 재검증했다. Python 전체는 다른 세션 APP1의 COM 잠금 해제를 기다린 후 실행한다. 사용자 문서 원본 수정 없음, main/GitLab 변경 금지, github/fix/todo-1113만 푸시.

# ATTP2 인계 (2026-11-13)

`fix/attachment-page-detect`에서 main `77557e7a`와 ATTP1 `f5f1499a`의 all 경로를 COM 잠금·사본으로 직접 비교했다. 앱 소스 수정 없음. 시연 전체 XML 문단 49→49·4→4쪽, 별도 COM 생성 문단 붙임 15→15·2→2쪽, 양쪽 header.xml·section0.xml 바이트 동일. 본문 직속 문단 각각 44·15개의 COM 위치·글 불일치 0, 붙임 XML 13번=COM (0,13,0). 보고서 불가능 분기 실제 실행, 붙임 전체 44.366→30.181초(유효 각 1회), 쪽 맞춤 13.01→0.43초. 첫 브랜치 붙임 시도는 측정 도구 인자 오류로 무효 처리하고 도구 수정 후 유효 실행했다.

저장소 attp_text/table/shape 3종은 양쪽 전부 한/글 열기 실패(6/6, 저장 0). 표·도형 붙임과 마지막 쪽 당김 불가능 분기 실기는 미완료다. 합성 XML 시험으로 실기 완료를 대신하지 않는다. Python 전체 2,941개 OK(제외 10), 실패·오류 0, 1,036.334초. 재생성 자료 4개 원복. 상세 `docs/ATTP2_COM_REGRESSION_2026-11-13.md`, 저장 증거 `reports/ATTP2`, 외부 산출물 `E:/docfit-tmp/attp2-results`. main·GitLab 변경 금지, GitHub 지정 브랜치만 푸시.

# SPD2 인계 (2026-11-13)

`perf/spacing-com-design`에서 `github/main` 병합(6810f631, 충돌 없음) 후 줄범위 내부의 동일한 두 경계 위치 읽기 기억을 확대했다. 기본 꺼짐, 편집 후 무효화 유지. 설계 3단계 추가 구현 없음.

시연 원본 사본 all·COM 계측 각각 1회: 꺼짐/켜짐 전체 144,274→128,702건, 본문 자간 1차 67,086→55,752건. 기존 1단계 재사용을 포함한 효과이며 이번 확장만의 추가 절감량은 분리 측정하지 않았다. 구조 문단 49→49·변경 0, header.xml·section0.xml 해시 동일. 원본·사본 불변, 오류 0. 전체 Python 2,923건·OK(제외 11), fixture 재생성 4개 복원. 보고서 `docs/SPD2_SPACING_MEMO_2026-11-13.md`, 산출물 `E:/docfit-tmp/perf2/spd2-*`. GitHub 지정 브랜치만 푸시, main 푸시·GitLab 접근 금지 유지.

# VF V9 인계 (2026-11-12)

전체 Python은 첫 진단 실행의 python314.dll 접근 위반과 최종 재실행의 기존 test_table_units 클래스 준비 지연으로 집계 전 종료/중단되어 미확인이다. V9 합격 판정에는 이 미확인과 CI/R17 실기 조건을 남겨야 한다. 관련 시험 85→98 통과와 전체 회귀를 혼동하지 말 것.

feature/vfolder-popup에서 CI kvf 빌드/꾸러미 해시 검사, Python 수명 명령 연결, Flutter 설정의 독립 켜기·끄기/상태/자동 실행을 구현했다. kvf 141/141, kagent 132 통과·1 제외 유지. Python 관련 기존 85/85·변경 후 신규 13개 포함 98/98, Flutter 신규 2/2·정적 분석 지적 0. 실제 Run 등록 검증 후 원복·kagent Run 불변 확인. kvf.exe 실행·푸시·태그·GitLab 접근 0회. V2~V4 기록 보존. CI 실실행·로그인 실기·전체 Flutter 회귀는 미확인. 상세는 docs/VF1_PLAN_2026-11-12.md의 V9 결과를 본다.

# VF V5~V7 인계 (2026-11-12)

feature/vfolder-popup에서 단축키·트레이·GDI 창·읽기/찾기·사본 열기/위치 열기를 구현했다. V5 9afd4352, V6 88e7a75f, V7은 이 기록을 포함한 로컬 커밋이다. kvf 최종 133/133(기본 병렬·직렬 모두), kagent 본체 109 통과·1 제외/분류 16/회귀 7로 기준선 유지. kvf.exe 446,464바이트. kagent 코드 변경 0, kvf.exe 실행 0회, 푸시·태그·원격 접근 없음.

상세 결과·격리 시험 범위·V6 시간 시험 1회 실패 및 최종 통과·미확인은 docs/VF1_PLAN_2026-11-12.md의 V5~V7 결과 절을 본다. 실제 ShellExecute/IME/풍선/외관·자원 측정은 미확인. V8~V10은 미구현. 기존 V3 DB 잠금 오류의 '?' 문구는 이번 범위 밖이라 남았다.

# SM2 인계 (2026-11-12)

feature/hs2-hsa에서 3단계 구현 완료. 종료 수명 관리 5d061082, 상태 통합 50493ff6, 화면 연결은 이 인계 문서를 포함한 커밋이다. 지정 Python 155개(서비스 71·본문검색 78·제목 재색인 6), Flutter 타임머신 화면 53개 통과. flutter analyze 지적 0건. 기존 시험 본문·명령·응답 필드·화면 문구 유지, 상태 필드와 시험 추가. 푸시·태그·원격 접근 없음.

상세 변경·실패 시도·실행 시간·미확인 범위는 docs/SM2_INDEX_STATE_IMPLEMENTATION_2026-11-12.md 참조. 실제 앱/EXE·COM·전체 시험은 미확인. 다음 단계인 main 병합·게시는 총괄 범위다.

# APP1-B 인계 (2026-11-14)

feature/kfind-app에서 github/main 최신 병합 확인, kapp 보존 stash 적용·원본 보존. 연결 상태 6개·36조합, KFS1 프레임·64KB 분할 ingest·취소·시간 한도 구현. APP1-A 메타데이터 보완, APP1-C 조건 충족 확인. 관련 25개 통과(1.457초), 이후 긴 문단 분할의 반복 직렬화 축소는 최종 전체 시험에서 확인 필요. 다른 comx 측정의 COM 잠금과 별도 전체 시험 종료 대기로 전체 시험·푸시 미완료. 대기 래퍼 G:/docfit-tmp/kapp-app1b-run-full.py, 로그 예정 G:/docfit-tmp/kapp-app1b-full.log. 상세 reports/APP1_B_2026-11-14.md. 후속 APP1-F/I의 기본 engine=kfind·오류 때 legacy 대체 결정 유지.

# RHIER1 인계 (2026-11-13)

fix/rust-hierarchy-parity(기준 3a69b3e4)에서 Python 검색 위계 규칙 판 5를 Rust 공용 분류기로 옮겼다. role_sequence와 검색 위계를 구분하는 search_levels 계약, 구형 정수 계약/실패 폴백, 제목 표·Markdown·번호·라벨·개요·기호 단계와 klib 연결을 구현했다. 기본 hierarchy_engine=False 유지.

합성 82문서·229문단: 기준 CLI 응답 누락 포함 차이 229→0, CLI·DLL·klib 687판정 차이 0. hierarchy 56 통과, klib 175 통과·기존 제외 2. Python 고유 관련 시험 58개 중 53 통과·제외 5(선택 실행 56개와 최종 계약/네이티브 7개는 5개 중복). 전체 Python·COM·실제 문서 평가·설치/배포는 미확인. 보고서 docs/RHIER1_RUST_PARITY_2026-11-13.md, 원시 결과 reports/RHIER1-parity.json. 검증 바이너리는 E:/docfit-tmp/rhier1-verified-binaries에 보존. E: 공간 부족과 공용 Cargo target의 의존성 타입 충돌을 겪었으므로 debug=0·incremental=0, crate별 target 분리로 빌드한다. main 병합·푸시·GitLab 접근은 하지 않는다.
