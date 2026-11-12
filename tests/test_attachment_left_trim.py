"""ATT2: ?? ???? ?? ?? ??."""
import copy
import unittest
import zipfile
from tests.test_attachment_block import DocumentTest


class AttachmentLeftTrimTest(DocumentTest):
    def _doc(self, body, marker_face='????'):
        path = super()._doc(body, marker_face)
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

    def test_left_trim_preserves_shared_style_and_number_geometry(self):
        body = (self._p('? ??', '1') + self._p('?? ??  ')
                + self._p('??  1. ?? 1?.  ') + self._p('   2. ?? 1?.\t ')
                + self._p('?.  '))
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
        self.assertEqual([self.ns['??_???'](p) for p in paras][1:],
                         ['?? ??  ', '??  1. ?? 1?.', '   2. ?? 1?.', '?.'])
        self.assertEqual(count, 3)
        self.assertNotEqual(paras[1].get('paraPrIDRef'), paras[2].get('paraPrIDRef'))


if __name__ == '__main__':
    unittest.main()
