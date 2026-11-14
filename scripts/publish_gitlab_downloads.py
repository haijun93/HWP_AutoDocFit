"""GitLab `downloads` 브랜치에 배포 파일을 올린다(공공기관 PC는 GitHub가 막혀 있음, 2026-10-22 사용자 요청).

사용: .venv\\Scripts\\python.exe scripts\\publish_gitlab_downloads.py 2.2.57

GitHub 릴리스에서 포터블·설치용 exe와 SHA256SUMS를 받아 해시를 확인하고,
GitLab 1회 푸시 용량 제한(약 100MB)에 맞춰 80MB 이하 단위로 분할하여 커밋별 순차 푸시한다.
각 푸시마다 서버 보안 검사 5종(Gitleaks, Semgrep, OSV-Scanner, Trivy, Syft) PASS를 확인한다.
배포 브랜치에는 join.cmd, join.ps1, latest.json, SHA256SUMS, README.md가 함께 제공되어
수신 측에서 무설치로 원본 EXE를 재결합하고 무결성을 자동 검증할 수 있다.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GITHUB = "https://github.com/haijun93/hwp-auto-docfit/releases/download/v{version}/{name}"
GITLAB = "https://gitlab.aigov.go.kr/haijun93/hwp_autodocfit.git"
EXAMPLE = ROOT / "release-assets" / "[모의데이터]문서편집_작업방식(한번에적용) 기능시연용문서.txt"
PART_SIZE = 80 * 1024 * 1024  # 80 MiB
SECURITY_CHECKS = ["Gitleaks", "Semgrep", "OSV-Scanner", "Trivy", "Syft"]


def _git(work: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-c", "core.autocrlf=false", *args],
        cwd=work,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=True,
    )


def _verify_security_scan(output: str) -> None:
    for check in SECURITY_CHECKS:
        if not re.search(re.escape(check) + r"\s+PASS", output):
            raise RuntimeError(f"GitLab 서버 보안 검사 미통과 ({check}):\n{output}")


def main(version: str) -> int:
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise SystemExit("버전은 1.80.4처럼 숫자 세 개로 주세요.")

    names = [f"HWP_AutoDocFit_v{version}_Portable.exe", f"HWP_AutoDocFit_v{version}_Setup.exe", "SHA256SUMS"]
    work = Path(tempfile.mkdtemp(prefix="docfit_gitlab_downloads_"))
    try:
        print(f"=== v{version} GitHub 릴리스 자산 내려받기 ===")
        for name in names:
            url = GITHUB.format(version=version, name=name)
            print(f"내려받는 중: {name}")
            subprocess.run(
                ["curl", "--fail", "--location", "--proto", "=https", "--silent", "--show-error",
                 "--max-time", "900", "--output", str(work / name), url],
                check=True,
            )

        sys.path.insert(0, str(ROOT))
        from docfit_app.update_service import extract_notes, parse_sha256sums

        expected = parse_sha256sums((work / "SHA256SUMS").read_text(encoding="utf-8-sig"))
        for name in names[:2]:
            actual = hashlib.sha256((work / name).read_bytes()).hexdigest()
            if expected.get(name) != actual:
                raise SystemExit(f"SHA-256 불일치: {name} (기대 {expected.get(name)} vs 실제 {actual})")
            print(f"SHA-256 확인 완료: {name} ({actual})")

        note_path = ROOT / "releases" / f"v{version}.md"
        notes = extract_notes(note_path.read_text(encoding="utf-8")) if note_path.exists() else []

        repo_dir = work / "repo"
        repo_dir.mkdir(parents=True, exist_ok=True)

        # 80MiB 단위 분할
        base_raw_url = f"https://gitlab.aigov.go.kr/haijun93/hwp_autodocfit/-/raw/downloads/"
        manifest = {
            "version": version,
            "distribution": "raw-split",
            "part_size": PART_SIZE,
            "files": {},
            "notes": notes,
        }
        sums_lines: list[str] = []
        ordered_parts: list[str] = []

        for kind, name in zip(("portable", "setup"), names[:2]):
            file_path = work / name
            whole_hash = hashlib.sha256()
            parts_info = []
            with file_path.open("rb") as stream:
                part_idx = 1
                while True:
                    chunk = stream.read(PART_SIZE)
                    if not chunk:
                        break
                    whole_hash.update(chunk)
                    part_name = f"{name}.part{part_idx:03d}"
                    part_file = repo_dir / part_name
                    part_file.write_bytes(chunk)
                    part_digest = hashlib.sha256(chunk).hexdigest()
                    parts_info.append({
                        "name": part_name,
                        "size": len(chunk),
                        "sha256": part_digest,
                        "url": f"{base_raw_url}{part_name}?inline=false",
                    })
                    sums_lines.append(f"{part_digest}  {part_name}")
                    ordered_parts.append(part_name)
                    print(f"  분할 파트 생성: {part_name} ({len(chunk):,} B, {part_digest})")
                    part_idx += 1

            assert whole_hash.hexdigest() == expected[name], f"분할 검증 실패: {name}"
            manifest["files"][kind] = {
                "name": name,
                "size": file_path.stat().st_size,
                "sha256": expected[name],
                "parts": parts_info,
                "requires_join": True,
            }
            sums_lines.append(f"{expected[name]}  {name}")

        (repo_dir / "latest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        (repo_dir / "SHA256SUMS").write_text("\n".join(sums_lines) + "\n", encoding="ascii")
        if EXAMPLE.exists():
            shutil.copyfile(EXAMPLE, repo_dir / EXAMPLE.name)

        # join.ps1 & join.cmd 생성
        join_ps1_tmpl = r'''param(
    [ValidateSet('Portable','Setup','All')][string]$Kind = 'Portable',
    [switch]$VerifyOnly
)
$ErrorActionPreference = 'Stop'
$Manifest = @'
__MANIFEST__
'@ | ConvertFrom-Json
function Assert-Hash($Path, $Expected, $Size) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing file: $Path" }
    if ((Get-Item -LiteralPath $Path).Length -ne $Size) { throw "Wrong size: $Path" }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Expected) {
        throw "SHA256 mismatch: $Path"
    }
}
try {
    $Kinds = @($Kind.ToLowerInvariant())
    if ($Kind -eq 'All') { $Kinds = @('portable','setup') }
    foreach ($Selected in $Kinds) {
        $File = $Manifest.files.$Selected
        $Destination = Join-Path $PSScriptRoot $File.name
        if ($VerifyOnly -or (Test-Path -LiteralPath $Destination)) {
            Assert-Hash $Destination $File.sha256 $File.size
            Write-Host "VERIFIED $($File.name) SHA256 $($File.sha256)"
            continue
        }
        foreach ($Part in $File.parts) {
            Assert-Hash (Join-Path $PSScriptRoot $Part.name) $Part.sha256 $Part.size
        }
        $Temporary = $Destination + '.joining'
        $Output = [System.IO.File]::Open($Temporary, [System.IO.FileMode]::CreateNew)
        try {
            foreach ($Part in $File.parts) {
                $InputStream = [System.IO.File]::OpenRead((Join-Path $PSScriptRoot $Part.name))
                try { $InputStream.CopyTo($Output) } finally { $InputStream.Dispose() }
            }
        } finally { $Output.Dispose() }
        Assert-Hash $Temporary $File.sha256 $File.size
        [System.IO.File]::Move($Temporary, $Destination)
        Write-Host "VERIFIED $($File.name) SHA256 $($File.sha256)"
    }
    exit 0
} catch {
    Write-Error -ErrorAction Continue $_
    Write-Host 'FAILED. Do not run an unverified EXE. Check downloads; remove any .joining file before retrying.'
    exit 1
}
'''.replace("__MANIFEST__", json.dumps(manifest, ensure_ascii=True, indent=2))
        (repo_dir / "join.ps1").write_text(join_ps1_tmpl, encoding="ascii")
        (repo_dir / "join.cmd").write_bytes(
            b'@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0join.ps1" %*\r\nexit /b %errorlevel%\r\n'
        )

        # downloads/README.md 생성
        links: list[str] = []
        copies: list[str] = []
        hashes: list[str] = []
        for kind, f in manifest["files"].items():
            part_bullets = "\n".join(f"- [{p['name']}]({p['url']}) — {p['size']:,} B" for p in f["parts"])
            links.append(f"### {kind.title()} ({f['size']:,} B)\n\n{part_bullets}")
            copies.append('copy /b ' + '+'.join('"' + p["name"] + '"' for p in f["parts"]) + f' "{f["name"]}"')
            hashes.append(f"- `{f['name']}`: `{f['sha256']}`")

        notes_bullets = "\n".join(f"- {n}" for n in manifest["notes"]) if manifest["notes"] else "- 정기 업데이트"

        readme_text = f"""# 한글편집 후처리 v{version} 내려받기 (GitLab)

**원하는 종류의 파트 {len(manifest['files']['portable']['parts'])}개와 join.ps1·join.cmd를 같은 폴더에 받은 뒤 `join.cmd Portable` 또는 `join.cmd Setup`을 실행하세요. 별도 설치 없이 원본 EXE로 합치고 SHA256을 자동 검증합니다.**

서버의 1회 Git 푸시 용량 제한에 맞춘 단순 바이너리 분할입니다. 파트를 직접 실행하지 마세요. 포터블은 설치 없이 실행하며 Setup은 설치형입니다. 두 종류를 모두 받았으면 `join.cmd All`도 가능합니다.

## 내려받을 파일

- [join.cmd]({base_raw_url}join.cmd?inline=false)
- [join.ps1]({base_raw_url}join.ps1?inline=false)
- [SHA256SUMS]({base_raw_url}SHA256SUMS?inline=false) — 파트 및 원본 EXE의 SHA256
- [latest.json]({base_raw_url}latest.json?inline=false) — 버전, 크기, 파트, 해시, 릴리스 노트 요약

{chr(10).join(links)}

## 합치기 및 검증

탐색기에서 해당 폴더를 열어 주소창에 `cmd`를 입력하고 다음 중 하나를 실행합니다. 인자 없이 join.cmd를 실행하면 Portable을 합칩니다. 창에 `VERIFIED`가 표시되고 종료 코드가 0이면 완료입니다. 스크립트는 파일을 자동 실행하지 않습니다.

```bat
join.cmd Portable
join.cmd Setup
```

PowerShell에서 직접 합치는 방법:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\\join.ps1 -Kind Portable
```

Windows 기본 `copy /b`로 직접 합치는 방법(명령 프롬프트에서 실행, 이후 반드시 검증):

```bat
{chr(10).join(copies)}
join.cmd Portable -VerifyOnly
join.cmd Setup -VerifyOnly
```

개별 해시를 직접 확인하려면 `certutil -hashfile HWP_AutoDocFit_v{version}_Portable.exe SHA256` 또는 PowerShell의 `Get-FileHash -Algorithm SHA256 .\\HWP_AutoDocFit_v{version}_Portable.exe` 결과를 SHA256SUMS와 대조합니다. 설치형은 파일명을 Setup으로 바꿉니다. 원본 해시는 GitHub v{version} 릴리스와 동일합니다.

{chr(10).join(hashes)}

누락·크기·해시 불일치 시 스크립트는 실패 코드 1로 멈춥니다. 정상 원본 EXE가 이미 있으면 검증만 수행하며, 해시가 다른 기존 EXE를 덮어쓰지 않습니다. 중단 후 남은 `.joining` 파일은 삭제하고 다시 합치세요. 실행 정책을 기관에서 강제한 PC는 기관 정책을 따르고, 허용되는 경우 위 `copy /b`와 `certutil` 명령으로 합치고 검증할 수 있습니다.

## v{version} 변경 요약

{notes_bullets}

이 브랜치는 배포 파일 전용입니다. 파트별 커밋과 푸시 이력을 유지해야 합니다. 다음 버전은 새 이력으로 교체하되 한 번에 전체 파트를 푸시하지 마세요.
"""
        (repo_dir / "README.md").write_text(readme_text, encoding="utf-8")

        # 로컬 재결합 자가 시험
        print("=== 로컬 분할 파일 재결합 및 무결성 사전 검증 ===")
        test_run = subprocess.run(
            ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(repo_dir / "join.ps1"), "-Kind", "All"],
            cwd=repo_dir,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        if test_run.returncode != 0:
            raise RuntimeError(f"join.ps1 사전 시험 실패:\n{test_run.stdout}\n{test_run.stderr}")
        print("  재결합 시험 성공:", test_run.stdout.strip())

        # 재결합된 임시 exe 파일 삭제 (원격에는 part 파일들만 푸시)
        for name in names[:2]:
            joined_file = repo_dir / name
            if joined_file.exists():
                joined_file.unlink()

        # Git 초기화 및 커밋별 순차 푸시
        print(f"=== GitLab downloads 브랜치 순차 푸시 (총 {len(ordered_parts)}개 커밋) ===")
        _git(repo_dir, "init", "-q", "-b", "downloads")
        _git(repo_dir, "config", "user.name", "마포구청 공혁준")
        _git(repo_dir, "config", "user.email", "mapo4good@mapo.go.kr")

        total_parts = len(ordered_parts)
        for i, part_name in enumerate(ordered_parts, 1):
            stage_files = [part_name]
            if i == total_parts:
                stage_files += ["README.md", "latest.json", "join.ps1", "join.cmd", "SHA256SUMS"]
                if (repo_dir / EXAMPLE.name).exists():
                    stage_files.append(EXAMPLE.name)

            _git(repo_dir, "add", "--", *stage_files)
            _git(repo_dir, "commit", "-q", "-m", f"v{version} 분할 배포 {i}/{total_parts}: {part_name}")
            commit_sha = _git(repo_dir, "rev-parse", "HEAD").stdout.strip()
            print(f"[{i}/{total_parts}] 커밋 생성: {part_name} ({commit_sha[:8]}) -> 푸시 시작...")

            push_args = ["-c", "http.postBuffer=1048576000", "push"]
            if i == 1:
                push_args.append("--force")
            push_args += [GITLAB, "HEAD:refs/heads/downloads"]

            push_res = _git(repo_dir, *push_args)
            push_log = (push_res.stdout or "") + (push_res.stderr or "")
            _verify_security_scan(push_log)
            print(f"  푸시 완료 및 보안 검사 5종 PASS 확인 ({i}/{total_parts})")

        final_sha = _git(repo_dir, "rev-parse", "HEAD").stdout.strip()
        print(f"\nGitLab downloads 브랜치에 v{version} 배포 파일 업로드가 완료되었습니다. (최종 커밋: {final_sha})")
        return 0

    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("사용: publish_gitlab_downloads.py <버전>")
    raise SystemExit(main(sys.argv[1]))
