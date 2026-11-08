"""외부 프로그램 3종 자동 내려받기·설치 코어(화면 없음). 설계·한계는 docs/EXTERNAL_APPS.md §5.

- 출처 목록 SOURCES(데이터): 앱마다 우선순위 순 출처. 앞 출처에서 자산을 못 찾으면 다음 출처.
- 받기: HTTPS만, 호스트 허용 목록(리디렉션 단계마다 검사), 크기 상한, 진행률, 취소, 임시 파일 후 교체.
- 무결성: 출처가 공개한 SHA-256(GitHub API digest·KDE .sha256)이 있으면 대조, 없으면 TOFU(처음 받은 해시 저장 후 비교).
- 설치: %LOCALAPPDATA%\\HWP_AutoDocFit\\tools\\<이름>\\ (관리자 권한 불필요). zip은 open_zip_safely로 검사 후 안전 풀기.
- 네트워크·설치 프로그램 실행은 주입식(opener, runner, popen) — 시험은 인터넷 없이 한다.
"""
from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import shutil
import stat
import subprocess
import threading
import time
import urllib.error
import urllib.request
import uuid
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from urllib.parse import quote, urljoin, urlsplit

from docfit_core.hwpx import HwpxLimits, HwpxSecurityError, open_zip_safely

# 허용 호스트. 리디렉션 도착지도 여기 안에 있어야 한다(없으면 HOST_BLOCKED).
# api.github.com·release-assets.githubusercontent.com은 릴리스 조회·현행 GitHub 자산 리디렉션에 필요해 더했다.
ALLOWED_HOSTS = frozenset({
    "github.com", "api.github.com", "objects.githubusercontent.com",
    "release-assets.githubusercontent.com", "download.kde.org",
})
MAX_API_BYTES = 5 * 1024 * 1024
CHUNK = 64 * 1024
TIMEOUT = 30
USER_AGENT = "HWP_AutoDocFit-external-app-installer"
ZIP_LIMITS = HwpxLimits(max_entries=50_000, max_total_size=4 * 1024 ** 3,
                        max_file_size=2 * 1024 ** 3, max_compression_ratio=1_000)
STATE_FILE = ".installed.json"
_SHA_RE = re.compile(r"\b([0-9a-fA-F]{64})\b")


class ExtDlError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message


# ── 출처 목록(데이터) ────────────────────────────────────────────────────
@dataclass(frozen=True)
class Source:
    kind: str            # github | kde
    where: str           # github: "owner/repo" · kde: 목록 URL(https://download.kde.org/stable/digikam/)
    pattern: str         # 자산 파일 이름 정규식(전체 일치)
    package: str         # zip | nsis_silent(/S /D=경로) | installer_interactive(설치 프로그램을 띄워 사용자가 진행)
    max_bytes: int
    note: str = ""


MB = 1024 * 1024
SOURCES = {
    "photo_album": (
        Source("github", "haijun93/mongdangview", r"(?i)(?!.*(?:fd|framework)).*\.zip", "zip", 400 * MB, "포크 휴대용 zip"),
        Source("github", "neelabo/NeeView", r"(?i)NeeView[^/]*\.zip", "zip", 400 * MB, "원본 휴대용 zip"),
    ),
    "outliner": (
        Source("github", "logseq/logseq", r"(?i)Logseq-win-x64-[^/]*\.zip", "zip", 800 * MB, "휴대용 zip"),
        Source("github", "logseq/logseq", r"(?i)Logseq-win-x64-[^/]*\.exe", "nsis_silent", 600 * MB, "NSIS 설치 exe"),
    ),
    "photo_manager": (
        Source("kde", "https://download.kde.org/stable/digikam/", r"digiKam-\d+(?:\.\d+)*-Qt6-Win64\.exe",
               "installer_interactive", 1500 * MB, "KDE 설치본(무인 옵션 미확인 → 사용자가 진행)"),
    ),
    "pdf_editor": (
        Source("github", "haijun93/mongdangpdf", r"(?i)pdfcraft-[0-9][^/]*windows-(?:x64|arm64)-portable\.zip", "zip", 800 * MB, "몽당PDF 포크 휴대용 zip"),
        Source("github", "storytold/pdfcraft", r"(?i)pdfcraft-[0-9][^/]*windows-(?:x64|arm64)-portable\.zip", "zip", 800 * MB, "원본 PDFCraft 휴대용 zip"),
    ),
}


@dataclass
class Release:
    version: str
    asset: str
    url: str
    size: int | None
    sha256: str | None   # 출처가 공개한 값(없으면 None → TOFU)
    source: Source


# ── 주소 검사·받기 ───────────────────────────────────────────────────────
def check_url(url, hosts=ALLOWED_HOSTS):
    try:
        parts = urlsplit(url)
        host = (parts.hostname or "").lower().rstrip(".")
        port = parts.port
    except ValueError:
        raise ExtDlError("URL_INVALID", "내려받기 주소가 올바르지 않습니다.") from None
    if parts.scheme != "https":
        raise ExtDlError("URL_NOT_HTTPS", "HTTPS 주소만 내려받습니다.")
    if parts.username or parts.password or port not in (None, 443):
        raise ExtDlError("URL_INVALID", "내려받기 주소가 올바르지 않습니다.")
    if host not in hosts:
        raise ExtDlError("HOST_BLOCKED", f"허용 목록에 없는 호스트입니다: {host or '(없음)'}")
    return url


class _StrictRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        check_url(urljoin(req.full_url, newurl))
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def default_opener(url, headers=None):
    """응답 객체(read·geturl·headers·close)를 돌려준다. 리디렉션 단계마다 허용 목록을 검사한다."""
    opener = urllib.request.build_opener(_StrictRedirect)
    request = urllib.request.Request(url, headers=dict(headers or {}), method="GET")
    try:
        return opener.open(request, timeout=TIMEOUT)
    except urllib.error.HTTPError as exc:
        raise ExtDlError("HTTP_ERROR", f"서버가 {exc.code} 오류를 돌려주었습니다.") from None
    except ExtDlError:
        raise
    except (urllib.error.URLError, OSError, ValueError) as exc:
        raise ExtDlError("NETWORK_ERROR", f"네트워크 연결에 실패했습니다: {exc}") from None


def _open_checked(url, opener, max_bytes, headers=None):
    check_url(url)
    sent = {"User-Agent": USER_AGENT}
    sent.update(headers or {})
    resp = opener(url, sent)
    try:
        check_url(resp.geturl())
        raw = resp.headers.get("Content-Length") if getattr(resp, "headers", None) is not None else None
        length = int(raw) if raw not in (None, "") else None
    except BaseException:
        resp.close()
        raise
    if length is not None and length > max_bytes:
        resp.close()
        raise ExtDlError("SIZE_EXCEEDED", f"파일이 너무 큽니다({length:,} > {max_bytes:,}바이트).")
    return resp, length


def read_limited(url, opener, max_bytes=MAX_API_BYTES, headers=None):
    resp, _length = _open_checked(url, opener, max_bytes, headers)
    try:
        data = bytearray()
        while True:
            chunk = resp.read(CHUNK)
            if not chunk:
                return bytes(data)
            data += chunk
            if len(data) > max_bytes:
                raise ExtDlError("SIZE_EXCEEDED", "응답이 너무 큽니다.")
    finally:
        resp.close()


def download_file(url, dest, *, opener, max_bytes, progress=None, cancel=None):
    """url → dest. 임시 파일(.part)에 받고 끝까지 받은 뒤 교체. (sha256, 바이트 수)를 돌려준다."""
    dest = Path(dest)
    resp, length = _open_checked(url, opener, max_bytes)
    tmp = dest.with_name(dest.name + ".part")
    digest = hashlib.sha256()
    got = 0
    try:
        with open(tmp, "wb") as out:
            while True:
                if cancel is not None and cancel.is_set():
                    raise ExtDlError("CANCELLED", "취소했습니다.")
                chunk = resp.read(CHUNK)
                if not chunk:
                    break
                got += len(chunk)
                if got > max_bytes:
                    raise ExtDlError("SIZE_EXCEEDED", f"파일이 너무 큽니다(> {max_bytes:,}바이트).")
                out.write(chunk)
                digest.update(chunk)
                if progress is not None:
                    progress(got, length)
        if length is not None and got != length:
            raise ExtDlError("INCOMPLETE", "내려받기가 중간에 끊겼습니다.")
        os.replace(tmp, dest)
    except BaseException:
        try:
            tmp.unlink()
        except OSError:
            pass
        raise
    finally:
        resp.close()
    return digest.hexdigest(), got


# ── 최신판 찾기 ──────────────────────────────────────────────────────────
def _version_key(text):
    return tuple(int(p) for p in text.split("."))


def resolve_github(source, opener):
    url = f"https://api.github.com/repos/{source.where}/releases/latest"
    try:
        data = json.loads(read_limited(url, opener, headers={"Accept": "application/vnd.github+json"}))
    except ValueError:
        raise ExtDlError("RELEASE_INVALID", "릴리스 정보를 읽지 못했습니다.") from None
    if not isinstance(data, dict) or not isinstance(data.get("assets"), list):
        raise ExtDlError("RELEASE_INVALID", "릴리스 정보를 읽지 못했습니다.")
    pattern = re.compile(source.pattern)
    assets = data["assets"]
    # PDFCraft는 릴리스에 첨부된 SHA256SUMS.txt로 개별 ZIP을 검증한다.
    sums = {}
    if source.where in ("haijun93/mongdangpdf", "storytold/pdfcraft"):
        sums_asset = next((a for a in assets if isinstance(a, dict) and a.get("name") == "SHA256SUMS.txt"), None)
        if sums_asset and isinstance(sums_asset.get("browser_download_url"), str):
            try:
                raw = read_limited(sums_asset["browser_download_url"], opener, 1024 * 1024).decode("ascii", "strict")
                for line in raw.splitlines():
                    match = re.fullmatch(r"\s*([0-9a-fA-F]{64})\s+[* ]?([^\s]+)\s*", line)
                    if match:
                        sums[match.group(2)] = match.group(1).lower()
            except (UnicodeError, ExtDlError):
                sums = {}
    for asset in assets:
        name = asset.get("name") if isinstance(asset, dict) else None
        link = asset.get("browser_download_url") if isinstance(asset, dict) else None
        if not isinstance(name, str) or not isinstance(link, str) or "/" in name or not pattern.fullmatch(name):
            continue
        if source.where in ("haijun93/mongdangpdf", "storytold/pdfcraft"):
            arch = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"
            if f"windows-{arch}-portable.zip" not in name.lower():
                continue
        digest = asset.get("digest")
        sha = digest[7:].lower() if isinstance(digest, str) and digest.lower().startswith("sha256:") \
            and re.fullmatch(r"[0-9a-fA-F]{64}", digest[7:]) else None
        size = asset.get("size") if isinstance(asset.get("size"), int) else None
        sha = sha or sums.get(name)
        return Release(str(data.get("tag_name") or ""), name, link, size, sha, source)
    raise ExtDlError("ASSET_NOT_FOUND", f"{source.where} 최신 릴리스에서 받을 파일을 찾지 못했습니다.")


def resolve_kde(source, opener):
    base = source.where if source.where.endswith("/") else source.where + "/"
    listing = read_limited(base, opener).decode("utf-8", "replace")
    versions = sorted({m for m in re.findall(r'href="(\d+(?:\.\d+)+)/"', listing)}, key=_version_key, reverse=True)
    pattern = re.compile(source.pattern)
    for version in versions[:3]:
        folder = f"{base}{version}/"
        try:
            names = re.findall(r'href="([^"/?]+)"', read_limited(folder, opener).decode("utf-8", "replace"))
        except ExtDlError as exc:
            if exc.code in ("HTTP_ERROR",):
                continue
            raise
        for name in names:
            if pattern.fullmatch(name):
                sha = None
                try:
                    found = _SHA_RE.search(read_limited(f"{folder}{quote(name)}.sha256", opener, 4096).decode("ascii", "replace"))
                    sha = found.group(1).lower() if found else None
                except ExtDlError as exc:
                    if exc.code != "HTTP_ERROR":
                        raise
                return Release(version, name, f"{folder}{quote(name)}", None, sha, source)
    raise ExtDlError("ASSET_NOT_FOUND", "KDE 배포처에서 받을 설치 파일을 찾지 못했습니다.")


_RESOLVERS = {"github": resolve_github, "kde": resolve_kde}


def resolve_release(sources, opener):
    last = None
    for source in sources:
        try:
            return _RESOLVERS[source.kind](source, opener)
        except ExtDlError as exc:
            if exc.code in ("HOST_BLOCKED", "URL_INVALID", "URL_NOT_HTTPS", "CANCELLED"):
                raise
            last = exc
    raise last or ExtDlError("ASSET_NOT_FOUND", "받을 출처가 없습니다.")


# ── 안전 풀기 ────────────────────────────────────────────────────────────
def extract_zip_safely(zip_path, dest, limits=ZIP_LIMITS, cancel=None):
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    root = dest.resolve()
    try:
        archive = open_zip_safely(zip_path, limits)
    except HwpxSecurityError as exc:
        raise ExtDlError("ZIP_UNSAFE", f"압축 파일이 안전하지 않습니다: {exc}") from None
    written = 0
    with archive:
        for info in archive.infolist():
            if cancel is not None and cancel.is_set():
                raise ExtDlError("CANCELLED", "취소했습니다.")
            name = info.filename.replace("\\", "/")
            mode = info.external_attr >> 16
            if stat.S_ISLNK(mode):
                raise ExtDlError("ZIP_UNSAFE", f"링크 항목은 풀지 않습니다: {info.filename}")
            parts = PurePosixPath(name).parts
            if PurePosixPath(name).is_absolute() or ".." in parts or (parts and ":" in parts[0]):
                raise ExtDlError("ZIP_UNSAFE", f"경로 탈출 항목입니다: {info.filename}")
            target = (root / Path(*parts)).resolve() if parts else root
            if target != root and root not in target.parents:
                raise ExtDlError("ZIP_UNSAFE", f"경로 탈출 항목입니다: {info.filename}")
            if info.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(info) as src, open(target, "wb") as out:
                while True:
                    chunk = src.read(CHUNK)
                    if not chunk:
                        break
                    written += len(chunk)
                    if written > limits.max_total_size:
                        raise ExtDlError("ZIP_UNSAFE", "풀린 크기가 제한을 넘었습니다.")
                    out.write(chunk)
    return root


def find_exe_under(folder, exe_name, max_depth=3):
    folder = Path(folder)
    if not folder.is_dir():
        return None
    base = len(folder.parts)
    want = exe_name.casefold()
    for current, dirs, files in os.walk(folder):
        if len(Path(current).parts) - base >= max_depth:
            dirs[:] = []
        for name in sorted(files):
            if name.casefold() == want:
                return Path(current) / name
    return None


# ── 설치기 ───────────────────────────────────────────────────────────────
def default_tools_root():
    base = os.environ.get("LOCALAPPDATA") or str(Path.home() / "AppData" / "Local")
    return Path(base) / "HWP_AutoDocFit" / "tools"


def default_runner(args, cancel):
    """설치 프로그램을 실행하고 끝날 때까지 기다린다(취소하면 종료). 반환: 종료 코드."""
    proc = subprocess.Popen(args, shell=False, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)
    while True:
        try:
            return proc.wait(timeout=0.5)
        except subprocess.TimeoutExpired:
            if cancel.is_set():
                proc.terminate()
                raise ExtDlError("CANCELLED", "취소했습니다.") from None


def default_popen(args):
    flags = getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
    return subprocess.Popen(args, shell=False, close_fds=True, creationflags=flags, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def _thread_spawn(fn):
    thread = threading.Thread(target=fn, name="external-app-install", daemon=True)
    thread.start()
    return thread


class _Job:
    def __init__(self):
        self.cancel = threading.Event()
        self.lock = threading.Lock()
        self.state = dict(phase="starting", running=True, ok=None, downloaded=0, total=None, percent=None,
                          code=None, message="", version="", asset="")

    def set(self, **kw):
        with self.lock:
            self.state.update(kw)

    def snapshot(self):
        with self.lock:
            return dict(self.state)


class ExternalAppInstaller:
    def __init__(self, tools_root=None, *, opener=default_opener, runner=default_runner, popen=default_popen,
                 spawn=_thread_spawn, sources=None, clock=time.time):
        self.tools_root = Path(tools_root) if tools_root else default_tools_root()
        self._opener = opener
        self._runner = runner
        self._popen = popen
        self._spawn = spawn
        self._sources = sources if sources is not None else SOURCES
        self._clock = clock
        self._jobs = {}
        self._guard = threading.Lock()

    # 위치·상태
    def install_dir(self, app):
        return self.tools_root / app.folder

    def installed_exe(self, app):
        """설치된 실행 파일 후보(검증 전). 상태 파일 → 바로 아래 → 얕은 탐색 순."""
        folder = self.install_dir(app)
        try:
            state = json.loads((folder / STATE_FILE).read_text(encoding="utf-8"))
            rel = state.get("exe_rel")
            if isinstance(rel, str) and rel:
                candidate = (folder / rel)
                if folder.resolve() in candidate.resolve().parents:
                    return candidate
        except (OSError, ValueError, AttributeError):
            pass
        return find_exe_under(folder, app.exe_name)

    def progress(self, app):
        job = self._jobs.get(app.app_id)
        return job.snapshot() if job else dict(phase="idle", running=False, ok=None, downloaded=0, total=None,
                                                percent=None, code=None, message="", version="", asset="")

    def cancel(self, app):
        job = self._jobs.get(app.app_id)
        if job is None or not job.snapshot()["running"]:
            return dict(self.progress(app), cancel_requested=False)
        job.cancel.set()
        return dict(job.snapshot(), cancel_requested=True)

    def start(self, app):
        """백그라운드 설치를 시작한다. 이미 돌고 있으면 그 상태를 돌려준다."""
        with self._guard:
            job = self._jobs.get(app.app_id)
            if job is not None and job.snapshot()["running"]:
                return dict(job.snapshot(), already_running=True)
            job = self._jobs[app.app_id] = _Job()
        self._spawn(lambda: self._run(app, job))
        return job.snapshot()

    def _run(self, app, job):
        try:
            self.install(app, job)
            job.set(running=False, ok=True, phase=job.snapshot()["phase"] if job.snapshot()["phase"] == "installer_launched" else "done",
                    percent=100, code=None, message="설치를 마쳤습니다." if job.snapshot()["phase"] != "installer_launched"
                    else "설치 프로그램을 열었습니다. 안내에 따라 설치를 마쳐 주세요.")
        except ExtDlError as exc:
            cancelled = exc.code == "CANCELLED"
            job.set(running=False, ok=False, phase="cancelled" if cancelled else "error", code=exc.code, message=exc.message)
        except Exception as exc:  # 예상 밖 오류도 화면에 상태로 남긴다
            job.set(running=False, ok=False, phase="error", code="INSTALL_FAILED", message=f"설치하지 못했습니다: {exc}")

    # 본체(동기). 시험은 이것을 직접 부를 수 있다.
    def install(self, app, job=None):
        job = job or _Job()
        sources = self._sources.get(app.app_id)
        if not sources:
            raise ExtDlError("NO_SOURCE", "받을 출처가 등록되어 있지 않습니다.")
        job.set(phase="resolving", message="최신판을 확인하는 중…")
        release = resolve_release(sources, self._opener)
        source = release.source
        job.set(phase="downloading", version=release.version, asset=release.asset, message="내려받는 중…",
                total=release.size)
        self.tools_root.mkdir(parents=True, exist_ok=True)
        stage = self.tools_root / f".stage-{app.folder}-{uuid.uuid4().hex[:8]}"
        stage.mkdir()
        keep_stage = False
        try:
            archive = stage / release.asset

            def on_progress(got, total):
                job.set(downloaded=got, total=total, percent=int(got * 100 / total) if total else None)
            sha, _size = download_file(release.url, archive, opener=self._opener, max_bytes=source.max_bytes,
                                       progress=on_progress, cancel=job.cancel)
            job.set(phase="verifying", message="무결성을 확인하는 중…")
            trust = self._verify(app, release, sha)
            if source.package == "zip":
                job.set(phase="extracting", message="압축을 푸는 중…")
                self._install_zip(app, release, archive, sha, trust, stage, job)
            elif source.package == "nsis_silent":
                job.set(phase="installing", message="설치하는 중…")
                self._install_nsis(app, release, archive, sha, trust, job)
            else:
                keep_stage = self._launch_interactive(app, release, archive, sha, trust, stage, job)
        finally:
            if not keep_stage:
                shutil.rmtree(stage, ignore_errors=True)
        return release

    # 무결성: 공개 해시 → 대조, 없으면 TOFU
    def _tofu_path(self):
        return self.tools_root / "tofu.json"

    def _read_tofu(self):
        try:
            data = json.loads(self._tofu_path().read_text(encoding="utf-8"))
            return data if isinstance(data, dict) else {}
        except (OSError, ValueError):
            return {}

    def _verify(self, app, release, actual):
        tofu = self._read_tofu()
        known = tofu.setdefault(app.app_id, {})
        if not isinstance(known, dict):
            known = tofu[app.app_id] = {}
        if release.sha256:
            if release.sha256 != actual:
                raise ExtDlError("HASH_MISMATCH", "내려받은 파일의 SHA-256이 공개된 값과 다릅니다. 설치하지 않았습니다.")
            trust = "published"
        else:
            first = known.get(release.asset)
            if isinstance(first, dict) and first.get("sha256") not in (None, actual):
                raise ExtDlError("HASH_MISMATCH", "처음 받았을 때의 해시와 다릅니다(같은 이름의 파일이 바뀜). 설치하지 않았습니다.")
            trust = "tofu"
        known.setdefault(release.asset, {"sha256": actual, "first_seen": int(self._clock())})
        try:
            self._tofu_path().write_text(json.dumps(tofu, ensure_ascii=False, indent=1), encoding="utf-8")
        except OSError:
            pass
        return trust

    def _write_state(self, app, release, sha, trust, folder, exe, extra=None):
        state = dict(app=app.app_id, version=release.version, asset=release.asset, sha256=sha, trust=trust,
                     package=release.source.package, exe_rel=exe.relative_to(folder).as_posix(),
                     installed_at=int(self._clock()))
        state.update(extra or {})
        (folder / STATE_FILE).write_text(json.dumps(state, ensure_ascii=False, indent=1), encoding="utf-8")

    def _swap_in(self, app, built):
        """built 폴더를 설치 위치로 교체(기존은 .old로 옮겼다 지운다)."""
        dest = self.install_dir(app)
        old = None
        if dest.exists():
            old = self.tools_root / f".old-{app.folder}-{uuid.uuid4().hex[:8]}"
            os.replace(dest, old)
        try:
            os.replace(built, dest)
        except OSError:
            if old is not None:
                os.replace(old, dest)
            raise
        if old is not None:
            shutil.rmtree(old, ignore_errors=True)
        return dest

    def _install_zip(self, app, release, archive, sha, trust, stage, job):
        built = stage / "extract"
        extract_zip_safely(archive, built, cancel=job.cancel)
        exe = find_exe_under(built, app.exe_name)
        if exe is None:
            raise ExtDlError("EXE_NOT_FOUND", f"압축 안에서 {app.exe_name}을(를) 찾지 못했습니다.")
        self._write_state(app, release, sha, trust, built, exe)
        self._swap_in(app, built)

    def _install_nsis(self, app, release, archive, sha, trust, job):
        dest = self.install_dir(app)
        if dest.exists():
            shutil.rmtree(dest, ignore_errors=True)
        # NSIS: /D=는 반드시 마지막이고 따옴표 없이 쓴다.
        code = self._runner([str(archive), "/S", f"/D={dest}"], job.cancel)
        exe = find_exe_under(dest, app.exe_name)
        if code not in (0, None) or exe is None:
            raise ExtDlError("INSTALL_FAILED", f"무인 설치가 끝나지 않았습니다(종료 코드 {code}).")
        self._write_state(app, release, sha, trust, dest, exe)

    def _launch_interactive(self, app, release, archive, sha, trust, stage, job):
        """무인 옵션이 확인되지 않은 설치 프로그램: 검증한 파일을 띄우고 사용자가 진행. stage를 남긴다."""
        dest = self.install_dir(app)
        keep = self.tools_root / "_installers" / app.folder
        shutil.rmtree(keep, ignore_errors=True)
        keep.mkdir(parents=True)
        final = keep / release.asset
        os.replace(archive, final)
        job.set(phase="installer_launched")
        # /D=는 NSIS 공통 '설치 폴더 미리 지정'(무인이 아님). 설치 프로그램이 무시하면 사용자가 폴더를 고른다.
        self._popen([str(final), f"/D={dest}"])
        (keep / "verified.json").write_text(json.dumps(dict(asset=release.asset, sha256=sha, trust=trust,
                                                            version=release.version), indent=1), encoding="utf-8")
        return False
