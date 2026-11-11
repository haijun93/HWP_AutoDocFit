import unittest

from scripts.startup_breakdown import parse_importtime_output


class ParseImporttimeOutputTests(unittest.TestCase):
    def test_sorts_by_cumulative_and_limits_results(self):
        sample = "\n".join((
            "import time:       100 |        100 | alpha",
            "import time:       200 |       1200 | beta",
            "import time:       300 |        500 | gamma",
            "not importtime output",
        ))
        result = parse_importtime_output(sample, limit=2)
        self.assertEqual([item["name"] for item in result], ["beta", "gamma"])
        self.assertEqual(result[0]["self_us"], 200)
        self.assertEqual(result[0]["cumulative_us"], 1200)

    def test_empty_input(self):
        self.assertEqual(parse_importtime_output("warning only"), [])


if __name__ == "__main__":
    unittest.main()
