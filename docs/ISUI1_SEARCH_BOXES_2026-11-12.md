# ISUI1 검색 입력란 전수 목록 (2026-11-12)

사용자 지시(2026-11-12): 앱·타임머신·가상 폴더·업무자료함의 모든 검색은 Enter 없이 2글자 이상이면 실시간으로 찾는다. 메뉴를 공유하는 검색창은 키워드를 유지한 채 바뀐 메뉴에서 자동으로 다시 찾는다.

## 범위(숫자)
- 대상: `flutter_ui/lib` 아래 `.dart` 101개. `TextField`·`TextFormField`·`SearchBar`·`SearchAnchor`를 찾았다(`SearchBar`·`SearchAnchor`는 0건).
- 입력란 생성자(`TextField(`·`TextFormField(`) 58곳(26개 파일) 가운데 **검색·찾기 입력란 12곳**. 나머지 46곳은 이름 바꾸기·설정값·메모·암호·서식 이름 같은 값 입력이라 뺐다.
- 줄 번호는 이 브랜치(`feature/instant-search-ui`) ISUI1 커밋 기준이다.

## 검색 입력란 표

| # | 화면 | 파일:줄 | 지금 방식 | 부르는 백엔드 명령 | 메뉴 공유 대상 | 다음 카드 |
|---|---|---|---|---|---|---|
| 1 | 타임머신 상단 검색창 | `features/timemachine/timemachine_screen.dart:1619` (`tm-file-search`) | **ISUI1 적용**: 2글자부터 실시간(150ms), Enter 즉시(1글자도), 탭 전환 시 자동 재검색 | 파일명: `timemachine/range`(query)·열 보기면 `file_search` / 본문: `content_search` / 위계: `related_search` (앞에 `content_index_status`) | 파일명 ↔ 본문검색 ↔ 위계검색 | 완료(ISUI1) |
| 2 | 타임머신 미리보기 안 찾기 | `features/timemachine/timemachine_screen.dart:1195` (`tm-preview-find`) | 실시간(입력마다, 화면 안 강조만) | 없음(받아 둔 미리보기 글에서 찾음) | 없음 | 바꿀 것 없음(백엔드 호출 없음, 1글자 강조가 쓸모 있음) |
| 3 | 타임머신 연관 자료 추가 대화상자 | `features/timemachine/related_documents_dialog.dart:323` (`tm-related-include-filter`) | 실시간(입력마다, 목록 거르기) | 없음(받아 둔 목록 거르기) | 없음 | 바꿀 것 없음 |
| 4 | 업무자료함 본문검색 | `features/notebook/notebook_screen.dart:898` (`nb-query`) | Enter·「찾기」 단추 | `notebook` → `nb_search` | 본문검색 ↔ 출처비교(이미 같은 `_queryController`를 씀, 전환 시 자동 검색은 없음) | ISUI2(업무자료함) |
| 5 | 업무자료함 출처비교 | `features/notebook/notebook_screen.dart:972` (`nb-compare-query`) | Enter·「비교」 단추 | `notebook` → `nb_compare` | 위와 같음 | ISUI2(업무자료함) |
| 6 | 가상 폴더(자료함) 자료 검색 | `features/library/library_screen.dart:1498` (`librarySearch`) | 실시간(입력마다, **지연 없음**, 늦은 응답 거름은 상자 id만) | `library` → `item_search` 2번(`searchContent`·`listItems`) | (추가 예정) 파일명 ↔ 본문검색 ↔ 위계검색 | ISUI3(가상 폴더) — 본문·위계검색 추가 |
| 7 | 표 단위 목록 검색 | `features/table_units/table_units_screen.dart:1205` (`searchTextField`) | 실시간(입력마다, 화면 안 거르기) | 없음(받아 둔 목록 거르기) | 없음 | 2글자 기준 적용 여부만 결정(백엔드 부담 없음) |
| 8 | 메모 편집기 찾기 | `features/editor/memo_editor.dart:717` (`memo-find-query`) | Enter(다음 찾기)·단추 | 없음(편집기 안 찾기) | 찾기 ↔ 바꾸기(같은 줄) | 바꾸지 않기 권장: 문서 안 "다음 찾기"는 Enter가 표준 동작 |
| 9 | 몽땅클립보드 기록 검색 | `features/kclip/kclip_dialog.dart:863` | 실시간(200ms 지연) | `kclip` → `list`(query) (앞에 `status`) | 분류 단추(분류는 바꿔도 검색어 유지) | 2글자 기준·늦은 응답 거름 적용 |
| 10 | 준말·본말 관리 검색 | `features/abbreviations/abbreviation_manager.dart:150` (`abbreviationSearchField`) | 실시간(입력마다, 화면 안 거르기) | 없음 | 없음 | 2글자 기준 적용 여부만 결정 |
| 11 | 상용구 찾기 | `features/settings/idiom_panel.dart:353` | 실시간(입력마다, 화면 안 거르기) | 없음 | 없음 | 2글자 기준 적용 여부만 결정 |
| 12 | 설정 검색 | `features/settings/settings_dialog.dart:552` | 실시간(입력마다, 화면 안 거르기) | 없음 | 없음 | 2글자 기준 적용 여부만 결정 |

### 검색이 아니라서 뺀 입력란(대표)
이름 바꾸기 대화상자(`library_screen.dart:586·887·910`, `file_rename_screen.dart:424·470`), 새 상자·자료 추가(`library_screen.dart:1388·1504·2260`), 설정값(`settings_dialog.dart:243`), 노트 제목·태그·본문(`notebook_screen.dart:1281·1287·1297`), 암호(`hwp_bulk_panel.dart:193`), 서식 이름(`table_sketch_screen.dart:3813`), 예시 경로(`table_template_panel.dart:78`), 편집기 본문(`memo_editor.dart:885·914`, `large_text_view.dart:341`) 등.

## 판단 메모
- 백엔드를 부르는 검색은 1·4·5·6·9 다섯 곳이다. 이 다섯 곳만 공용 컨트롤러(`lib/core/instant_search.dart`)가 꼭 필요하다(지연·요청 번호·진행 중 1건 제한).
- 7·10·11·12는 받아 둔 목록을 거르는 것이라 이미 실시간이다. "2글자 미만이면 결과 비움"을 그대로 적용하면 1글자로 거르던 동작이 사라지므로(목록이 전체로 돌아감) 사용자 결정이 필요하다.
- 2·3·8은 문서·목록 안 찾기라 지시 대상에서 빼는 것을 권한다(8은 Enter가 "다음 찾기").
- 6(가상 폴더)은 지금 지연 없이 글자마다 `item_search`를 2번 부르고, 늦게 온 응답을 상자 id로만 거른다. 빨리 치면 옛 검색어 결과가 새 결과를 덮을 수 있다(ISUI3에서 컨트롤러로 바꾸면 해결).

## 백엔드 취소 조사(카드 4번)
- 데스크톱 IPC(`docfit_app/ipc_server.py`)는 연결 하나에서 요청을 한 줄씩 차례로 읽어 처리하고, 처리 전체를 `self.server.app.lock`으로 감싼다. Flutter `NativeClient`는 연결 하나만 쓴다. 따라서 **보낸 요청을 서버에서 취소할 수 없고, 겹쳐 보내면 뒤 요청이 앞 요청 뒤에 줄을 선다.** 취소용 프로토콜 메시지도 없다.
- `timemachine_service.content_search`는 `ContentSearchIndex.keyword_search`(SQLite)를 바로 부른다. SQLite는 `Connection.interrupt()`로 끊을 수는 있으나, 위 잠금 때문에 취소 요청 자체가 앞 요청이 끝난 뒤에야 처리되므로 지금 구조에서는 쓸 수 없다.
- 그래서 ISUI1은 화면 쪽에서만 처리했다: (1) 150ms 지연으로 연속 입력을 한 번으로 합침, (2) **동시에 1건만 보내고 진행 중 들어온 입력은 마지막 1건만 대기**(중간 검색어는 보내지 않음), (3) 요청 번호가 다른 늦은 응답은 버림. 서버 쪽 취소는 IPC를 연결 여러 개(또는 요청별 스레드)로 바꾸고 전역 잠금을 검색 명령에서 풀어야 가능하며, 이는 별도 설계 카드가 필요하다.

## 확인 필요(이번 카드에서 고치지 않음)
- (ISUI2에서 해결, 아래 "ISUI2 결과" 참고) 타임머신 위계검색은 `related_search`에 `{query, limit, sort}`를 보내지만, `timemachine_service.py`의 `related_search`는 `path`(기준 문서)를 요구하고 없으면 `INVALID_INPUT`을 돌려준다. 시험의 가짜 백엔드는 query만으로 결과를 주므로 위젯 시험은 통과한다. 실제 앱에서 위계검색이 검색어로 결과를 내는지 확인이 필요하다(ISUI1 이전부터 있던 계약 차이).

## ISUI2 결과 (2026-11-12, 브랜치 `feature/instant-search-ui`)
위 표의 4·5·6번과 "확인 필요"의 위계검색 계약 차이를 처리했다(표 줄 번호는 ISUI1 기준 그대로 둔다).

- **4·5 업무자료함 본문검색 ↔ 출처 비교**: 공용 `InstantSearchController`(탭 `search`·`compare`, 기존 `_queryController` 공유). 2글자부터 150ms 뒤 자동 검색, Enter·「본문검색」「비교」 단추는 즉시(1글자도). 가운데 칸 메뉴를 본문검색↔출처 비교로 바꾸면 같은 검색어로 곧바로 다시 찾는다(미리보기·변환은 검색하지 않음). 2글자 미만이 되면 그 메뉴 결과를 비운다(출처 비교는 Markdown·HWPX 내보내기에 붙는 비교 검색어도 함께 비움). 검색 중에는 상단 진행 막대만 보이고 다른 단추는 막지 않는다. 검색 중 업무자료함을 바꾸면 늦은 응답은 버린다.
- **6 가상 폴더 검색창**: 같은 컨트롤러(탭 `file`·`content`·`related`). 검색창 옆에 「파일명·본문검색·위계검색」 메뉴(`librarySearchMode`)를 붙였고 세 메뉴가 검색어를 공유하며 바꾸면 곧바로 다시 찾는다. 파일명은 기존 `item_search` 그대로(단, 이제 2글자부터·150ms 지연·늦은 응답 거름). 본문·위계는 새 명령 없이 타임머신 `content_search`·`related_search`(검색어만)를 부르고 결과 경로를 이 가상 폴더 자료의 원본 경로(`source`)·복사본 경로(`managedPath`)와 맞춰(대소문자·구분자 무시) 맞는 자료만 남긴다. 구현: `features/library/library_document_search.dart`.
- **위계검색 계약**: `timemachine_service.py`의 `related_search`가 `path` 없이 `query`만 오면 `ContentSearchIndex.search`(위계 순위)로 찾아 본문검색과 같은 형태(`path`·`file_name` 포함, `mode: "query"`)로 돌려준다. `path`가 있으면 예전 연관 문서 방식 그대로(검색어가 함께 와도). 조사 폴더가 없으면 빈 결과. `limit` 1~200, `sort`는 `hierarchy_relevance`·`mentions`·`relevance`만.
- 7·10·11·12(목록 거르기)와 2·3·8(문서 안 찾기)은 바꾸지 않았다.

### 남은 문제
- 가상 폴더 본문·위계검색은 **타임머신 조사 폴더 안에서 본문 색인된 문서만** 찾는다. 조사 폴더 밖 파일이나 가상 폴더의 글(메모) 자료는 나오지 않는다(빈 결과일 때 화면에 안내). 가상 폴더 자체 색인(klib `search`)과 합칠지는 결정이 필요하다.
- 가상 폴더 파일명 검색은 예전에는 1글자도 실시간으로 걸렀으나 이제 2글자부터(1글자는 Enter). 사용자 지시(2글자 기준)를 따른 변경이다.
- 실제 앱(한/글·IPC 포함)에서 손으로 확인하지 않았다. 위젯 시험과 서비스 시험만 통과.
