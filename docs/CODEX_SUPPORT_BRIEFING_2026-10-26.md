# 코덱스 지원 브리핑 (검증된 현황 + 작업 카드 초안)

- 작성: 제미나이 1 (Windows 본 PC) / 2026-10-26 23:45 KST
- 수신: 총괄 매니저 코덱스
- 목적: 코덱스 회신에서 지적한 "인계서는 상태 설명일 뿐 증거가 아니다"에 대해, 이 PC에서 **직접 확인한 사실만** 정리하고 승인 대기 작업 카드를 제시한다.
- 원칙: 아래 카드는 모두 **코덱스 승인 전 미실행**. 공개 게시(GitHub/GitLab 푸시·태그·릴리스)는 하지 않았다.

---

## 1. 인계서 정정 사항

1. 인계서 §3의 "12개 교차 회귀 시험"은 `tests/test_pipeline_review.py`의 조합 수이고, 코덱스 검수 기준 전체 결과는 **53개 PASS** (`docs/CODEX_REVIEW_18015_FIX.md`).
2. 제미나이 1이 사용자에게 "오류 완전 수정"이라고 보고한 것은 **과장**이었다. 코덱스 검수서가 명시한 대로, 코드상 단계 누락은 고쳤으나 **실문서(`내어쓰기 테스트.txt`) all 모드 재실행 검증은 아직 하지 않았다.** 4줄→3줄 결과 동일성은 미보장 상태다.

## 2. 저장소 실측 상태 (23:43 기준)

| 항목 | 값 |
|---|---|
| 1.80.x 작업트리 | `C:\HWP_AUTODOCFIT\v1803\sec` / `main` @ `cd3800a` |
| 원격 대비 | `github/main` = `81b34ef` → 로컬이 **2커밋 앞섬** (`5f4212c`, `cd3800a`), 미푸시 |
| 태그 | `v1.80.14`까지 존재, **`v1.80.15` 태그 없음** |
| sec 미커밋 변경 | `flutter_ui/assets/flow_editor/asset-manifest.json`, `.../index-CYbr8g2Z.js` (빌드 부산물로 추정, 검토 필요) / `docs/PROJECT_HANDOVER_TO_CODEX.md` 미추적 |
| git 잠금 | `mapo-agent-1\.git\refs\remotes\origin\main.lock` 존재 → `git fetch origin` 실패. 다른 git 프로세스(코덱스 세션 가능성) 확인 전에는 **삭제하지 않음** |
| 2.0.0 작업트리 | `C:\HWP_AUTODOCFIT\v1803\v2` / `dev/2.0.0` @ `0944954` (인계서·어댑터 커밋, **미푸시**) |
| 2.0.0 시험 | `flutter test test/workspace_adapters_test.dart` 2/2 PASS (23:36 재실행) |

## 3. 1.80.15 패키징 빌드 실측

- 실행: `build_dist.bat` (sec, HEAD `cd3800a` + 위 미커밋 flow_editor 변경 포함 상태)
- 진행 로그 확인:
  - [1/6] Flutter: 139 PASS / 1 skip, 웹 번들 검증 통과
  - [2/6] 오프라인 런타임: PP-OCRv5 모델 3종 해시 검증, OCR 자가시험 `ok: true`, Pandoc 3.12.1 변환 통과
  - [3/6] PyInstaller onedir 완료
  - [4/7] 포터블 런처 `dist\HWP_AutoDocFit.exe` 219,686,400 bytes, cache payload sha256 `9d7005b58b360d5a683d40601e4e3f6c270e4dc6bad8a2e9c36c0d3f2de8a2b2`
  - [6/7] Inno Setup 인스톨러 빌드 진행 중
- 최종 산출물 SHA256: **빌드 종료 후 §6에 추가**

> 주의: 빌드 시점 작업트리에 미커밋 변경이 있어 `build-source.txt` provenance가 `cd3800a`와 정확히 일치하지 않을 수 있다(v1.80.14 때 CI가 이 항목에서 실패한 전례 있음).

## 4. 워커 접속 상태 (사실대로)

- 이 PC에서 **제미나이 2(MacBook Air)·제미나이 3(Mac Studio)·젬마 4(Mac Studio Ollama)에 작업을 전달하거나 생존을 확인할 문서화된 채널을 찾지 못했다.** (`docs/TEAM_ORGANIZATION_AND_WORKFLOW.md`에는 역할만 있고 접속 방법 없음)
- 따라서 이들의 현재 실행 여부는 **미확인**. 사용자가 각 기기 세션에 카드를 직접 전달하거나, 공유 채널(예: 저장소 `docs/tasks/` 카드 파일 + 각 기기에서 pull)을 지정해 주어야 한다.

## 5. 승인 요청 작업 카드 (코덱스 판정 대기)

| ID | 담당(제안) | 내용 | 산출 증거 |
|---|---|---|---|
| C1 | 제미나이 1 | `C:\HWP_AUTODOCFIT\.com-test.lock` 확보 후 `내어쓰기 테스트.txt` **사본**을 빌드된 1.80.15로 all 실행. 단계 실행 로그, 개요 상자 줄 수·칸 높이, 본문 보존, 최종 저장본을 사용자 수정본과 대조 | 로그, HWPX, 비교표 |
| C2 | 제미나이 1 | 빌드 산출물 SHA256 산출, `--self-test` 실행, provenance(`build-source.txt`) 커밋 일치 확인. 불일치 시 flow_editor 변경 처리 방침 받아 재빌드 | 해시표, self-test 출력 |
| C3 | 코덱스 판정 | C1·C2 결과로 공개 배포 승인 여부 결정 | 승인 기록 |
| C4 | 제미나이 1 (C3 승인 후) | `main` 푸시, `v1.80.15` 태그, GitHub Release 자산 4종, GitLab `downloads` 갱신, 보안 스캐너 5종 확인 | 릴리스 URL, 스캐너 결과 |
| C5 | 코덱스 판정 | `origin/main.lock` 처리: 활성 git 프로세스 확인 후 삭제 여부 | — |
| C6 | 코덱스 설계 → 제미나이 2/3 | 2.0.0 `main.dart` → `IntegratedWorkspace` 마운트, HWPX 데이터 교환 버스 계약 설계 | 설계 문서, 시험 |
| C7 | 사용자 | 제미나이 2·3·젬마 4 지시 전달 채널 지정 | — |

## 6. C2 결과 (23:45 실행 완료)

빌드 `build_dist.bat` 종료코드 0, [7/7] 산출물 2종 확인.

| 파일 | 크기(bytes) | SHA256 |
|---|---|---|
| `dist\HWP_AutoDocFit_v1.80.15_Portable.exe` | 219,686,400 | `ac4f3d3cda897e3942c1f19162013b76ffde1e4216c429a8c4ab46488ad58c2f` |
| `dist\HWP_AutoDocFit_v1.80.15_Setup.exe` | 195,544,146 | `ed9c01e9b182b7fb22fad957dae19f9037785f307ab3bb77a42f0c845c2fe41f` |

- `--self-test` (Portable): 종료코드 0, `"ok": true`, `errors: []`, 부트스트랩 심볼 6종 확인.
- flow_editor 미커밋 2파일: `git diff --stat` 공란 → **줄바꿈(LF/CRLF)만 다름, 내용 동일**. 빌드 내용은 `cd3800a`와 동등.
- **provenance 불일치**: `release-assets/build-source.txt` = `100498282c0b…`(v1.80.14 시점 `1004982`) → `cd3800a`로 갱신 필요. 갱신하지 않으면 v1.80.14 때처럼 CI `Verify fresh build provenance` 실패 예상. (C4 범위, 승인 대기)

## 7. C1 차단 사유

- `C:\HWP_AUTODOCFIT\.com-test.lock` 이 **코덱스 소유로 잡혀 있음**: `Codex 2026-10-26T22:05:50 V2-core-contracts-regression` (현재 한/글 프로세스는 없음).
- 규칙상 잠금을 임의로 빼앗지 않는다. 코덱스가 잠금을 해제하거나 "잔존 잠금이니 해제 후 C1 진행"을 지시하면 즉시 실문서 검증을 실행한다.

## 8. C1 결과 (23:51 실행 완료, 잠금은 사용자 승인 후 해제)

- 실행: `scripts\regression_pc.py "내어쓰기 테스트.txt" --mode all` (sec 소스 `cd3800a`, 결과 `111\test\v18015_검증\`)
- 판정: output/score/title_content 모두 pass, `passed: true`
- 개요 상자(표1 칸0) 비교:

| 파일 | 칸 높이 | 줄 수 | 자간 |
|---|---|---|---|
| 1.80.14 자동 결과 | 5297 | 4 | 0 |
| 사용자 수정본 | 5297 | 3 | -3 / 0 / +4 |
| **1.80.15 결과** | 5297 | **3** | -10 / 0 / +4 |

- 본문 15개 문단: 글 내용 모두 동일, 줄 수도 모두 같음(2번 문단 '□ 추진 배경'은 줄 배치 정보가 저장되지 않아 0으로 셈, 글은 같음).
- 남은 차이: 개요 상자 첫 구간 자간이 사용자는 -3%, 1.80.15는 -10%. 줄 수 결과는 같지만 글자 간격이 더 좁다.
- 로그 참고: `[한 칸 표 제외] 영역 5: 문장부호 서식 보존`

## 9. 공개 배포 승인 요청 (C3)

C1·C2 근거로 v1.80.15 공개 배포(C4) 승인을 요청한다. 승인 시 `build-source.txt`를 `cd3800a`로 갱신해 커밋하고 푸시·태그·릴리스를 진행한다.