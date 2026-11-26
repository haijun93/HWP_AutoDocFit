"""명시 실행용 실제 kordoc 합성 입력 시험(DOCFIT_CTXEXT1_ENGINE_TEST=1)."""
import hashlib
import os
from pathlib import Path
import struct
import tempfile
import unittest

from docfit_core.conversion_fixtures import build_pptx
from docfit_core.conversion_fidelity import extract_hwpx
from docfit_core.file_conversion import convert_input_to_hwpx
from docfit_core.hwpx import validate_hwpx


def synthetic_xls(path):
    """표준 라이브러리만으로 만드는 OLE/BIFF8 통합문서. XLSX 이름 변경 아님."""
    def record(kind, data=b''):
        return struct.pack('<HH', kind, len(data)) + data
    def bof(kind):
        return record(0x0809, struct.pack('<HHHHII', 0x0600, kind, 0x0DBB, 1997, 0xC0, 6))
    strings = ['CTXEXT1 XLS', 'Count', 'Synthetic', '42']
    sst = struct.pack('<II', 4, 4)
    for value in strings:
        sst += struct.pack('<HB', len(value), 0) + value.encode('ascii')
    prefix = bof(5) + record(0x0042, struct.pack('<H', 1252))
    tail = record(0x00FC, sst) + record(0x000A)
    bound_size = 4 + 8 + 4  # record header, BOUNDSHEET fields, "Data".
    bound = record(0x0085, struct.pack('<IBBBB', len(prefix) + bound_size + len(tail),
                                     0, 0, 4, 0) + b'Data')
    sheet = bof(0x10) + record(0x0200, struct.pack('<IIHHH', 0, 2, 0, 2, 0))
    for index in range(4):
        sheet += record(0x00FD, struct.pack('<HHHI', index // 2, index % 2, 0, index))
    workbook = (prefix + bound + tail + sheet + record(0x000A)).ljust(4096, b'\0')

    free, end = 0xFFFFFFFF, 0xFFFFFFFE
    header = bytearray(512)
    header[:8] = bytes.fromhex('d0cf11e0a1b11ae1')
    struct.pack_into('<HHHHH', header, 24, 0x003E, 3, 0xFFFE, 9, 6)
    struct.pack_into('<IIIIIIIII', header, 40, 0, 1, 8, 0, 4096, end, 0, end, 0)
    struct.pack_into('<109I', header, 76, 9, *([free] * 108))
    def entry(name, kind, child, start, size):
        data = bytearray(128)
        encoded = (name + '\0').encode('utf-16le')
        data[:len(encoded)] = encoded
        struct.pack_into('<HBBIII', data, 64, len(encoded), kind, 1, free, free, child)
        struct.pack_into('<IQ', data, 116, start, size)
        return data
    directory = (entry('Root Entry', 5, 1, end, 0)
                 + entry('Workbook', 2, free, 0, 4096) + bytes(256))
    fat = struct.pack('<128I', *range(1, 8), end, end, 0xFFFFFFFD, *([free] * 118))
    path.write_bytes(header + workbook + directory + fat)


@unittest.skipUnless(os.environ.get('DOCFIT_CTXEXT1_ENGINE_TEST') == '1',
                     '실제 kordoc 시험은 명시 실행')
class ContextEngineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='ctxext1-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def check_conversion(self, source, tokens):
        original = hashlib.sha256(source.read_bytes()).hexdigest()
        target = self.root / (source.suffix[1:] + '-result.hwpx')
        convert_input_to_hwpx(source, target)
        validate_hwpx(target, structure_only=True, parse_xml=True)
        text = extract_hwpx(target)['text']
        for token in tokens:
            self.assertIn(token, text)
        self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), original)
        before = target.read_bytes()
        with self.assertRaises(FileExistsError):
            convert_input_to_hwpx(source, target)
        self.assertEqual(target.read_bytes(), before)
        print(f'CTXEXT1 {source.suffix}: HWPX {target.stat().st_size} bytes; '
              f'본문 {tokens!r} 보존; 원본 SHA256 동일; 덮어쓰기 거부')

    def test_xls(self):
        source = self.root / 'synthetic.xls'
        synthetic_xls(source)
        self.check_conversion(source, ['CTXEXT1 XLS', 'Synthetic', '42'])

    def test_pptx(self):
        source = self.root / 'synthetic.pptx'
        build_pptx([{'title': 'CTXEXT1 PPTX', 'bullets': [(0, '합성 슬라이드 본문')]}], source)
        self.check_conversion(source, ['CTXEXT1 PPTX', '합성 슬라이드 본문'])

    def test_hml(self):
        source = self.root / 'synthetic.hml'
        source.write_text('<?xml version="1.0" encoding="UTF-8"?>'
                          '<HWPML Version="2.8"><HEAD/><BODY><SECTION>'
                          '<P><TEXT><CHAR>CTXEXT1 HML 합성 본문 42</CHAR></TEXT></P>'
                          '</SECTION></BODY></HWPML>', encoding='utf-8')
        self.check_conversion(source, ['CTXEXT1 HML', '합성 본문', '42'])

    def test_ppt_is_rejected_without_publishing(self):
        # 미지원 확장자는 파일 내용을 읽기 전에 거부한다. 실제 PPT 파싱 시험이 아니다.
        source = self.root / 'unsupported.ppt'
        source.write_bytes(b'CTXEXT1 synthetic unsupported input')
        original = source.read_bytes()
        target = self.root / 'ppt-result.hwpx'
        with self.assertRaisesRegex(ValueError, '변환할 수 없는 입력 형식'):
            convert_input_to_hwpx(source, target)
        self.assertFalse(target.exists())
        self.assertEqual(source.read_bytes(), original)
        print('CTXEXT1 .ppt: 입력 형식 거부; 결과 미생성; 원본 동일(실제 PPT 파싱 미실행)')


if __name__ == '__main__':
    unittest.main()
