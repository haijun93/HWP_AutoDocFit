"""MERGE110 관련 시험: 공개 HWP fixture 입력도 메모리에서 만든 합성 자료로 대체한다.

저장소 루트에서 가상환경 Python으로 실행한다. 기존 시험 파일과 fixture는 바꾸지 않는다.
"""
from __future__ import annotations

import io
import struct
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

from PIL import Image

MODULES = (
    'test_photo_library', 'test_library_db', 'test_scan_stream', 'test_photo_album_service',
    'test_photofix1', 'test_fileops1', 'test_dblopen1', 'test_default_library_scan',
    'test_library_exclusion_settings', 'test_thumb_size_setting', 'test_doc_grid_service',
    'test_doc_grid_bridge', 'test_scan_cache_service', 'test_svccheck1', 'test_kfind_process',
    'test_merge110',
)


def synthetic_preview() -> bytes:
    """HWP 미리보기용 최소 CFB: PrvImage 스트림에 합성 PNG만 넣는다."""
    free, end, fat_sector = 0xffffffff, 0xfffffffe, 0xfffffffd
    header = bytearray(512)
    header[:8] = bytes.fromhex('d0cf11e0a1b11ae1')
    struct.pack_into('<HHHHH', header, 24, 0x3e, 3, 0xfffe, 9, 6)
    struct.pack_into('<IIIIIIIII', header, 40, 0, 1, 1, 0, 4096, end, 0, end, 0)
    struct.pack_into('<109I', header, 76, 0, *([free] * 108))
    fat = [free] * 128
    fat[0], fat[1] = fat_sector, end
    for sector in range(2, 10):
        fat[sector] = sector + 1 if sector < 9 else end

    def entry(name: str, kind: int, start: int = end, size: int = 0, child: int = free) -> bytes:
        out = bytearray(128)
        encoded = (name + '\0').encode('utf-16le')
        out[:len(encoded)] = encoded
        struct.pack_into('<HBBIII', out, 64, len(encoded), kind, 1, free, free, child)
        struct.pack_into('<IQ', out, 116, start, size)
        return bytes(out)

    directory = entry('Root Entry', 5, child=1) + entry('PrvImage', 2, start=2, size=4096) + bytes(256)
    image = io.BytesIO()
    Image.new('RGB', (400, 500), '#3875ab').save(image, format='PNG')
    assert len(image.getvalue()) <= 4096
    return bytes(header) + struct.pack('<128I', *fat) + directory + image.getvalue().ljust(4096, b'\0')


def main() -> int:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    synthetic = synthetic_preview()
    read_bytes = Path.read_bytes
    uses = []

    def only_synthetic(path: Path) -> bytes:
        if path.name == 'KIDX13_reference.hwp':
            uses.append(True)
            return synthetic
        return read_bytes(path)

    suite = unittest.defaultTestLoader.loadTestsFromNames(['tests.' + name for name in MODULES])
    with patch.object(Path, 'read_bytes', only_synthetic):
        result = unittest.TextTestRunner(verbosity=1).run(suite)
    print('SYNTHETIC_HWP_FIXTURE_READS=' + str(len(uses)))
    return 0 if result.wasSuccessful() else 1


if __name__ == '__main__':
    raise SystemExit(main())
