"""창끼리 파일 넘기기·상주 작업·안전 종료·로그온 시작(RUSTSIDE2).

`Sidebar*` 시험은 빌드한 sidebar.exe 가 있을 때만 돈다(DOCFIT_SIDEBAR_TEST_EXE 또는 dock/target/release).
모두 임시 폴더·임시 레지스트리 키를 쓰고, `--close-all` 은 임시 설치 폴더 안의 시험 창만 닫는다.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
import uuid

from docfit_app import handoff, resident, screen_mode
from docfit_app.api_contract import ServiceError

ROOT = Path(__file__).resolve().parents[1]


def read(relative):
    return (ROOT / relative).read_text(encoding='utf-8-sig')


class Temp(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.addCleanup(temp.cleanup)
        self.folder = Path(temp.name).resolve()
        patch = mock.patch.dict(os.environ, {'LOCALAPPDATA': str(self.folder / 'local')})
        patch.start()
        self.addCleanup(patch.stop)

    def make(self, name):
        path = self.folder / name
        path.write_bytes(b'original')
        return str(path)

    def request(self, menu, paths, name='1', **extra):
        box = handoff.mailbox(menu)
        box.mkdir(parents=True, exist_ok=True)
        target = box / f'{name}.json'
        target.write_text(json.dumps({'paths': paths, **extra}), encoding='utf-8')
        return target


class RulesTest(unittest.TestCase):
    def test_dart_rust_python_tables_agree(self):
        dart = read('flutter_ui/lib/features/common/file_handoff.dart')
        names = {'documentEditor': 0, 'virtualFolder': 12, 'notebook': 13, 'conversion': 10, 'pdfEdit': 11}
        extensions = dart[dart.index('kFileTargetExtensions'):dart.index('kFileTargetLimits')]
        for name, menu in names.items():
            body = re.search(rf'FileTarget\.{name}: (null|\{{.*?\}})', extensions, re.S)[1]
            expected = None if body == 'null' else frozenset(re.findall(r"'(\w+)'", body))
            limit = int(re.search(rf'FileTarget\.{name}: (\d+)', dart[dart.index('kFileTargetLimits'):])[1])
            self.assertEqual(handoff.TARGETS[menu], (expected, limit), name)
        rust = read('grid_core/src/handoff.rs')
        found = {int(m[1]): (frozenset(re.findall(r'"(\w+)"', m[2])) or None, int(m[3])) for m in re.finditer(
            r'menu: (\d+),\s*label: "[^"]*",\s*extensions: &\[(.*?)\],\s*limit: (\d+)', rust, re.S)}
        self.assertEqual(found, handoff.TARGETS)


class TakeTest(Temp):
    def test_takes_each_request_once_and_revalidates(self):
        good = self.make('가 나.pdf')
        bad = [self.make('a.hwp'), str(self.folder / 'none.pdf'), 'relative.pdf', str(self.folder)]
        request = self.request(11, [good, *bad, good])
        self.assertEqual(handoff.take(11), {'paths': [good], 'photo_paste': False, 'rejected': 4})
        self.assertFalse(request.exists())
        self.assertEqual(handoff.take(11)['paths'], [])
        self.assertEqual(Path(good).read_bytes(), b'original')

    def test_only_own_mailbox_and_known_menus(self):
        self.request(0, [self.make('a.hwpx')])
        self.assertEqual(handoff.take(11)['paths'], [])
        self.assertEqual(handoff.take(3)['paths'], [])
        self.assertEqual(len(handoff.take(0)['paths']), 1)

    def test_expired_broken_and_oversized_requests_are_dropped(self):
        path = self.make('a.pdf')
        old = self.request(11, [path], 'old')
        os.utime(old, (time.time() - 700, time.time() - 700))
        broken = self.request(11, [path], 'broken')
        broken.write_text('{', encoding='utf-8')
        many = self.request(11, [path] * 101, 'many')
        self.assertEqual(handoff.take(11), {'paths': [], 'photo_paste': False, 'rejected': 101})
        self.assertFalse(any(p.exists() for p in (old, broken, many)))

    def test_batch_over_the_limit_waits_for_the_next_turn(self):
        first = [self.make(f'{i}.pdf') for i in range(60)]
        second = [self.make(f'x{i}.pdf') for i in range(60)]
        self.request(11, first, '1')
        later = self.request(11, second, '2')
        self.assertEqual(handoff.take(11)['paths'], first)
        self.assertTrue(later.exists())
        self.assertEqual(handoff.take(11)['paths'], second)

    def test_photo_paste_request(self):
        self.request(5, [], photo_paste=True)
        self.assertTrue(handoff.take(5)['photo_paste'])


class SendTest(Temp):
    def test_validates_before_running_and_reports_the_sidebar_message(self):
        calls = []

        def runner(args, **kwargs):
            listing = Path(args[args.index('--list') + 1])
            calls.append((args, json.loads(listing.read_text(encoding='utf-8')), listing))
            return subprocess.CompletedProcess(args, 2, stdout='열지 못했습니다.'.encode())

        find = lambda: Path('sidebar.exe')
        for menu, paths in ((11, [self.make('a.hwp')]), (7, [self.make('a.pdf')]), (11, []), (11, 'a.pdf')):
            with self.assertRaises(ServiceError):
                handoff.send(menu, paths, runner=runner, finder=find, settings_folder=self.folder)
        self.assertEqual(calls, [])
        pdf = self.make('b.pdf')
        with self.assertRaisesRegex(ServiceError, '열지 못했습니다'):
            handoff.send(11, [pdf], runner=runner, finder=find, settings_folder=self.folder)
        args, body, listing = calls[0]
        self.assertEqual(args[args.index('--send') + 1], '11')
        self.assertEqual(body, {'paths': [pdf]})
        self.assertFalse(listing.exists(), '목록 임시 파일은 지운다')

    def test_missing_sidebar(self):
        with self.assertRaisesRegex(ServiceError, 'sidebar.exe'):
            handoff.send(11, [self.make('a.pdf')], finder=lambda: None)


class BridgeTest(Temp):
    def tearDown(self):
        screen_mode.activate(None)

    def call(self, payload):
        import desktop_web_ui
        bridge = desktop_web_ui.DesktopWebBridge.__new__(desktop_web_ui.DesktopWebBridge)
        return bridge.handoff(payload)

    def test_normal_app_takes_nothing_and_cannot_send(self):
        self.request(0, [self.make('a.hwpx')])
        self.assertEqual(self.call({'action': 'take'})['data']['paths'], [])
        self.assertFalse(self.call({'action': 'send', 'menu': 11, 'paths': []})['ok'])

    def test_screen_window_takes_its_own_mailbox(self):
        path = self.make('a.hwpx')
        self.request(0, [path])
        screen_mode.activate(0)
        self.assertEqual(self.call({'action': 'take'})['data']['paths'], [path])


class _Parent:
    def __init__(self, app_running=False, claimed=True, waits=()):
        self.app_running, self.claimed, self.waits = app_running, claimed, list(waits)
        self.waited = []

    def open_parent(self, pid):
        return None if pid == 999 else 5

    def claim(self, name):
        return self.claimed

    def mutex_exists(self, name):
        return self.app_running

    def wait(self, handle, seconds):
        self.waited.append(seconds)
        return not self.waits or self.waits.pop(0)


class ResidentTest(Temp):
    def run_main(self, api, arguments=('--resident', '--parent', '12')):
        calls = []
        patches = [
            mock.patch.object(resident, 'start_app_pipe', lambda handler: calls.append('pipe')),
            mock.patch.object(resident, 'upgrade_helpers', lambda: calls.append('upgrade') or []),
            mock.patch.object(resident, 'sync_search', lambda read: calls.append('search')),
            mock.patch('docfit_app.shell_menu.refresh_registered', lambda: calls.append('menu')),
        ]
        for patch in patches:
            patch.start()
        try:
            code = resident.main(list(arguments), '1.0.0', api=api, folder=self.folder)
        finally:
            for patch in patches:
                patch.stop()
        return code, calls

    def test_owns_the_checks_when_only_the_sidebar_runs(self):
        self.assertEqual(self.run_main(_Parent()), (0, ['menu', 'upgrade', 'search', 'pipe']))

    def test_leaves_checks_to_a_running_app(self):
        self.assertEqual(self.run_main(_Parent(app_running=True)), (0, ['pipe']))

    def test_second_resident_or_missing_parent_does_nothing(self):
        self.assertEqual(self.run_main(_Parent(claimed=False)), (0, []))
        self.assertEqual(self.run_main(_Parent(), ('--resident', '--parent', '999')), (0, []))
        for bad in (['--resident'], ['--resident', '--parent', '0'], ['--resident', '--parent', 'x']):
            self.assertEqual(resident.main(bad, '1'), 2, bad)

    def test_update_notice_follows_the_check_and_the_setting(self):
        notice = self.folder / resident.NOTICE
        service = mock.Mock()
        service.state.return_value = {'step': 'available', 'latest': '9.9.9'}
        self.assertTrue(resident.check_update(self.folder, service))
        self.assertEqual(json.loads(notice.read_text(encoding='utf-8')), {'latest': '9.9.9'})
        service.state.return_value = {'step': 'failed'}
        resident.check_update(self.folder, service)
        self.assertTrue(notice.exists(), '확인 실패는 알림을 지우지 않는다')
        (self.folder / 'settings.json').write_text('{"check_updates_on_start": false}', encoding='utf-8')
        service.reset_mock()
        self.assertFalse(resident.check_update(self.folder, service))
        self.assertFalse(notice.exists())
        service.check.assert_not_called()

    def test_entry_point_dispatches_before_gui_imports(self):
        source = read('hwp-auto-docfit.py')
        self.assertLess(source.index('["--resident"]'), source.index('from docfit_engine.runtime'))
        self.assertEqual(len(re.findall(r'^APP_VERSION = ', source, re.M)), 1)


class PackagingTest(unittest.TestCase):
    def test_installer_closes_windows_before_killing_helpers(self):
        iss = read('installer/setup.iss')
        body = iss[iss.index('function PrepareToInstall'):]
        self.assertLess(body.index('CloseAppWindows('), body.index("StopHelper('kagent.exe')"))
        self.assertIn('function InitializeUninstall', iss)
        self.assertIn('#include "close_windows.iss"', iss)
        self.assertIn('ValueName: "HwpAutoDocFitSidebar"; Flags: dontcreatekey uninsdeletevalue', iss)
        code = [line for line in read('installer/close_windows.iss').splitlines() if not line.lstrip().startswith('//')]
        self.assertFalse([line for line in code if 'taskkill' in line])


def sidebar_exe():
    for candidate in (os.environ.get('DOCFIT_SIDEBAR_TEST_EXE'), ROOT / 'dock' / 'target' / 'release' / 'sidebar.exe'):
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    return None


@unittest.skipUnless(os.name == 'nt' and sidebar_exe(), 'sidebar.exe 빌드가 필요합니다')
class SidebarSendTest(Temp):
    def run_send(self, *arguments):
        source = self.folder / 'source'
        source.mkdir(exist_ok=True)
        (source / 'hwp-auto-docfit.py').write_text(
            'import sys, pathlib\n'
            'pathlib.Path(__file__).with_name("launched.txt").write_text(" ".join(sys.argv[1:]))\n', encoding='utf-8')
        return subprocess.run([str(sidebar_exe()), '--settings', str(self.folder / 'settings.json'),
                               '--resources', str(self.folder / 'resources'), '--python', sys.executable,
                               '--source-root', str(source), *arguments], capture_output=True, timeout=30)

    def launched(self):
        marker = self.folder / 'source' / 'launched.txt'
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if marker.is_file() and marker.read_text():
                return marker.read_text()
            time.sleep(0.1)
        return None

    def test_writes_a_private_request_opens_the_window_and_the_app_takes_it(self):
        path = self.make('보고 자료.pdf')
        done = self.run_send('--send', '11', '--path', path)
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertEqual(self.launched(), '--screen 11')
        root = self.folder / 'local' / 'HWP_AutoDocFit' / 'open_requests'
        acl = subprocess.run(['icacls', str(root)], capture_output=True, text=True, errors='replace').stdout
        entries = [line for line in acl.splitlines() if ':(' in line]
        self.assertEqual(len(entries), 2, acl)  # 현재 사용자와 SYSTEM
        self.assertFalse(any('(I)' in line for line in entries), acl)
        self.assertEqual(handoff.take(11)['paths'], [path])
        self.assertEqual(Path(path).read_bytes(), b'original')

    def test_rejects_without_writing_or_opening(self):
        for arguments in (['--send', '11', '--path', self.make('a.hwp')], ['--send', '11', '--path', 'a.pdf'],
                          ['--send', '7', '--path', self.make('b.pdf')]):
            done = self.run_send(*arguments)
            self.assertEqual(done.returncode, 2, arguments)
            self.assertTrue(done.stdout.decode('utf-8'))
        self.assertFalse((self.folder / 'local').exists())
        self.assertFalse((self.folder / 'source' / 'launched.txt').exists())

    def test_sender_does_not_wait_for_the_window_it_opened(self):
        """띄운 창이 출력 파이프를 물려받으면 파이프로 결과를 받는 앱이 창이 닫힐 때까지 멈춘다."""
        source = self.folder / 'source'
        source.mkdir()
        (source / 'hwp-auto-docfit.py').write_text('import time; time.sleep(8)', encoding='utf-8')
        begin = time.monotonic()
        done = subprocess.run([str(sidebar_exe()), '--settings', str(self.folder / 'settings.json'),
                               '--resources', str(self.folder), '--python', sys.executable,
                               '--source-root', str(source), '--send', '11', '--path', self.make('d.pdf')],
                              capture_output=True, timeout=30)
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertLess(time.monotonic() - begin, 6)

    def test_python_sender_goes_through_the_same_program(self):
        path = self.make('c.hwpx')
        listing = self.folder / 'list.json'
        listing.write_text(json.dumps({'paths': [path]}), encoding='utf-8')
        self.assertEqual(self.run_send('--send', '0', '--list', str(listing)).returncode, 0)
        self.assertEqual(self.launched(), '--screen 0')
        self.assertEqual(handoff.take(0)['paths'], [path])

    def test_logon_start_is_off_by_default_and_toggles_one_value(self):
        import winreg
        key = rf'Software\HwpAutoDocFitTest_{uuid.uuid4().hex}'
        self.addCleanup(lambda: _delete_key(key))

        def autostart(value):
            done = subprocess.run([str(sidebar_exe()), '--settings', str(self.folder / 'settings.json'),
                                   '--autostart', value], capture_output=True, timeout=30,
                                  env={**os.environ, 'DOCFIT_SIDEBAR_RUN_KEY': key})
            self.assertEqual(done.returncode, 0, done.stdout)
            return done.stdout.decode()

        self.assertEqual(autostart('status'), 'off')
        self.assertEqual(autostart('on'), 'on')
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key) as handle:
            command = winreg.QueryValueEx(handle, 'HwpAutoDocFitSidebar')[0]
        self.assertTrue(command.startswith('"') and '" --logon' in command, command)
        self.assertIn(f'--settings "{self.folder / "settings.json"}"', command)
        self.assertEqual(autostart('off'), 'off')
        self.assertEqual(autostart('off'), 'off')


def _delete_key(key):
    import winreg
    try:
        winreg.DeleteKey(winreg.HKEY_CURRENT_USER, key)
    except OSError:
        pass


CSC = Path(os.environ.get('WINDIR', r'C:\Windows')) / 'Microsoft.NET' / 'Framework64' / 'v4.0.30319' / 'csc.exe'
WINDOW_SOURCE = '''
using System;
using System.Windows.Forms;
class P {
    [STAThread] static void Main(string[] a) {
        var f = new Form(); f.Text = a[0];
        f.FormClosing += (s, e) => { if (a.Length > 1) e.Cancel = true; };
        Application.Run(f);
    }
}
'''


@unittest.skipUnless(os.name == 'nt' and sidebar_exe() and CSC.is_file(), 'sidebar.exe 와 csc.exe 가 필요합니다')
class SidebarCloseAllTest(Temp):
    def setUp(self):
        super().setUp()
        self.install = self.folder / 'install dir'
        self.outside = self.folder / 'install dir 2'
        self.install.mkdir()
        self.outside.mkdir()
        source = self.folder / 'w.cs'
        source.write_text(WINDOW_SOURCE, encoding='utf-8')
        self.exe = self.install / 'testwindow.exe'
        subprocess.run([str(CSC), '/nologo', '/target:winexe', f'/out:{self.exe}', str(source)], check=True,
                       capture_output=True, timeout=60)
        shutil.copy(self.exe, self.outside / 'testwindow.exe')
        self.processes = []
        self.addCleanup(self.kill)

    def kill(self):
        for process in self.processes:
            process.kill()
            process.wait(10)

    def start(self, folder, *arguments):
        import ctypes
        process = subprocess.Popen([str(folder / 'testwindow.exe'), *arguments])
        self.processes.append(process)
        deadline = time.monotonic() + 20
        while not ctypes.windll.user32.FindWindowW(None, arguments[0]) and time.monotonic() < deadline:
            time.sleep(0.1)
        return process

    def close_all(self, wait):
        report = self.folder / 'report.txt'
        done = subprocess.run([str(sidebar_exe()), '--close-all', '--install-dir', str(self.install),
                               '--wait', str(wait), '--report', str(report)], capture_output=True, timeout=60)
        return done.returncode, report.read_text(encoding='utf-8-sig')

    def test_closes_only_windows_of_the_install_folder(self):
        tag = uuid.uuid4().hex
        inside = self.start(self.install, f'inside-{tag}')
        other = self.start(self.outside, f'outside-{tag}')
        self.assertEqual(self.close_all(15), (0, ''))
        self.assertIsNotNone(inside.poll())
        self.assertIsNone(other.poll(), '설치 폴더 밖의 창은 건드리지 않는다')

    def test_a_window_that_refuses_is_reported_and_never_killed(self):
        title = f'저장 안 함 {uuid.uuid4().hex}'
        refusing = self.start(self.install, title, 'veto')
        plain = self.start(self.install, f'plain-{uuid.uuid4().hex}')
        code, report = self.close_all(4)
        self.assertEqual(code, 3, report)
        self.assertIn(title, report)
        self.assertIsNone(refusing.poll())
        self.assertIsNotNone(plain.poll())


if __name__ == '__main__':
    unittest.main()
