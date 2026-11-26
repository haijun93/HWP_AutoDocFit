"""상태형 COM 대역과 디스크의 합성 HWPX로 삭제/삽입/복구 경계를 시험한다."""
from copy import deepcopy
from html import escape
from pathlib import Path
import runpy
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import Mock
from zipfile import ZipFile

from defusedxml import ElementTree as ET

from tests import test_pipeline_review


BOX = ['┌──┬──┐', '│ 가 │ 나 │', '└──┴──┘']


class StatefulHwp:
    def __init__(self, failure=None):
        self.paras = ['앞 문단의 충분히 긴 원문 보존을 확인합니다'] + BOX + ['뒤 문단의 충분히 긴 원문 보존을 확인합니다']
        self.original = deepcopy(self.paras)
        self.bold = '1'
        self.pos = (0, 0, 0)
        self.selection = None
        self.failure = failure
        self.cancelled = False
        self.saves = []
        self.deletes = 0
        self.cell = 0
        self.table = None
        self.HAction = SimpleNamespace(GetDefault=lambda *a: True, Execute=self.execute)
        self.params = SimpleNamespace(HSet=object(), TableProperties=SimpleNamespace())
        self.HParameterSet = SimpleNamespace(HTableCreation=self.params)

    def GetPos(self):
        return self.pos

    def SetPos(self, *pos):
        self.pos = (pos[0], min(pos[1], len(self.paras) - 1), pos[2])

    def SelectText(self, sp, sc, ep, ec):
        self.selection = (sp, ep)
        return True

    def run(self, name):
        area, para, _ = self.pos
        if name == 'MoveDocBegin':
            self.pos = (0, 0, 0)
        elif name == 'MoveNextParaBegin':
            self.SetPos(0, para + 1, 0)
        elif name == 'MoveParaEnd':
            self.pos = (area, para, len(self.paras[para]))
        elif name == 'Delete':
            sp, ep = self.selection
            self.paras[sp:ep + 1] = ['']
            self.selection = None
            self.deletes += 1
            if self.failure == 'delete_false':
                return False  # 변경 후 False도 복원해야 한다.
        elif name == 'TableRightCell':
            if self.failure == 'move_false':
                return False
            self.cell += 1
        return True

    def execute(self, name, params):
        if self.failure == 'create_false':
            return False
        if self.failure == 'create_exception':
            raise RuntimeError('TableCreate 예외')
        if self.failure == 'silent_create':
            return True
        self.table = [[''] * self.params.Cols for _ in range(self.params.Rows)]
        self.paras[self.pos[1]] = self.table
        return True

    def insert(self, text):
        if self.failure == 'text_false':
            return False
        if self.table is not None:
            row, col = divmod(self.cell, self.params.Cols)
            self.table[row][col] = text
        if self.failure == 'cancel':
            self.cancelled = True
        return True

    def SaveAs(self, Path=None, Format=None, arg=None):
        path = globals()['Path'](Path)
        self.saves.append(path)
        if self.failure == 'snapshot_false':
            return False
        def paragraph(text):
            return '<hp:p><hp:run charPrIDRef="0"><hp:t>' + escape(text) + '</hp:t></hp:run></hp:p>'
        parts = []
        for item in self.paras:
            if isinstance(item, str):
                parts.append(paragraph(item))
            else:
                parts.append('<hp:p><hp:run><hp:tbl>' + ''.join(
                    '<hp:tr>' + ''.join('<hp:tc>' + paragraph(cell) + '</hp:tc>' for cell in row) + '</hp:tr>'
                    for row in item) + '</hp:tbl></hp:run></hp:p>')
        with ZipFile(path, 'w') as archive:
            archive.writestr('Contents/header.xml', '<hh:head xmlns:hh="urn:head"><hh:charPr bold="' + self.bold + '"/></hh:head>')
            archive.writestr('Contents/section0.xml', '<hs:sec xmlns:hs="urn:section" xmlns:hp="urn:para">' + ''.join(parts) + '</hs:sec>')
            archive.writestr('BinData/synthetic.bin', b'synthetic image')
        return True

    def open(self, hwp, path, *args):
        # 최초 입력도 이 대역이 작성한 합성 HWPX다.
        with ZipFile(path) as archive:
            root = ET.fromstring(archive.read('Contents/section0.xml'))
            self.paras = []
            for para in root:
                table = para.find('.//{urn:para}tbl')
                if table is None:
                    self.paras.append(''.join(para.itertext()))
                else:
                    self.paras.append([[''.join(cell.itertext()) for cell in row] for row in table])
            self.bold = ET.fromstring(archive.read('Contents/header.xml'))[0].get('bold')
        if self.failure == 'restore_corrupt' and globals()['Path'](path).name == 'before.hwpx':
            self.bold = '0'  # Open=True여도 서식 복원 검증은 실패해야 한다.
        if self.failure == 'restore_false' and globals()['Path'](path).name == 'before.hwpx':
            return False
        self.pos = (0, 0, 0)
        return True


class BoxRollbackTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py'))

    def process(self, failure, finish=True):
        fake = StatefulHwp(failure)
        error, result = None, None
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'synthetic.hwpx'
            fake.failure = None
            fake.SaveAs(source)
            original = source.read_bytes()
            fake.saves.clear()
            fake.failure = failure
            extra = {
                'hwp': fake, 'hwp_run': fake.run, '현재문단_텍스트': lambda: fake.paras[fake.pos[1]],
                '텍스트_삽입': fake.insert, '표준서식_사용': True, '쪽범위_요청': None,
                'stage_enabled': lambda _, key, *a: key == 'text_table_convert',
                '중단_요청됨': lambda: fake.cancelled, '쪽맞춤_원본_보고서_기록': Mock(),
                '알파_일괄서식_가능': lambda: False, '정밀표_선행적용': lambda: True,
                '제목붙임_선행적용': lambda *a, **k: True,
                '문서_처리_1회': lambda *a: finish, '디자인표_보호영역_갱신': Mock(),
            }
            if failure in ('restore_corrupt', 'restore_false'):
                # 복구 실패는 삽입 예외 뒤에만 유도한다.
                fake.execute = lambda *a: (_ for _ in ()).throw(RuntimeError('삽입 예외'))
                fake.HAction.Execute = fake.execute
            try:
                result, _, _ = test_pipeline_review.PipelineReviewTest._process_document(self, source, extra | {
                    '한글_문서_열기': fake.open,
                })
            except RuntimeError as exc:
                error = exc
            self.assertEqual(source.read_bytes(), original)
            output = list(Path(folder).glob('*(서식적용)*'))
            outputs = [path.read_bytes() for path in output]
        return fake, result, error, outputs

    def test_failed_table_execute_restores_original_and_blocks_output(self):
        fake, _, error, outputs = self.process('create_false')
        self.assertEqual(fake.paras, fake.original)
        self.assertIsNotNone(error)
        self.assertEqual(outputs, [])

    def test_insert_exception_restores_original_and_blocks_output(self):
        fake, _, error, outputs = self.process('create_exception')
        self.assertEqual(fake.paras, fake.original)
        self.assertIsNotNone(error)
        self.assertEqual(outputs, [])

    def test_all_mutating_failure_boundaries_restore(self):
        for failure in ('text_false', 'move_false', 'silent_create', 'delete_false', 'cancel'):
            with self.subTest(failure=failure):
                fake, result, error, outputs = self.process(failure)
                self.assertEqual(fake.paras, fake.original)
                self.assertTrue(error is not None or result is False)
                self.assertEqual(outputs, [])

    def test_restore_is_verified_including_character_format(self):
        fake, _, error, outputs = self.process('restore_corrupt')
        self.assertIsNotNone(error)
        self.assertIn('복원', str(error))
        self.assertEqual(outputs, [])

    def test_restore_open_false_is_fatal(self):
        _, _, error, outputs = self.process('restore_false')
        self.assertIsNotNone(error)
        self.assertIn('복원', str(error))
        self.assertEqual(outputs, [])

    def test_snapshot_failure_prevents_any_deletion(self):
        fake, _, error, outputs = self.process('snapshot_false')
        self.assertEqual(fake.deletes, 0)
        self.assertIsNotNone(error)
        self.assertEqual(outputs, [])

    def test_success_preserves_text_order_and_saves_synthetic_hwpx(self):
        fake, result, error, outputs = self.process(None, finish=True)
        self.assertIsNone(error)
        self.assertTrue(result)
        self.assertEqual(fake.paras, ['앞 문단의 충분히 긴 원문 보존을 확인합니다', [['가', '나']], '뒤 문단의 충분히 긴 원문 보존을 확인합니다'])
        self.assertEqual(len(outputs), 1)
        self.assertTrue(outputs[0].startswith(b'PK'))


if __name__ == '__main__':
    unittest.main()
