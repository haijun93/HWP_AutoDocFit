"""사이드바 실행 방식(RUSTSIDE1): 전용 화면 창(--screen N)의 충돌 분리, 실행 선택, 이름 일치, 패키징."""
import json
import os
from pathlib import Path
import re
import tempfile
import threading
import unittest
from unittest import mock

from docfit_app import screen_mode, sidebar_launch

ROOT = Path(__file__).resolve().parents[1]


def read(relative):
    return (ROOT / relative).read_text(encoding='utf-8')


class ScreenModeTest(unittest.TestCase):
    def tearDown(self):
        screen_mode.activate(None)
        screen_mode._mutex = None

    def test_parse_accepts_only_known_screens(self):
        self.assertIsNone(screen_mode.parse([]))
        self.assertIsNone(screen_mode.parse(['a.hwpx']))
        self.assertIsNone(screen_mode.parse(['--rename', '--screen']))
        self.assertEqual(screen_mode.parse(['--screen', '0']), 0)
        self.assertEqual(screen_mode.parse(['--screen', '13']), 13)
        for bad in (['--screen'], ['--screen', '7'], ['--screen', '16'], ['--screen', '-1'],
                    ['--screen', '²'], ['--screen', '1', 'x']):
            with self.assertRaises(ValueError, msg=bad):
                screen_mode.parse(bad)

    def test_titles_match_sidebar_and_do_not_match_main_window_search(self):
        rust = read('dock/src/sidebar.rs')
        labels = re.findall(r'"([^"]*)"', re.search(r'const LABELS: \[&str; 17\] = \[(.*?)\];', rust, re.S).group(1))
        for number, label in screen_mode.SCREENS.items():
            self.assertEqual(labels[number], label)
            title = screen_mode.window_title(number)
            self.assertNotIn('한글편집', title)  # 본 앱의 '이미 실행 중' 창 찾기에 걸리지 않는다
        self.assertIn('format!("{} — DocFit", label(id))', rust)
        dart = read('flutter_ui/lib/features/workspace/tab_navigation_shell.dart')
        dart_labels = re.findall(r"'([^']*)'", re.search(r'static const _labels = \[(.*?)\];', dart, re.S).group(1))
        for number in screen_mode.SCREENS:
            if number not in (12, 13):  # 12·13 은 왼쪽 메뉴의 별도 항목 이름
                self.assertEqual(dart_labels[number], screen_mode.SCREENS[number])

    def test_normal_app_saves_everything(self):
        screen_mode.remember({'a': 1})
        settings = {'a': 1, 'b': 2}
        self.assertIs(screen_mode.changed_only(settings), settings)
        self.assertEqual(screen_mode.job_state_name(), 'desktop-jobs.json')

    def test_screen_window_writes_only_what_it_changed(self):
        screen_mode.activate(3)
        screen_mode.remember({'theme': 'light', 'verify': True, 'fonts': {'a': 1}})
        screen_mode.remember({'theme': 'later load is ignored'})
        self.assertEqual(screen_mode.changed_only({'theme': 'light', 'verify': False, 'fonts': {'a': 1}}),
                         {'verify': False})
        self.assertEqual(screen_mode.changed_only({'theme': 'light', 'verify': False, 'fonts': {'a': 1}}), {})
        self.assertEqual(screen_mode.changed_only({'theme': 'light', 'verify': True}), {'verify': True})
        self.assertEqual(screen_mode.job_state_name(), 'desktop-jobs-screen-3.json')

    def test_save_does_not_revert_another_windows_change(self):
        from docfit_app.settings_service import SettingsService
        with tempfile.TemporaryDirectory() as folder:
            store = SettingsService(Path(folder) / 'settings.json')
            store.update(lambda current: current.update({'mine': 1, 'theirs': 'old'}))
            screen_mode.activate(0)
            screen_mode.remember(store.read())
            store.update(lambda current: current.update({'theirs': 'new'}))  # 다른 창의 저장
            patch = screen_mode.changed_only({'mine': 2, 'theirs': 'old'})
            store.update(lambda current: current.update(patch))
            self.assertEqual(store.read(), {'mine': 2, 'theirs': 'new'})


class _Api:
    def __init__(self, exists, window_after=None, free_after=None):
        self.exists, self.window_after, self.free_after = exists, window_after, free_after
        self.names, self.closed, self.activations = [], [], 0
        self.tries = 0

    def create_mutex(self, name):
        self.names.append(name)
        return 77, self.exists

    def activate_window(self, title):
        self.activations += 1
        return self.window_after is not None and self.activations > self.window_after

    def try_acquire(self, handle):
        self.tries += 1
        return self.free_after is not None and self.tries > self.free_after

    def close(self, handle):
        self.closed.append(handle)


class ClaimTest(unittest.TestCase):
    def setUp(self):
        screen_mode.activate(8)
        self.now = 0.0

    def tearDown(self):
        screen_mode.activate(None)
        screen_mode._mutex = None

    def claim(self, api):
        def sleep(seconds):
            self.now += seconds
        return screen_mode.claim(api, wait_seconds=1.0, sleep=sleep, clock=lambda: self.now)

    def test_first_instance_owns_the_menu(self):
        api = _Api(exists=False)
        self.assertTrue(self.claim(api))
        self.assertEqual(api.names, ['Local\\HwpAutoDocFit.Screen.8'])
        self.assertEqual(api.closed, [])

    def test_second_instance_brings_the_window_forward_and_leaves(self):
        api = _Api(exists=True, window_after=3)  # 먼저 뜬 쪽이 시작 중
        self.assertFalse(self.claim(api))
        self.assertEqual(api.closed, [77])

    def test_takes_over_when_the_previous_window_is_closing(self):
        api = _Api(exists=True, free_after=2)
        self.assertTrue(self.claim(api))
        self.assertEqual(api.closed, [])

    def test_gives_up_without_window_or_mutex(self):
        api = _Api(exists=True)
        self.assertFalse(self.claim(api))
        self.assertEqual(api.closed, [77])


class SidebarLaunchTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)

    def settings(self, value):
        (self.folder / 'settings.json').write_text(json.dumps(value), encoding='utf-8')

    def wanted(self, *arguments, environ=None):
        return sidebar_launch.wanted(list(arguments), self.folder, environ or {})

    def test_existing_app_is_the_default(self):
        self.assertFalse(self.wanted())
        for value in ({}, {'launch_mode': 'app'}, {'launch_mode': True}, []):
            self.settings(value)
            self.assertFalse(self.wanted(), value)
        (self.folder / 'settings.json').write_text('{', encoding='utf-8')
        self.assertFalse(self.wanted())

    def test_setting_or_argument_selects_the_sidebar(self):
        self.assertTrue(self.wanted('--sidebar'))
        self.settings({'launch_mode': 'sidebar'})
        self.assertTrue(self.wanted())
        # 파일 열기·전용 창·명령줄 호출은 설정과 상관없이 앱이 받는다.
        for arguments in (['a.hwpx'], ['--screen', '1'], ['--rename', 'a'], ['--sidebar', 'a.hwpx']):
            self.assertFalse(self.wanted(*arguments), arguments)

    def test_never_redirects_while_updating(self):
        self.settings({'launch_mode': 'sidebar'})
        self.assertFalse(self.wanted(environ={'DOCFIT_LAUNCHER_HANDOFF_EVENT': 'e'}))
        (self.folder / 'updates').mkdir()
        (self.folder / 'updates' / 'update_busy.flag').write_text('', encoding='utf-8')
        self.assertFalse(self.wanted())

    def run_redirect(self, argv, finder, launcher):
        with mock.patch.object(sidebar_launch.os, 'name', 'nt'):
            return sidebar_launch.redirect(argv, folder=self.folder, environ={}, finder=finder, launcher=launcher)

    def test_redirect_starts_the_sidebar_with_this_settings_file(self):
        # os.name 을 바꾸면 pathlib 이 흔들리므로 Windows 에서만 실행한다.
        if os.name != 'nt':
            self.skipTest('Windows 전용')
        calls = []
        argv = ['app', '--sidebar']
        self.assertTrue(self.run_redirect(argv, lambda: Path('sidebar.exe'),
                                          lambda args, **kwargs: calls.append(args)))
        self.assertEqual(calls[0][:3], ['sidebar.exe', '--settings', str(self.folder / 'settings.json')])
        self.assertIn('--source-root', calls[0])

    def test_missing_or_broken_sidebar_falls_back_to_the_app(self):
        if os.name != 'nt':
            self.skipTest('Windows 전용')
        argv = ['app', '--sidebar']
        self.assertFalse(self.run_redirect(argv, lambda: None, None))
        self.assertEqual(argv, ['app'])
        argv = ['app', '--sidebar']

        def broken(*args, **kwargs):
            raise OSError
        self.assertFalse(self.run_redirect(argv, lambda: Path('sidebar.exe'), broken))
        self.assertEqual(argv, ['app'])

    def test_full_app_argument_overrides_the_setting(self):
        self.settings({'launch_mode': 'sidebar'})
        argv = ['app', '--full-app']
        self.assertFalse(self.run_redirect(argv, lambda: self.fail('사이드바를 찾지 않는다'), None))
        self.assertEqual(argv, ['app'])


class SpawnGuardTest(unittest.TestCase):
    def test_guard_serialises_threads(self):
        if os.name != 'nt':
            self.skipTest('Windows 전용')
        from docfit_core.hwp_process import spawn_guard
        entered = threading.Event()

        def other():
            with spawn_guard(5000):
                entered.set()

        with spawn_guard():
            thread = threading.Thread(target=other)
            thread.start()
            self.assertFalse(entered.wait(0.3))
        thread.join(5)
        self.assertTrue(entered.is_set())


class _Event:
    def __iadd__(self, handler):
        return self


class WebviewStartTest(unittest.TestCase):
    """run_webview 의 시작 작업: 상주 도우미 점검은 보통 앱만 하고, 하나도 조용히 빠지지 않는다."""

    def tearDown(self):
        screen_mode.activate(None)

    def start(self, screen):
        import desktop_web_ui
        screen_mode.activate(screen)
        started, created = {}, {}
        window = mock.MagicMock()
        window.events.closing = window.events.loaded = _Event()

        def create_window(title, **kwargs):
            created.update(title=title, url=kwargs['url'])
            return window

        fake = mock.MagicMock()
        fake.screens = [mock.Mock(width=1920, height=1080)]
        fake.create_window = create_window
        fake.start = lambda callback, args, **kwargs: started.update(callback=callback, args=args)
        bridge = mock.MagicMock()
        bridge._tk.return_value = (1920, 1080)
        calls = []
        services = {
            'docfit_app.kagent_app_service.start_background': lambda: calls.append('kagent-pipe'),
            'docfit_app.kagent_app_service.set_photo_edit_handler': lambda handler: None,
            'docfit_app.kclip_service.KClipService': mock.MagicMock(),
            'docfit_app.kvf_client.KvfClient': mock.MagicMock(),
            'docfit_app.ktm_service.sync_in_background': lambda enabled: calls.append('ktm'),
            'docfit_app.configuration_service.ConfigurationService': mock.MagicMock(),
        }
        patches = [mock.patch(name, value) for name, value in services.items()]
        patches += [mock.patch.dict('sys.modules', {'webview': fake}),
                    mock.patch('importlib.util.find_spec', return_value=object()),
                    mock.patch.object(desktop_web_ui, '_guard_pywebview_form_closed'),
                    mock.patch.object(desktop_web_ui, '_install_webview_rendering_guard'),
                    mock.patch.object(desktop_web_ui, 'DesktopWebBridge', return_value=bridge),
                    mock.patch.object(desktop_web_ui, '_BrowserApi'),
                    mock.patch.object(desktop_web_ui, '_resource_path', return_value=ROOT / 'desktop_web' / 'index.html'),
                    mock.patch.object(desktop_web_ui, 'smoke_cdp_port', return_value=None),
                    mock.patch.object(desktop_web_ui.threading, 'Thread')]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        self.assertTrue(desktop_web_ui.run_webview(mock.MagicMock()))
        started['callback'](*started['args'])
        return created, calls

    def test_normal_app_runs_every_resident_check(self):
        created, calls = self.start(None)
        self.assertEqual(calls, ['kagent-pipe', 'ktm'])
        self.assertNotIn('#screen', created['url'])

    def test_screen_window_skips_them_and_carries_the_screen_number(self):
        created, calls = self.start(11)
        self.assertEqual(calls, [])
        self.assertEqual(created['title'], 'PDF 편집 — DocFit')
        self.assertTrue(created['url'].endswith('index.html#screen=11'))


class PackagingTest(unittest.TestCase):
    def test_sidebar_is_shipped_and_stopped_by_the_installer(self):
        workflow = read('.github/workflows/build-windows-exe.yml')
        self.assertIn('Copy-Item dock/target/release/sidebar.exe resources/sidebar.exe', workflow)
        self.assertIn("'sidebar.exe'", workflow)
        self.assertIn("sidebar_exe = root / 'resources' / 'sidebar.exe'", read('HWP_AutoDocFit.spec'))
        self.assertIn("StopHelper('sidebar.exe')", read('installer/setup.iss'))

    def test_default_shortcut_still_runs_the_app(self):
        self.assertNotIn('--sidebar', read('installer/setup.iss'))


if __name__ == '__main__':
    unittest.main()
