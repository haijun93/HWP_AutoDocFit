import tempfile, unittest
from pathlib import Path
from docfit_core.token_economizer import SessionEconomizer

class TokenEconomizerTests(unittest.TestCase):
    def test_repeated_fact_and_persistence(self):
        with tempfile.TemporaryDirectory() as d:
            e=SessionEconomizer(Path(d)); self.assertFalse(e.record_fact('tests','python -m unittest')); self.assertFalse(e.record_fact('tests','python -m unittest')); self.assertTrue(e.record_fact('tests','python -m unittest')); self.assertEqual(SessionEconomizer(Path(d)).active_facts()['tests'],'python -m unittest')
    def test_loop_guard_once(self):
        with tempfile.TemporaryDirectory() as d:
            e=SessionEconomizer(Path(d)); self.assertFalse(e.loop_guard('x')); self.assertFalse(e.loop_guard('x')); self.assertTrue(e.loop_guard('x')); self.assertFalse(e.loop_guard('x'))
