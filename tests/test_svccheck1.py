"""SVCCHECK1: 합성 응답만 사용. 설치 서비스·실제 문서에는 접속하지 않는다."""
import io
import json
import os
import struct
import tempfile
import unittest
from datetime import datetime
from pathlib import Path
from unittest.mock import patch, Mock

from docfit_app import rust_scan as rs
from docfit_app.doc_grid_service import DocGridService
from docfit_app.photo_album_service import PhotoAlbumService
from docfit_app.kfind_process import restart_service


class Pipe:
    def __init__(self, replies):
        self.replies = iter(replies)
        self.requests = []
        self.response = io.BytesIO()

    def __enter__(self): return self
    def __exit__(self, *_): return False

    def write(self, frame):
        self.requests.append(json.loads(frame[4:]))
        body = json.dumps(next(self.replies)).encode()
        self.response = io.BytesIO(struct.pack('<I', len(body)) + body)
        return len(frame)

    def read(self, size): return self.response.read(size)


def reply(**result): return {'ok': True, 'result': result}


class SvcCheckTests(unittest.TestCase):
    def setUp(self):
        self.saved = rs.get_service_version_status()
        rs.set_service_version_status('unknown')
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.addCleanup(lambda: rs.set_service_version_status(self.saved['status'],
                        self.saved['message'], self.saved['version']))

    def test_modern_capabilities_even_while_building(self):
        pipe = Pipe([reply(state='building', version=2, features=['names_list', 'metadata_v2'])])
        state = rs.check_service_version(opener=lambda _: pipe)
        self.assertEqual(state['status'], 'ok')
        self.assertEqual(state['version'], 2)
        self.assertGreater(state['last_checked'], 1_700_000_000)
        self.assertEqual(len(pipe.requests), 1)

    def test_advertised_missing_command_is_outdated(self):
        pipe = Pipe([reply(state='ready', features=[], commands=['names_status'])])
        self.assertIsNone(rs.names_query({'hwpx'}, opener=lambda _: pipe))
        self.assertTrue(rs.get_service_version_status()['outdated'])
        self.assertEqual(len(pipe.requests), 1)

    def test_real_old_error_envelope_is_detected_and_redacted(self):
        secret = 'PRIVATE_DOCUMENT_NAME'
        pipe = Pipe([reply(state='ready', mode='service'),
                     {'ok': False, 'error': "bad request: unknown variant `names_list` " + secret}])
        with self.assertLogs(rs.logger, level='DEBUG') as log:
            self.assertIsNone(rs.names_query({'hwpx'}, opener=lambda _: pipe))
        self.assertTrue(rs.get_service_version_status()['outdated'])
        self.assertNotIn(secret, json.dumps(rs.get_service_version_status()) + str(log.output))

    def test_old_supported_protocol_is_probed_without_inventing_version(self):
        pipe = Pipe([reply(state='ready'), reply(items=[], next=0, done=True)])
        state = rs.check_service_version(opener=lambda _: pipe)
        self.assertEqual(state['status'], 'outdated')
        self.assertIsNone(state['version'])
        self.assertEqual(pipe.requests[1]['command'], 'names_list')
        self.assertEqual(pipe.requests[1]['limit'], 1)

    def test_transient_failure_is_unavailable_not_outdated(self):
        pipe = Pipe([{'ok': False, 'error': 'names index unavailable PRIVATE_PATH'}])
        state = rs.check_service_version(opener=lambda _: pipe)
        self.assertEqual(state['status'], 'unavailable')
        self.assertFalse(state['outdated'])
        self.assertNotIn('PRIVATE_PATH', state['message'])

    def test_oversize_response_and_non_object_result_are_rejected(self):
        class Oversize(Pipe):
            def write(self, _):
                self.response = io.BytesIO(struct.pack('<I', rs._MAX_PIPE_RESPONSE + 1))
        for pipe in [Oversize([]), Pipe([{'ok': True, 'result': []}])]:
            with self.subTest(pipe=type(pipe).__name__):
                self.assertEqual(rs.check_service_version(opener=lambda _: pipe)['status'], 'unavailable')

    def test_cursor_and_metadata_with_old_rows_are_preserved(self):
        pipe = Pipe([reply(state='ready', mode='service'),
            reply(items=[{'path': 'C:/Synthetic/a.hwpx', 'name': 'a.hwpx', 'size': 0,
                          'mtime': 1732600000.5, 'mtime_ns': 1732600000500000000, 'cloud': True}], next=1234, done=False),
            reply(items=[{'path': 'C:/Synthetic/b.hwpx', 'name': 'b.hwpx'}], next=9000, done=True)])
        emitted = []
        with patch('docfit_app.library_scope.current_default_roots', return_value=[Path('C:/Synthetic')]):
            rows = rs.names_query({'hwpx'}, opener=lambda _: pipe, on_row=emitted.append)
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows, emitted)
        self.assertEqual(rows[0]['size'], 0)
        self.assertTrue(rows[0]['cloud'])
        self.assertEqual(rows[0]['mtime_ns'], 1732600000500000000)
        self.assertNotIn('size', rows[1])
        self.assertEqual([r['offset'] for r in pipe.requests[1:]], [0, 1234])
        self.assertEqual(rs.get_service_version_status()['status'], 'outdated')

    def test_invalid_metadata_is_unavailable(self):
        for bad in [{'size': -1}, {'mtime': float('nan')}, {'cloud': 'false'}, {'mtime_ns': 'unknown'}]:
            with self.subTest(keys=list(bad)):
                pipe = Pipe([reply(state='ready', mode='service'),
                    reply(items=[{'path': 'C:/Synthetic/a.hwpx', 'name': 'a.hwpx', **bad}], next=1, done=True)])
                self.assertIsNone(rs.names_query({'hwpx'}, opener=lambda _: pipe))
                self.assertEqual(rs.get_service_version_status()['status'], 'unavailable')

    def test_stalled_cursor_falls_back(self):
        pipe = Pipe([reply(state='ready', mode='service'), reply(items=[], next=0, done=False)])
        self.assertIsNone(rs.names_query({'hwpx'}, opener=lambda _: pipe))
        self.assertEqual(rs.get_service_version_status()['status'], 'unavailable')

    def test_real_data_guard_covers_both_entry_points(self):
        with patch.dict(os.environ, {'DOCFIT_FORBID_REAL_DATA': '1'}), \
             patch('docfit_app.kfind_process.is_pipe_alive') as alive:
            rs.check_service_version()
            self.assertIsNone(rs.names_query({'hwpx'}))
        alive.assert_not_called()

    def test_both_bridges_require_confirmation_and_reset_after_restart(self):
        services = [(DocGridService(thumb_dir=self.root/'docs'), 'doc_grid_service'),
                    (PhotoAlbumService(db_path=self.root/'photos.db', thumb_dir=self.root/'photos'), 'photo_album_service')]
        for service, module in services:
            with self.subTest(module=module), patch('docfit_app.'+module+'.restart_service', return_value=(True, '완료')) as restart:
                self.assertFalse(service.command('service_restart')['ok'])
                restart.assert_not_called()
                rs.set_service_version_status('outdated')
                self.assertTrue(service.command('scan_progress')['service_status']['outdated'])
                res = service.command('service_restart', {'confirmed': True})
                self.assertTrue(res['ok'])
                self.assertEqual(res['service_status']['status'], 'unknown')
                restart.assert_called_once()

    def test_restart_uses_one_elevation_and_reports_failure(self):
        for success in [True, False]:
            with self.subTest(success=success), patch.dict(os.environ, {'DOCFIT_FORBID_REAL_DATA': '0'}), \
                 patch('docfit_app.kfind_process.subprocess.run', return_value=Mock(returncode=0, stdout='--restart')), \
                 patch('docfit_app.kfind_process._run_elevated', return_value=(success, 'PRIVATE_PATH')) as run:
                ok, msg = restart_service(exe=Path('synthetic/kfind.exe'))
                self.assertEqual(ok, success)
                self.assertNotIn('PRIVATE_PATH', msg)
                self.assertEqual(run.call_count, 1)
                self.assertEqual(run.call_args.args[1], '--restart')

    def test_old_helper_with_zero_exit_does_not_report_restart_success(self):
        with patch.dict(os.environ, {'DOCFIT_FORBID_REAL_DATA': '0'}), \
             patch('docfit_app.kfind_process.subprocess.run', return_value=Mock(returncode=0, stdout='--start | --stop')), \
             patch('docfit_app.kfind_process._run_elevated') as run:
            ok, message = restart_service(exe=Path('synthetic/kfind.exe'))
        self.assertFalse(ok)
        self.assertIn('이전 버전', message)
        run.assert_not_called()

    def test_restart_guard_never_elevates_in_tests(self):
        with patch.dict(os.environ, {'DOCFIT_FORBID_REAL_DATA': '1'}), \
             patch('docfit_app.kfind_process._run_elevated') as run:
            self.assertFalse(restart_service()[0])
        run.assert_not_called()

    def test_photo_metadata_keeps_exif_and_skips_unchanged_or_cloud_reads(self):
        svc = PhotoAlbumService(db_path=self.root/'photos.db', thumb_dir=self.root/'thumbs')
        path = str(self.root/'synthetic.jpg')
        row = {'size': 1234, 'mtime': 1732600000.5, 'cloud': False}
        with patch('docfit_app.photo_album_service.read_exif_metadata', return_value=(datetime(2020,1,2,3,4,5),6)) as exif:
            self.assertEqual(svc._apply_default_rows([(path,row,None)], 'service', None)['added'], 1)
            old = svc.db.list_photos()[0]
            self.assertEqual(old['date_taken'], '2020-01-02 03:04:05')
            self.assertEqual(old['orientation'], 6)
            svc._apply_default_rows([(path,row,old)], 'service', None)
            svc._apply_default_rows([(path,{},old)], 'service', None)
            self.assertEqual(exif.call_count, 1)
            cloud = {'size': 0, 'mtime': row['mtime'], 'cloud': True}
            svc._apply_default_rows([(path,cloud,old)], 'service', None)
            self.assertEqual(exif.call_count, 1)
            saved = svc.db.list_photos()[0]
            self.assertEqual(saved['size'], 0)
            self.assertEqual(saved['cloud'], 1)


if __name__ == '__main__':
    unittest.main()
