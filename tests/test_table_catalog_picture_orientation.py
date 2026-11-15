"""표템플릿 미리보기 그림 개체의 회전·뒤집기 회귀 시험(2026-11-15, 재발 방지).

신구대조표·모양 1 가운데 칸 화살표는 그림 개체(hp:pic)다. 원본 그림은 위쪽 화살표(가로로 넓음)이고
한/글은 flip(가로·세로)+rotationInfo angle=270으로 세로로 긴 오른쪽 화살표를 그린다.
미리보기가 이 변환을 빼먹으면 위쪽 화살표로 보인다(여러 번 되돌아간 문제). 그림 방향을 숫자로 검사한다.
"""
import base64
import io
import unittest

from PIL import Image

from docfit_core.table_catalog import list_catalog


def _pictures(node):
    if isinstance(node, dict):
        if node.get("kind") == "pic" and str(node.get("src", "")).startswith("data:"):
            yield node
        for value in node.values():
            yield from _pictures(value)
    elif isinstance(node, list):
        for value in node:
            yield from _pictures(value)


def _image(src):
    return Image.open(io.BytesIO(base64.b64decode(src.split(",", 1)[1]))).convert("RGBA")


class PictureOrientationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog = list_catalog()

    def _template(self, word, shape):
        for category in self.catalog["categories"]:
            for template in category.get("templates", []):
                title = " ".join(str(template.get(k, "")) for k in ("title", "name", "id"))
                if word in title and shape in title:
                    return template
        self.fail(f"템플릿 없음: {word} {shape}")

    def test_old_new_comparison_arrow_points_right(self):
        template = self._template("신구대조표", "모양 1")
        images = [_image(p["src"]) for p in _pictures(template)]
        self.assertTrue(images, "신구대조표·모양 1에 그림 개체가 없음")
        arrow = max(images, key=lambda im: im.size[0] * im.size[1])
        w, h = arrow.size
        self.assertGreater(h, w, "화살표 그림은 세로로 길어야 함(한/글 표시 모양)")
        alpha = arrow.getchannel("A").load()
        opaque = [(x, y) for y in range(h) for x in range(w) if alpha[x, y] > 128]
        tip_x = max(x for x, _ in opaque)
        tip_rows = [y for x, y in opaque if x >= tip_x - 1]
        tip_y = sum(tip_rows) / len(tip_rows) / h
        self.assertGreater(tip_x / w, 0.85, "화살표 끝은 오른쪽 끝에 있어야 함")
        self.assertTrue(0.35 < tip_y < 0.65, f"오른쪽 끝점이 세로 가운데여야 함(오른쪽 화살표): {tip_y:.2f}")

    def test_rotated_pictures_swap_size_like_hangul(self):
        # 90·270도 회전 그림은 미리보기에서도 가로·세로가 바뀌어야 한다(다른 템플릿 재발 방지).
        import zipfile
        from defusedxml import ElementTree

        from docfit_core.table_catalog import default_catalog_path

        hp = "{http://www.hancom.co.kr/hwpml/2011/paragraph}"
        with zipfile.ZipFile(default_catalog_path()) as archive:
            rotated = {}
            for name in archive.namelist():
                if not name.startswith("Contents/section"):
                    continue
                root = ElementTree.fromstring(archive.read(name))
                for pic in root.iter(hp + "pic"):
                    rot = pic.find(hp + "rotationInfo")
                    img = next((n for n in pic.iter() if n.tag.endswith("}img")), None)
                    if rot is None or img is None or rot.get("rotateimage", "1") != "1":
                        continue
                    if round(float(rot.get("angle", 0)) / 90) % 2 == 1:
                        rotated[img.get("binaryItemIDRef")] = True
            raw = {}
            for name in archive.namelist():
                if name.startswith("BinData/"):
                    stem = name.split("/")[-1].rsplit(".", 1)[0]
                    if stem in rotated:
                        raw[stem] = Image.open(io.BytesIO(archive.read(name))).size
        shown = {_image(p["src"]).size for c in self.catalog["categories"] for t in c.get("templates", []) for p in _pictures(t)}
        for stem, (w, h) in raw.items():
            if w != h:
                self.assertIn((h, w), shown, f"{stem}: 90/270도 회전 그림이 미리보기에서 회전되지 않음")
                self.assertNotIn((w, h), shown, f"{stem}: 회전 전 그림이 그대로 미리보기에 나옴")


if __name__ == "__main__":
    unittest.main()
