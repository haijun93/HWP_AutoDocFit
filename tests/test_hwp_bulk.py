import csv
import struct
import tempfile
import threading
import unittest
import zipfile
from pathlib import Path
from unittest import mock

from docfit_core import hwp_bulk

OLE = hwp_bulk.OLE_SIGNATURE + b"\0" * 64


def _hwp5_with_flags(flags: int = 0x01, version: int = 0x05010001) -> bytes:
    """합성 HWP 5.0 OLE 파일 바이트를 생성한다(FileHeader 포함)."""
    header = bytearray(256)
    header[:17] = b"HWP Document File"
    struct.pack_into("<I", header, 32, version)
    struct.pack_into("<I", header, 36, flags)
    data = bytearray(hwp_bulk.OLE_SIGNATURE)
    data.extend(b"\0" * (512 - len(hwp_bulk.OLE_SIGNATURE)))
    data.extend(header)
    data.extend(b"\0" * (1024 - len(data)))
    return bytes(data)


def _hwpx(path):
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("mimetype", "application/hwp+zip")
        z.writestr("Contents/header.xml", "<head/>")
        z.writestr("Contents/section0.xml", "<sec/>")


class FakeHwp:
    """Open: 암호 문서는 suspendpassword면 실패, 암호(responder.current)가 맞으면 성공."""

    def __init__(self, responder=None, password="pw", fail_save=False):
        self.responder, self.password, self.opened = responder, password, None
        self.fail_save = fail_save
        self.calls = []

    def Open(self, path, fmt, arg):
        self.calls.append((Path(path).name, fmt, arg))
        name = Path(path).name
        if name.startswith("암호"):
            if "suspendpassword" in arg:
                return False
            return self.password in (self.responder.current if self.responder else [])
        if Path(path).read_bytes().startswith(b"DRM"):
            return False
        self.opened = path
        return True

    def SaveAs(self, path, fmt, arg):
        if self.fail_save:
            return False
        _hwpx(path)
        return True

    def Clear(self, option):
        self.calls.append(("Clear", option))

    def Quit(self):
        pass


class FindTests(unittest.TestCase):
    def test_skips_system_folders_existing_results_and_other_files(self):
        with tempfile.TemporaryDirectory() as root:
            base = Path(root)
            for rel in ("a.hwp", "sub/b.HWP", "sub/AppData/c.hwp", "Windows/d.hwp", "e.hwpx", "f.txt", "g.hwp",
                        "hnc/skip1.hwp", "hancom/skip2.hwp", "서식/skip3.hwp", "서식예시/skip4.hwp", "hanwp.hwp"):
                (base / rel).parent.mkdir(parents=True, exist_ok=True)
                (base / rel).write_bytes(OLE)
            (base / "g.hwpx").write_bytes(b"x")
            found, skipped = hwp_bulk.find_hwp([base])
            self.assertEqual([p.name for p in found], ["a.hwp", "b.HWP"])
            self.assertEqual(skipped, 1)
            flat, _ = hwp_bulk.find_hwp([base], recursive=False)
            self.assertEqual([p.name for p in flat], ["a.hwp"])
            stop = threading.Event()
            stop.set()
            self.assertEqual(hwp_bulk.find_hwp([base], stop=stop)[0], [])

    def test_classify_and_drm_header(self):
        with tempfile.TemporaryDirectory() as root:
            ole, old = Path(root) / "1.hwp", Path(root) / "2.hwp"
            ole.write_bytes(OLE)
            old.write_bytes(b"HWP Document File V3.00 \x1a\x01\x02\x03\x04\x05")

            # 합성 파일: OLE 서명이 아니고 다양한 DRM 머리를 가진 파일
            drm_samples = [
                Path(root) / "softcamp.hwp",
                Path(root) / "fasoo.hwp",
                Path(root) / "markany.hwp",
                Path(root) / "hanwp.hwp",
            ]
            drm_samples[0].write_bytes(b"SCDSA002" + b"\0" * 40)
            drm_samples[1].write_bytes(b"FASOO_SECURE_HEADER" + b"\0" * 30)
            drm_samples[2].write_bytes(b"MARKANY_DOCUMENT_SA" + b"\0" * 30)
            drm_samples[3].write_bytes(b"DRM_HEADER_MODULE" + b"\0" * 40)

            self.assertEqual(hwp_bulk.classify(ole), "hwp")
            self.assertEqual(hwp_bulk.classify(old), "hwp3")
            for p in drm_samples:
                self.assertEqual(hwp_bulk.classify(p), "protected")
            self.assertEqual(hwp_bulk.classify(Path(root) / "missing.hwp"), "unreadable")

    def test_read_hwp5_header(self):
        with tempfile.TemporaryDirectory() as root:
            # 1) 일반 압축 문서
            normal = Path(root) / "normal.hwp"
            normal.write_bytes(_hwp5_with_flags(flags=0x01))
            info = hwp_bulk.read_hwp5_header(normal)
            self.assertIsNotNone(info)
            self.assertTrue(info.is_compressed)
            self.assertFalse(info.is_distributed)
            self.assertFalse(info.is_drm)

            # 2) 배포용 문서 (FLAG_DISTRIBUTED = 0x04)
            dist = Path(root) / "dist.hwp"
            dist.write_bytes(_hwp5_with_flags(flags=0x01 | 0x04))
            info_dist = hwp_bulk.read_hwp5_header(dist)
            self.assertIsNotNone(info_dist)
            self.assertTrue(info_dist.is_distributed)

            # 3) DRM 플래그 문서 (FLAG_DRM = 0x10)
            drm_flag = Path(root) / "drm_flag.hwp"
            drm_flag.write_bytes(_hwp5_with_flags(flags=0x01 | 0x10))
            info_drm = hwp_bulk.read_hwp5_header(drm_flag)
            self.assertIsNotNone(info_drm)
            self.assertTrue(info_drm.is_drm)


class ConvertTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def _file(self, name, data=OLE):
        path = self.root / name
        path.write_bytes(data)
        return path

    def test_plain_password_and_drm(self):
        responder = hwp_bulk.DialogResponder(pid=None, passwords=["x", "pw"])
        responder.pid = 1  # 창 감시 스레드는 띄우지 않고 암호 순서만 시험한다
        app = FakeHwp(responder)

        # 1. 일반 문서 변환
        plain = hwp_bulk.convert_one(app, self._file("일반.hwp"), responder)
        self.assertEqual(plain.status, "success")
        self.assertEqual(plain.kind, hwp_bulk.KIND_NORMAL)
        self.assertTrue(Path(plain.output).is_file())
        self.assertEqual(app.calls[0], ("일반.hwp", "HWP", "forceopen:true;suspendpassword:true"))

        # 2. 암호 문서 변환 성공
        locked = hwp_bulk.convert_one(app, self._file("암호.hwp"), responder)
        self.assertEqual(locked.status, "success")
        self.assertEqual(locked.kind, hwp_bulk.KIND_PASSWORD)
        self.assertIn("같은 암호", locked.message)

        # 3. 기관 보안(DRM) 문서는 파일 머리로 미리 판별해 한/글 변환 시도(app.Open) 없이 '보안 문서(변환 불가)'로 분류
        calls_before = list(app.calls)
        drm = hwp_bulk.convert_one(app, self._file("보안.hwp", b"SCDSA002_DRM_HEADER" + b"\0" * 40), responder)
        self.assertEqual((drm.status, drm.kind, drm.message), ("protected", hwp_bulk.KIND_DRM, hwp_bulk.MSG_DRM))
        # 변환 시도(app.Open)가 전혀 없었는지 검증
        self.assertEqual(app.calls, calls_before)

        # 4. 이미 있는 파일 건너뜀
        again = hwp_bulk.convert_one(app, self.root / "일반.hwp", responder)
        self.assertEqual(again.status, "skipped")
        self.assertEqual(again.kind, hwp_bulk.KIND_EXISTING)
        self.assertFalse(list(self.root.glob(".~docfit_*")))  # 임시 파일 남기지 않음
        self.assertIn(("Clear", 1), app.calls)  # 저장 묻지 않고 닫아 원본을 덮지 않는다

    def test_distributed_document_classified_without_open(self):
        """배포용 문서는 FileHeader의 FLAG_DISTRIBUTED로 한/글 열기 시도 없이 분류된다."""
        dist_data = _hwp5_with_flags(flags=0x01 | 0x04)
        dist_file = self._file("배포용.hwp", dist_data)
        app = FakeHwp()
        result = hwp_bulk.convert_one(app, dist_file, None)
        self.assertEqual(result.status, "protected")
        self.assertEqual(result.kind, hwp_bulk.KIND_DISTRIBUTED)
        self.assertEqual(result.message, hwp_bulk.MSG_DISTRIBUTED)
        # app.Open이 불리지 않음
        self.assertEqual(app.calls, [])

    def test_save_rejection_classified_as_distributed(self):
        """한/글이 저장을 거부한 경우(배포용 문서 등) 보호 문서로 분류된다."""
        app = FakeHwp(fail_save=True)
        plain_file = self._file("저장거부.hwp")
        result = hwp_bulk.convert_one(app, plain_file, None)
        self.assertEqual(result.status, "protected")
        self.assertEqual(result.kind, hwp_bulk.KIND_DISTRIBUTED)
        self.assertEqual(result.message, hwp_bulk.MSG_DISTRIBUTED)

    def test_password_missing_or_wrong(self):
        responder = hwp_bulk.DialogResponder(pid=None, passwords=["nope"])
        responder.pid = 1
        result = hwp_bulk.convert_one(FakeHwp(responder), self._file("암호.hwp"), responder)
        self.assertEqual((result.status, result.message, result.kind),
                         ("failed", hwp_bulk.MSG_PASSWORD, hwp_bulk.KIND_PASSWORD))
        result = hwp_bulk.convert_one(FakeHwp(), self._file("암호2.hwp"), None)
        self.assertEqual((result.status, result.message, result.kind),
                         ("failed", hwp_bulk.MSG_PASSWORD, hwp_bulk.KIND_PASSWORD))

    def test_run_bulk_writes_report(self):
        self._file("일반.hwp")
        self._file("암호.hwp")
        self._file("보안.hwp", b"DRM_HEADER" + b"\0" * 32)
        dist_data = _hwp5_with_flags(flags=0x01 | 0x04)
        self._file("배포.hwp", dist_data)

        state = hwp_bulk.BulkState()
        with mock.patch.object(hwp_bulk, "hwp_pids", return_value=set()):
            results = hwp_bulk.run_bulk([self.root], app_factory=FakeHwp, state=state, stop=threading.Event(),
                                         report_dir=self.root / "report")
        # 4개 중 1개 변환(일반), 1개 실패(암호), 2개 보호 문서(보안, 배포)
        self.assertEqual((state.phase, state.found, state.succeeded, state.failed, state.protected),
                         ("done", 4, 1, 1, 2))
        self.assertEqual(len(results), 4)

        with open(state.report, encoding="utf-8-sig") as stream:
            rows = list(csv.reader(stream))
        self.assertEqual(rows[0], ["결과", "종류", "원본", "HWPX", "설명"])

        # 행별 검증
        row_map = {Path(r[2]).name: r for r in rows[1:]}
        self.assertEqual(row_map["일반.hwp"][0], "변환")
        self.assertEqual(row_map["일반.hwp"][1], "일반")

        self.assertEqual(row_map["암호.hwp"][0], "실패")
        self.assertEqual(row_map["암호.hwp"][1], "암호")

        self.assertEqual(row_map["보안.hwp"][0], "보안 문서(변환 불가)")
        self.assertEqual(row_map["보안.hwp"][1], "DRM")
        self.assertEqual(row_map["보안.hwp"][4], hwp_bulk.MSG_DRM)

        self.assertEqual(row_map["배포.hwp"][0], "보안 문서(변환 불가)")
        self.assertEqual(row_map["배포.hwp"][1], "배포용")
        self.assertEqual(row_map["배포.hwp"][4], hwp_bulk.MSG_DISTRIBUTED)

        # failures에는 실패(failed) 항목만 들어가고 protected는 들어가지 않음
        self.assertEqual(len(state.failures), 1)
        self.assertEqual(state.as_dict()["failures"][0]["message"], hwp_bulk.MSG_PASSWORD)

    def test_large_image_hwpx_passes_result_validation_while_external_blocked(self):
        """그림 많은 큰 합성 HWPX: 한도를 작게 주입했을 때 결과 검증(convert_one)은 통과하고 외부 입력 검증은 차단된다."""
        from docfit_core.hwpx import HwpxLimits, HwpxSecurityError, validate_hwpx

        class FakeHwpLargeImages(FakeHwp):
            def SaveAs(self, path, fmt, arg):
                with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
                    z.writestr("mimetype", "application/hwp+zip")
                    z.writestr("Contents/header.xml", "<hh:head xmlns:hh='urn:head' />")
                    z.writestr("Contents/section0.xml", "<hs:sec xmlns:hs='urn:section' />")
                    for i in range(5):
                        z.writestr(f"BinData/image{i}.jpg", b"X" * 2000)
                return True

        hwp_file = self._file("큰그림문서.hwp")
        app = FakeHwpLargeImages()
        small_limits = HwpxLimits(max_total_size=1024)

        # 1) convert_one 실행 -> 방금 한/글이 만든 결과물이므로 압축 폭탄 한도 대신 구조 검증을 적용하여 통과!
        with mock.patch("docfit_core.hwpx.HwpxLimits", return_value=small_limits):
            res = hwp_bulk.convert_one(app, hwp_file, None)
        self.assertEqual(res.status, "success")
        self.assertEqual(res.kind, hwp_bulk.KIND_NORMAL)
        out_hwpx = Path(res.output)
        self.assertTrue(out_hwpx.exists())

        # 2) 반면 외부 입력 검증(validate_hwpx 기본/strict 모드에 작은 한도 주입)은 여전히 막힘!
        with self.assertRaises(HwpxSecurityError) as ctx:
            validate_hwpx(out_hwpx, limits=small_limits)
        self.assertIn("압축 해제 예상 크기", str(ctx.exception))

    def test_cloud_placeholder_is_skipped_without_downloading(self):
        """클라우드 전용(온라인 전용) 파일은 내용을 내려받지 않고 건너뛴다."""
        cloud_file = self._file("클라우드.hwp")
        app = FakeHwp()
        with mock.patch("docfit_core.hwp_bulk.is_cloud_placeholder", return_value=True):
            result = hwp_bulk.convert_one(app, cloud_file, None)
        self.assertEqual(result.status, "skipped")
        self.assertEqual(result.kind, hwp_bulk.KIND_CLOUD)
        self.assertEqual(result.message, hwp_bulk.MSG_CLOUD)
        self.assertEqual(app.calls, [])

    def test_locked_file_auto_retries_at_end_of_bulk(self):
        """잠김(다른 프로그램 사용 중) 문서는 일괄 변환 끝에 1회 자동 재시도되어 성공 시 반영된다."""
        locked_file = self._file("잠김문서.hwp")

        attempts = {"count": 0}
        orig_classify = hwp_bulk.classify

        def fake_classify(path):
            if str(path) == str(locked_file):
                attempts["count"] += 1
                if attempts["count"] == 1:
                    exc = OSError("Sharing violation")
                    exc.winerror = 32
                    raise exc
            return orig_classify(path)

        state = hwp_bulk.BulkState()
        with mock.patch("docfit_core.hwp_bulk.classify", side_effect=fake_classify):
            with mock.patch.object(hwp_bulk, "hwp_pids", return_value=set()):
                results = hwp_bulk.run_bulk([locked_file], app_factory=FakeHwp, state=state,
                                             stop=threading.Event(), report_dir=self.root / "report")

        self.assertEqual(state.succeeded, 1)
        self.assertEqual(state.failed, 0)
        self.assertEqual(len(state.failures), 0)
        self.assertEqual(results[0].status, "success")
        self.assertEqual(results[0].kind, hwp_bulk.KIND_NORMAL)

    def test_retry_failures_runs_only_failed_sources(self):
        """실패한 파일 목록만 대상으로 다시 일괄 변환을 돌릴 수 있다."""
        fail_file = self._file("재시도시작.hwp")
        failures = [{"source": str(fail_file), "message": "실패", "kind": hwp_bulk.KIND_PASSWORD}]
        state = hwp_bulk.BulkState()
        with mock.patch.object(hwp_bulk, "hwp_pids", return_value=set()):
            results = hwp_bulk.retry_failures(failures, app_factory=FakeHwp, state=state,
                                               stop=threading.Event(), report_dir=self.root / "report")
        self.assertEqual(state.succeeded, 1)
        self.assertEqual(state.failed, 0)
        self.assertEqual(results[0].status, "success")


if __name__ == "__main__":
    unittest.main()
