# 업무 연보: 인수인계·실적 보고서 데이터 스키마 (2026-11-18)

본 문서는 업무 연보 엔진(`kfind-annual`)에서 생성하는 인수인계·실적 보고서용 데이터 구조(JSON)의 스키마 명세서입니다. Windows·한/글 환경의 HWPX 보고서 생성 모듈이 본 JSON 파일을 그대로 읽어 표, 차트, 타임라인 등에 데이터를 자동 주입할 수 있도록 설계되었습니다.

인코딩은 UTF-8이며, 외부 의존성(serde 등) 없이 RFC 8259 표준을 준수하여 생성됩니다.

---

## 1. 최상위 구조 (Root Object)

| 필드명 | 형식 (Type) | 필수 여부 | 한글 필드 설명 |
|---|---|---|---|
| `period` | Object | 필수 | 분석 대상 전체 기간 (시작일 및 종료일) |
| `total_docs` | Integer | 필수 | 분석 기간 내 총 문서 수 |
| `monthly_docs` | Array<Object> | 필수 | 1월부터 12월까지 월별 작업/작성 문서 수 목록 |
| `top_topics` | Array<Object> | 필수 | 업무 비중 상위 주제(N개) 상세 목록 (이름, 문서 수, 대표 문서 3건 경로, 기간) |
| `week_day_heatmap` | Array<Object> | 필수 | ISO 연도 및 주차별 요일(월~일) 작업량 열지도 요약 |
| `timeline` | Array<Object> | 필수 | 주요 업무 타임라인 (주제별 시작 월, 종료 월, 월별 문서 수 상세) |

---

## 2. 세부 객체 명세

### 2.1 `period` (기간)

분석 대상 문서들의 전체 기간 범위입니다.

| 필드명 | 형식 (Type) | 한글 설명 | 예시 |
|---|---|---|---|
| `start` | String | 분석 시작 날짜 (`YYYY-MM-DD` 형식, 미지정 시 `""`) | `"2026-01-01"` |
| `end` | String | 분석 종료 날짜 (`YYYY-MM-DD` 형식, 미지정 시 `""`) | `"2026-12-31"` |

### 2.2 `monthly_docs` (월별 문서 수 항목)

1월(1)부터 12월(12)까지 12개 원소로 구성됩니다.

| 필드명 | 형식 (Type) | 한글 설명 | 예시 |
|---|---|---|---|
| `month` | Integer | 월 (1 ~ 12) | `1` |
| `doc_count` | Integer | 해당 월에 작업/생성된 문서 수 | `18` |

### 2.3 `top_topics` (상위 주제 항목)

문서 수 및 비중 기준 상위 N개 주제의 상세 정보입니다.

| 필드명 | 형식 (Type) | 한글 설명 | 예시 |
|---|---|---|---|
| `topic_id` | Integer | 주제 고유 식별 번호 (0-based) | `0` |
| `topic_name` | String | 정규화된 대표 주제 명칭 | `"예산 및 결산 정산"` |
| `doc_count` | Integer | 해당 주제에 속한 문서 수 | `35` |
| `percentage` | Number (Float) | 전체 문서 대비 해당 주제의 비중 (%) | `23.3` |
| `representative_docs` | Array<String> | 대표 문서 최대 3건의 원본 파일 경로 목록 | `["C:/Work/2026_예산안.hwpx"]` |
| `period` | Object | 해당 주제의 작업 수행 기간 | `{"start": "2026-01-10", "end": "2026-11-20"}` |

`top_topics[].period` 하위 필드:
- `start` (String): 주제 첫 문서 작업일 (`YYYY-MM-DD` 또는 `YYYY-MM`)
- `end` (String): 주제 마지막 문서 작업일 (`YYYY-MM-DD` 또는 `YYYY-MM`)

### 2.4 `week_day_heatmap` (주차×요일 열지도 요약)

ISO 연도와 ISO 주차별로 월요일부터 일요일까지의 작업량을 집계한 요약 데이터입니다.

| 필드명 | 형식 (Type) | 한글 설명 | 예시 |
|---|---|---|---|
| `iso_year` | Integer | ISO 주차 기준 연도 (연도 경계 주차 반영) | `2026` |
| `iso_week` | Integer | ISO 주차 번호 (1 ~ 53) | `1` |
| `days` | Array<Integer> | 요일별 문서 수 배열 (길이 7: [월, 화, 수, 목, 금, 토, 일]) | `[4, 6, 2, 5, 3, 0, 0]` |
| `total` | Integer | 해당 주차의 7일간 문서 수 합계 | `20` |

### 2.5 `timeline` (주요 업무 타임라인)

주제별 업무 착수 및 마무리 시점을 나타내는 타임라인 데이터입니다.

| 필드명 | 형식 (Type) | 한글 설명 | 예시 |
|---|---|---|---|
| `topic_id` | Integer | 주제 고유 식별 번호 | `0` |
| `topic_name` | String | 주제 명칭 | `"예산 및 결산 정산"` |
| `start_month` | String | 업무 시작 연월 (`YYYY-MM` 형식, 없으면 `""`) | `"2026-01"` |
| `end_month` | String | 업무 종료 연월 (`YYYY-MM` 형식, 없으면 `""`) | `"2026-11"` |
| `active_months` | Array<Object> | 실제 문서 작업이 발생한 연월과 건수 목록 | 아래 표 참조 |

`timeline[].active_months` 하위 필드:
- `year` (Integer): 역년 (예: `2026`)
- `month` (Integer): 월 (1 ~ 12)
- `year_month` (String): 연월 문자열 (`"2026-01"`)
- `doc_count` (Integer): 해당 연월 작업 문서 수

---

## 3. 완전한 JSON 예시

```json
{
  "period": {
    "start": "2026-01-01",
    "end": "2026-12-31"
  },
  "total_docs": 120,
  "monthly_docs": [
    {"month": 1, "doc_count": 15},
    {"month": 2, "doc_count": 8},
    {"month": 3, "doc_count": 12},
    {"month": 4, "doc_count": 10},
    {"month": 5, "doc_count": 7},
    {"month": 6, "doc_count": 14},
    {"month": 7, "doc_count": 6},
    {"month": 8, "doc_count": 5},
    {"month": 9, "doc_count": 11},
    {"month": 10, "doc_count": 13},
    {"month": 11, "doc_count": 12},
    {"month": 12, "doc_count": 7}
  ],
  "top_topics": [
    {
      "topic_id": 0,
      "topic_name": "예산 및 \"정산\" 관리",
      "doc_count": 35,
      "percentage": 29.2,
      "representative_docs": [
        "C:\\문서\\2026_본예산안_최종.hwpx",
        "C:\\문서\\2026_1분기_집행현황.hwpx",
        "C:\\문서\\2026_상반기_정산보고서.hwpx"
      ],
      "period": {
        "start": "2026-01-05",
        "end": "2026-11-28"
      }
    },
    {
      "topic_id": 1,
      "topic_name": "업무 연보 A1 개발",
      "doc_count": 28,
      "percentage": 23.3,
      "representative_docs": [
        "C:\\문서\\연보엔진_설계안.hwpx",
        "C:\\문서\\연보엔진_구현명세.hwpx",
        "C:\\문서\\연보엔진_검증보고.hwpx"
      ],
      "period": {
        "start": "2026-03-01",
        "end": "2026-10-30"
      }
    }
  ],
  "week_day_heatmap": [
    {
      "iso_year": 2025,
      "iso_week": 52,
      "days": [2, 1, 0, 0, 0, 0, 0],
      "total": 3
    },
    {
      "iso_year": 2026,
      "iso_week": 1,
      "days": [4, 5, 2, 3, 1, 0, 0],
      "total": 15
    }
  ],
  "timeline": [
    {
      "topic_id": 0,
      "topic_name": "예산 및 \"정산\" 관리",
      "start_month": "2026-01",
      "end_month": "2026-11",
      "active_months": [
        {"year": 2026, "month": 1, "year_month": "2026-01", "doc_count": 10},
        {"year": 2026, "month": 6, "year_month": "2026-06", "doc_count": 15},
        {"year": 2026, "month": 11, "year_month": "2026-11", "doc_count": 10}
      ]
    },
    {
      "topic_id": 1,
      "topic_name": "업무 연보 A1 개발",
      "start_month": "2026-03",
      "end_month": "2026-10",
      "active_months": [
        {"year": 2026, "month": 3, "year_month": "2026-03", "doc_count": 8},
        {"year": 2026, "month": 10, "year_month": "2026-10", "doc_count": 20}
      ]
    }
  ]
}
```

---

## 4. 직렬화 및 파일 출력 규칙

1. **외부 의존성 없음**: `serde` derive나 `serde_json` 매크로에 의존하지 않고 전용 경량 이스케이프 함수를 통해 직접 직렬화합니다.
2. **한글 및 유니코드 보존**: 한글 문자는 UTF-8 그대로 보존되어 HWPX 생성기가 추가 디코딩 없이 즉시 읽을 수 있습니다.
3. **특수 문자 이스케이프**: 문자열 내 따옴표(`"`), 역슬래시(`\`), 줄바꿈(`\n`, `\r`), 탭(`\t`) 및 0x20 미만의 ASCII 제어 문자는 RFC 8259에 맞게 정확히 이스케이프됩니다.
4. **원자적 파일 교체 (Atomic Write)**: 출력 함수(`write_report_json`)는 동일 폴더에 임시 파일(`.tmp`)을 생성하여 내용을 완전히 기록한 후 원자적으로 대상 경로로 교체(`rename`)하여 프로세스 비정상 종료 시에도 파일 손상을 방지합니다.
