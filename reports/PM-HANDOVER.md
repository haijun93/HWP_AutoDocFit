# CILINUX2 인계 (2026-11-26)

`feature/CILINUX2`, 기준 `cd138da5`. 일반 zlib 1.3.1 대조에서 Linux CI와 같은 해시 접두부 및 기존 실패 4건을 재현했다. 환경 간 fixture 비교를 ZIP 전체 압축 해시에서 모든 항목의 원시 바이트·15개 메타데이터 필드 대조로 수정하고 각 실물의 manifest 해시·크기 검증을 유지했다. fixture·manifest 갱신 및 skip 추가 없음. 관련 46시험은 기본 zlib-ng·일반 zlib 모두 OK. MCP pywin32 Windows 마커로 OSV 추출 오류 해소, 기존 스캔 경로 36개 누락 0·전이 해석·pywin32 검사 유지. OSV 기존 영향 항목 8건/취약점 11건과 조치안은 결과 문서에 기록했고 감사 전체 성공은 아니다. Windows 전체 Ran 3751 tests in 929.047s / OK (skipped=29), 종료 코드 0. 기존 atexit SQLite 정리 WinError 32·ResourceWarning은 남는다. 생성 바이너리 4개 원복·COM 잠금 해제·기존 한/글 2개 보존. Linux 수정 후 직접 실행은 미확인. 결과: `docs/CILINUX2_RESULT_2026-11-26.md`. 커밋만 수행하며 푸시·태그·병합·배포 없음.

# UNIFIED1M 인계 (2026-11-26)

`feature/UNIFIED1M`: v2.2.101 `e550ad48` + UNIFIED1 `a56e7c60`을 병합 커밋 `158650bb`로 통합했다. window.rs 충돌에서 SEMI2 칩·묶음 색·근거·입력 계측과 통합 스트리밍·취소·정렬을 모두 보존했다. 두 설정을 켜면 파일명은 원문 질의의 names_related, 위계·본문은 main과 같은 연관 문단 backend를 사용하며 pipe 모드도 같다. 설정 4조합 회귀 포함 ktm 276개 통과·기존 수동 벤치 1개 제외, release 빌드 성공. 전체 Python Ran 3695 tests in 1494.272s / OK (skipped=22), 종료 코드 0. 전체 시험은 20분 내 확인 못 함을 기록한 뒤 최종 완료됐으며 기존 표 썸네일 시험의 defusedxml 파싱 실행을 읽기 전용으로 확인했다. COM 차단 요청 0건·공통 잠금 해제·생성 fixture 4개 원복. 기존 ResourceWarning·sandbox SQLite 정리 PermissionError는 남는다. unified_search와 related_queries 기본 false, 16ms 목표 미달 상태 유지·재측정 없음. 실사용 문서·COM·설치 서비스·실제 UI·맥은 미확인. 결과: `docs/UNIFIED1M_RESULT_2026-11-25.md`. 로컬 커밋만, main 이동·푸시·태그·배포 없음. 카드 target은 PM 정리 대상.

종료 시 공유 github/main 참조는 v2.2.102 `43e59a14`로 4커밋 진행했다. 위 합격은 지정 v2.2.101 기준에 한정하며 v2.2.102 추가 통합 검증을 대신하지 않는다.

# UNIFIED1 인계 (2026-11-25)

`feature/UNIFIED1`, 기준 `8b18b99d`. 기본 false `unified_search` 뒤에 ktm 단일 요청·파일명→위계→본문 스트리밍과 후보 창 안의 최고 단 대표 행을 구현했다. 단별 200후보→중복 제거→20/20/40 출력이다. 합성 1천 문서의 독립 기대값·실제 격리 UI 2질의 일치, 정렬 5종의 단 순서/요약 보존, Rust 전체 271통과·기존 수동 벤치 1제외. 보정한 입력→이름 첫 GDI 19.77/15.84ms로 16ms 미달, 적은 결과의 전체 탐침은 약 6.89ms 증가, 쉴 때 개인 작업 집합 2.71MiB. 기본 활성화/승격 근거 없음. 첫 Python 실행은 정렬 결함 보완을 위해 의도 중단했고 최종 전체는 Ran 3660 tests in 953.856s / OK(skipped=22), 종료 코드 0이다. COM 활성화 0건, 기존 sandbox SQLite 정리 PermissionError 출력, binary fixture 4개 원복. 전역 최고 단·페이지 처리·실키보드 IME·설치 서비스·사용자 문서·COM·타사 비교는 미확인. 상세 `docs/UNIFIED1_RESULT_2026-11-25.md`, 설계·예비/최종 원시 JSON 동반. 중간 커밋 cf2762f8. 커밋만 하며 푸시·태그·병합·배포 금지.

# KORDOCC2 인계 (2026-11-24)

`feature/KORDOCC2`, 기준 `d741c7ad`, 구현 `554456b3`: 안전한 PNG/JPG BinData·대체글·비율/칸 폭, 4~6단 제목 스타일, 고정폭 코드, 링크 표시/강조와 경고를 구현했다. 구조/합성 정답 19/19, kordoc 4.24.9 대비 10/19, 한/글 2020 열기·사본 저장·개행 위치/그림 해시·대체글·크기·코드 보존 19/19(각 1쪽), 추가 PDF 그림 표시 3/3. 단위 72개 중 71 통과·기존 경고 개수 기대 1 실패. 전체 3,620개 실행·3,591 통과·28 제외·같은 실패 1·오류 0, 1,026.512초, 종료 코드 1. 기존 시험 수정 금지에 따라 미지원 경고 4→2 기대 충돌은 그대로 보고한다. 완전 통과/병합 가능으로 간주하지 않는다. 하이퍼링크 필드는 미구현(경고+표시 글자), 앱 미연결·푸시 없음. `docs/KORDOCC2_RESULT_2026-11-24.md`와 근거 JSON 참조. KORDOCD 입력 어댑터·설정 키·한 번만 kordoc 폴백하는 연결 설계 제안 포함.

# KORDOCC1 인계 (2026-11-24)

`feature/KORDOCC1`: Python 내부 Markdown→HWPX 생성기와 합성 Markdown 11종을 구현했다. 구조 11/11, 정답 대비 10/11, 실제 kordoc 4.24.9 대비 5/11이며 그림 미지원·기준본의 제목/목록/줄바꿈 차이로 전면 대체는 보류한다. 한/글 2020 열기·사본 저장·저장 전후 동일 harness 보존 11/11(모두 1쪽), 관련 단위 61개 통과. 전체 3,609개 실행, 조건부 제외 28개, 실패 0·오류 0, 981.618초, 종료 코드 0. 통과 3,581개. `docs/KORDOCC1_RESULT_2026-11-24.md`와 근거 JSON 참조. KORDOCD 공통 블록 입력 어댑터·그림 임베딩 후속 제안. 사용자 문서 사용·앱 연결·푸시 없음.

# HANG1 인계 (2026-11-24)

보고서형 합성 TXT는 이번 PC에서 약 14분 만에 정상 완료했다. 지연의 대부분은 본문 자간·단어 분리 단계(전체 입력 792.1초, 352,420 COM 호출)였고, 앞 3장 절반 입력은 389.6초·176,278회였다. 15분 타임아웃·단계 로그·창/모달 스냅샷을 추가했다. 모달은 관찰되지 않았고, 최소 정지 구조는 재현되지 않았다. 회귀시험 및 미확인 범위는 `docs/HANG1_RESULT_2026-11-24.md`를 확인한다. 현재 작업은 feature/HANG1에서 결과 문서·픽스처·재현기·시험을 로컬 커밋하는 단계이며 푸시하지 않는다.
# SRCH6 인계 (2026-11-24, 로컬 검증 완료·승격 불가)

`feature/SRCH6`, 기준 `3f65771d`, 구현 소스 `988e58c2`(후속 진행 문서는 별도 커밋). Rust JSONL 검색을 페이지 선택 후 복원·세대별 경로/문단/단어 집합 캐시·동의어 무효화·위계 가중 발생 수 순위로 개선했다. 기본 SQLite·공용 자료함 순위 유지. Windows release·관련 Python 26개 OK; 전체 Python 3,605개(20제외) OK·COM 요청 0건. cargo는 시험 환경 panic=unwind로 실행해 169통과·4실패·1제외였고 4실패 모두 기준선에서도 재현됐다(기존 시험 수정 없음). 합성 405문서/18질의 품질 SQLite 동률·1천 4유형 전체 집합 100%, 최종 정식 warm 1천 4.907~13.030ms·5천 8.057~27.594ms, 실제 위계 모드 2/5단어도 30ms 이하. 기본 승격 불가, 실문서·맥 실기 빌드·5천 전체 집합은 미확인. `docs/SRCH6_RESULT_2026-11-24.md`와 맥 설계를 본다. 생성 fixture 4개 원복, 기존 SQLite 정리 경고 기록. 이 카드는 로컬 커밋만 하며 푸시·태그·COM·사용자 원본 사용을 하지 않았다.

# OUTLINERQA1 인계 (2026-11-23)

`feature/OUTLINERQA1`에서 Logseq 공개 기능 기준과 Flutter·Win32·엔진 차이를 문서화했다(`docs/OUTLINERQA1_RESULT_2026-11-22.md`). Flutter Tab/Shift+Tab이 입력 문자를 저장하지 않던 결함을 먼저 실패 시험으로 재현하고 `_setTextAndEdit` 경로로 수정했다. Core 70+CLI 1, Win32 124, Flutter 위젯 6, Python 서비스 9 통과; `flutter analyze` 지적 0. 실제 한글 IME, 설치본의 `outliner_win.exe`, 다중 선택·드래그·undo/redo·블록 참조 UI·Flutter 전역 검색/역링크는 남았다. Logseq Windows 기본 블록 이동은 Alt+Shift+위/아래이고 현재 Win32 Alt+위/아래와 달라 실기 UX 판정이 필요하다. 푸시·태그·한/글 COM·실행 앱 접근 없음.

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

# T4 1단계 인계 (2026-11-13)
작업 브랜치 feature/kfind-service, 기준 f2355aca. 새 실행 크레이트 crates/kfind가 기존 kfind/core를 재사용한다.
SCM 명령·SID별 검색 파이프·대체 방식 저장·공개 색인 제거를 구현했다. 실제 SCM 등록과 예약 작업 생성은 시험하지 않는다.
구현 범위·프로토콜·시험 결과·후속 작업은 docs/T4_KFIND_SERVICE_2026-11-13.md에 기록한다.
이번 지시는 로컬 커밋만 허용한다. 푸시·태그·GitLab 접근 금지.
