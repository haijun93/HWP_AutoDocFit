"""PDFWIN2: Rust pdf_classify 대조 시험용 합성 PDF 와 Python _classify 기대값(expected.json) 생성기.

사용: python tools/gen_classify_fixtures.py [출력폴더]
기본 출력: pdf_edit_core/tests/fixtures/classify
"""
from __future__ import annotations

import io
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from pypdf import PdfWriter  # noqa: E402
from pypdf.generic import (ArrayObject, DictionaryObject, NameObject,  # noqa: E402
                           NumberObject)

from docfit_app.api_contract import ServiceError  # noqa: E402
from docfit_app.pdf_edit_service import PdfEditService  # noqa: E402


def _write(writer) -> bytes:
    buf = io.BytesIO()
    writer.write(buf)
    return buf.getvalue()


def _pages(n, rotate=0):
    w = PdfWriter()
    for i in range(n):
        p = w.add_blank_page(width=100 + i, height=200)
        if rotate:
            p.rotate(rotate)
    return w


def _encrypted(user, algorithm, owner='owner-pw'):
    w = _pages(2)
    w.encrypt(user, owner, algorithm=algorithm)
    return _write(w)


def cases() -> dict[str, bytes]:
    out: dict[str, bytes] = {}
    out['ok_single'] = _write(_pages(1))
    out['ok_multi'] = _write(_pages(5))
    out['ok_rotated'] = _write(_pages(3, rotate=90))
    w = _pages(1)
    w.add_metadata({'/Title': 'meta'})
    out['ok_metadata'] = _write(w)
    # 서명이 아닌 위젯 주석은 통과해야 한다.
    w = _pages(1)
    annot = DictionaryObject({NameObject('/Type'): NameObject('/Annot'), NameObject('/Subtype'): NameObject('/Widget'),
                              NameObject('/FT'): NameObject('/Tx'),
                              NameObject('/Rect'): ArrayObject([NumberObject(0), NumberObject(0), NumberObject(10), NumberObject(10)])})
    w.pages[0][NameObject('/Annots')] = ArrayObject([w._add_object(annot)])
    out['ok_widget_annot'] = _write(w)
    # 서명 필드
    w = _pages(2)
    sig = DictionaryObject({NameObject('/Type'): NameObject('/Annot'), NameObject('/Subtype'): NameObject('/Widget'),
                            NameObject('/FT'): NameObject('/Sig'),
                            NameObject('/Rect'): ArrayObject([NumberObject(0), NumberObject(0), NumberObject(10), NumberObject(10)])})
    w.pages[1][NameObject('/Annots')] = ArrayObject([w._add_object(sig)])
    out['signed_widget'] = _write(w)
    # 구조 손상
    out['not_pdf_text'] = b'hello world, this is not a pdf\n'
    out['empty_file'] = b''
    out['header_only'] = b'%PDF-1.4\n%%EOF\n'
    out['leading_junk'] = b'\n\n' + out['ok_single']
    base = out['ok_single']
    out['no_eof'] = base[:base.rfind(b'%%EOF')]
    out['truncated_half'] = base[:len(base) // 2] + b'\n%%EOF\n'
    out['zero_pages'] = _write(PdfWriter())
    out['garbled_startxref'] = base.replace(b'startxref', b'startxref\n99999999\nxx', 1) if b'startxref' in base else base
    # xref 오프셋 오염(첫 객체 위치 틀어짐)
    out['shifted_offsets'] = base.replace(b'%PDF-', b'%PDF-' + b'\n' * 7, 1).replace(b'\n' * 7, b'', 1)[:0] or (b'%PDF-1.7\n' + b'#' * 40 + base[9:])
    out['xref_zeroed'] = base.replace(b'0000000000 65535 f', b'0000000000 65535 n', 1)
    # --- 파서 엄격도 시험(pypdf strict 대 lopdf) ---
    w = _pages(2)
    from pypdf.generic import StreamObject
    cs = StreamObject()
    cs._data = b'BT /F1 12 Tf (hello) Tj ET'
    w.pages[0][NameObject('/Contents')] = w._add_object(cs)
    good = _write(w)
    out['ok_content_stream'] = good
    out['bad_stream_length'] = good.replace(b'/Length 26', b'/Length 9999', 1) if b'/Length 26' in good else good + b'#bad_len_marker'
    out['missing_endobj'] = good.replace(b'endobj', b'', 1)
    out['no_root'] = good.replace(b'/Root', b'/Rxxt', 1)
    out['kids_type_broken'] = good.replace(b'/Kids', b'/Kidz', 1)
    out['xref_offset_wrong'] = good.replace(b'0000000015 00000 n', b'0000000999 00000 n', 1)
    out['trailing_junk_small'] = good + b'\n' + b'x' * 100
    out['trailing_junk_5000'] = good + b'\n' + b'x' * 5000
    out['incremental_update'] = good + b'\n% incremental\n'
    out['crlf_whole_file'] = good.replace(b'\n', b'\r\n')
    # /Encrypt 가 있으나 /P 가 없는 원시 바이트(가짜)
    out['fake_encrypt_no_p'] = b'%PDF-1.4\n/Encrypt\n%%EOF\n'
    # 암호화(열기 암호 없음: 권한 제한 / 열기 암호 있음: 암호화)
    for algo in ('RC4-40', 'RC4-128', 'AES-128', 'AES-256'):
        key = algo.lower().replace('-', '')
        out[f'enc_{key}_nouserpw'] = _encrypted('', algo)
        out[f'enc_{key}_userpw'] = _encrypted('secret', algo)
    return out


def main(argv):
    dest = Path(argv[1]) if len(argv) > 1 else ROOT / 'pdf_edit_core' / 'tests' / 'fixtures' / 'classify'
    dest.mkdir(parents=True, exist_ok=True)
    expected = {}
    for name, data in cases().items():
        path = dest / f'{name}.pdf'
        path.write_bytes(data)
        try:
            PdfEditService._classify(path)
            expected[name] = 'OK'
        except ServiceError as exc:
            expected[name] = exc.code
    # 존재하지 않는 파일
    try:
        PdfEditService._classify(dest / '__missing__.pdf')
        expected['__missing__'] = 'OK'
    except ServiceError as exc:
        expected['__missing__'] = exc.code
    import pypdf
    (dest / 'expected.json').write_text(json.dumps({'pypdf_version': pypdf.__version__, 'cases': expected},
                                                    ensure_ascii=False, indent=1, sort_keys=True) + '\n', encoding='utf-8')
    for k, v in sorted(expected.items()):
        print(f'{k:28s} {v}')


if __name__ == '__main__':
    main(sys.argv)
