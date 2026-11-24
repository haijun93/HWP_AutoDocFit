# KORDOCC2 조사·계획

- 상태: 입력 해석 → 안전한 그림 읽기(실패는 경고+원문) → XML/ZIP 조립 → 구조 검증 → 새 파일 게시. 검증 실패 후 게시, 기존 파일 덮어쓰기 전이는 금지한다.
- RSXML0의 ZIP 멤버·바이너리 해시·텍스트/tail 보존 원칙, RSXML4의 서식 ID/개수 및 본문/칸 폭 규칙을 따른다. RSXML 문서에는 BinData 등록 명세가 없어 저장소 HWPX 표 서식모음의 실제 XML을 함께 조사했다.
- 조사 표본: `resources/서식예시/표 서식모음.hwpx`. `hc:img/@binaryItemIDRef=image1`은 content.hpf의 `opf:item id=image1 href=BinData/image1.jpg media-type=image/jpg isEmbeded=1`을 가리킨다. header.xml에는 별도 binaryData 목록이 없다. OPF namespace는 끝 `/` 포함, href는 ZIP 루트 기준이다. 이 구조를 사용하고 합성본 COM 재저장으로 확인한다.
- PNG/JPEG는 기존 Pillow로 검증하고 바이트 그대로 포함한다. 이미지 디렉터리를 명시적으로 받아 상대 경로만 허용한다. path_utils.path_is_within과 resolve를 사용해 상위 이탈·링크 이탈을 막고 파일/픽셀/전체 자원 상한을 둔다.
- 제목 4~6은 독립 스타일, 코드는 고정폭 글꼴+왼쪽 정렬 문단. 링크는 표시 글자 보존과 경고를 우선 보장하고, 하이퍼링크 필드는 20분 안에 검증되지 않으면 확인 못 함으로 남긴다.
- 기존 11종을 유지하고 그림/제목/링크/코드/줄바꿈 합성 사례를 추가한다. 기존 harness 판정선을 유지하고 그림 해시·크기·대체글·줄바꿈·서식은 추가 검사한다. 사용자 문서는 사용하지 않는다.
