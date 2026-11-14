"""서로 다른 줄범위 요청에서 경계 읽기만 공유하는 기억 시험."""
import os
import unittest
from unittest.mock import Mock, patch
from types import FunctionType

from docfit_engine.spacing import _줄검사기억, _install


class BoundaryMemoTest(unittest.TestCase):
    def test_different_line_requests_share_only_boundary_scan(self):
        """줄범위 전체는 다시 재되 공통 경계의 COM 읽기만 생략한다."""
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': 'reuse'}):
            start, end, probe = (0, 2, 0), (0, 2, 10), (0, 2, 11)
            hwp = Mock()
            hwp.GetPos.side_effect = [start, end, probe, probe] * 2
            scan = Mock(return_value=False)
            namespace = {'hwp': hwp, 'hwp_run': Mock(),
                         '두_위치_사이_줄바꿈_문자인가': scan}
            def bind(fn, ns):
                return FunctionType(fn.__code__, ns, fn.__name__, fn.__defaults__, fn.__closure__)
            with patch('docfit_engine.spacing.bind_function', bind):
                fn = _install(_docfit_namespace=namespace, _docfit_symbol='단어모드_줄범위')
            memo = namespace['_줄검사_기억']
            memo.시작()
            self.assertEqual(fn((0, 2, 3)), (start, end))
            self.assertEqual(fn((0, 2, 7)), (start, end))
            self.assertEqual(hwp.GetPos.call_count, 8)
            scan.assert_called_once_with(end, probe)
            self.assertEqual(memo.적중, 1)

    def test_exact_boundary_reuse_and_edit_invalidation(self):
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': 'reuse'}):
            memo = _줄검사기억()
            memo.시작()
            read = Mock(return_value=False)
            a, b = (0, 2, 10), (0, 2, 11)
            self.assertFalse(memo.줄바꿈_검사(a, b, read))
            self.assertFalse(memo.줄바꿈_검사(list(a), list(b), read))
            read.assert_called_once_with(a, b)
            memo.줄바꿈_검사(a, (0, 2, 12), read)
            self.assertEqual(read.call_count, 2)
            memo.편집발생()
            memo.줄바꿈_검사(a, b, read)
            memo.줄바꿈_검사(a, b, read)
            self.assertEqual(read.call_count, 4)
            self.assertEqual(memo.기억, {})
            memo.종료()

    def test_verify_reads_and_reports_mismatch(self):
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': 'verify'}):
            memo = _줄검사기억()
            memo.시작()
            read = Mock(side_effect=[False, True])
            log = Mock()
            a, b = (0, 2, 10), (0, 2, 11)
            self.assertFalse(memo.줄바꿈_검사(a, b, read, log))
            self.assertTrue(memo.줄바꿈_검사(a, b, read, log))
            self.assertEqual(read.call_count, 2)
            self.assertEqual(memo.불일치, 1)
            log.assert_called_once()

    def test_default_reads_without_retaining_boundary(self):
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': ''}):
            memo = _줄검사기억()
            memo.시작()
            read = Mock(return_value=True)
            for _ in range(2):
                self.assertTrue(memo.줄바꿈_검사((0, 0, 1), (0, 0, 2), read))
            self.assertEqual(read.call_count, 2)
            self.assertEqual(memo.기억, {})
