"""Pure helper and SQL timing tests; no host paths or COM."""
import random
import sqlite3
import unittest
from scripts.benchmark_is1 import SQLTimer, sentence, sql_key, summary


class BenchmarkTests(unittest.TestCase):
    def test_summary_nearest_rank(self):
        result = summary([4, 1, 3, 2])
        self.assertEqual(result['median_ms'], 2.5)
        self.assertEqual(result['p95_ms'], 4)
        self.assertEqual(result['samples_ms'], [4, 1, 3, 2])

    def test_summary_empty(self):
        with self.assertRaises(ValueError):
            summary([])

    def test_sentence_deterministic(self):
        self.assertEqual(sentence(random.Random(7)), sentence(random.Random(7)))
        self.assertEqual(len(sentence(random.Random(7)).split()), 12)
        self.assertTrue(sentence(random.Random(7)).endswith('.'))

    def test_sql_key(self):
        self.assertEqual(sql_key('SELECT  *\n FROM t'), 'SELECT * FROM t')

    def test_sql_timer_preserves_rows_and_parameters(self):
        db = sqlite3.connect(':memory:')
        self.addCleanup(db.close)
        with db:
            db.execute('CREATE TABLE t(id INTEGER PRIMARY KEY, body TEXT)')
            db.executemany('INSERT INTO t VALUES(?,?)', [(1, '예산'), (2, '집행')])
            timer = SQLTimer(db)
            with timer:
                timer.execute('INSERT INTO t VALUES(?,?)', (3, 'third'))
            sql = 'SELECT body FROM t WHERE id=?'
            self.assertEqual(timer.execute(sql, (1,)).fetchone(), ('예산',))
            self.assertEqual(list(timer.execute(sql, (2,))), [('집행',)])
            record = timer.records[sql]
            self.assertEqual(record['calls'], 2)
            self.assertTrue(any('INTEGER PRIMARY KEY' in r[3] for r in record['plan']))
            self.assertGreaterEqual(record['ms'], 0)


if __name__ == '__main__':
    unittest.main()
