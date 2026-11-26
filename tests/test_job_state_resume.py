"""JOBSTATE1 F08: 파일 키·옵션 지문 기준 이어하기."""
import inspect
from pathlib import Path
import unittest

from tests import test_job_state_batch as batch_tests
from tests import test_job_state_save as save_tests


class ResumeEngineTests(unittest.TestCase):
    setUpClass = classmethod(batch_tests.BatchLoopTests.setUpClass.__func__)
    run_batch = batch_tests.BatchLoopTests.run_batch

    def test_resume_skips_files_by_key_and_keeps_their_state(self):
        def completed(files):
            output = Path(files[1]).with_name('b-result.hwpx')
            output.write_bytes(b'x')
            return [{'source': files[1], 'output': str(output), 'status': 'integrity_failed'},
                    {'source': files[2], 'output': files[2] + '.missing', 'status': 'saved'}]
        run = self.run_batch([True, True, True], 완료_문서=completed)
        self.assertEqual(run['calls'], [0, 2])   # 결과가 사라진 c 는 다시 처리한다.
        kept = run['records'][0]
        self.assertEqual((kept['source'], kept['status'], kept['success'], kept['integrity_ok']),
                         (run['files'][1], 'integrity_failed', True, False))


class ResumeScreenTests(save_tests.ScreenTests):
    test_integrity_failure_is_shown_separately = None

    def stop_after_b(self):
        self.start()
        self.feed(('document_error', self.files[0], '실패'),
                  ('saved', self.files[1], str(self.output), 1), ('stopped', None))

    def test_resume_passes_completed_files_not_a_count(self):
        self.stop_after_b()
        args, kwargs = self.start()
        # 화면이 넘기는 위치 인자가 실제 작업_실행의 매개변수와 맞아야 한다.
        bound = inspect.signature(self.engine).bind(*args, **kwargs).arguments
        self.assertEqual(bound['실행모드'], 'all')
        self.assertEqual(args[0], self.files)
        self.assertEqual(bound['시작_인덱스'], 1)        # 위치(시작_인덱스)로 건너뛰지 않는다.
        self.assertEqual([(d['source'], d['output'], d['status']) for d in kwargs['완료_문서']],
                         [(self.files[1], str(self.output), 'saved')])

    def test_changed_option_starts_over(self):
        self.stop_after_b()
        self.app.punctuation_var.set(not self.app.punctuation_var.get())
        _, kwargs = self.start()
        self.assertEqual(kwargs['완료_문서'], [])
        self.assertEqual(self.app._결과목록, [])


if __name__ == '__main__':
    unittest.main()
