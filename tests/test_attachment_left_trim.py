"""ATT2: 붙임 정렬 및 끝 공백 회귀 시험."""
import copy
import unittest
import zipfile
from tests import test_attachment_block as attachment_tests


class AttachmentLeftTrimTest(unittest.TestCase):
    setUpClass = classmethod(attachment_tests.DocumentTest.setUpClass.__func__)
    _p = staticmethod(attachment_tests.DocumentTest._p)
    _convert = attachment_tests.DocumentTest._convert
    BODY = attachment_tests.DocumentTest.BODY

    def _doc(self, body, marker_face='휴먼명조'):
        path = attachment_tests.DocumentTest._doc(self, body, marker_face)
        with zipfile.ZipFile(path) as z:
            contents = {n: z.read(n) for n in z.namelist()}
        header = self.parse(contents['Contents/header.xml'])
        group = next(x for x in header.iter() if self.name(x) == 'paraProperties')
        base = copy.deepcopy(next(iter(group)))
        base.set('id', '0')
        next(x for x in base if self.name(x) == 'align').set('horizontal', 'JUSTIFY')
        group.insert(0, base)
        group.set('itemCnt', str(len(group)))
        contents['Contents/header.xml'] = self.ns['ET'].tostring(header)
        with zipfile.ZipFile(path, 'w') as z:
            for name, data in contents.items():
                z.writestr(name, data)
        return path

    def test_later_com_unification_preserves_attachment_alignment(self):
        from unittest.mock import patch
        from tests.test_style_unify_elements import UnifyElementTests
        harness = UnifyElementTests()
        harness.ns = self.ns
        texts = ['1. 본문 하나', '2. 본문 둘', '3. 본문 셋',
                 '붙임  1. 계획 1부.', '2. 일정 1부.', '끝.']
        with harness.document([{}, {}, {}, {'align': 'LEFT'}, {'align': 'LEFT'}, {'align': 'LEFT'}]) as (fn, com, action, _):
            read = fn.__globals__['현재문단_텍스트']
            with patch.dict(fn.__globals__, {
                '현재문단_텍스트': lambda: texts[int(read().rsplit(' ', 1)[1])],
                '_서식통일_XML_조사': lambda: None,
            }):
                self.assertTrue(fn())
            self.assertNotIn(('AlignType', 0), harness.para_items(action))

    def test_layout_without_font_reference_and_second_pass(self):
        from unittest.mock import patch
        source = self._doc(self._p('붙임  자료 1부. 끝. \t'))
        fn = self.ns['붙임글꼴_hwpx_처리']
        target = source.with_name('layout.hwpx')
        with patch.dict(fn.__globals__, {'표준서식_기호_사용': False}):
            found = fn(source)
            self.assertEqual(found, {'Contents/section0.xml': [(0, 0)]})
            self.assertEqual(fn(source, target, found), 1)
            self.assertEqual(fn(target), {'Contents/section0.xml': []})

    def test_xml_tabs_and_separate_runs_are_trimmed(self):
        body = self._p('붙임  자료 1부.  끝.')
        body = body.replace('</hp:t>', '<hp:tab/> </hp:t>')
        body = body.replace('</hp:p>', '<hp:run charPrIDRef="0"><hp:t> \t </hp:t></hp:run></hp:p>')
        _, target, _, _, _, section = self._convert(body)
        para = list(section)[0]
        self.assertEqual(self.ns['제목_문자열'](para), '붙임  자료 1부.  끝.')
        self.assertFalse(any(self.name(x) == 'tab' for x in para.iter()))
        self.assertEqual(self.ns['붙임글꼴_hwpx_처리'](target), {'Contents/section0.xml': []})

    def test_left_trim_preserves_shared_style_and_number_geometry(self):
        body = (self._p('ㅇ 본문', '1') + self._p('일반 문장  ')
                + self._p('붙임  1. 계획 1부.  ') + self._p('   2. 일정 1부.\t ')
                + self._p('끝.  '))
        _, _, _, count, header, section = self._convert(body)
        paras = list(section)
        styles = {x.get('id'): x for x in header.iter() if self.name(x) == 'paraPr'}
        original = styles[paras[1].get('paraPrIDRef')]
        for para in paras[2:]:
            changed = styles[para.get('paraPrIDRef')]
            self.assertEqual(next(x for x in changed if self.name(x) == 'align').get('horizontal'), 'LEFT')
            a, b = copy.deepcopy(original), copy.deepcopy(changed)
            a.attrib.pop('id'); b.attrib.pop('id')
            next(x for x in a if self.name(x) == 'align').set('horizontal', 'LEFT')
            self.assertEqual(self.ns['ET'].tostring(a), self.ns['ET'].tostring(b))
        self.assertEqual([self.ns['제목_문자열'](p) for p in paras][1:],
                         ['일반 문장  ', '붙임  1. 계획 1부.', '   2. 일정 1부.', '끝.'])
        self.assertEqual(count, 3)
        self.assertNotEqual(paras[1].get('paraPrIDRef'), paras[2].get('paraPrIDRef'))


if __name__ == '__main__':
    unittest.main()
