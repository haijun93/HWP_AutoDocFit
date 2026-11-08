"""외부 프로그램(사진첩·아웃라이너·사진관리) 공용 실행기 시험: 찾기·경로 검증·인자 목록 실행·개발자 모드 제한."""
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from docfit_app import external_apps_service as eas
from docfit_app.api_contract import ServiceError
from docfit_app.external_apps_download import ExternalAppInstaller
from docfit_app.external_apps_service import APPS, ExternalAppsService, validate_exe
from docfit_app.native_file_dialog_service import PURPOSES
from docfit_app.settings_service import SettingsService

PHOTO = APPS["photo_album"]
OUTLINER = APPS["outliner"]
PICS = APPS["photo_manager"]


class FakeDialogs:
    def __init__(self, picked=None, cancelled=False):
        self.picked = picked
        self.cancelled = cancelled
        self.calls = []

    def select_files(self, request_id, purpose, mode="open", multiple=False):
        self.calls.append((purpose, mode, multiple))
        if self.cancelled:
            return {"ok": True, "cancelled": True}
        return {"ok": True, "cancelled": False, "selection_id": "sel-1"}

    def resolve_selection(self, selection_id, purpose):
        assert selection_id == "sel-1" and purpose == eas.DIALOG_PURPOSE
        return [self.picked]


class RegistryTests(unittest.TestCase):
    def test_registered_apps_have_renamed_fork_urls(self):
        self.assertEqual(list(APPS), ["photo_album", "outliner", "photo_manager", "pdf_editor"])
        self.assertEqual([a.label for a in APPS.values()], ["사진첩", "아웃라이너", "사진관리", "PDF편집"])
        self.assertEqual([a.folder for a in APPS.values()], ["mongdangview", "mongdangoutliner", "mondangpics", "mongdangpdf"])
        for app in APPS.values():
            self.assertEqual(app.fork_url, f"https://github.com/haijun93/{app.folder}")
            self.assertTrue(app.official_url.startswith("https://"))
            self.assertTrue(app.exe_name.lower().endswith(".exe"))
        self.assertNotIn("NeeView", "".join(a.fork_url for a in APPS.values()))

    def test_dialog_purpose_registered_for_exe_only(self):
        self.assertEqual(PURPOSES[eas.DIALOG_PURPOSE], ("open", frozenset({".exe"})))


class ExternalAppsServiceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        # ExternalAppInstaller의 기본 설치 경로가 실제 사용자 LOCALAPPDATA를
        # 읽지 않도록 테스트별 임시 프로필로 격리한다.
        self._localappdata_patch = patch.dict(os.environ, {"LOCALAPPDATA": str(self.root / "localappdata")})
        self._localappdata_patch.start()
        self.app = self.root / "app"
        self.app.mkdir()
        self.settings = SettingsService(self.root / "settings.json")
        self.launched = []

    def tearDown(self):
        self._localappdata_patch.stop()
        self.tmp.cleanup()

    def _exe(self, folder, name):
        folder.mkdir(parents=True, exist_ok=True)
        exe = folder / name
        exe.write_bytes(b"MZ fake")
        return exe

    def _bundle(self, app):
        return self._exe(self.app / "tools" / app.folder, app.exe_name)

    def _popen(self, args, cwd):
        self.launched.append((args, cwd))
        return type("P", (), {"pid": 4321})()

    def _service(self, **kwargs):
        kwargs.setdefault("folders", lambda: [self.app])
        kwargs.setdefault("popen", self._popen)
        return ExternalAppsService(self.settings, **kwargs)

    def test_each_app_found_in_its_tools_folder_and_launched_with_argument_list(self):
        service = self._service()
        for app in APPS.values():
            exe = self._bundle(app)
            status = service.command("status", {"app": app.app_id})
            self.assertTrue(status["available"], app)
            self.assertEqual(status["source"], "bundled")
            result = service.command("launch", {"app": app.app_id})
            self.assertTrue(result["ok"], result)
            self.assertEqual(result["pid"], 4321)
            args, cwd = self.launched[-1]
            # 인자 목록 1개(실행 파일만) — 셸 문자열이 아니다.
            self.assertEqual(args, [str(exe.resolve())])
            self.assertEqual(cwd, str(exe.resolve().parent))

    def test_exe_of_other_app_in_tools_folder_is_not_used(self):
        # tools/mongdangoutliner/NeeView.exe 같은 엇갈린 배치는 아웃라이너로 인정하지 않는다.
        self._exe(self.app / "tools" / OUTLINER.folder, PHOTO.exe_name)
        self.assertFalse(self._service().command("status", {"app": "outliner"})["available"])

    def test_list_reports_all_apps(self):
        self._bundle(PICS)
        listed = self._service().command("list")
        self.assertTrue(listed["ok"])
        self.assertEqual([a["app"] for a in listed["apps"]], list(APPS))
        self.assertEqual([a["available"] for a in listed["apps"]], [False, False, True, False])
        self.assertEqual(listed["apps"][1]["license"], "AGPL-3.0")
        self.assertEqual(listed["apps"][2]["tools_folder"], "tools\\mondangpics")

    def test_latest_release_metadata_and_installed_version_comparison(self):
        from docfit_app.external_apps_download import Release, Source
        source = Source("github", "haijun93/mongdangview", r".*\.zip", "zip", 100)
        release = Release("v2.4.0", "album.zip", "https://github.com/x/album.zip", 12, "a" * 64, source)
        installer = ExternalAppInstaller(self.root / "tools", sources={"photo_album": ()})
        service = self._service(installer=installer, release_lookup=lambda app: release)
        latest = service.command("latest", {"app": PHOTO.app_id})
        self.assertEqual((latest["version"], latest["size"], latest["license"]), ("2.4.0", 12, PHOTO.license))
        self.assertFalse(latest["update_available"])
        folder = self.root / "tools" / PHOTO.folder
        folder.mkdir(parents=True)
        (folder / ".installed.json").write_text('{"version":"v2.3.1","exe_rel":"NeeView.exe"}', encoding="utf-8")
        (folder / "NeeView.exe").write_bytes(b"MZ fake")
        latest = service.command("latest", {"app": PHOTO.app_id})
        self.assertTrue(latest["update_available"])
        self.assertEqual(latest["installed_version"], "2.3.1")

    def test_missing_exe_returns_install_guidance_with_both_links(self):
        result = self._service().command("launch", {"app": "photo_manager"})
        self.assertFalse(result["ok"])
        self.assertEqual(result["code"], "APP_MISSING")
        self.assertIn("tools\\mondangpics", result["message"])
        self.assertIn(PICS.fork_url, result["message"])
        self.assertIn(PICS.official_url, result["message"])
        self.assertEqual(result["fork_url"], PICS.fork_url)
        self.assertEqual(self.launched, [])

    def test_custom_path_chosen_by_dialog_is_saved_per_app(self):
        exe = self._exe(self.root / "Program Files" / "Logseq", "Logseq.exe")
        dialogs = FakeDialogs(picked=str(exe))
        service = self._service(dialogs=dialogs)
        chosen = service.command("choose", {"app": "outliner"})
        self.assertTrue(chosen["ok"], chosen)
        self.assertFalse(chosen["cancelled"])
        self.assertEqual(dialogs.calls, [(eas.DIALOG_PURPOSE, "open", False)])
        self.assertEqual(self.settings.read()[eas.SETTING_KEY], {"outliner": str(exe.resolve())})
        self.assertEqual(chosen["source"], "custom")
        self.assertTrue(service.command("launch", {"app": "outliner"})["ok"])
        self.assertEqual(self.launched[0][0], [str(exe.resolve())])
        # 다른 프로그램에는 영향이 없다.
        self.assertFalse(service.command("status", {"app": "photo_album"})["available"])

    def test_bundled_exe_wins_over_custom_path(self):
        bundled = self._bundle(PHOTO)
        custom = self._exe(self.root / "other", PHOTO.exe_name)
        self.settings.update(lambda s: s.update({eas.SETTING_KEY: {"photo_album": str(custom)}}))
        status = self._service().command("status", {"app": "photo_album"})
        self.assertEqual(status["source"], "bundled")
        self.assertEqual(status["path"], str(bundled.resolve()))

    def test_dialog_cancel_keeps_previous_setting(self):
        self.settings.update(lambda s: s.update({eas.SETTING_KEY: {"outliner": "C:\\old\\Logseq.exe"}}))
        result = self._service(dialogs=FakeDialogs(cancelled=True)).command("choose", {"app": "outliner"})
        self.assertTrue(result["cancelled"])
        self.assertEqual(self.settings.read()[eas.SETTING_KEY], {"outliner": "C:\\old\\Logseq.exe"})

    def test_choosing_other_program_is_rejected_and_not_saved(self):
        other = self._exe(self.root, "NeeView.exe")
        result = self._service(dialogs=FakeDialogs(picked=str(other))).command("choose", {"app": "photo_manager"})
        self.assertFalse(result["ok"])
        self.assertEqual(result["code"], "APP_WRONG_EXE")
        self.assertIn("digikam.exe", result["message"])
        self.assertNotIn(eas.SETTING_KEY, self.settings.read())

    def test_stale_custom_path_reports_missing(self):
        self.settings.update(lambda s: s.update({eas.SETTING_KEY: {"photo_album": str(self.root / "gone" / "NeeView.exe")}}))
        service = self._service()
        self.assertFalse(service.command("status", {"app": "photo_album"})["available"])
        result = service.command("launch", {"app": "photo_album"})
        self.assertEqual(result["code"], "APP_MISSING")
        self.assertIn("다시 지정", result["message"])

    def test_malformed_setting_is_ignored(self):
        self.settings.update(lambda s: s.update({eas.SETTING_KEY: "C:\\x\\NeeView.exe"}))
        self.assertFalse(self._service().command("status", {"app": "photo_album"})["available"])

    def test_clear_removes_only_that_app(self):
        self.settings.update(lambda s: s.update({eas.SETTING_KEY: {"photo_album": "C:\\x\\NeeView.exe",
                                                                   "outliner": "C:\\y\\Logseq.exe"}, "other": 1}))
        service = self._service()
        service.command("clear", {"app": "photo_album"})
        self.assertEqual(self.settings.read(), {eas.SETTING_KEY: {"outliner": "C:\\y\\Logseq.exe"}, "other": 1})
        service.command("clear", {"app": "outliner"})
        self.assertEqual(self.settings.read(), {"other": 1})

    def test_dialog_service_error_becomes_error_response(self):
        class Broken(FakeDialogs):
            def select_files(self, *a, **k):
                raise ServiceError("SELECTION_EXPIRED", "파일 선택을 다시 확인해 주세요.")
        result = self._service(dialogs=Broken()).command("choose", {"app": "outliner"})
        self.assertEqual(result, {"ok": False, "app": "outliner", "code": "SELECTION_EXPIRED",
                                  "message": "파일 선택을 다시 확인해 주세요."})

    def test_choose_without_dialogs_is_unavailable(self):
        self.assertEqual(self._service().command("choose", {"app": "outliner"})["code"], "DIALOG_UNAVAILABLE")

    def test_unknown_action_or_app_rejected(self):
        service = self._service()
        self.assertEqual(service.command("run_shell", {"cmd": "x"})["code"], "INVALID_ACTION")
        self.assertEqual(service.command("_popen")["code"], "INVALID_ACTION")
        self.assertEqual(service.command("list_apps")["code"], "INVALID_ACTION")
        self.assertEqual(service.command("launch")["code"], "UNKNOWN_APP")
        self.assertEqual(service.command("launch", {"app": "cmd"})["code"], "UNKNOWN_APP")
        self.assertEqual(service.command("launch", {"app": ["outliner"]})["code"], "UNKNOWN_APP")
        self.assertEqual(self.launched, [])

    def test_launch_os_error_is_reported(self):
        self._bundle(OUTLINER)
        def broken(args, cwd):
            raise OSError("access denied")
        result = self._service(popen=broken).command("launch", {"app": "outliner"})
        self.assertEqual(result["code"], "APP_LAUNCH_FAILED")

    def test_open_download_uses_fixed_urls_only(self):
        opened = []
        service = self._service(browser=opened.append)
        self.assertTrue(service.command("open_download", {"app": "outliner", "url": "http://evil"})["ok"])
        service.command("open_download", {"app": "outliner", "target": "official"})
        service.command("open_download", {"app": "photo_manager", "target": "http://evil"})
        self.assertEqual(opened, [OUTLINER.fork_url, OUTLINER.official_url, PICS.fork_url])


class ValidateExeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.exe = self.root / "Logseq.exe"
        self.exe.write_bytes(b"MZ")

    def tearDown(self):
        self.tmp.cleanup()

    def code(self, value, app=OUTLINER):
        with self.assertRaises(eas.ExternalAppError) as caught:
            validate_exe(app, value)
        return caught.exception.code

    def test_accepts_local_absolute_exe_case_insensitive(self):
        self.assertEqual(validate_exe(OUTLINER, str(self.exe)), self.exe.resolve())
        lower = self.root / "sub" / "DIGIKAM.EXE"
        lower.parent.mkdir()
        lower.write_bytes(b"MZ")
        self.assertEqual(validate_exe(PICS, lower), lower.resolve())

    def test_rejects_bad_inputs(self):
        self.assertEqual(self.code(None), "APP_INVALID_PATH")
        self.assertEqual(self.code(""), "APP_INVALID_PATH")
        self.assertEqual(self.code("C:\\a\x00\\Logseq.exe"), "APP_INVALID_PATH")
        self.assertEqual(self.code("Logseq.exe"), "APP_INVALID_PATH")
        self.assertEqual(self.code("\\\\server\\share\\Logseq.exe"), "APP_INVALID_PATH")
        self.assertEqual(self.code("//server/share/Logseq.exe"), "APP_INVALID_PATH")
        self.assertEqual(self.code(str(self.exe) + ":stream"), "APP_INVALID_PATH")
        self.assertEqual(self.code(str(self.root / "cmd.exe")), "APP_WRONG_EXE")
        self.assertEqual(self.code(str(self.root / "Logseq.exe.bat")), "APP_WRONG_EXE")
        self.assertEqual(self.code(str(self.exe), app=PHOTO), "APP_WRONG_EXE")
        self.assertEqual(self.code(str(self.root / "none" / "Logseq.exe")), "APP_MISSING")

    def test_rejects_directory_named_like_exe(self):
        folder = self.root / "d" / "Logseq.exe"
        folder.mkdir(parents=True)
        self.assertEqual(self.code(str(folder)), "APP_MISSING")

    @unittest.skipUnless(os.name == "nt", "Windows 정션 전용")
    def test_rejects_path_through_junction(self):
        real = self.root / "real"
        real.mkdir()
        (real / "Logseq.exe").write_bytes(b"MZ")
        link = self.root / "link"
        made = subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(real)],
                              capture_output=True, check=False)
        if made.returncode != 0:
            self.skipTest("정션을 만들 수 없음")
        self.assertEqual(self.code(str(link / "Logseq.exe")), "APP_INVALID_PATH")


class DefaultPopenTests(unittest.TestCase):
    def test_default_popen_never_uses_shell_and_detaches(self):
        with patch.object(eas.subprocess, "Popen") as popen:
            eas.default_popen(["C:\\x\\Logseq.exe"], "C:\\x")
        args, kwargs = popen.call_args
        self.assertEqual(args[0], ["C:\\x\\Logseq.exe"])
        self.assertIs(kwargs["shell"], False)
        self.assertEqual(kwargs["cwd"], "C:\\x")
        if os.name == "nt":
            self.assertTrue(kwargs["creationflags"] & subprocess.DETACHED_PROCESS)

    def test_default_popen_rejects_string_command(self):
        with patch.object(eas.subprocess, "Popen") as popen:
            with self.assertRaises(TypeError):
                eas.default_popen("C:\\x\\Logseq.exe --flag", "C:\\x")
        popen.assert_not_called()

    def test_app_folders_include_repo_root(self):
        root = Path(eas.__file__).resolve().parents[1]
        self.assertIn(root, eas.app_folders())


class BridgeTests(unittest.TestCase):
    """DesktopWebBridge.external_app: 개발자 모드가 꺼져 있으면 서비스를 만들지 않는다."""

    def _bridge(self, developer):
        import desktop_web_ui
        bridge = desktop_web_ui.DesktopWebBridge.__new__(desktop_web_ui.DesktopWebBridge)
        bridge._developer_mode_enabled = lambda: developer
        return bridge

    def test_requires_developer_mode(self):
        result = self._bridge(False).external_app("launch", {"app": "outliner"})
        self.assertEqual(result["code"], "DEVELOPER_MODE_REQUIRED")

    def test_forwards_to_service_in_developer_mode(self):
        bridge = self._bridge(True)
        calls = []
        bridge._external_apps_service = type("S", (), {"command": lambda self, a, p=None: calls.append((a, p)) or {"ok": True}})()
        self.assertEqual(bridge.external_app("status", {"app": "outliner"}), {"ok": True})
        self.assertEqual(calls, [("status", {"app": "outliner"})])

    def test_browser_api_exposes_external_app_only(self):
        import inspect
        import desktop_web_ui
        source = inspect.getsource(desktop_web_ui._BrowserApi)
        self.assertIn('"external_app"', source)
        self.assertNotIn('"photo_album"', source)


class ApplicationServiceTests(unittest.TestCase):
    def test_native_backend_dispatch_requires_developer_mode(self):
        from docfit_app.application_service import ApplicationService
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "settings.json"
            app = ApplicationService(path)
            self.assertEqual(app.dispatch("external_app", ["status", {"app": "outliner"}])["code"], "DEVELOPER_MODE_REQUIRED")
            app.store.update(lambda s: s.update(developer_mode=True))
            status = app.dispatch("external_app", ["status", {"app": "outliner"}])
            self.assertTrue(status["ok"])
            self.assertEqual(status["app_info"]["fork_url"], OUTLINER.fork_url)
            self.assertEqual(len(app.dispatch("external_app", ["list"])["apps"]), len(APPS))


if __name__ == "__main__":
    unittest.main()
