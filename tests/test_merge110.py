"""MERGE110: 목록 DB와 서비스 메타데이터를 함께 쓰는 합성 회귀 시험."""
import os
import tempfile
import time
import unittest
from datetime import datetime
from pathlib import Path
from unittest.mock import patch

from docfit_app import rust_scan
from docfit_app.doc_grid_service import DocGridService
from docfit_app.library_db import LibraryDB, ViewSync
from docfit_app.photo_album_service import PhotoAlbumService
from tests.test_svccheck1 import Pipe, reply


class Merge110Tests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.saved_status = rust_scan.get_service_version_status()
        self.addCleanup(lambda: rust_scan.set_service_version_status(
            self.saved_status['status'], self.saved_status['message'], self.saved_status['version']))

    def drain(self, service):
        rows, since = [], 0
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            progress = service.command('scan_progress', {'since': since})
            rows.extend(progress['rows'])
            since = progress['next']
            if progress['state'] not in ('running', 'cancelling') and since >= progress['total']:
                self.assertEqual(progress['state'], 'complete', progress)
                return rows, progress
            time.sleep(0.005)
        self.fail('합성 검색 시간 초과')

    def test_both_views_reuse_service_metadata_and_publish_saved_list_before_query(self):
        for photo in (False, True):
            with self.subTest(photo=photo):
                folder = self.root / ('photos' if photo else 'docs')
                folder.mkdir()
                path = folder / ('synthetic.jpg' if photo else 'synthetic.pdf')
                path.write_bytes(b'synthetic')
                st = path.stat()
                item = dict(path=str(path), name=path.name, size=st.st_size,
                            mtime=st.st_mtime, mtime_ns=st.st_mtime_ns, cloud=False)
                if photo:
                    service = PhotoAlbumService(db_path=folder/'photos.db', thumb_dir=folder/'thumbs',
                                                drive_roots=[folder])
                    self.addCleanup(lambda svc=service: (svc.exif.cancel(), svc.exif.wait(10)))
                else:
                    service = DocGridService(thumb_dir=folder/'thumbs', db_path=folder/'view.db',
                                             drive_roots=[folder])
                expect_snapshot = False

                def query(extensions, excluded=(), **kwargs):
                    if expect_snapshot:
                        # 원천 응답을 기다리는 시점에 저장 목록은 이미 브리지에 도착해야 한다.
                        pending = service.command('scan_progress', {'since': 0})
                        self.assertEqual([r['path'] for r in pending['rows']], [str(path)])
                    pipe = Pipe([reply(state='ready', mode='service', version=2, features=['names_list', 'metadata_v2']),
                                  reply(items=[item], next=1, done=True)])
                    return rust_scan.names_query(extensions, excluded, opener=lambda _: pipe, **kwargs)

                with patch.dict(os.environ, {'DOCFIT_SCAN_ENGINE': ''}), \
                     patch('docfit_app.doc_grid_service.query_names_service', side_effect=query), \
                     patch('docfit_app.library_db.FolderStats._scan', side_effect=AssertionError('중복 stat')), \
                     patch('docfit_app.photo_album_service.read_exif_metadata',
                           return_value=(datetime(2020, 1, 2, 3, 4, 5), 6)) as reader:
                    first = service.command('scan_default' if photo else 'list_default')
                    self.assertTrue(first['ok'])
                    rows, progress = self.drain(service)
                    self.assertEqual([r['path'] for r in rows], [str(path)])
                    self.assertEqual(progress['service_status']['status'], 'ok')
                    if photo:
                        service.exif.wait(10)
                        self.assertEqual(reader.call_count, 1)
                        service.db.toggle_favorite(str(path))
                    expect_snapshot = True
                    second = service.command('scan_default' if photo else 'list_default')
                    self.assertTrue(second['from_db'])
                    rows, progress = self.drain(service)
                    self.assertEqual(len(rows), 1)
                    self.assertNotIn('op', rows[0])
                    if photo:
                        service.exif.wait(10)
                        self.assertEqual(reader.call_count, 1)
                        self.assertEqual(rows[0]['date_taken'], '2020-01-02 03:04:05')
                        self.assertEqual(rows[0]['orientation'], 6)
                        self.assertTrue(rows[0]['favorite'])

    def test_missing_partial_and_unknown_service_metadata_use_folder_fallback(self):
        path = self.root/'synthetic.pdf'
        path.write_bytes(b'synthetic')
        for metadata in ({}, {'size': 0}, {'mtime': 0}, {'size': 0, 'mtime': 0},
                         {'size': None, 'mtime': None}):
            with self.subTest(metadata=metadata):
                db = LibraryDB(self.root/'view.db')
                events = []
                sync = ViewSync(db, events.append, roots=[self.root])
                sync.publish_snapshot()
                sync.feed(dict(path=str(path), name=path.name, **metadata))
                sync.finish(True)
                saved = db.load()[0]
                self.assertEqual(saved[4], path.stat().st_size)
                self.assertAlmostEqual(saved[5], path.stat().st_mtime, places=5)

    def test_empty_file_with_known_mtime_needs_no_stat(self):
        sync = ViewSync(LibraryDB(self.root/'view.db'), lambda _: None, roots=[self.root])
        with patch('docfit_app.library_db.FolderStats.stat', side_effect=AssertionError('빈 파일 stat')):
            sync.feed(dict(path=str(self.root/'empty.pdf'), name='empty.pdf', size=0, mtime=12345))
        self.assertEqual(sync.items()[0]['size'], 0)


if __name__ == '__main__':
    unittest.main()
