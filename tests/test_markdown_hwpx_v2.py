"""그림 안전 경계와 2단계 문서 의미를 독립 정답으로 검사한다."""
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from zipfile import ZipFile

from defusedxml import ElementTree as ET

from docfit_core import conversion_fidelity as cf
from docfit_core import markdown_hwpx as md
from docfit_core.markdown_hwpx_fixtures import fixtures_v2, write_images


class MarkdownHwpxV2Test(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.assets = self.root / 'assets'
        write_images(self.assets)

    def render(self, source, **kwargs):
        data, doc = md.render_hwpx(source, image_dir=self.assets, **kwargs)
        return ZipFile(io.BytesIO(data)), doc

    def test_nineteen_independent_expectations(self):
        cases = fixtures_v2()
        self.assertGreaterEqual(len(cases), 15)
        for name, (source, expected) in cases.items():
            with self.subTest(name=name):
                target = self.root / f'{name}.hwpx'
                md.generate_hwpx(source, target, image_dir=self.assets)
                result = cf.compare(dict(expected, form='hwpx'), cf.extract_hwpx(target))
                self.assertTrue(result['verdict']['pass'], result)

    def test_png_jpg_manifest_bytes_alt_size_and_repeatability(self):
        source = '![넓은 <그림> & 설명](wide.png)\n\n![세로](images/photo.jpg)'
        first, _ = md.render_hwpx(source, image_dir=self.assets)
        self.assertEqual(first, md.render_hwpx(source, image_dir=self.assets)[0])
        with ZipFile(io.BytesIO(first)) as z:
            section = ET.fromstring(z.read('Contents/section0.xml'))
            package = ET.fromstring(z.read('Contents/content.hpf'))
            opf = '{http://www.idpf.org/2007/opf/}'
            items = {i.get('id'): i for i in package.findall(f'{opf}manifest/{opf}item')}
            for pic, path, ratio, alt in zip(section.findall(f'.//{{{md.HP}}}pic'),
                    ['wide.png', 'images/photo.jpg'], [4, 2/3], ['넓은 <그림> & 설명', '세로']):
                ref = pic.find(f'{{{md.HC}}}img').get('binaryItemIDRef')
                item = items[ref]
                self.assertEqual(item.get('isEmbeded'), '1')
                self.assertEqual(z.read(item.get('href')), (self.assets / path).read_bytes())
                self.assertEqual(pic.find(f'{{{md.HP}}}shapeComment').text, alt)
                size = pic.find(f'{{{md.HP}}}sz')
                width, height = int(size.get('width')), int(size.get('height'))
                self.assertLessEqual(width, md.BODY_WIDTH)
                self.assertAlmostEqual(width / height, ratio, places=3)

    def test_picture_inside_table_fits_cell_and_keeps_emphasis(self):
        z, _ = self.render('| A | B |\n| --- | --- |\n| **![그림](wide.png)** | 설명 |')
        with z:
            root = ET.fromstring(z.read('Contents/section0.xml'))
        cell = root.findall(f'.//{{{md.HP}}}tc')[2]
        width = int(cell.find(f'{{{md.HP}}}cellSz').get('width'))
        picwidth = int(cell.find(f'.//{{{md.HP}}}pic/{{{md.HP}}}sz').get('width'))
        self.assertLessEqual(picwidth, width - 280)

    def test_unsafe_missing_and_unsupported_paths_remain_literal(self):
        for path in ['../image1.png', 'images/../../outside.png', '/root.png',
                     'C:/secret.png', 'C:secret.png', '//server/share.png',
                     r'..\outside.png', 'file.png:stream', 'https://example.invalid/a.png',
                     'absent.png', 'file.gif']:
            with self.subTest(path=path):
                source = f'![그림]({path})'
                data, doc = md.render_hwpx(source, image_dir=self.assets)
                self.assertTrue(doc.warnings)
                self.assertEqual(''.join(r.text for r in doc.blocks[0].runs), source)
                with ZipFile(io.BytesIO(data)) as z:
                    self.assertFalse(any(n.startswith('BinData/') for n in z.namelist()))

    def test_symlink_escape_rejected(self):
        outside = self.root / 'outside.png'
        outside.write_bytes((self.assets / 'image1.png').read_bytes())
        try:
            (self.assets / 'link.png').symlink_to(outside)
        except OSError:
            self.skipTest('이 환경은 심볼릭 링크를 만들 수 없음')
        _, doc = md.render_hwpx('![외부](link.png)', image_dir=self.assets)
        self.assertIn('이탈', doc.warnings[0])

    def test_corrupt_spoofed_and_large_images_fall_back(self):
        (self.assets / 'broken.png').write_bytes(b'not PNG')
        (self.assets / 'spoof.png').write_bytes((self.assets / 'images/photo.jpg').read_bytes())
        for path in ['broken.png', 'spoof.png']:
            _, doc = md.render_hwpx(f'![원문]({path})', image_dir=self.assets)
            self.assertTrue(doc.warnings)
        for limit, value in [('MAX_IMAGE_BYTES', 4), ('MAX_IMAGE_PIXELS', 4),
                             ('MAX_TOTAL_IMAGE_BYTES', 4), ('MAX_IMAGES', 0)]:
            with self.subTest(limit=limit), patch.object(md, limit, value):
                _, doc = md.render_hwpx('![원문](image1.png)', image_dir=self.assets)
                self.assertTrue(doc.warnings)

    def test_heading_456_real_styles_no_markers(self):
        z, doc = self.render('#### 사\n\n##### 오\n\n###### 육')
        self.assertFalse(doc.warnings)
        self.assertEqual([p.level for p in doc.blocks], [4, 5, 6])
        with z:
            header = ET.fromstring(z.read('Contents/header.xml'))
            section = z.read('Contents/section0.xml').decode('utf-8')
        styles = {s.get('name'): s for s in header.findall(f'.//{{{md.HH}}}style')}
        for level in [4, 5, 6]:
            self.assertIn(f'제목 {level}', styles)
        self.assertNotIn('####', section)

    def test_code_font_indent_blank_lines_and_literal_html(self):
        source = '```\n  <br> **그대로**\n\n\t끝\n```'
        z, doc = self.render(source)
        self.assertFalse(doc.warnings)
        with z:
            header = ET.fromstring(z.read('Contents/header.xml'))
            section = ET.fromstring(z.read('Contents/section0.xml'))
        font = header.find(f'.//{{{md.HH}}}fontface/{{{md.HH}}}font[@id="2"]')
        self.assertEqual(font.get('face'), '굴림체')
        paragraph = section.findall(f'{{{md.HP}}}p')[-1]
        self.assertEqual(paragraph.get('styleIDRef'), '7')
        self.assertEqual(len(paragraph.findall(f'.//{{{md.HP}}}lineBreak')), 2)
        self.assertIn('<br> **그대로**', ''.join(paragraph.itertext()))
        self.assertTrue(doc.blocks[0].code)

    def test_link_label_and_warning_preserved(self):
        z, doc = self.render('[**한국어** *English*](https://example.invalid/a?x=1&y=2)')
        with z:
            root = ET.fromstring(z.read('Contents/section0.xml'))
        self.assertEqual(''.join(root.itertext()), '한국어 English')
        self.assertTrue(any('표시 글자' in w for w in doc.warnings))
        self.assertTrue(doc.blocks[0].runs[0].bold)
        self.assertTrue(doc.blocks[0].runs[-1].italic)

    def test_parentheses_angle_paths_titles_and_jpeg_suffix(self):
        (self.assets / '합성 (그림).PNG').write_bytes((self.assets / 'image1.png').read_bytes())
        (self.assets / 'photo.jpeg').write_bytes((self.assets / 'images/photo.jpg').read_bytes())
        source = ('![그림](<합성 (그림).PNG> "선택 설명") ![사진](photo.jpeg) '
                  '[문서](https://example.invalid/a(b(c)))')
        z, doc = self.render(source)
        self.assertEqual(len(doc.warnings), 1)  # 링크 대상만 미지원
        with z:
            section = ET.fromstring(z.read('Contents/section0.xml'))
        self.assertEqual(len(section.findall(f'.//{{{md.HP}}}pic')), 2)
        self.assertEqual(''.join(t.text or '' for t in section.findall(f'.//{{{md.HP}}}t')), '  문서')

    def test_heading_link_linebreaks_and_table_tail(self):
        z, _ = self.render('#### [제목](https://example.invalid)\n\n가  \n나\\\n다<br />라')
        with z:
            section = ET.fromstring(z.read('Contents/section0.xml'))
        self.assertEqual(len(section.findall(f'.//{{{md.HP}}}lineBreak')), 3)
        self.assertEqual(''.join(section.itertext()), '제목가나다라')


if __name__ == '__main__':
    unittest.main()
