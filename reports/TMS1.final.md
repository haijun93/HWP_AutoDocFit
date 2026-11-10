# TMS1 타임머신 Rust 스캔

## 1단계 — Python 기준 및 출력 계약

`docfit_core/timemachine/scan.py` 공개 진입점은 `scan_root(store, root, rules=None, *, cancel=None, progress=None, chunk=500, recalculate=None)`, `scan_roots(store, roots, rules=None, *, cancel=None, progress=None)`, `default_roots(home=None, environ=None)`이다. `_stat_times(info)`는 생성 시각으로 Windows `st_birthtime`을 우선하고 없으면 `st_ctime`을 쓰며 수정 시각은 `st_mtime`이다.

| 함수/출력 | 기존 계약 |
|---|---|
| `ScanResult` | `seen`, `written`, `removed`, `skipped_dirs` 정수, `(path, exception_class)` 튜플 배열 `errors`, `cancelled` 불리언 |
| `scan_root` | 루트 절대경로화 → 제외 행 정리 → 기존 색인 조회 → 스택 기반 scandir → 증분 upsert → 완료 시 누락 행 삭제·`time_mode` 기록. 취소하면 삭제 정리와 모드 기록 생략. 폴더/항목 OSError는 기록하고 계속 진행. 진행 콜백은 청크 flush 후와 종료 시 호출. |
| 포함 파일 레코드 | `path`, 절대 `root`, 원본 `name`, `kind`, `size`, `mtime`, `ctime`, `work_time`, `flags`, `cloud`. kind는 확장자 룩업, `work_time`은 max 또는 mtime, flags는 copied/new, cloud는 RECALL 속성. DB 키/root는 `path_key`(normcase·normpath·casefold). |
| `scan_roots` | 루트별 결과 합산, 취소 시 다음 루트 중단, 정상 완료 후 모드 기록. |
| `default_roots` | Desktop/Documents의 사용자 폴더와 OneDrive 후보 중 존재하며 중복되지 않는 경로. 다운로드 제외. |
| 규칙 적용 | symlink 파일 제외, 정션 디렉터리 제외(0x400), `Rules.skip_dir/skip_file`, 지원 확장자·숨김·임시/무시 이름, `cloud_only`. `rules.py`의 규칙 결과를 완전히 동일하게 유지해야 함. |

기준 시간은 지정된 Python으로 임시 폴더 10개와 텍스트 파일 10,000개를 만들고 `scan_roots`를 한 차례 호출해 측정했다. DB 초기 생성과 첫 쓰기를 포함한다.

| 환경/결과 | 값 |
|---|---:|
| Python | `C:/Users/haiju/OneDrive/바탕 화면/HWP_AutoDocFit/mapo-agent-1/.venv/Scripts/python.exe` |
| Python 1회 경과 | 0.4152초 |
| 결과 | seen 10,000, written 10,000, removed 0, errors 0 |

다음 단계의 `tools/tmscan_regress.py`는 임시 트리를 한 번만 만들고 Python/Rust 실행별 결과를 같은 메타데이터 계약으로 정규화해 차이 수를 집계한다. `--dump`는 행별 diff를 출력한다. 경로·크기·mtime·ctime·kind·work_time·flags·cloud·오류·건너뛴 디렉터리를 비교한다.

## 비교 및 연결 판단

Rust 하위 명령 `klib tm-scan <root> <규칙 JSON>`을 구현하고, 위 도구를 동일한 임시 트리 10,000개에서 1회씩 실행했다. 비교 정규화 오류(Windows `\\?\` 경로 접두사)를 고친 뒤 최종 차이는 0이었다.

| 동일 임시 트리 1회 | Python 전체 `scan_roots` | Rust 명령 실행 | 결과 |
|---|---:|---:|---|
| 경과 시간 | 0.6703초 | 1.0729초 | Rust가 0.4026초 느림 |
| 포함 행 | 10,000 | 10,000 | 차이 0 |
| 제외 폴더 / 오류 | 1 / 0 | 1 / 0 | 동일 |

Rust 명령은 독립 실행과 JSON 반환을 포함해 Python 전체 경로보다 느렸다. 요청한 성능 기준을 만족하지 못했으므로 앱 설정 `rust_timemachine_scan` 추가와 앱 연결은 진행하지 않았다. 기존 동작은 바뀌지 않았다. Python 기준 시간 측정은 별도 최초 트리에서 0.4152초였고, 회귀 도구 트리는 제외 규칙 검증 항목을 추가한 동일 비교 트리라 0.6703초가 최종 직접 비교값이다.

