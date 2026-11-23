"""INSTALL3 시험: 실행 중(잠긴) 색인 프로그램이 있어도 업그레이드가 롤백되지 않는지 확인한다.

사용: python scripts/install3_locked_exe_test.py --iscc <ISCC.exe 경로> --work <빈 임시 폴더> [--keep]

- 더미 dist(ping.exe 를 kfind.exe 등으로 복사 + 표지 바이트)로 시험용 설치 프로그램 3벌(v1·v2·v3)을 만든다.
- 실제 PC 를 건드리지 않도록 시험용 사본 setup.iss 만 아래처럼 바꾼다(검증 대상 [Code] 로직은 그대로):
  AppId·AppName 교체(제거 정보·시작 메뉴 바로 가기 충돌 방지), install.log 위치를 설치 폴더 옆으로,
  StopHelper 의 taskkill 대상 이름을 바꿔 사용자 프로세스를 죽이지 않게 함(시험용 잠금 프로세스도 살려 두기 위함).
- 설치는 모두 --work 아래 /DIR= 로만 한다.
"""
import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PING = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32" / "PING.EXE"


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()[:12]


def make_exe(dst: Path, marker: str):
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(PING.read_bytes() + b"\0MARK:" + marker.encode())


def build_setup(iscc: str, work: Path, ver: str, files: dict) -> Path:
    """files: 상대경로(_internal/resources 기준) -> 표지 문자열(exe/dll 은 ping 복사, 그 밖은 텍스트)."""
    root = work / f"build_{ver}"
    if root.exists():
        shutil.rmtree(root)
    (root / "installer").mkdir(parents=True)
    (root / "resources").mkdir()
    shutil.copy(ROOT / "resources" / "app_icon.ico", root / "resources" / "app_icon.ico")
    iss = (ROOT / "installer" / "setup.iss").read_text(encoding="utf-8")
    iss = iss.replace("{A7D3B5B8-824B-4F97-A33D-C208F6E895F4}", "{A7D3B5B8-0000-4F97-A33D-C208F6E895F4}")
    iss = iss.replace('#define AppName "한글편집 후처리 (HWP_AutoDocFit)"', '#define AppName "INSTALL3 TEST"')
    iss = iss.replace(r"ExpandConstant('{localappdata}\HwpAutoDocFit\logs')", r"ExpandConstant('{app}\..\logs')")
    iss = iss.replace("'/F /IM ' + ExeName", "'/F /IM nonexistent-test-' + ExeName")
    assert "nonexistent-test-" in iss and r"{app}\..\logs" in iss, "시험용 치환 실패"
    (root / "installer" / "setup.iss").write_text(iss, encoding="utf-8")
    dist = root / "dist" / "HWP_AutoDocFit"
    make_exe(dist / "HWP_AutoDocFit.exe", "app")
    res = dist / "_internal" / "resources"
    for name, marker in files.items():
        if name.lower().endswith((".exe", ".dll")):
            make_exe(res / name, f"{name}-{marker}")
        else:
            res.mkdir(parents=True, exist_ok=True)
            (res / name).write_text(f"{name}-{marker}", encoding="utf-8")
    env = dict(os.environ, DOCFIT_VERSION=ver)
    r = subprocess.run([iscc, str(root / "installer" / "setup.iss")], env=env, capture_output=True, text=True,
                       encoding="utf-8", errors="replace")
    tail = [ln for ln in r.stdout.splitlines() if ln.strip()][-3:]
    print(f"[ISCC v{ver}] rc={r.returncode} :: " + " | ".join(tail))
    if r.returncode != 0:
        print(r.stdout[-2000:], r.stderr[-1000:])
        sys.exit(2)
    return root / "dist" / f"HWP_AutoDocFit_v{ver}_Setup.exe"


def run_setup(setup: Path, target: Path, logfile: Path) -> int:
    r = subprocess.run([str(setup), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", f"/DIR={target}",
                        f"/LOG={logfile}"], capture_output=True, text=True)
    return r.returncode


def lock_by_running(exe: Path):
    """exe 를 실제로 실행해 잠근다(ping 이 오래 도는 채)."""
    return subprocess.Popen([str(exe), "-n", "300", "127.0.0.1"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def lock_file_exclusive(path: Path):
    ps = ("$f=[IO.File]::Open('%s','Open','Read','None'); Start-Sleep 300" % str(path).replace("'", "''"))
    p = subprocess.Popen(["powershell", "-NoProfile", "-Command", ps], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2.5)
    return p


def listing(res: Path):
    return sorted(f"{p.name}[{sha(p)}]" if p.suffix.lower() in (".exe", ".dll") else p.name for p in res.iterdir())


def check(results, name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(("PASS " if cond else "FAIL ") + name + (" :: " + detail if detail else ""))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--iscc", required=True)
    ap.add_argument("--work", required=True)
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()
    work = Path(a.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    target = work / "inst"
    res = target / "_internal" / "resources"
    results = []
    procs = []
    base = {"kfind.exe": "", "klib.exe": "", "tool.dll": "", "data.bin": ""}
    s1 = build_setup(a.iscc, work, "9.0.1", {**base, "kfind.exe": "v1", "klib.exe": "v1", "tool.dll": "v1",
                                             "data.bin": "v1", "legacy.exe": "v1"})
    s2 = build_setup(a.iscc, work, "9.0.2", {"kfind.exe": "v2", "klib.exe": "v2", "tool.dll": "v2", "data.bin": "v2"})
    s3 = build_setup(a.iscc, work, "9.0.3", {"kfind.exe": "v3", "klib.exe": "v3", "tool.dll": "v3", "data.bin": "v3"})
    s4 = build_setup(a.iscc, work, "9.0.4", {"kfind.exe": "v4", "klib.exe": "v4", "tool.dll": "v4", "data.bin": "v4"})
    try:
        # (가) 1차 설치
        rc = run_setup(s1, target, work / "setup1.log")
        check(results, "A 1차 설치 종료 코드 0", rc == 0, f"rc={rc} files={listing(res)}")
        k1 = sha(res / "kfind.exe")

        # (나) kfind.exe·legacy.exe 를 실행해 잠근 채 2차 설치(v2 꾸러미에는 legacy.exe 없음)
        pk = lock_by_running(res / "kfind.exe")
        pl = lock_by_running(res / "legacy.exe")
        procs += [pk, pl]
        time.sleep(1.5)
        rc = run_setup(s2, target, work / "setup2.log")
        ls = listing(res)
        olds = [f for f in ls if ".old-" in f]
        check(results, "B 잠긴 채 업그레이드 종료 코드 0", rc == 0, f"rc={rc}")
        check(results, "B 새 kfind.exe 가 원래 이름으로 설치", (res / "kfind.exe").exists() and sha(res / "kfind.exe") != k1,
              f"new={sha(res / 'kfind.exe')} old={k1}")
        check(results, "B 옛 kfind.exe 는 .old-* 로 남음(실행 중이라 못 지움)", any(f.startswith("kfind.exe.old-") for f in ls), str(olds))
        check(results, "B 새 꾸러미에 없는 legacy.exe 는 원래 이름으로 되돌아옴", (res / "legacy.exe").exists()
              and not any(f.startswith("legacy.exe.old-") for f in ls), str(ls))
        check(results, "B 실행 중 프로세스 2개 계속 동작", pk.poll() is None and pl.poll() is None,
              f"kfind={pk.poll()} legacy={pl.poll()}")
        check(results, "B 잠기지 않은 klib.exe·tool.dll 은 .old 없이 교체", not any(f.startswith(("klib.exe.old", "tool.dll.old")) for f in ls), str(ls))
        log = (target.parent / "logs" / "install.log")
        check(results, "B install.log 에 서비스 재시작 안내 기록",
              log.exists() and "서비스를 다시 시작해야 적용됩니다" in log.read_text(encoding="cp949", errors="replace"))
        pl.kill()
        pl.wait()

        # (다) 일부러 실패: kfind.exe 실행(잠금) + data.bin 독점 잠금 → 파일 교체 중단·롤백
        k2 = sha(res / "kfind.exe")
        d2 = (res / "data.bin").read_text(encoding="utf-8")
        pk.kill()
        pk.wait()
        time.sleep(0.5)
        pk3 = lock_by_running(res / "kfind.exe")
        procs.append(pk3)
        lk = lock_file_exclusive(res / "data.bin")
        procs.append(lk)
        rc = run_setup(s3, target, work / "setup3.log")
        ls = listing(res)
        check(results, "C 잠금 때문에 설치 중단(종료 코드 0 아님)", rc != 0, f"rc={rc}")
        check(results, "C kfind.exe 가 원래 이름·원래 내용으로 복원", (res / "kfind.exe").exists() and sha(res / "kfind.exe") == k2, f"{ls}")
        check(results, "C 복원 뒤 kfind.exe.old-* 가 새로 남지 않음(앞선 B 의 잠금 잔여 제외)",
              sum(f.startswith("kfind.exe.old-") for f in ls) <= 1, str([f for f in ls if ".old-" in f]))
        check(results, "C 잠긴 kfind.exe 프로세스 계속 동작", pk3.poll() is None)
        lk.kill()
        lk.wait()
        time.sleep(1)
        check(results, "C data.bin 내용 불변", (res / "data.bin").read_text(encoding="utf-8") == d2)
        pk3.kill()
        pk3.wait()
        time.sleep(1.5)

        # (라) 아무것도 잠기지 않은 일반 업그레이드: 이전과 같은 결과(.old 없음, 새 파일, 앞선 .old 정리)
        rc = run_setup(s4, target, work / "setup4.log")
        ls = listing(res)
        check(results, "D 일반 업그레이드 종료 코드 0", rc == 0, f"rc={rc}")
        check(results, "D .old-* 없음(앞선 잔여도 정리)", not any(".old-" in f for f in ls), str(ls))
        check(results, "D 새 파일로 교체", (res / "data.bin").read_text(encoding="utf-8") == "data.bin-v4", str(ls))
        text = log.read_text(encoding="cp949", errors="replace")
        print("---- install.log ----\n" + text)
    finally:
        for p in procs:
            try:
                p.kill()
            except Exception:
                pass
        if not a.keep:
            time.sleep(1)
            shutil.rmtree(work, ignore_errors=True)
    bad = [r for r in results if not r[1]]
    print(f"\n결과: {len(results) - len(bad)}/{len(results)} 통과")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
