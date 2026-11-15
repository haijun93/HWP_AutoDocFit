"""잠금 파일에서 제3자 구성요소·라이선스 목록을 만든다(2026-11-15).

대상: 배포에 들어가는 Rust 크레이트(Cargo.lock → cargo metadata), Node 꾸러미(package-lock.json),
Python 패키지(requirements-lock.txt → PyPI 정보), Flutter 패키지(pubspec.lock, 라이선스는 표시만).
결과: <출력 폴더>/third-party-inventory.md, third-party-inventory.csv.
GPL·LGPL·AGPL·MPL·라이선스 미상 항목은 따로 모아 보여 준다(배포 전 사람이 확인).
네트워크: cargo metadata(크레이트 정보)와 PyPI JSON을 읽는다. CI(ubuntu)에서 실행한다.

사용: python scripts/third_party_inventory.py --out dist/third-party
"""
from __future__ import annotations

import argparse
import csv
import json
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# 설치본에 들어가는 Rust 실행 파일·DLL의 매니페스트(벤치마크용 크레이트는 뺀다).
RUST_MANIFESTS = [
    "klib/Cargo.toml", "kagent/Cargo.toml", "kagent/ffi/Cargo.toml", "hierarchy/Cargo.toml",
    "ktm/Cargo.toml", "kvf/Cargo.toml", "crates/kfind/Cargo.toml", "editor_core/Cargo.toml",
]
NPM_LOCKS = ["kordoc_pack/package-lock.json", "kordoc_ocr_pack/package-lock.json", "diagram_editor/package-lock.json"]
PY_LOCK = "requirements-lock.txt"
PUB_LOCK = "flutter_ui/pubspec.lock"
WINDOWS_TARGET = "x86_64-pc-windows-msvc"
REVIEW = re.compile(r"\b(A?GPL|LGPL|MPL|EUPL|CDDL|SSPL|BUSL)", re.IGNORECASE)


def rust_rows(manifest: str) -> list[dict]:
    done = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--manifest-path", str(ROOT / manifest),
         "--filter-platform", WINDOWS_TARGET],
        capture_output=True, text=True, encoding="utf-8", check=True)
    data = json.loads(done.stdout)
    used = {node["id"] for node in (data.get("resolve") or {}).get("nodes", [])}
    rows = []
    for package in data["packages"]:
        if package.get("source") is None or package["id"] not in used:
            continue  # 저장소 안 크레이트(path)와 Windows 빌드에 안 쓰는 것은 뺀다
        rows.append({"ecosystem": "rust", "name": package["name"], "version": package["version"],
                     "license": package.get("license") or ("파일: " + package["license_file"] if package.get("license_file") else ""),
                     "used_by": manifest.rsplit("/", 1)[0], "scope": "runtime"})
    return rows


def npm_rows(lock: str) -> list[dict]:
    path = ROOT / lock
    if not path.is_file():
        return []
    data = json.loads(path.read_text(encoding="utf-8"))
    rows = []
    for key, info in (data.get("packages") or {}).items():
        if not key:
            continue
        name = info.get("name") or key.split("node_modules/")[-1]
        license_value = info.get("license") or ""
        if isinstance(license_value, dict):
            license_value = license_value.get("type", "")
        other_platform = (info.get("os") and "win32" not in info["os"]) or (info.get("cpu") and "x64" not in info["cpu"])
        scope = "dev" if info.get("dev") else ("other-platform" if other_platform else "runtime")
        if not license_value and scope == "runtime":
            license_value = npm_license(name, info.get("version", ""))
        rows.append({"ecosystem": "npm", "name": name, "version": info.get("version", ""),
                     "license": str(license_value), "used_by": lock.split("/")[0], "scope": scope})
    return rows


def npm_license(name: str, version: str) -> str:
    """잠금 파일에 라이선스가 없으면 npm 레지스트리에서 그 버전의 license 값을 읽는다."""
    try:
        with urllib.request.urlopen(f"https://registry.npmjs.org/{name}/{version}", timeout=30) as response:  # noqa: S310
            value = json.load(response).get("license") or ""
    except Exception as exc:
        return f"(조회 실패: {type(exc).__name__})"
    return value.get("type", "") if isinstance(value, dict) else str(value)


def pypi_license(name: str, version: str) -> str:
    url = f"https://pypi.org/pypi/{name}/{version}/json"
    try:
        with urllib.request.urlopen(url, timeout=30) as response:  # noqa: S310 - 고정 주소(PyPI)
            info = json.load(response)["info"]
    except Exception as exc:  # 네트워크 실패는 목록에 이유를 남기고 계속한다
        return f"(조회 실패: {type(exc).__name__})"
    expression = info.get("license_expression")
    if expression:
        return expression
    classifiers = [c.split("::")[-1].strip() for c in info.get("classifiers") or [] if c.startswith("License ::")]
    if classifiers:
        return " / ".join(classifiers)
    text = (info.get("license") or "").strip()
    return text.splitlines()[0][:80] if text else ""


def python_rows() -> list[dict]:
    rows = []
    for line in (ROOT / PY_LOCK).read_text(encoding="utf-8").splitlines():
        match = re.match(r"^([A-Za-z0-9_.\-]+)==([^\s;\\]+)", line)
        if match:
            name, version = match.groups()
            rows.append({"ecosystem": "python", "name": name, "version": version,
                         "license": pypi_license(name, version), "used_by": "app",
                         "scope": "build" if name.lower() in {"pyinstaller", "pyinstaller-hooks-contrib", "altgraph",
                                                               "pefile", "playwright", "greenlet", "pyee", "setuptools",
                                                               "packaging"} else "runtime"})
    return rows


def pub_rows() -> list[dict]:
    path = ROOT / PUB_LOCK
    if not path.is_file():
        return []
    rows, name = [], None
    for line in path.read_text(encoding="utf-8").splitlines():
        if re.match(r"^  [A-Za-z0-9_]+:$", line):
            name = line.strip()[:-1]
            rows.append({"ecosystem": "flutter", "name": name, "version": "", "license": "(pub.dev 확인 필요)",
                         "used_by": "flutter_ui", "scope": "runtime"})
        elif name and line.strip().startswith("version:"):
            rows[-1]["version"] = line.split(":", 1)[1].strip().strip('"')
        elif name and line.strip().startswith("dependency:") and "dev" in line:
            rows[-1]["scope"] = "dev"
    return rows


def merge(rows: list[dict]) -> list[dict]:
    """같은 구성요소(생태계·이름·버전)는 한 줄로 합치고 쓰는 곳을 모은다."""
    merged: dict[tuple, dict] = {}
    for row in rows:
        key = (row["ecosystem"], row["name"], row["version"])
        if key in merged:
            users = set(merged[key]["used_by"].split(", ")) | {row["used_by"]}
            merged[key]["used_by"] = ", ".join(sorted(users))
            if row["scope"] == "runtime":
                merged[key]["scope"] = "runtime"
        else:
            merged[key] = dict(row)
    return sorted(merged.values(), key=lambda r: (r["ecosystem"], r["name"].lower(), r["version"]))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default="dist/third-party")
    parser.add_argument("--skip-network", action="store_true", help="PyPI 조회를 건너뛴다(시험용)")
    args = parser.parse_args()
    rows: list[dict] = []
    for manifest in RUST_MANIFESTS:
        rows += rust_rows(manifest)
    if args.skip_network:
        global pypi_license, npm_license  # noqa: PLW0603 - 시험에서만 바꾼다
        pypi_license = npm_license = lambda name, version: "(조회 생략)"  # noqa: E731
    for lock in NPM_LOCKS:
        rows += npm_rows(lock)
    rows += python_rows()
    rows += pub_rows()
    rows = merge(rows)

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    with (out / "third-party-inventory.csv").open("w", encoding="utf-8-sig", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=["ecosystem", "name", "version", "license", "scope", "used_by"])
        writer.writeheader()
        writer.writerows(rows)

    # Flutter는 pubspec.lock에 라이선스가 없어 따로 안내하고(대부분 BSD-3-Clause), 개발용·다른 OS 전용은 뺀다.
    review = [r for r in rows if r["scope"] == "runtime" and r["ecosystem"] != "flutter"
              and (not r["license"] or r["license"].startswith(("(조회", "(pub.dev")) or REVIEW.search(r["license"]))]
    counts: dict[str, int] = {}
    for row in rows:
        counts[row["ecosystem"]] = counts.get(row["ecosystem"], 0) + 1
    lines = ["# 제3자 구성요소 목록(자동 생성)", "",
             "잠금 파일 기준. 법적 고지는 `THIRD_PARTY_NOTICES.md`를 따른다. 이 목록은 점검용이다.", "",
             "| 생태계 | 개수 |", "|---|---|"]
    lines += [f"| {k} | {v} |" for k, v in sorted(counts.items())]
    lines += ["", "Flutter 패키지 라이선스는 잠금 파일에 없어 이 목록에서 판정하지 않는다(`flutter pub deps`와 각 패키지 LICENSE로 확인).",
              "", f"## 사람이 확인할 항목 ({len(review)}개: 카피레프트·라이선스 미상, 개발용 제외)", "",
              "| 생태계 | 이름 | 버전 | 라이선스 | 쓰는 곳 |", "|---|---|---|---|---|"]
    lines += [f"| {r['ecosystem']} | {r['name']} | {r['version']} | {r['license'] or '(없음)'} | {r['used_by']} |"
              for r in review]
    lines += ["", "## 전체 목록", "", "| 생태계 | 이름 | 버전 | 라이선스 | 범위 | 쓰는 곳 |", "|---|---|---|---|---|---|"]
    lines += [f"| {r['ecosystem']} | {r['name']} | {r['version']} | {r['license'] or '(없음)'} | {r['scope']} | {r['used_by']} |"
              for r in rows]
    (out / "third-party-inventory.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"구성요소 {len(rows)}개, 확인 필요 {len(review)}개 → {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
