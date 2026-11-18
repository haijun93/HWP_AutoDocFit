import unittest

from docfit_app.focus_home_event import (
    FOCUS_HOME_EVENT_NAME,
    FOCUS_HOME_WINDOW_EVENT,
    event_is_available,
)


class FocusHomeEventTests(unittest.TestCase):
    def test_event_name_and_availability_predicate(self):
        seen = []
        self.assertTrue(event_is_available(lambda name: seen.append(name) or 1))
        self.assertEqual(seen, [FOCUS_HOME_EVENT_NAME])
        self.assertFalse(event_is_available(lambda _name: 0))
        self.assertEqual(FOCUS_HOME_WINDOW_EVENT, 'docfit-focus-home')


if __name__ == '__main__':
    unittest.main()
