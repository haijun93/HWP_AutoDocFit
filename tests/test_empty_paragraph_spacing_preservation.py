"""회귀 시험: 빈 문단을 여백으로 쓴 HWPX에서 두 서식 처리를 이어 적용한다."""

from pathlib import Path
import tempfile
import unittest
import zipfile

from defusedxml import ElementTree as ET

from docfit_core.batch_format import ParagraphStyle, apply_batch
from docfit_core.spacing_reset import reset_spacing


NS = (
    'xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" '
    'xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" '
    'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"'
)


def _fixture(path):
    langs = "hangul latin hanja japanese other symbol user".split()
    zeros = " ".join(f'{lang}="0"' for lang in langs)
    ten = " ".join(f'{lang}="10"' for lang in langs)
    header = (
        f'<hh:head {NS}><hh:refList>'
        '<hh:charProperties itemCnt="2">'
        f'<hh:charPr id="0" height="1200"><hh:fontRef {zeros}/>'
        f'<hh:spacing {zeros}/></hh:charPr>'
        f'<hh:charPr id="1" height="1200"><hh:fontRef {zeros}/>'
        f'<hh:spacing {ten}/></hh:charPr>'
        '</hh:charProperties>'
        '<hh:paraProperties itemCnt="1"><hh:paraPr id="0">'
        '<hh:align horizontal="JUSTIFY"/><hh:lineSpacing type="PERCENT" value="0"/>'
        '<hh:margin><hc:left value="0"/><hc:right value="0"/><hc:intent value="0"/>'
        '<hc:prev value="0"/><hc:next value="0"/></hh:margin>'
        '</hh:paraPr></hh:paraProperties></hh:refList></hh:head>'
    ).encode()

    def para(text, char_id):
        return (f'<hp:p paraPrIDRef="0"><hp:run charPrIDRef="{char_id}">'
                f'<hp:t>{text}</hp:t></hp:run></hp:p>')

    # 두 본문 문단 사이를 20개 빈 문단으로 띄운다. 빈 문단도 0 간격/줄 간격이다.
    paragraphs = [para("앞 본문", 1)] + [para("", 1) for _ in range(20)] + [para("뒤 본문", 1)]
    section = f'<hp:sec {NS}>' + "".join(paragraphs) + '</hp:sec>'
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as package:
        package.writestr("mimetype", "application/hwp+zip")
        package.writestr("version.xml", "<HWPVersion/>" )
        package.writestr("Contents/header.xml", header)
        package.writestr("Contents/section0.xml", section.encode())
        package.writestr("META-INF/container.xml", '<container/>')


def _tag(node):
    return node.tag.rsplit("}", 1)[-1]


def _paragraph_signature(section):
    paragraphs = [node for node in section if _tag(node) == "p"]
    return [
        ("".join(t.text or "" for run in para if _tag(run) == "run"
                 for t in run if _tag(t) == "t"),
         tuple((run.get("charPrIDRef"), tuple((child.get("type"), child.get("value"))
                                                for child in para
                                                if _tag(child) == "lineSpacing"))
               for run in para if _tag(run) == "run"))
        for para in paragraphs
    ]


class EmptyParagraphSpacingPreservationTest(unittest.TestCase):
    def test_twenty_zero_spacing_paragraphs_survive_unify_and_spacing_reset(self):
        with tempfile.TemporaryDirectory() as temp:
            source = Path(temp) / "synthetic.hwpx"
            target = Path(temp) / "formatted.hwpx"
            _fixture(source)

            with zipfile.ZipFile(source) as package:
                original = ET.fromstring(package.read("Contents/section0.xml"))
                original_header = ET.fromstring(package.read("Contents/header.xml"))
            before = _paragraph_signature(original)
            self.assertEqual(sum(not text for text, _ in before), 20)
            self.assertEqual([text for text, _ in before if text], ["앞 본문", "뒤 본문"])

            def planner(text, _paragraph):
                if text and text.strip():
                    return ParagraphStyle(text=text, size_pt=13, line_percent=100, prev_pt=0)
                return None

            result = apply_batch(source, target, planner)
            self.assertEqual(result.applied, 2)

            with zipfile.ZipFile(target) as package:
                header = ET.fromstring(package.read("Contents/header.xml"))
                section = ET.fromstring(package.read("Contents/section0.xml"))
            after_unify = _paragraph_signature(section)
            self.assertEqual([text for text, _ in after_unify], [text for text, _ in before])
            self.assertEqual(sum(not text for text, _ in after_unify), 20)

            reset_spacing(header, [section], lambda _text: 0)
            final = _paragraph_signature(section)
            self.assertEqual([text for text, _ in final], [text for text, _ in before])
            self.assertEqual(sum(not text for text, _ in final), 20)

            chars = {node.get("id"): node for node in header.iter() if _tag(node) == "charPr"}
            blank_ids = [run_id for text, runs in final if not text for run_id, _ in runs]
            self.assertEqual(len(blank_ids), 20)
            for char_id in blank_ids:
                spacing = next(node for node in chars[char_id] if _tag(node) == "spacing")
                self.assertTrue(all(value == "0" for value in spacing.attrib.values()))


if __name__ == "__main__":
    unittest.main()
