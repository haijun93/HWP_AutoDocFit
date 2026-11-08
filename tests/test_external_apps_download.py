"""외부 프로그램 자동 내려받기·설치 코어 시험. 실제 인터넷·한/글 COM 없이, 주입식 opener로만 한다."""
import hashlib
import io
import json
import platform
import tempfile
import threading
import unittest
import zipfile
from pathlib import Path

from docfit_app import external_apps_download as dl
from docfit_app.external_apps_download import ExtDlError, ExternalAppInstaller, Source
from docfit_app.external_apps_service import APPS, ExternalAppsService
from docfit_app.settings_service import SettingsService

PHOTO = APPS["photo_album"]
OUTLINER = APPS["outliner"]
PICS = APPS["photo_manager"]


def make_zip(entries):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in entries.items():
            z.writestr(name, data)
    return buf.getvalue()


class FakeResp:
    def __init__(self, url, body, length="auto", on_read=None):
        self._url, self._body, self._pos, self._on_read = url, body, 0, on_read
        self.headers = {} if length is None else {"Content-Length": str(len(body) if length == "auto" else length)}
        self.closed = False

    def geturl(self):
        return self._url

    def read(self, n=-1):
        if self._on_read:
            self._on_read()
        chunk = self._body[self._pos:self._pos + (n if n > 0 else len(self._body))]
        self._pos += len(chunk)
        return chunk

    def close(self):
        self.closed = True


class FakeNet:
    """url → (본문, 최종 URL, 옵션). 호출 기록을 남긴다."""

    def __init__(self):
        self.routes, self.calls = {}, []

    def add(self, url, body, final=None, **kw):
        self.routes[url] = (body if isinstance(body, bytes) else body.encode(), final or url, kw)

    def __call__(self, url, headers=None):
        self.calls.append(url)
        if url not in self.routes:
            raise ExtDlError("HTTP_ERROR", "서버가 404 오류를 돌려주었습니다.")
        body, final, kw = self.routes[url]
        return FakeResp(final, body, **kw)


def release_json(name, url, sha=None, tag="v1.0", size=None):
    asset = {"name": name, "browser_download_url": url, "size": size or 0}
    if sha:
        asset["digest"] = "sha256:" + sha
    return json.dumps({"tag_name": tag, "assets": [asset]})


class UrlPolicyTests(unittest.TestCase):
    def test_pdfcraft_resolves_release_checksum_asset(self):
        payload = b"portable zip"
        sha = hashlib.sha256(payload).hexdigest()
        net = FakeNet()
        api = "https://api.github.com/repos/haijun93/mongdangpdf/releases/latest"
        sums_url = "https://github.com/haijun93/mongdangpdf/releases/download/v1/SHA256SUMS.txt"
        arch = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"
        asset_name = f"pdfcraft-1.2-windows-{arch}-portable.zip"
        zip_url = f"https://github.com/haijun93/mongdangpdf/releases/download/v1/{asset_name}"
        net.add(api, json.dumps({"tag_name":"v1", "assets":[
            {"name":"SHA256SUMS.txt", "browser_download_url":sums_url},
            {"name":asset_name, "browser_download_url":zip_url, "size":len(payload)}]}))
        net.add(sums_url, f"{sha}  {asset_name}\n")
        release = dl.resolve_github(dl.SOURCES["pdf_editor"][0], net)
        self.assertEqual(release.sha256, sha)
        self.assertEqual(release.asset, asset_name)

    def test_https_and_allowlist(self):
        dl.check_url("https://github.com/a/b")
        dl.check_url("https://download.kde.org/x")
        for bad, code in (("http://github.com/x", "URL_NOT_HTTPS"), ("https://evil.example/x", "HOST_BLOCKED"),
                          ("https://github.com.evil.example/x", "HOST_BLOCKED"), ("https://u:p@github.com/x", "URL_INVALID"),
                          ("https://github.com:8443/x", "URL_INVALID"), ("file:///etc/passwd", "URL_NOT_HTTPS"),
                          ("ftp://download.kde.org/x", "URL_NOT_HTTPS")):
            with self.assertRaises(ExtDlError) as ctx:
                dl.check_url(bad)
            self.assertEqual(ctx.exception.code, code, bad)

    def test_redirect_to_blocked_host_rejected(self):
        net = FakeNet()
        net.add("https://github.com/a/f.zip", b"x", final="https://mirror.evil.example/f.zip")
        with tempfile.TemporaryDirectory() as t, self.assertRaises(ExtDlError) as ctx:
            dl.download_file("https://github.com/a/f.zip", Path(t) / "f.zip", opener=net, max_bytes=100)
        self.assertEqual(ctx.exception.code, "HOST_BLOCKED")

    def test_redirect_within_allowlist_ok(self):
        net = FakeNet()
        net.add("https://github.com/a/f.zip", b"abc", final="https://objects.githubusercontent.com/f.zip")
        with tempfile.TemporaryDirectory() as t:
            sha, size = dl.download_file("https://github.com/a/f.zip", Path(t) / "f.zip", opener=net, max_bytes=100)
            self.assertEqual((sha, size), (hashlib.sha256(b"abc").hexdigest(), 3))
            self.assertEqual((Path(t) / "f.zip").read_bytes(), b"abc")
            self.assertFalse((Path(t) / "f.zip.part").exists())

    def test_default_redirect_handler_checks_every_hop(self):
        import urllib.request
        req = urllib.request.Request("https://github.com/a")
        handler = dl._StrictRedirect()
        with self.assertRaises(ExtDlError):
            handler.redirect_request(req, None, 302, "Found", {}, "http://objects.githubusercontent.com/x")
        with self.assertRaises(ExtDlError):
            handler.redirect_request(req, None, 302, "Found", {}, "https://mirror.example/x")


class DownloadTests(unittest.TestCase):
    def test_size_limit_by_header_and_by_stream(self):
        net = FakeNet()
        net.add("https://github.com/big", b"x" * 50)
        net.add("https://github.com/liar", b"x" * 50, length=None)  # 길이 헤더 없음
        with tempfile.TemporaryDirectory() as t:
            for name in ("big", "liar"):
                with self.assertRaises(ExtDlError) as ctx:
                    dl.download_file(f"https://github.com/{name}", Path(t) / name, opener=net, max_bytes=10)
                self.assertEqual(ctx.exception.code, "SIZE_EXCEEDED")
                self.assertEqual(list(Path(t).iterdir()), [])

    def test_incomplete_download_removed(self):
        net = FakeNet()
        net.add("https://github.com/cut", b"abc", length=10)
        with tempfile.TemporaryDirectory() as t, self.assertRaises(ExtDlError) as ctx:
            dl.download_file("https://github.com/cut", Path(t) / "f", opener=net, max_bytes=100)
        self.assertEqual(ctx.exception.code, "INCOMPLETE")

    def test_cancel_and_progress(self):
        net = FakeNet()
        net.add("https://github.com/f", b"y" * (dl.CHUNK * 3))
        cancel, seen = threading.Event(), []
        with tempfile.TemporaryDirectory() as t:
            def progress(got, total):
                seen.append((got, total))
                cancel.set()
            with self.assertRaises(ExtDlError) as ctx:
                dl.download_file("https://github.com/f", Path(t) / "f", opener=net, max_bytes=10 ** 7,
                                 progress=progress, cancel=cancel)
            self.assertEqual(ctx.exception.code, "CANCELLED")
            self.assertEqual(seen, [(dl.CHUNK, dl.CHUNK * 3)])
            self.assertEqual(list(Path(t).iterdir()), [])


class ZipTests(unittest.TestCase):
    def extract(self, entries):
        with tempfile.TemporaryDirectory() as t:
            z = Path(t) / "a.zip"
            z.write_bytes(make_zip(entries))
            return dl.extract_zip_safely(z, Path(t) / "out")

    def test_path_escape_rejected(self):
        for name in ("../evil.txt", "a/../../evil.txt", "/abs.txt", "C:/win.txt", "..\\evil.txt"):
            with self.assertRaises(ExtDlError, msg=name) as ctx:
                self.extract({name: b"x"})
            self.assertEqual(ctx.exception.code, "ZIP_UNSAFE", name)

    def test_bomb_rejected(self):
        with tempfile.TemporaryDirectory() as t:
            z = Path(t) / "bomb.zip"
            z.write_bytes(make_zip({"zeros.bin": b"\0" * (20 * 1024 * 1024)}))
            with self.assertRaises(ExtDlError) as ctx:
                dl.extract_zip_safely(z, Path(t) / "out")
            self.assertEqual(ctx.exception.code, "ZIP_UNSAFE")

    def test_symlink_entry_rejected(self):
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as z:
            info = zipfile.ZipInfo("link")
            info.external_attr = 0o120777 << 16
            z.writestr(info, "/etc/passwd")
        with tempfile.TemporaryDirectory() as t:
            p = Path(t) / "l.zip"
            p.write_bytes(buf.getvalue())
            with self.assertRaises(ExtDlError) as ctx:
                dl.extract_zip_safely(p, Path(t) / "out")
            self.assertEqual(ctx.exception.code, "ZIP_UNSAFE")

    def test_normal_zip(self):
        root = self.extract({"App/NeeView.exe": b"MZ", "App/lib/a.dll": b"d"})
        self.assertTrue(root is not None)


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "tools"
        self.net = FakeNet()
        self.zip = make_zip({"NeeView-1/NeeView.exe": b"MZ-exe", "NeeView-1/readme.txt": b"hi"})
        self.sha = hashlib.sha256(self.zip).hexdigest()

    def installer(self, **kw):
        return ExternalAppInstaller(self.root, opener=self.net, spawn=lambda fn: fn(), **kw)

    def add_photo(self, sha="auto", body=None):
        body = self.zip if body is None else body
        sha = self.sha if sha == "auto" else sha
        self.net.add("https://api.github.com/repos/haijun93/mongdangview/releases/latest",
                     release_json("mongdangview-x64.zip", "https://github.com/haijun93/mongdangview/releases/download/v1/mongdangview-x64.zip", sha))
        self.net.add("https://github.com/haijun93/mongdangview/releases/download/v1/mongdangview-x64.zip", body,
                     final="https://objects.githubusercontent.com/x/mongdangview-x64.zip")

    def test_install_zip_with_published_digest(self):
        self.add_photo()
        inst = self.installer()
        inst.install(PHOTO)
        exe = inst.installed_exe(PHOTO)
        self.assertEqual(exe, self.root / "mongdangview" / "NeeView-1" / "NeeView.exe")
        self.assertEqual(exe.read_bytes(), b"MZ-exe")
        state = json.loads((self.root / "mongdangview" / ".installed.json").read_text(encoding="utf-8"))
        self.assertEqual((state["trust"], state["sha256"], state["version"]), ("published", self.sha, "v1.0"))
        self.assertEqual([p.name for p in self.root.iterdir() if p.name.startswith((".stage", ".old"))], [])

    def test_hash_mismatch_installs_nothing(self):
        self.add_photo(sha="0" * 64)
        inst = self.installer()
        with self.assertRaises(ExtDlError) as ctx:
            inst.install(PHOTO)
        self.assertEqual(ctx.exception.code, "HASH_MISMATCH")
        self.assertFalse((self.root / "mongdangview").exists())
        self.assertEqual([p.name for p in self.root.iterdir()], [])

    def test_blocked_host_asset(self):
        self.net.add("https://api.github.com/repos/haijun93/mongdangview/releases/latest",
                     release_json("mongdangview-x64.zip", "https://evil.example/mongdangview-x64.zip", self.sha))
        with self.assertRaises(ExtDlError) as ctx:
            self.installer().install(PHOTO)
        self.assertEqual(ctx.exception.code, "HOST_BLOCKED")
        self.assertNotIn("https://evil.example/mongdangview-x64.zip", self.net.calls)

    def test_size_limit_enforced(self):
        self.add_photo()
        tiny = (Source("github", "haijun93/mongdangview", r".*\.zip", "zip", 10),)
        inst = self.installer(sources={"photo_album": tiny})
        with self.assertRaises(ExtDlError) as ctx:
            inst.install(PHOTO)
        self.assertEqual(ctx.exception.code, "SIZE_EXCEEDED")

    def test_zip_path_escape_installs_nothing(self):
        evil = make_zip({"../../evil.exe": b"x", "NeeView.exe": b"y"})
        self.add_photo(sha=hashlib.sha256(evil).hexdigest(), body=evil)
        with self.assertRaises(ExtDlError) as ctx:
            self.installer().install(PHOTO)
        self.assertEqual(ctx.exception.code, "ZIP_UNSAFE")
        self.assertFalse((self.root / "mongdangview").exists())
        self.assertFalse((self.root.parent / "evil.exe").exists())

    def test_zip_without_exe(self):
        z = make_zip({"a.txt": b"x"})
        self.add_photo(sha=hashlib.sha256(z).hexdigest(), body=z)
        with self.assertRaises(ExtDlError) as ctx:
            self.installer().install(PHOTO)
        self.assertEqual(ctx.exception.code, "EXE_NOT_FOUND")

    def test_fallback_to_upstream_when_fork_has_no_release(self):
        self.net.add("https://api.github.com/repos/neelabo/NeeView/releases/latest",
                     release_json("NeeView42.0.zip", "https://github.com/neelabo/NeeView/releases/download/v42/NeeView42.0.zip", self.sha))
        self.net.add("https://github.com/neelabo/NeeView/releases/download/v42/NeeView42.0.zip", self.zip)
        inst = self.installer()
        rel = inst.install(PHOTO)
        self.assertEqual(rel.asset, "NeeView42.0.zip")
        self.assertIsNotNone(inst.installed_exe(PHOTO))

    def test_tofu_records_then_detects_change(self):
        self.add_photo(sha=None)
        inst = self.installer()
        inst.install(PHOTO)
        state = json.loads((self.root / "mongdangview" / ".installed.json").read_text(encoding="utf-8"))
        self.assertEqual((state["trust"], state["sha256"]), ("tofu", self.sha))
        tofu = json.loads((self.root / "tofu.json").read_text(encoding="utf-8"))
        self.assertEqual(tofu["photo_album"]["mongdangview-x64.zip"]["sha256"], self.sha)
        inst.install(PHOTO)  # 같은 파일 다시 → 통과
        changed = make_zip({"NeeView.exe": b"tampered"})
        self.add_photo(sha=None, body=changed)
        with self.assertRaises(ExtDlError) as ctx:
            inst.install(PHOTO)
        self.assertEqual(ctx.exception.code, "HASH_MISMATCH")
        self.assertEqual(inst.installed_exe(PHOTO).read_bytes(), b"MZ-exe")  # 기존 설치 보존

    def test_upgrade_replaces_existing_install(self):
        self.add_photo()
        inst = self.installer()
        inst.install(PHOTO)
        new = make_zip({"NeeView.exe": b"v2"})
        self.add_photo(sha=hashlib.sha256(new).hexdigest(), body=new)
        inst.install(PHOTO)
        self.assertEqual(inst.installed_exe(PHOTO).read_bytes(), b"v2")
        self.assertFalse((self.root / "mongdangview" / "NeeView-1").exists())

    def test_cancel_during_download(self):
        self.add_photo()
        inst = self.installer()
        job = dl._Job()
        job.cancel.set()
        with self.assertRaises(ExtDlError) as ctx:
            inst.install(PHOTO, job)
        self.assertEqual(ctx.exception.code, "CANCELLED")
        self.assertFalse((self.root / "mongdangview").exists())
        self.assertEqual([p.name for p in self.root.iterdir()], [])

    def test_nsis_silent_args_and_fallback_order(self):
        exe = b"installer"
        sha = hashlib.sha256(exe).hexdigest()
        # zip 자산이 없고 exe만 있는 릴리스 → 두 번째 출처(NSIS)로
        self.net.add("https://api.github.com/repos/logseq/logseq/releases/latest",
                     release_json("Logseq-win-x64-2.0.2.exe", "https://github.com/logseq/logseq/releases/download/2.0.2/Logseq-win-x64-2.0.2.exe", sha, tag="2.0.2"))
        self.net.add("https://github.com/logseq/logseq/releases/download/2.0.2/Logseq-win-x64-2.0.2.exe", exe)
        calls = []

        def runner(args, cancel):
            calls.append(args)
            dest = Path(args[2][3:])
            dest.mkdir(parents=True)
            (dest / "Logseq.exe").write_bytes(b"MZ")
            return 0
        inst = self.installer(runner=runner)
        inst.install(OUTLINER)
        self.assertEqual(calls[0][1], "/S")
        self.assertEqual(calls[0][2], f"/D={self.root / 'mongdangoutliner'}")
        self.assertEqual(calls[0][2][:3], "/D=")
        self.assertEqual(inst.installed_exe(OUTLINER).name, "Logseq.exe")

    def test_nsis_failure(self):
        exe = b"installer"
        self.net.add("https://api.github.com/repos/logseq/logseq/releases/latest",
                     release_json("Logseq-win-x64-2.0.2.exe", "https://github.com/logseq/logseq/releases/download/2.0.2/Logseq-win-x64-2.0.2.exe", hashlib.sha256(exe).hexdigest()))
        self.net.add("https://github.com/logseq/logseq/releases/download/2.0.2/Logseq-win-x64-2.0.2.exe", exe)
        with self.assertRaises(ExtDlError) as ctx:
            self.installer(runner=lambda a, c: 2).install(OUTLINER)
        self.assertEqual(ctx.exception.code, "INSTALL_FAILED")

    def test_kde_nsis_silent_with_sha256_file(self):
        body = b"digikam-installer"
        sha = hashlib.sha256(body).hexdigest()
        base = "https://download.kde.org/stable/digikam/"
        name = "digiKam-9.1.0-Qt6-Win64.exe"
        self.net.add(base, '<a href="8.9.0/">8.9.0/</a><a href="9.1.0/">9.1.0/</a><a href="../">..</a>')
        self.net.add(base + "9.1.0/", f'<a href="{name}">x</a><a href="{name}.sha256">s</a><a href="digiKam-9.1.0-Win64.exe">q5</a>')
        self.net.add(base + f"9.1.0/{name}.sha256", f"{sha}  {name}\n")
        self.net.add(base + f"9.1.0/{name}", body, final="https://download.kde.org/stable/digikam/9.1.0/" + name)
        calls = []
        def runner(args, cancel):
            calls.append(args)
            dest = Path(args[2][3:])
            dest.mkdir(parents=True)
            (dest / "digikam.exe").write_bytes(b"MZ")
            return 0
        inst = self.installer(runner=runner)
        rel = inst.install(PICS)
        self.assertEqual((rel.version, rel.sha256), ("9.1.0", sha))
        self.assertEqual(calls[0][1], "/S")
        self.assertEqual(calls[0][2], f"/D={self.root / 'mondangpics'}")
        self.assertIsNotNone(inst.installed_exe(PICS))

    def test_kde_mirror_outside_allowlist_blocked_and_hash_mismatch(self):
        base = "https://download.kde.org/stable/digikam/"
        name = "digiKam-9.1.0-Qt6-Win64.exe"
        self.net.add(base, '<a href="9.1.0/">9.1.0/</a>')
        self.net.add(base + "9.1.0/", f'<a href="{name}">x</a>')
        self.net.add(base + f"9.1.0/{name}.sha256", "0" * 64)
        self.net.add(base + f"9.1.0/{name}", b"data", final="https://mirror.example.org/" + name)
        with self.assertRaises(ExtDlError) as ctx:
            self.installer(runner=lambda a, c: None).install(PICS)
        self.assertEqual(ctx.exception.code, "HOST_BLOCKED")
        self.net.add(base + f"9.1.0/{name}", b"data")
        with self.assertRaises(ExtDlError) as ctx:
            self.installer(runner=lambda a, c: None).install(PICS)
        self.assertEqual(ctx.exception.code, "HASH_MISMATCH")


class ServiceCommandTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name).resolve()  # macOS /var 링크 회피(validate_exe는 링크 경로 거절)
        (root / "app").mkdir()
        self.net = FakeNet()
        self.installer = ExternalAppInstaller(root / "tools", opener=self.net, spawn=lambda fn: fn())
        self.service = ExternalAppsService(SettingsService(root / "settings.json"), folders=lambda: [root / "app"],
                                           installer=self.installer)

    def test_install_progress_status_flow(self):
        z = make_zip({"NeeView.exe": b"MZ"})
        self.net.add("https://api.github.com/repos/haijun93/mongdangview/releases/latest",
                     release_json("m.zip", "https://github.com/haijun93/mongdangview/releases/download/v1/m.zip", hashlib.sha256(z).hexdigest()))
        self.net.add("https://github.com/haijun93/mongdangview/releases/download/v1/m.zip", z)
        first = self.service.command("install", {"app": "photo_album"})
        self.assertTrue(first["ok"] and not first["skipped"])
        prog = self.service.command("progress", {"app": "mongdangview"})  # 폴더 이름으로도
        self.assertEqual((prog["install"]["phase"], prog["install"]["ok"], prog["available"]), ("done", True, True))
        self.assertEqual(prog["source"], "downloaded")
        calls = len(self.net.calls)
        again = self.service.command("install", {"app": "photo_album"})
        self.assertTrue(again["skipped"])
        self.assertEqual(len(self.net.calls), calls)  # 이미 있으면 네트워크 접근 없음
        self.assertTrue(self.service.command("status", {"app": "photo_album"})["available"])

    def test_install_error_reported_in_progress(self):
        self.service.command("install", {"app": "outliner"})  # 경로 없음 → 404
        prog = self.service.command("progress", {"app": "outliner"})["install"]
        self.assertEqual((prog["phase"], prog["ok"], prog["code"]), ("error", False, "ASSET_NOT_FOUND"))

    def test_cancel_when_idle_and_unknown_app(self):
        self.assertFalse(self.service.command("cancel", {"app": "outliner"})["install"]["cancel_requested"])
        self.assertEqual(self.service.command("install", {"app": "nope"})["code"], "UNKNOWN_APP")

    def test_cancel_running_job(self):
        pending = []
        installer = ExternalAppInstaller(Path(self.tmp.name) / "t2", opener=self.net, spawn=pending.append)
        service = ExternalAppsService(SettingsService(Path(self.tmp.name) / "s2.json"),
                                      folders=lambda: [Path(self.tmp.name) / "app"], installer=installer)
        service.command("install", {"app": "outliner"})
        self.assertEqual(service.command("progress", {"app": "outliner"})["install"]["phase"], "starting")
        self.assertTrue(service.command("cancel", {"app": "outliner"})["install"]["cancel_requested"])
        self.assertTrue(service.command("install", {"app": "outliner"})["install"]["already_running"])


if __name__ == "__main__":
    unittest.main()
