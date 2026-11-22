# DOCK2 독 동작 보강 결과 (2026-11-22)

## 범위와 동작

설정 `dock_auto_hide` 하나로 자동 숨김을 켜고 끈다. 기본값은 `false`이며, 설정이 없거나 잘못된 형식이어도 꺼진다.

| 항목 | 변경 전 | 변경 후 |
|---|---|---|
| 일반 최대화 창 | 모니터 전체 사각형을 덮는 창만 숨김 판정에 걸림. 작업 표시줄 영역을 남기는 최대화 창에서는 독이 남음. | 설정을 켜면 포그라운드 창 사각형과 독 사각형의 교차 여부로 숨김 판정. 설정을 끄면 기존 모니터 전체 판정을 유지. |
| 다시 나타남 | 숨겨진 독을 가장자리 호버로 표시하지 않음. | 설정을 켜면 독이 놓인 모니터 가장자리 2px 안으로 포인터가 들어올 때 표시. 현재 슬라이드 애니메이션은 없음. |
| 색상 | 검토 문서는 독이 `ui_theme`와 분리되어 있다고 설명함. | 실제 현재 코드는 이미 `ui_theme` 의존성, LIGHT/DARK 팔레트, 고대비 시스템 색상을 사용함. 코드 확인 결과를 반영했으며, 테마 팔레트 연결을 새로 중복 구현하지 않음. |

판정 상태는 기존 `STATE`(사용자/프로그램 요청으로 독 표시 허용)와 OS 표시 상태를 따른다. 설정을 켜면 포그라운드 창이 독 영역과 교차할 때 숨기고, 포인터가 해당 모니터 가장자리 판정 영역에 들어오면 다시 표시한다. 수동 숨김 상태(`STATE == false`)에서는 가장자리 호버로 표시하지 않는다.

순수 함수 `foreground_covers_dock`과 `pointer_on_reveal_edge`를 분리하고 겹침 경계, 빈 구간, 가장자리 허용 폭을 단위 시험으로 확인했다. 설정 해석 함수 `parse_auto_hide`는 기본 꺼짐과 참/잘못된 값을 구분한다.

## 확인 결과

`cargo test --release --locked --manifest-path dock/Cargo.toml` (릴리스 빌드 출력은 `CARGO_TARGET_DIR` 환경변수 사용, cargo 경로는 MinGW 디렉터리를 PATH 맨 앞에 두고 실행):

```text
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

관련 Python 시험:

```text
Ran 10 tests in 0.198s
OK
```

포함 대상은 `test_dock_runtime`, `test_dock_runtime_events`, `test_settings_list_keys`다.

헤드리스 `--capture-dir ... --dpi 96` 결과 `dock_light_96.png`를 직접 열어 확인했다. 캡처에는 16개 메뉴 아이콘과 그룹 구분선, 한국어 라벨이 표시되고 잘림이나 빈 렌더링은 보이지 않았다.

## 확인하지 않은 점

- 자동 숨김을 켠 실제 데스크톱에서 최대화 창·다중 모니터·작업 표시줄 조합을 직접 조작해 확인하지 않았다. 판정 함수 단위 시험과 빌드만 확인했다.
- 독이 가장자리에 닿을 때 이동하는 슬라이드 애니메이션은 구현하지 않았다.
- `ui_theme` 팔레트 적용은 기존 코드에서 확인한 상태라 팔레트 색상 변경은 없었다.
