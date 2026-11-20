import unittest

from docfit_app.thumb_size_setting import handle_thumb_size


class FakeStore:
    def __init__(self):
        self.data = {}

    def read(self):
        return dict(self.data)

    def update(self, fn):
        fn(self.data)


class ThumbSizeSettingTests(unittest.TestCase):
    def test_defaults_roundtrip_and_clamp(self):
        s = FakeStore()
        self.assertEqual(handle_thumb_size(s, "doc_grid", "thumb_size_get", {})["size"], 190.0)
        self.assertEqual(handle_thumb_size(s, "photo_album", "thumb_size_get", {})["size"], 160.0)
        self.assertEqual(handle_thumb_size(s, "doc_grid", "thumb_size_set", {"size": 220})["size"], 220.0)
        self.assertEqual(handle_thumb_size(s, "doc_grid", "thumb_size_set", {"size": 999})["size"], 240.0)
        self.assertEqual(handle_thumb_size(s, "doc_grid", "thumb_size_get", {})["size"], 240.0)
        self.assertEqual(handle_thumb_size(s, "photo_album", "thumb_size_get", {})["size"], 160.0)
        self.assertFalse(handle_thumb_size(s, "doc_grid", "thumb_size_set", {"size": "x"})["ok"])

    def test_services_dispatch(self):
        from docfit_app.doc_grid_service import DocGridService
        s = FakeStore()
        svc = DocGridService(settings_store=s)
        self.assertTrue(svc.command("thumb_size_set", {"size": 120})["ok"])
        self.assertEqual(svc.command("thumb_size_get")["size"], 120.0)


if __name__ == "__main__":
    unittest.main()
