"""자간 단계 한/글 호출 줄이기 1단계: 줄검사 기억 상태 머신 및 검증 모드(verify) 테스트."""
import os
import unittest
from unittest.mock import Mock, patch
from types import FunctionType

from docfit_engine.spacing import _줄검사기억, _install


class SpacingMemoUnitTest(unittest.TestCase):
    """_줄검사기억 상태 머신 및 검증 모드 기본 동작 단위 테스트."""

    def test_default_mode_is_inactive(self):
        """환경 변수가 없거나 verify가 아니면 기억이 비활성('없음')이다."""
        with patch.dict(os.environ, {}, clear=True):
            memo = _줄검사기억()
            memo.시작()
            self.assertEqual(memo.상태, "없음")
            self.assertFalse(memo.활성)
            # 비활성 상태에서는 호출을 기록하지 않음
            memo.검증_줄범위((0, 0, 10), ((0, 0, 0), (0, 0, 20)))
            self.assertEqual(memo.호출, 0)
            self.assertEqual(memo.적중, 0)
            self.assertIsNone(memo.종료())

    def test_verify_mode_lifecycle_and_hits(self):
        """DOCFIT_SPACING_MEMO=verify일 때의 적중 및 호출 집계 검증."""
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': 'verify'}):
            memo = _줄검사기억()
            memo.시작()
            self.assertEqual(memo.상태, "편집전")
            self.assertTrue(memo.활성)

            log_mock = Mock()

            # 단어모드_줄범위 1회차 (기억 등록)
            pos1 = (0, 0, 10)
            range1 = ((0, 0, 0), (0, 0, 25))
            memo.검증_줄범위(pos1, range1, log_mock)
            self.assertEqual(memo.호출, 1)
            self.assertEqual(memo.적중, 0)
            self.assertEqual(memo.불일치, 0)

            # 단어모드_줄범위 2회차 (동일 위치, 동일 결과 -> 적중)
            memo.검증_줄범위(pos1, range1, log_mock)
            self.assertEqual(memo.호출, 2)
            self.assertEqual(memo.적중, 1)
            self.assertEqual(memo.불일치, 0)
            log_mock.assert_not_called()

            # 단어모드_한글자 1회차 (기억 등록)
            char_pos = (0, 0, 5)
            char_res = ((0, 0, 5), (0, 0, 6), "한")
            memo.검증_한글자(char_pos, False, char_res, log_mock)
            self.assertEqual(memo.호출, 3)
            self.assertEqual(memo.적중, 1)

            # 단어모드_한글자 2회차 (적중)
            memo.검증_한글자(char_pos, False, char_res, log_mock)
            self.assertEqual(memo.호출, 4)
            self.assertEqual(memo.적중, 2)
            log_mock.assert_not_called()

            # 종료 시 상태 복원 및 진단 요약 반환
            summary = memo.종료()
            self.assertEqual(memo.상태, "없음")
            self.assertFalse(memo.활성)
            self.assertIn("기억 적중 2 / 실제 호출 4 / 상태=편집전", summary)

    def test_verify_mode_mismatch_logging(self):
        """검증 모드에서 기억과 실제 결과가 다를 때 불일치 카운트 및 진단로그 확인."""
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': 'verify'}):
            memo = _줄검사기억()
            memo.시작()
            log_mock = Mock()

            pos = (1, 0, 5)
            memo.검증_줄범위(pos, ((1, 0, 0), (1, 0, 10)), log_mock)

            # 동일 위치인데 실제 결과가 달라짐
            memo.검증_줄범위(pos, ((1, 0, 0), (1, 0, 12)), log_mock)
            self.assertEqual(memo.호출, 2)
            self.assertEqual(memo.적중, 0)
            self.assertEqual(memo.불일치, 1)
            log_mock.assert_called_once()
            self.assertIn("[줄검사 기억 불일치]", log_mock.call_args[0][0])

            summary = memo.종료()
            self.assertIn("불일치 1", summary)

    def test_edit_transition_and_cache_invalidation(self):
        """편집 발생 시 '편집전' -> '편집후' 전이 및 기억 비움."""
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': 'verify'}):
            memo = _줄검사기억()
            memo.시작()
            self.assertEqual(memo.상태, "편집전")

            pos = (0, 0, 10)
            range1 = ((0, 0, 0), (0, 0, 20))
            memo.검증_줄범위(pos, range1)
            self.assertIn(('단어모드_줄범위', pos), memo.기억)

            # 편집 발생
            memo.편집발생()
            self.assertEqual(memo.상태, "편집후")
            self.assertEqual(len(memo.기억), 0)

            # 편집후 상태에서는 더 이상 기억 및 적중하지 않음
            memo.검증_줄범위(pos, range1)
            self.assertEqual(len(memo.기억), 0)
            self.assertEqual(memo.적중, 0)

            # 다시 편집발생을 불러도 편집후 상태 유지 (불가능한 전이 방지)
            memo.편집발생()
            self.assertEqual(memo.상태, "편집후")

            summary = memo.종료()
            self.assertEqual(memo.상태, "없음")
            self.assertIn("상태=편집후", summary)


class SpacingIntegrationHooksTest(unittest.TestCase):
    """편집 발생 시 _줄검사_기억_편집발생 트리거 연동 확인."""

    def test_edit_functions_trigger_memo_edit(self):
        """자간적용, 장평적용, 색상적용, 문서편집시도가 _줄검사_기억_편집발생을 부르는지 확인."""
        memo = _줄검사기억()
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': 'verify'}):
            memo.시작()
            self.assertEqual(memo.상태, "편집전")

            # _줄검사_기억_편집발생 호출
            memo.편집발생()
            self.assertEqual(memo.상태, "편집후")
            memo.종료()

    def test_reuse_skips_com_and_edit_disables_reuse(self):
        """실제 설치 함수가 적중 시 COM을 생략하고 편집 후 다시 읽는다."""
        with patch.dict(os.environ, {'DOCFIT_SPACING_MEMO': 'reuse'}):
            for symbol, result, direction in (
                ('단어모드_줄범위', ((0, 0, 0), (0, 0, 10)), None),
                ('단어모드_한글자', None, False),
            ):
                namespace = {'hwp': Mock()}
                def bind(fn, ns):
                    return FunctionType(fn.__code__, ns, fn.__name__, fn.__defaults__, fn.__closure__)
                with patch('docfit_engine.spacing.bind_function', bind):
                    fn = _install(_docfit_namespace=namespace, _docfit_symbol=symbol)
                memo = namespace['_줄검사_기억']
                memo.시작()
                pos = (0, 0, 5)
                if direction is None:
                    memo.검증_줄범위(pos, result)
                else:
                    memo.검증_한글자(pos, direction, result)
                self.assertEqual(fn(pos), result)
                self.assertEqual(memo.적중, 1)
                namespace['hwp'].SetPos.assert_not_called()
                memo.편집발생()
                namespace['hwp'].SetPos.side_effect = RuntimeError('COM reached')
                with self.assertRaisesRegex(RuntimeError, 'COM reached'):
                    fn(pos)
                memo.종료()


if __name__ == '__main__':
    unittest.main()
