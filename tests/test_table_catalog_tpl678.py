import unittest

from docfit_core.table_catalog import list_catalog


class Tpl678CatalogTests(unittest.TestCase):
    def test_shapes_6_7_8_keep_original_preview_cells_and_arrow_directions(self):
        catalog = list_catalog()
        wanted = {"\ucd94\uc9c4\uc808\ucc28(\uac00\ub85c) \ubaa8\uc5916",
                  "\ucd94\uc9c4\uc808\ucc28(\uac00\ub85c) \ubaa8\uc5917",
                  "\ucd94\uc9c4\uc808\ucc28(\uac00\ub85c) \ubaa8\uc5918"}
        groups = {group["name"]: group for group in catalog["categories"] if group["name"] in wanted}
        self.assertEqual(set(groups), wanted)
        self.assertEqual([groups[name]["templates"][0]["rows"] for name in sorted(wanted)], [5, 4, 11])

        def arrows(group_name):
            cells = groups[group_name]["templates"][0]["cell_meta"]
            return [cell["text"].strip() for row in cells for cell in row
                    if cell["text"].strip() in {"\u25b6", "\u25c0", "\u25bc"}]

        self.assertEqual(arrows("\ucd94\uc9c4\uc808\ucc28(\uac00\ub85c) \ubaa8\uc5916"), ["\u25b6"])
        self.assertEqual(arrows("\ucd94\uc9c4\uc808\ucc28(\uac00\ub85c) \ubaa8\uc5917"), ["\u25b6"] * 6)
        self.assertEqual(arrows("\ucd94\uc9c4\uc808\ucc28(\uac00\ub85c) \ubaa8\uc5918"),
                         ["\u25b6", "\u25b6", "\u25bc", "\u25c0", "\u25c0"])


if __name__ == "__main__":
    unittest.main()
