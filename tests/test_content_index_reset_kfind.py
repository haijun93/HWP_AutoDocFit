"""INTEG119: 색인 초기화가 실제 kfind 에 담긴 앱 문서를 지우는지(격리 파이프·합성 자료만)."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import tests  # isolate all profile/settings access before importing services
from docfit_app.kfind_client import KfindClient, KfindError
from docfit_app.timemachine_service import TimemachineService
from docfit_core.timemachine.kfind_records import doc_from_records


def synthetic(path, scopes):
    record = SimpleNamespace(paragraph_index=0, level_rank=8, text="합성 예산 집행")
    stat = SimpleNamespace(st_size=10, st_mtime=1_700_000_000, st_ctime=1_700_000_000)
    return doc_from_records(path, [record], stat, "2023-11-15", scopes)


@unittest.skipUnless(os.name == "nt" and os.environ.get("KFIND_TEST_EXE"),
                     "explicit synthetic kfind build required")
class ResetRemovesServiceDocumentsTest(unittest.TestCase):
    def test_every_app_document_is_removed_and_other_scopes_stay(self):
        with tempfile.TemporaryDirectory() as folder:
            pipe = rf"\\.\pipe\integ119-reset-{uuid.uuid4().hex}"
            server = subprocess.Popen(
                [os.environ["KFIND_TEST_EXE"], "--user", "--no-auto-names",
                 "--pipe-name", pipe, "--index-root", folder],
                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            client = KfindClient(pipe, timeout=20.0)
            try:
                deadline = time.monotonic() + 20
                while True:
                    try:
                        client.status()
                        break
                    except KfindError:
                        self.assertLess(time.monotonic(), deadline)
                        client.close()
                        time.sleep(0.1)
                # 긴 경로 450개: 열쇠를 한 번에 보내면 요청 한도 64KB를 넘는다.
                long_folder = "C:\\synthetic\\" + "긴 폴더 이름 " * 8
                for n in range(450):
                    client.ingest(synthetic(f"{long_folder}\\문서{n:04}.txt", ["tm"]))
                other = synthetic("C:\\synthetic\\memo.txt", ["box:1"])
                other["doc_key"] = "memo:1:1"
                client.ingest(other)
                self.assertEqual(client.status()["docs"], 451)

                constants = {name: value for name, value in vars(TimemachineService).items()
                             if name.startswith("_KFIND_")}
                removed, error = TimemachineService._clear_kfind_documents(
                    SimpleNamespace(kfind=client, **constants))
                self.assertIsNone(error)
                self.assertEqual(removed, 450)
                self.assertEqual(client.status()["docs"], 1)
            finally:
                client.close()
                server.kill()
                server.wait(timeout=10)


if __name__ == "__main__":
    unittest.main()
