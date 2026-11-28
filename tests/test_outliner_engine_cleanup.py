"""INTEG119: 상주 아웃라이너 엔진의 임시 사본 정리와 끄기 설정(합성 자료만)."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import tests  # isolate all profile/settings access before importing services
from docfit_app.outliner_engine import OWNER, PREFIX, sweep_stale
from docfit_app.outliner_service import RESIDENT_ENV, RESIDENT_SETTING, OutlinerService


class FakeSettings:
    def __init__(self, data):
        self.data = data

    def read(self):
        return dict(self.data)


class ResidentSwitchTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)

    def service(self, **kwargs):
        service = OutlinerService(self.root, executable=self.root / 'outliner.exe', **kwargs)
        self.addCleanup(service.close)
        return service

    def test_resident_engine_is_on_by_default_and_can_be_turned_off(self):
        with mock.patch.dict(os.environ):
            os.environ.pop(RESIDENT_ENV, None)
            self.assertIsNotNone(self.service()._transport)
            self.assertIsNotNone(self.service(settings=FakeSettings({}))._transport)
            self.assertIsNone(self.service(settings=FakeSettings({RESIDENT_SETTING: False}))._transport)
            os.environ[RESIDENT_ENV] = '0'
            self.assertIsNone(self.service()._transport)


@unittest.skipUnless(os.name == 'nt', 'Windows file locking')
class SweepTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)

    def leftover(self, name, *files):
        folder = self.root / (PREFIX + name)
        folder.mkdir()
        for file in files:
            (folder / file).write_bytes(b'x')
        return folder

    def test_removes_only_unowned_copies_of_our_own_files(self):
        dead = self.leftover('dead', OWNER, 'outliner.exe')
        live = self.leftover('live', OWNER, 'outliner.exe')
        foreign = self.leftover('foreign', OWNER, 'notes.txt')
        starting = self.leftover('starting')
        old = self.leftover('old', 'outliner.exe')
        past = time.time() - 7200
        os.utime(old, (past, past))
        with open(live / OWNER, 'wb'):
            sweep_stale(self.root, 'outliner.exe')
            self.assertTrue((live / 'outliner.exe').exists())
        self.assertFalse(dead.exists())
        self.assertFalse(old.exists())
        self.assertTrue((foreign / 'notes.txt').exists())
        self.assertTrue(starting.exists())


@unittest.skipUnless(os.name == 'nt' and os.environ.get('OUTLINER_TEST_EXE'),
                     'explicit synthetic release engine required')
class ForcedExitTest(unittest.TestCase):
    def test_killed_app_leaves_no_running_engine_and_next_start_removes_the_copy(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            packaged = root / 'outliner.exe'
            shutil.copy2(os.environ['OUTLINER_TEST_EXE'], packaged)
            code = (
                'import json,os,sys,time; from docfit_app.outliner_service import OutlinerService; '
                's=OutlinerService(sys.argv[1],executable=sys.argv[2],runtime_parent=sys.argv[1]); '
                's.execute({"action":"parse","page":"Home","markdown":"- synthetic\\n"}); '
                'print(json.dumps({"pid":os.getpid(),"private":s._transport._resources["directory"].name}),'
                'flush=True); time.sleep(60)'
            )
            parent = subprocess.Popen(
                [sys.executable, '-c', code, folder, str(packaged)], stdout=subprocess.PIPE,
                text=True, encoding='utf-8', creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            try:
                info = json.loads(parent.stdout.readline())
                private = Path(info['private'])
                # 설치 프로그램: 앱이 상주 중이어도 원본은 바꿀 수 있다.
                packaged.rename(root / 'old.exe')
                shutil.copy2(root / 'old.exe', packaged)
                os.kill(info['pid'], signal.SIGTERM)  # Windows: TerminateProcess
                parent.wait(timeout=15)
            finally:
                parent.kill()
                parent.stdout.close()
            self.assertTrue(private.exists())  # 정리 코드가 돌 기회가 없었다.
            # 입력 파이프가 닫혀 엔진이 스스로 끝나야 실행 파일을 지울 수 있다.
            deadline = time.monotonic() + 15
            while True:
                try:
                    (private / 'outliner.exe').unlink()
                    break
                except PermissionError:
                    self.assertLess(time.monotonic(), deadline, '엔진이 끝나지 않았습니다.')
                    time.sleep(0.1)
            (private / 'outliner.exe').write_bytes(b'leftover')
            service = OutlinerService(root / 'data', executable=packaged, runtime_parent=root)
            try:
                service.execute({'action': 'parse', 'page': 'Home', 'markdown': '- synthetic\n'})
                self.assertFalse(private.exists())
            finally:
                service.close()
            self.assertEqual(list(root.glob(PREFIX + '*')), [])


if __name__ == '__main__':
    unittest.main()
