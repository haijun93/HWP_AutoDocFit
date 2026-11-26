"""필수 글자 모양 수정 실패는 단계/문서 실패이며 결과 저장에 도달하지 않는다."""
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import Mock, patch

from docfit_engine.pipeline import PipelineCallbacks
from tests import test_pipeline_review
from tests.test_hwpx_core import make_hwpx


class CharShapeHwp:
    def __init__(self, outcomes=(False,), item_failure=None):
        self.outcomes = list(outcomes)
        self.item_failure = item_failure
        self.items = {}
        self.applied = {}
        self.executions = 0

    def CreateAction(self, name):
        owner = self

        class Params:
            def SetItem(self, key, value):
                if key == owner.item_failure:
                    raise RuntimeError('SetItem 예외')
                owner.items[key] = value

        class Action:
            def CreateSet(self):
                return Params()

            def Execute(self, params):
                owner.executions += 1
                result = owner.outcomes.pop(0)
                if isinstance(result, Exception):
                    raise result
                if result:
                    owner.applied.update(owner.items)
                return result

        return Action()

    def PointToHwpUnit(self, value):
        return value * 100

    def FontType(self, value):
        return {'TTF': 1, 'HFT': 2}[value]


class RequiredEditTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'hwp-auto-docfit.py'))

    def test_execute_false_and_exception_propagate(self):
        fn = self.ns['문자모양_적용_현재선택']
        for result in (False, RuntimeError('Execute 예외')):
            with self.subTest(result=result), patch.dict(fn.__globals__, {'hwp': CharShapeHwp((result,)), '로그': Mock()}):
                with self.assertRaises(RuntimeError):
                    fn(굵게=True)

    def test_requested_parameter_failure_is_required(self):
        fn = self.ns['문자모양_적용_현재선택']
        with patch.dict(fn.__globals__, {'hwp': CharShapeHwp((True,), item_failure='Bold'), '로그': Mock()}):
            with self.assertRaises(RuntimeError):
                fn(굵게=True)

    def test_setitem_false_is_required_failure(self):
        fn = self.ns['문자모양_적용_현재선택']
        fake = CharShapeHwp((True,))
        action = fake.CreateAction('CharShape')
        params = action.CreateSet()
        params.SetItem = lambda *args: False
        action.CreateSet = lambda: params
        fake.CreateAction = lambda _: action
        with patch.dict(fn.__globals__, {'hwp': fake, '로그': Mock()}):
            with self.assertRaisesRegex(RuntimeError, '필드 설정 실패'):
                fn(굵게=True)
        self.assertEqual(fake.executions, 0)

    def test_missing_hwp_is_required_failure(self):
        fn = self.ns['문자모양_적용_현재선택']
        with patch.dict(fn.__globals__, {'hwp': None, '로그': Mock()}):
            with self.assertRaises(RuntimeError):
                fn(굵게=True)

    def test_success_and_font_type_retry_keep_existing_behavior(self):
        fn = self.ns['문자모양_적용_현재선택']
        fake = CharShapeHwp((False, True))
        with patch.dict(fn.__globals__, {'hwp': fake, '로그': Mock(), '_글꼴형식': {}}):
            fn(폰트='합성 글꼴', 굵게=True)
        self.assertEqual(fake.executions, 2)
        self.assertEqual(fake.applied['Bold'], 1)
        self.assertEqual(fake.applied['FontTypeHangul'], 2)

    def test_font_retry_exhaustion_is_required_failure(self):
        fn = self.ns['문자모양_적용_현재선택']
        fake = CharShapeHwp((False, False))
        with patch.dict(fn.__globals__, {'hwp': fake, '로그': Mock(), '_글꼴형식': {}}):
            with self.assertRaisesRegex(RuntimeError, '문자모양 적용 실패'):
                fn(폰트='합성 글꼴')
        self.assertEqual(fake.executions, 2)

    def test_spacing_preservation_read_error_blocks_edit(self):
        fn = self.ns['문자모양_적용_현재선택']
        fake = CharShapeHwp((True,))
        with patch.dict(fn.__globals__, {'hwp': fake, '로그': Mock()}):
            with self.assertRaisesRegex(RuntimeError, '자간 보존값 읽기 실패'):
                fn(굵게=True, 자간_유지=True)
        self.assertEqual(fake.executions, 0)

    def test_swallowed_edit_error_still_fails_stage_callback(self):
        fn = self.ns['_문서_처리_1회']
        events = []

        def action():
            try:
                self.ns['문자모양_적용_현재선택'](굵게=True)
            except RuntimeError:
                pass  # 기존 호출부의 넓은 예외 처리까지 재현한다.
            return True

        settings = {
            'hwp': CharShapeHwp(), '중단_요청됨': lambda: False,
            'hwp_run': lambda _: False, '순회_시작': Mock(), '로그': Mock(),
            '단계표시': Mock(), '상태': Mock(), '작업_모드': 'unify',
            '서식통일_전체_적용': action,
            'stage_enabled': lambda *a: False,
        }
        callbacks = PipelineCallbacks(on_stage_enter=lambda name: events.append(('enter', name)),
                                      on_stage_exit=lambda name, ok: events.append(('exit', ok)),
                                      on_stage_error=lambda name, err: events.append(('error', str(err))))
        with patch.dict(fn.__globals__, settings):
            with self.assertRaises(RuntimeError):
                fn('synthetic.hwpx', callbacks=callbacks)
        self.assertTrue(any(event[0] == 'error' for event in events))
        self.assertNotIn(('exit', True), events)

    def test_failed_required_edit_blocks_document_save_even_if_caught(self):
        fake = CharShapeHwp()
        fake.SaveAs = Mock(return_value=True)

        def one_pass(*args):
            try:
                self.ns['문자모양_적용_현재선택'](굵게=True)
            except RuntimeError:
                pass
            return True

        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'synthetic.hwpx'
            make_hwpx(source)
            with self.assertRaisesRegex(RuntimeError, '문자모양 적용 실패'):
                test_pipeline_review.PipelineReviewTest._process_document(self, source, {
                    'hwp': fake, '문서_처리_1회': one_pass,
                })
        fake.SaveAs.assert_not_called()

    def test_previous_document_failure_is_cleared_at_next_document(self):
        fake = CharShapeHwp((True,))
        fake.SaveAs = Mock(return_value=True)
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'synthetic.hwpx'
            make_hwpx(source)
            result, _, _ = test_pipeline_review.PipelineReviewTest._process_document(self, source, {
                'hwp': fake, '문서_처리_1회': lambda *a: False,
                '_필수수정_실패': '이전 문서 실패',
            })
        self.assertFalse(result)
        fake.SaveAs.assert_not_called()

    def test_caught_spacing_restore_error_also_blocks_document_save(self):
        fake = CharShapeHwp()
        fake.SaveAs = Mock(return_value=True)
        start, end = (0, 0, 0), (0, 0, 2)
        runs = ([(start, end, (0,) * 7)], [(start, end, (100,) * 7)])

        def apply_ratio(saved, delta):
            if delta == 0:
                raise RuntimeError('장평 복원 실패')

        def search(maximum, check, **kwargs):
            check(5)
            return None

        def one_pass(*args):
            try:
                self.ns['단어_장평_추가축소_시도'](start, end, start, end, 4, 보관=runs)
            except RuntimeError:
                pass
            return True

        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'synthetic.hwpx'
            make_hwpx(source)
            with self.assertRaisesRegex(RuntimeError, '복원 실패'):
                test_pipeline_review.PipelineReviewTest._process_document(self, source, {
                    'hwp': fake, '문서_처리_1회': one_pass, '장평_줄이기_사용': True,
                    '단어모드_장평적용': apply_ratio, '단어모드_자간적용': lambda *a: None,
                    '단어모드_줄범위': lambda _: (start, end), '최소_성공단계_탐색': search,
                })
        fake.SaveAs.assert_not_called()

    def test_movement_false_without_edit_failure_is_still_allowed(self):
        fn = self.ns['_문서_처리_1회']
        with patch.dict(fn.__globals__, {
            'hwp_run': lambda _: False, '순회_시작': Mock(), '중단_요청됨': lambda: False,
            '로그': Mock(), '단계표시': Mock(), '상태': Mock(), '작업_모드': 'unify',
            '서식통일_전체_적용': lambda: True, 'stage_enabled': lambda *a: False,
        }):
            self.assertTrue(fn('synthetic.hwpx'))


if __name__ == '__main__':
    unittest.main()
