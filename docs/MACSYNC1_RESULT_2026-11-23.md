# MACSYNC1 결과 — 맥 job 폴더 `.git` 없음·낡은 기준 문제 (2026-11-23)

## 1. 결론(원인)

**주원인: 오케스트레이터가 같은 카드를 2~3번 동시에 기동 → 같은 이름의 `run_mac.sh` 가 동시에 job 폴더를 `rm -rf`·`git worktree add` 하며 서로 부순다.**

증거(모두 이 PC·맥의 실제 기록):

| 항목 | 근거 |
|---|---|
| 중복 기동 | `E:/docfit-tmp/orch/orch.log`: `08:19 시작 HIER3E3` ×2, `20:45 시작 KORDOCB1` ×2 + `20:46` ×1(HOUSE2 는 5회). 사이에 `mv: cannot stat '.../queue.tsv.tmp'` — `orch.sh` 가 동시에 둘 이상 돌며 `setstate` 의 `queue.tsv.tmp` 를 서로 가로채 상태 갱신이 겹친다 |
| 중복 실행 흔적 | `chain.log`: `HIER3E3 mac exit 1`, `HIER3E3 mac exit 128` / `KORDOCB1 mac exit 255`, `exit 1`, `exit 128` (카드 하나에 종료 기록 2~3개) |
| Studio 저장소 | `repo` 의 reflog: 20:46:20, 20:46:21 에 `docfit-KORDOCB1.bundle` fetch 가 1초 간격 2번. HIER3E3 도 같은 패턴 |
| 깨진 job 폴더 | `jobs/KORDOCB1`: `.git`·`.cargo`·`.claude`·`.github` 없음(파일 76개), `git worktree list` 에 `prunable`(gitdir 파일 없음). `jobs/HIER3E3`: `.git` 없음, worktree 등록도 사라짐. 정상 폴더(HIER3E2, STAGEDECL1)는 `.git` 파일 있음 |
| 카드 쪽 증상 | `KORDOCB1-mac.log`·`HIER3E3-mac.log`: codex 의 첫 `git status` 가 `fatal: not a git repository`, `ls -la .git` → `No such file` |
| 마지막 결과 가져오기 실패 | `KORDOCB1.run`: `fatal: '//Users/.../jobs/KORDOCB1' does not appear to be a git repository` |

동시 실행 때 일어나는 일: A 가 `worktree add` 로 폴더를 채우는 동안 B 가 `rm -rf $J` → B 의 `worktree add` 가 A 의 `index.lock` 과 충돌하거나, `.git` 파일만 지워진 반쪽 폴더가 남는다. B 쪽 `git worktree prune` 이 등록 정보까지 지운다. 이 상태에서 카드는 시작되고, 마지막 `git add -A; git commit` 이 반쪽 폴더를 그대로 커밋하기도 한다.

### 재현(수정 전 스크립트, 임시 BASE `~/docfit-ext/macsync1-test`, 같은 이름 3개 동시)
- 맥 로그: `fatal: Unable to create '.../worktrees/MACSYNC1T/index.lock': File exists.`
- 결과 폴더에 `D .claude/skills/gongmunseo-report-writing/SKILL.md`(삭제된 상태)가 남고, 그 상태가 `MACSYNC1T: 맥(studio/shell) 작업 결과` 커밋(61f43889)으로 들어갔다. (단독 실행은 정상)
- 이번 재현에서는 `.git` 자체까지 사라지지는 않았으나(파일 삭제 순서는 실행마다 다름) 반쪽 폴더가 생기는 경로는 동일하다.

### 부차 원인(같은 코드에서 발견, 낡은 기준 증상 `(가)`의 후보)
1. `MACSHA=$(ssh …)` 가 ssh 실패(255)와 "저장소 없음"을 구분하지 않았다. 연결이 한 번 실패하면 빈 값 → 전체 번들 생성 → `scp -q` 실패는 검사하지 않음 → 맥에 번들이 없으면 `[ ! -f bundle ]` 분기가 `:` 로 넘어가 **낡은 `main` 에서 worktree 를 만들고 카드를 시작**했다.
2. `git branch -f bundlemain github/main -q 2>/dev/null` — 실패(다른 worktree 에서 체크아웃 중 등)해도 침묵. 이번 조사에서도 `bundlemain` 이 `github/main` 보다 한 커밋 뒤에 있었다(조사 직전 새 커밋이 생겨서였으나, 실패했다면 그대로 낡은 기준이 된다).
3. 증분 번들 실패(`fetch … || fetch …` 후 `|| true`)도 무시하고 계속 진행.
4. 카드가 시작할 때 `.git` 존재·HEAD 일치를 전혀 확인하지 않았다.

## 2. 가설별 판정

| 가설 | 판정 | 근거 |
|---|---|---|
| 같은 이름 카드 동시 실행으로 job 폴더 파손 | **원인(확정)** | 위 표, 재현 |
| 외장 디스크 마운트 해제 후 빈 폴더 | 아님(현재 확인 범위) | Studio `/Volumes/2T hard` 마운트됨(`/dev/disk8s1`, apfs), 여유 961GiB, `~/docfit-ext` → `/Volumes/2T hard/docfit-work` 심볼릭 링크(11-18 생성). 마운트 시각·과거 해제 이력은 확인하지 못함. 마운트가 빠지면 `/Volumes/…` 는 일반 사용자가 못 만들어 `mkdir -p` 가 실패하므로 빈 폴더가 생기는 경로는 아님 |
| 증분 번들이 맥에 없는 기준을 가정 | 가능성은 있으나 이번 증상의 원인 아님 | `MACSHA` 는 맥에서 읽고 `cat-file -e` 로 로컬 존재를 확인한 뒤에만 증분을 만든다(가정이 틀릴 수 없음). 다만 실패 시 폴백이 없었음 → 수정 |
| 번들 전송 실패 무시 | 코드상 사실(부차 원인 1) | 이번 사례에서 발생했다는 증거는 못 찾음 |
| git 실패를 감지 못하고 카드 시작 | 사실(부차 원인 4) | 카드가 `.git` 없는 폴더에서 시작됨 |
| 권한·디스크 | 아님 | Studio 데이터 볼륨 41GiB 여유, 외장 961GiB, 쓰기 가능 |

Air 는 `~/work/docfit`(내장 디스크, 외장 없음)이고 같은 기간 중복 기동은 HOUSE2 에서 있었으나(5회) HOUSE2 결과는 정상 커밋됐다 — 운이 좋았거나 타이밍 차이로 보이며, 같은 위험에 노출돼 있었다.

## 3. 수정(`C:/HWP_AUTODOCFIT/v1803/tools/run_mac.sh`, 백업 `run_mac.sh.bak_1123`)

1. **같은 이름 중복 실행 차단**: `/e/docfit-tmp/.macrun-<이름>.lock`(mkdir 잠금+pid). 두 번째 이후 실행은 아무것도 만들지 않고 선행 실행 종료까지 기다린 뒤 `exit 0`(`.run` 에 `exit` 줄이 선행 실행 끝나는 시점에 찍히므로 오케스트레이터가 일찍 완료로 오판하지 않는다).
2. **요청 기준 고정**: `TARGET=$(git rev-parse github/main)`, `bundlemain` 을 그 해시로 강제하고 다르면 코드 3.
3. **ssh 실패 구분**: `MACSHA` 조회에서 255 는 3회 재시도, 계속 실패하면 코드 3. `scp` 실패도 코드 3.
4. **저장소 준비를 별도 ssh 단계로 분리·점검**(맥에서 `bash -s` 로 실행, `SYNC_OK host=… job=… git=yes HEAD=… clean=…` 한 줄 출력): 외장 디스크 마운트(`/Volumes/*` 심볼릭 대상)·여유 2GB·쓰기 권한 점검 → 저장소 갱신은 맥 쪽 잠금으로 직렬화 → 증분 번들 적용 → 요청 해시가 없으면 **전체 번들로 자동 폴백**(옆 폴더 `repo.new` 에 먼저 clone, 성공할 때만 기존 저장소를 `repo.bad-<시각>` 으로 옮기고 교체; 실패하면 기존 저장소 유지) → `worktree add -B feature/<이름> <TARGET 해시>` → **`.git` 존재, HEAD==TARGET, 브랜치명, `git status` 깨끗함** 점검(실패 시 1회 재생성) → 그래도 실패면 **카드를 시작하지 않고 코드 3**(`<이름>.final.md` 에 사유, `chain.log` 에 `exit 3 (sync)`).
5. **카드 시작 직전 한 번 더** `.git`·HEAD 확인(어긋나면 코드 3).
6. 종료 코드: 카드 실행 단계가 3 이면 3 으로 종료(이전에는 항상 마지막 명령의 0).
7. 시험용: `tool=shell`(카드 `.md` 를 bash 로 job 폴더에서 실행, 맥 환경 문구 첨부 없음), 환경변수 `MACBASE`(BASE 재정의), `MACSYNC_TEST_CORRUPT=inc|full|incfull`(번들 일부러 손상). 운영 동작에는 영향 없음.
8. 점검 로그: `reports/<이름>-sync.log`.
9. `git bundle create … -q` 는 이 PC 의 git 에서 **segfault** 가 나서(시험 중 발견) 쓰지 않는다.

## 4. 시험 결과(수정 후 스크립트, 시험 카드 = `git rev-parse HEAD`·`.git`·`git status` 출력 후 종료)

임시 BASE `~/docfit-ext/macsync1-test`(Studio)에서 오류 경로를, 실제 BASE 에서 정상 경로를 시험했다.

| 시험 | 결과 |
|---|---|
| T1 증분 번들 정상(맥 main=5e05060a → 요청 ddde1c45) | `SYNC_OK … HEAD=ddde1c45… git=yes clean=yes`, rc 0 |
| T2 증분 번들 손상 | `fatal: early EOF` → `증분 번들로 부족 → 전체 번들 필요` → `증분 실패 → 전체 번들로 폴백` → `SYNC_OK … HEAD==요청`, rc 0 |
| T3 같은 이름 3중 동시 실행 | 1개만 실행(`SYNC_OK`, 카드 출력 정상), 2개는 `이미 진행 중(pid …)` 메시지 후 대기·rc 0. 수정 전 같은 시험은 index.lock 충돌·삭제 상태 커밋(위 재현) |
| T4 증분·전체 모두 손상 | `SYNC_FAIL: 전체 번들을 적용해도 요청 기준 … 가 없음` → **rc 3**, `.final.md` 에 `카드를 시작하지 않았다`, 맥에 job 폴더 없음, 기존 저장소 `repo` 유지(main 그대로) |
| T5 맥에 저장소 없음 → 전체 번들 clone | `전체 번들로 새로 clone 후 교체`, `SYNC_OK`, rc 0 |
| **T6 Studio 실제 BASE** (`~/docfit-ext`, 요청 cf6d1dbb) | `SYNC_OK host=Hyeokjuns-Mac-Studio … git=yes HEAD=cf6d1dbb3012c27a4db6bd48c41e9c368c50f206 clean=yes`. 카드 출력: `.git=yes`, `BRANCH=feature/MACSYNC1Tstudio`, `HEAD=cf6d1dbb…`, `git status`: 추적 파일 변경 없음(카드 파일 `.card.md` 하나만 미추적), 종료 후 HEAD 그대로(cf6d1dbb) |
| **T7 Air 실제 BASE** (`~/work/docfit`, 요청 7dce5c9d — 시험 사이 github/main 이 전진) | `SYNC_OK host=Hyeokjunui-MacBookAir … git=yes HEAD=7dce5c9d018e8319ed0bb6f20c4bf3d5fbfc4ffc clean=yes`. 카드 출력 `.git=yes`, 브랜치 `feature/MACSYNC1Tair`, HEAD 일치 |

두 맥 모두 HEAD 가 요청한 github/main 해시와 일치, `.git` 있음, 작업 트리 깨끗(카드 파일 제외).
정리: 이 카드가 만든 `MACSYNC1T*` job 폴더·브랜치(맥 2대, 이 PC `mac/MACSYNC1T*`)·임시 BASE `macsync1-test`·번들·`reports/MACSYNC1T*` 를 삭제했다. 다른 폴더·사용자 자료는 건드리지 않았다. (`chain.log` 에 시험 줄 `MACSYNC1T…` 가 남아 있다 — 오케스트레이터가 같은 파일에 쓰고 있어 지우지 않음.)

## 5. 확인하지 못한 것 / 남은 위험

- **근본 원인은 `orch.sh` 의 중복 기동**(여러 인스턴스가 동시에 돌며 `queue.tsv.tmp` 충돌, 같은 카드를 여러 번 `launch`). 이 카드는 `run_mac.sh` 만 고쳤으므로 중복 기동 자체는 그대로다. pc 쪽 `run_codex.sh`·`run_h2b_sonnet.sh` 도 같은 위험을 가진다. 권고: `orch.sh` 에 단일 인스턴스 잠금과 `setstate` 의 `flock`/mkdir 잠금(별도 카드). 현재 몇 개의 `orch.sh` 가 도는지는 이 환경의 `ps` 로 확인하지 못했다.
- 오케스트레이터는 `.run` 마지막 줄이 `exit N` 이면 N 과 무관하게 `done` 으로 표시한다(`fail` 상태 미사용). 코드 3 이 나도 상황판에는 `done` 으로 보인다 — 실제 실패 표시는 `reports/<이름>.final.md`(`MACSYNC 실패…`)와 `chain.log`(`exit 3 (sync)`)로 확인. 오케스트레이터가 `exit 3` 을 `fail` 로 쓰게 하는 수정은 하지 않았다.
- 전체 번들 폴백은 약 500MB 전송(이번 시험에서 번들 502,814,149B). 맥 저장소가 망가졌을 때만 일어난다.
- `repo.bad-<시각>` 로 옮겨진 저장소를 참조하는 옛 job 폴더 worktree 는 `.git` 링크가 끊긴다(결과는 이미 가져온 뒤라 영향 낮음). 폴백이 실제 운영에서 발동한 적은 없다.
- 다른 카드가 같은 맥에서 동시에 돌 때(워커당 1개 규칙으로 평소 없음)는 저장소 갱신 잠금만 직렬화한다.
- Studio 외장 디스크의 과거 마운트 해제 이력, Studio·Air `docfit-ext` 이외 경로(Air `~/docfit-ext` 없음)는 확인하지 못했다.
- `run_mac.sh` 는 저장소 밖 도구라 저장소에는 아래 diff 로만 기록한다.

## 6. 수정 전후 diff (`diff -u run_mac.sh.bak_1123 run_mac.sh`)

```diff
--- run_mac.before.sh	2026-11-23 21:48:24.254255600 +0900
+++ run_mac.new.sh	2026-11-23 22:12:41.429635700 +0900
@@ -1,34 +1,113 @@
 #!/bin/bash
-# 사용: run_mac.sh <studio|air> <codex|gemini> <카드.md> <이름> [effort=low]
+# 사용: run_mac.sh <studio|air> <codex|gemini|claude|shell> <카드.md> <이름> [effort=low]
 # 맥에서 카드를 실행한다(2026-11-18). 코드는 GitHub 자격 증명 없이 git bundle로 보내고, 결과 브랜치는 이 PC가 ssh로 받아온다(refs/heads/mac/<이름>).
 # 맡길 수 있는 일: Windows API 없는 Rust 크레이트(kfind-core·outliner_core·kfind/annual·hierarchy·pdf_edit_core)와 순수 Python. Win32·한/글 COM·Flutter 빌드·exe는 맡기지 말 것.
+# 2026-11-23 MACSYNC1: 저장소 준비 점검 추가. job 폴더에 .git 이 있고 HEAD 가 요청한 github/main 해시와 같아야 카드를 시작한다.
+#   증분 번들이 안 맞으면 전체 번들로 자동 폴백, 그래도 안 되면 카드를 시작하지 않고 종료 코드 3. 같은 이름 카드의 동시 실행(오케스트레이터 중복 기동)은 막는다.
+#   tool=shell 은 카드(.md)를 bash 스크립트로 job 폴더에서 실행하는 시험용(MACSYNC1 점검 카드).
 HOST=$1; TOOL=$2; CARD=$3; N=$4; EFF=${5:-low}
 case $HOST in studio) IP=192.168.0.4; BASE='$HOME/docfit-ext';; air) IP=192.168.0.3; BASE='$HOME/work/docfit';; *) echo bad host; exit 2;; esac
+[ -n "$MACBASE" ] && BASE=$MACBASE   # 시험용 재정의
 K="-i $HOME/.ssh/id_ed25519_mac -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=60"
 S="ssh $K hyeokjunkong@$IP"
 R=C:/HWP_AUTODOCFIT/v1803/reports; W=/e/docfit-tmp/rel267
-git -C $W fetch -q github
-git -C $W branch -f bundlemain github/main -q 2>/dev/null
-# 1) 맥 저장소 준비(없으면 전체 bundle, 있으면 증분 bundle)
-MACSHA=$($S "B=\"$BASE\"; eval B=\"\$B\"; [ -d \"\$B/repo/.git\" ] && git -C \"\$B/repo\" rev-parse main 2>/dev/null")
 BF=/e/docfit-tmp/mac-$N.bundle
-if [ -n "$MACSHA" ] && git -C $W cat-file -e "$MACSHA^{commit}" 2>/dev/null; then rm -f $BF; [ "$MACSHA" != "$(git -C $W rev-parse bundlemain)" ] && git -C $W bundle create $BF bundlemain "^$MACSHA" 2>/dev/null || rm -f $BF; else MACSHA=""; git -C $W bundle create $BF bundlemain 2>/dev/null; fi
-[ -s "$BF" ] && scp -q $K $BF hyeokjunkong@$IP:docfit-$N.bundle
+LOCK=/e/docfit-tmp/.macrun-$N.lock
+SYNCLOG=$R/$N-sync.log
+fail3(){ echo "[MACSYNC] $N: $* — 카드를 시작하지 않는다(코드 3)" | tee -a "$SYNCLOG" >&2; echo "MACSYNC 실패: $*. 카드를 시작하지 않았다." > "$R/$N.final.md"; echo "$N mac exit 3 (sync)" >> /e/docfit-tmp/chain.log; exit 3; }
+# 0) 같은 이름 카드의 동시 실행 방지(오케스트레이터가 같은 카드를 2~3번 동시에 기동하면 서로 job 폴더를 지워 .git 이 사라진다)
+if ! mkdir "$LOCK" 2>/dev/null; then
+  OLD=$(cat "$LOCK/pid" 2>/dev/null)
+  if [ -n "$OLD" ] && kill -0 "$OLD" 2>/dev/null; then
+    echo "$N: 같은 이름의 맥 실행이 이미 진행 중(pid $OLD) — 중복 실행은 아무것도 하지 않고 선행 실행이 끝나기를 기다린 뒤 종료한다"
+    while kill -0 "$OLD" 2>/dev/null; do sleep 20; done; exit 0
+  fi
+  rm -rf "$LOCK"; mkdir "$LOCK" 2>/dev/null || { echo "$N: 잠금 생성 실패"; exit 4; }
+fi
+echo $$ > "$LOCK/pid"; trap 'rm -rf "$LOCK"; rm -f "$BF"' EXIT
+: > "$SYNCLOG"
+# 1) 요청 기준 = github/main (fetch 실패는 경고만, 로컬 github/main 사용)
+git -C $W fetch -q github || echo "[MACSYNC] 경고: github fetch 실패, 로컬 github/main 사용" | tee -a "$SYNCLOG"
+TARGET=$(git -C $W rev-parse --verify -q 'github/main^{commit}') || fail3 "github/main 해시를 알 수 없음"
+git -C $W branch -f bundlemain "$TARGET" -q 2>/dev/null || git -C $W update-ref refs/heads/bundlemain "$TARGET"
+[ "$(git -C $W rev-parse bundlemain)" = "$TARGET" ] || fail3 "bundlemain 을 $TARGET 로 맞추지 못함"
+echo "[MACSYNC] $N $HOST 요청 기준 $TARGET" | tee -a "$SYNCLOG"
+# 2) 맥 저장소의 현재 main (ssh 연결 실패 255 는 재시도, 끝내 실패하면 중단 — 예전에는 빈 값으로 취급해 낡은 기준으로 진행했다)
+MACSHA=""
+for i in 1 2 3; do
+  MACSHA=$($S "B=\"$BASE\"; eval B=\"\$B\"; [ -d \"\$B/repo/.git\" ] && git -C \"\$B/repo\" rev-parse --verify -q main 2>/dev/null; true"); rc=$?
+  [ $rc -ne 255 ] && break; sleep 5
+done
+[ $rc -eq 255 ] && fail3 "맥($IP) ssh 연결 실패 3회"
+mkb(){ rm -f "$BF"; if [ "$1" = inc ]; then git -C $W bundle create "$BF" bundlemain "^$MACSHA" 2>/dev/null; else git -C $W bundle create "$BF" bundlemain 2>/dev/null; fi; [ -s "$BF" ]; }
+sendb(){ case "$MACSYNC_TEST_CORRUPT" in *"$1"*) head -c 2000 "$BF" > "$BF.t"; mv "$BF.t" "$BF"; echo "[MACSYNC] (시험) $1 번들을 일부러 손상" | tee -a "$SYNCLOG";; esac  # MACSYNC_TEST_CORRUPT=inc|full|incfull: 시험 전용
+  scp -q $K "$BF" hyeokjunkong@$IP:docfit-$N.bundle || fail3 "번들 전송(scp) 실패"; }
+prep(){ # $1=inc|full : 맥에서 저장소 갱신 + job 폴더(worktree) 생성 + 점검. 종료 0=성공, 10=전체 번들 필요, 그 외=실패
+  $S "bash -s -- '$N' '$TARGET' '$BASE' '$1'" <<'PREP' 2>&1 | tee -a "$SYNCLOG"
+N=$1; T=$2; MODE=$4; eval "B=\"$3\""
+export PATH=/opt/homebrew/bin:$HOME/.cargo/bin:$PATH
+BFR=$HOME/docfit-$N.bundle
+bad(){ echo "SYNC_FAIL: $*"; exit 3; }
+# 외장 디스크 마운트·디스크 여유·권한 점검
+LT=$(readlink "$B" 2>/dev/null || echo "$B")
+case "$LT" in /Volumes/*) MP=/Volumes/$(echo "$LT" | cut -d/ -f3); mount | grep -q " on $MP " || bad "외장 디스크 $MP 가 마운트돼 있지 않음";; esac
+mkdir -p "$B/jobs" || bad "작업 폴더 생성 실패 $B"
+AV=$(df -k "$B" | awk 'NR==2{print $4}'); [ "${AV:-0}" -ge 2000000 ] || bad "디스크 여유 부족(${AV}KB)"
+touch "$B/.w" 2>/dev/null && rm -f "$B/.w" || bad "쓰기 권한 없음 $B"
+# 저장소 갱신은 동시에 하나만(10분 넘은 잠금은 낡은 것으로 간주)
+LK="$B/.sync.lock"; i=0
+until mkdir "$LK" 2>/dev/null; do
+  [ -n "$(find "$LK" -maxdepth 0 -mmin +10 2>/dev/null)" ] && { rm -rf "$LK"; continue; }
+  i=$((i+1)); [ $i -gt 36 ] && bad "저장소 잠금 대기 초과"; sleep 5
+done
+trap 'rm -rf "$LK"; rm -f "$BFR"' EXIT
+has(){ [ -d "$B/repo/.git" ] && git -C "$B/repo" cat-file -e "$T^{commit}" 2>/dev/null; }
+if has; then echo "SYNC: 맥 저장소에 요청 기준이 이미 있음"
+elif [ -d "$B/repo/.git" ] && [ -s "$BFR" ]; then
+  git -C "$B/repo" fetch -q "$BFR" +bundlemain:refs/heads/main --update-head-ok 2>&1 || echo "SYNC: 번들 fetch 실패($MODE)"
+fi
+if ! has && [ "$MODE" = full ] && [ -s "$BFR" ]; then
+  # 전체 번들로 새로 clone: 먼저 옆 폴더에 받아 성공했을 때만 교체(기존 저장소는 지우지 않고 repo.bad-<시각> 으로 옮겨 둔다)
+  rm -rf "$B/repo.new"
+  if git clone -q -b bundlemain "$BFR" "$B/repo.new" 2>&1 && git -C "$B/repo.new" branch -m main 2>&1 && git -C "$B/repo.new" cat-file -e "$T^{commit}" 2>/dev/null; then
+    [ -d "$B/repo" ] && { mv "$B/repo" "$B/repo.bad-$(date +%Y%m%d%H%M%S)" || bad "기존 저장소 이동 실패"; }
+    mv "$B/repo.new" "$B/repo" && echo "SYNC: 전체 번들로 새로 clone 후 교체"
+  else rm -rf "$B/repo.new"; echo "SYNC: 전체 번들 clone 실패(기존 저장소는 그대로 둠)"; fi
+fi
+has || { [ "$MODE" = inc ] && { echo "SYNC: 증분 번들로 부족 → 전체 번들 필요"; exit 10; }; bad "전체 번들을 적용해도 요청 기준 $T 가 없음"; }
+git -C "$B/repo" update-ref refs/heads/main "$T" 2>/dev/null
+J="$B/jobs/$N"
+mk(){ rm -rf "$J"; git -C "$B/repo" worktree prune; git -C "$B/repo" worktree add -q -B "feature/$N" "$J" "$T" 2>&1; }
+chk(){ [ -e "$J/.git" ] && [ "$(git -C "$J" rev-parse HEAD 2>/dev/null)" = "$T" ] && [ "$(git -C "$J" rev-parse --abbrev-ref HEAD 2>/dev/null)" = "feature/$N" ] && [ -z "$(git -C "$J" status --porcelain 2>/dev/null)" ]; }
+mk; chk || { echo "SYNC: job 폴더 점검 실패 → 1회 재생성"; mk; chk || bad "job 폴더 .git/HEAD 점검 실패"; }
+echo "SYNC_OK host=$(hostname -s) job=$J git=$([ -e "$J/.git" ] && echo yes) HEAD=$(git -C "$J" rev-parse HEAD) clean=$([ -z "$(git -C "$J" status --porcelain)" ] && echo yes || echo no)"
+PREP
+  return ${PIPESTATUS[0]}
+}
+if [ -z "$MACSHA" ]; then MODE=full; mkb full || fail3 "전체 번들 생성 실패"; sendb full
+elif [ "$MACSHA" = "$TARGET" ]; then MODE=inc; rm -f "$BF"
+elif git -C $W cat-file -e "$MACSHA^{commit}" 2>/dev/null && mkb inc; then MODE=inc; sendb inc
+else MODE=full; mkb full || fail3 "전체 번들 생성 실패"; sendb full; fi
+prep $MODE; rc=$?
+if [ $rc -eq 10 ]; then echo "[MACSYNC] 증분 실패 → 전체 번들로 폴백" | tee -a "$SYNCLOG"; mkb full || fail3 "전체 번들 생성 실패"; sendb full; prep full; rc=$?; fi
+[ $rc -eq 0 ] || fail3 "맥 저장소·job 폴더 준비 실패(rc=$rc), 로그 $SYNCLOG"
 CMODEL=$(sh C:/HWP_AUTODOCFIT/v1803/tools/codex_model.sh | cut -d' ' -f1)
-{ cat "$CARD"; cat <<'EOT'
+card_stream(){ cat "$CARD"; [ "$TOOL" = shell ] && return; cat <<'EOT'
 
 ## 맥 실행 환경(자동 첨부)
 - 이 맥에는 Windows 도구가 없다. 카드의 Windows 경로·PATH 지시·전체 Python 스위트·Flutter·Win32 크레이트(ktm·kvf·kagent·dock) 시험은 무시한다. 해당 Rust 크레이트와 관련 Python 모듈 시험만 실행한다. cargo는 `cargo test --manifest-path <크레이트>/Cargo.toml`, 타깃 폴더는 CARGO_TARGET_DIR=$HOME/cargo-target-<카드이름> 으로 지정한다(작업 폴더 밖).
 - 커밋하지 말 것(실행기가 한다). 보고서는 작업 폴더의 REPORT.md에 쓰고 마지막 답변에도 요약한다(모든 test result 줄 포함).
 EOT
-} | $S "set -e; B=\"$BASE\"; eval B=\"\$B\"; mkdir -p \"\$B/jobs\"
+}
+# 3) 카드 실행(저장소·job 폴더는 위에서 준비·점검 완료). 시작 직전 한 번 더 .git/HEAD 확인.
+card_stream | $S "set -e; B=\"$BASE\"; eval B=\"\$B\"
   export PATH=/opt/homebrew/bin:\$HOME/.cargo/bin:\$HOME/work/venv314/bin:\$PATH
-  if [ ! -f \$HOME/docfit-$N.bundle ]; then :; elif [ ! -d \"\$B/repo/.git\" ]; then git clone -q -b bundlemain \$HOME/docfit-$N.bundle \"\$B/repo\"; git -C \"\$B/repo\" branch -m main; else git -C \"\$B/repo\" fetch -q \$HOME/docfit-$N.bundle bundlemain:main --update-head-ok 2>/dev/null || git -C \"\$B/repo\" fetch -q \$HOME/docfit-$N.bundle bundlemain; git -C \"\$B/repo\" branch -f main FETCH_HEAD 2>/dev/null || true; fi
-  rm -f \$HOME/docfit-$N.bundle
-  J=\"\$B/jobs/$N\"; rm -rf \"\$J\"; git -C \"\$B/repo\" worktree prune; git -C \"\$B/repo\" worktree add -q -B feature/$N \"\$J\" main
+  J=\"\$B/jobs/$N\"
+  if [ ! -e \"\$J/.git\" ] || [ \"\$(git -C \"\$J\" rev-parse HEAD 2>/dev/null)\" != \"$TARGET\" ]; then echo 'MACSYNC: 카드 시작 직전 점검 실패(.git 없음 또는 HEAD 불일치)'; exit 3; fi
   cat > \"\$J/.card.md\"
   cd \"\$J\"
   if [ $TOOL = codex ]; then PJ=\$(cd \"\$J\" && pwd -P); cd \"\$PJ\"; codex exec -m $CMODEL -c model_reasoning_effort=$EFF -s workspace-write --skip-git-repo-check -C \"\$PJ\" - < \"\$PJ/.card.md\"
+  elif [ $TOOL = shell ]; then bash \"\$J/.card.md\"
   elif [ $TOOL = claude ]; then # Claude Code도 키체인 때문에 화면(Terminal)에서 켜 둔 ~/work/claude_worker.sh 를 통해서만 실행된다: ~/work/cjobs/<이름>.job (첫 줄=작업 폴더, 나머지=카드)
     mkdir -p \$HOME/work/cjobs; CJ=\$HOME/work/cjobs/$N
     rm -f \$CJ.done \$CJ.ran
@@ -44,8 +123,10 @@
   fi
   rm -f \"\$J/.card.md\"; git add -A; git diff --cached --quiet || git commit -qm \"$N: 맥($HOST/$TOOL) 작업 결과\"
   git log -1 --format='%h %s'" > "$R/$N.final.md" 2> "$R/$N-mac.log"
-echo "$N mac exit $?" >> /e/docfit-tmp/chain.log
-# 2) 결과 브랜치를 이 PC로 가져온다
+RC=$?
+echo "$N mac exit $RC" >> /e/docfit-tmp/chain.log
+# 4) 결과 브랜치를 이 PC로 가져온다
 U="ssh://hyeokjunkong@$IP/$(echo "$BASE" | sed 's/ /%20/g; s|\$HOME|/Users/hyeokjunkong|')/jobs/$N"
 GIT_SSH_COMMAND="ssh $K" git -C $W fetch -q "$U" "feature/$N:refs/heads/mac/$N" && echo "$N fetched mac/$N" >> /e/docfit-tmp/chain.log
-rm -f $BF
+[ $RC -eq 3 ] && exit 3
+exit 0
```
