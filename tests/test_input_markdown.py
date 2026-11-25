"""합성 Office/HTML 입력의 의미 보존 및 읽기 전용 경계."""
import hashlib
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile, ZIP_DEFLATED

from docfit_core import conversion_fixtures as fx, conversion_fidelity as cf
from docfit_core.input_markdown import adapt_to_markdown, UnsupportedInput
from docfit_core.markdown_hwpx import generate_hwpx


class InputMarkdownTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def check_conversion(self, source, expected):
        original = source.read_bytes()
        result = adapt_to_markdown(source, source.with_suffix('.assets'))
        self.assertFalse(result.warnings, result.warnings)
        target = source.with_suffix('.hwpx')
        generated = generate_hwpx(result.markdown, target, image_dir=result.image_dir)
        self.assertFalse(generated.warnings)
        got = cf.extract_hwpx(target)
        verdict = cf.compare(dict(expected, form='hwpx'), got)
        self.assertTrue(verdict['verdict']['pass'], verdict)
        self.assertEqual(source.read_bytes(), original)
        return result, target

    def test_five_docx_and_html_cases(self):
        specs = [*fx.DOC_SPECS.values(),
                 [('h', 1, '그림'), ('img', '사진'), ('p', '뒤 문단')],
                 [('h', 2, '계층'), ('ul', [(0, '가'), (1, '나'), (2, '다')]), ('ol', [(0, '첫째'), (0, '둘째')])]]
        (self.root / 'image1.png').write_bytes(fx.png_bytes())
        for i, blocks in enumerate(specs):
            for fmt in ('docx', 'html'):
                with self.subTest(case=i, format=fmt):
                    source = self.root / f'case{i}.{fmt}'
                    if fmt == 'docx':
                        fx.build_docx(blocks, source)
                    else:
                        source.write_text(fx.render_html(blocks), encoding='utf-8')
                    # 같은 stem의 형식별 자원·결과 충돌을 피한다.
                    folder = self.root / f'run{i}-{fmt}'
                    folder.mkdir()
                    copied = folder / source.name
                    copied.write_bytes(source.read_bytes())
                    (folder / 'image1.png').write_bytes(fx.png_bytes())
                    self.check_conversion(copied, fx.expected_from_blocks(blocks))

    def test_five_xlsx_cases(self):
        specs = [*fx.SHEET_SPECS.values(),
                 [('빈칸', [['열1', '열2', '열3'], ['가', '', '끝'], ['', '1', '']])],
                 [('문자', [['한국어', 'English'], ['특수 & < >', '1,234.50']])],
                 [('열', [[str(i) for i in range(8)], ['값'] * 8])],
                 [('행', [['번호', '이름']] + [[str(i), f'항목{i}'] for i in range(12)])]]
        for i, sheets in enumerate(specs):
            with self.subTest(case=i):
                source = self.root / f'sheet{i}.xlsx'
                fx.build_xlsx(sheets, source)
                self.check_conversion(source, fx.expected_from_sheets(sheets))

    def test_five_pptx_cases(self):
        specs = [*fx.SLIDE_SPECS.values(), [{'title': '첫째'}, {'title': '둘째'}],
                 [{'title': '계층', 'bullets': [(0, '위'), (1, '중간'), (2, '아래')]}],
                 [{'title': '표', 'table': [['키', '값'], ['가', '2']]}],
                 [{'title': '그림', 'image': True}]]
        for i, slides in enumerate(specs):
            with self.subTest(case=i):
                source = self.root / f'slide{i}.pptx'
                fx.build_pptx(slides, source)
                _, target = self.check_conversion(source, fx.expected_from_slides(slides))
                if any(s.get('image') for s in slides):
                    with ZipFile(target) as archive:
                        images = [archive.read(n) for n in archive.namelist() if n.startswith('BinData/')]
                    self.assertIn(hashlib.sha256(fx.png_bytes()).digest(), [hashlib.sha256(b).digest() for b in images])

    def rewrite_part(self, source, part, transform):
        with ZipFile(source) as archive:
            items = {n: archive.read(n) for n in archive.namelist()}
        items[part] = transform(items[part].decode('utf-8')).encode('utf-8')
        with ZipFile(source, 'w', ZIP_DEFLATED) as archive:
            for name, value in items.items():
                archive.writestr(name, value)

    def test_unsupported_merged_formula_and_drawing_are_explicit(self):
        source = self.root / 'unsupported.xlsx'
        fx.build_xlsx([('시트', [['항목', '값'], ['가', '1']])], source)
        self.rewrite_part(source, 'xl/worksheets/sheet1.xml', lambda t: t.replace('</worksheet>',
                          '<mergeCells><mergeCell ref="A1:B1"/></mergeCells><drawing/><f>1+1</f></worksheet>'))
        result = adapt_to_markdown(source, self.root / 'assets')
        for keyword in ('mergeCell', 'drawing', 'f'):
            self.assertTrue(any(w.endswith(': ' + keyword) for w in result.warnings), result.warnings)

    def test_external_office_image_is_never_read(self):
        source = self.root / 'external.docx'
        fx.build_docx([('p', '본문'), ('img', '사진')], source)
        self.rewrite_part(source, 'word/_rels/document.xml.rels', lambda t: t.replace('Target="media/image1.png"',
                          'Target="https://example.invalid/private.png" TargetMode="External"'))
        result = adapt_to_markdown(source, self.root / 'assets')
        self.assertTrue(result.warnings)
        self.assertEqual(list(result.image_dir.iterdir()), [])

    def test_html_rejects_external_and_parent_image_and_warns_css(self):
        source = self.root / 'unsafe.html'
        source.write_text('<h1>제목</h1><p style="color:red">본문</p><img src="../private.png"><img src="https://example.invalid/x.png">', encoding='utf-8')
        result = adapt_to_markdown(source, self.root / 'assets')
        self.assertEqual(len(result.warnings), 2)  # 같은 종류의 그림 거절은 중복 제거
        self.assertEqual(list(result.image_dir.iterdir()), [])

    def test_xml_entities_are_rejected(self):
        source = self.root / 'entity.docx'
        fx.build_docx([('p', '본문')], source)
        self.rewrite_part(source, 'word/document.xml', lambda t: t.replace('<w:document', '<!DOCTYPE x [<!ENTITY xxe SYSTEM "file:///private">]><w:document').replace('본문', '&xxe;'))
        with self.assertRaises(Exception) as raised:
            adapt_to_markdown(source, self.root / 'assets')
        self.assertIn('EntitiesForbidden', type(raised.exception).__name__)

    def test_existing_assets_and_unsupported_source_are_rejected(self):
        source = self.root / 'source.txt'
        source.write_text('본문', encoding='utf-8')
        with self.assertRaises(UnsupportedInput):
            adapt_to_markdown(source, self.root / 'assets')
        source = self.root / 'source.html'
        source.write_text('<p>본문</p>', encoding='utf-8')
        (self.root / 'assets').mkdir()
        with self.assertRaises(FileExistsError):
            adapt_to_markdown(source, self.root / 'assets')


if __name__ == '__main__':
    unittest.main()
