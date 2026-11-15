from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

from docfit_core import (HwpxSecurityError, compare_documents, export_markdown, inspect_hwpx,
                        validate_and_inspect_hwpx, validate_hwpx, open_hwpx_safely)


HEADER = b'<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="urn:head" />'
SECTION = b'''<?xml version="1.0" encoding="UTF-8"?>
<hs:sec xmlns:hs="urn:section" xmlns:hp="urn:para">
  <hp:p><hp:run><hp:t>1. \xeb\xac\xb8\xec\x84\x9c \xec\xa0\x9c\xeb\xaa\xa9</hp:t></hp:run></hp:p>
  <hp:tbl><hp:tr><hp:tc><hp:p><hp:run><hp:t>\xed\x95\xad\xeb\xaa\xa9</hp:t></hp:run></hp:p></hp:tc><hp:tc><hp:p><hp:run><hp:t>\xeb\x82\xb4\xec\x9a\xa9</hp:t></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>
</hs:sec>'''


def make_hwpx(path: Path, section: bytes = SECTION):
    with ZipFile(path, "w", ZIP_DEFLATED) as archive:
        archive.writestr("Contents/header.xml", HEADER)
        archive.writestr("Contents/section0.xml", section)
        archive.writestr("BinData/image.png", b"png")


class HwpxCoreTest(unittest.TestCase):
    def test_safe_opener_validates_before_read_and_is_read_only(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "sample.hwpx"
            make_hwpx(source)
            with open_hwpx_safely(source) as archive:
                self.assertIn("Contents/header.xml", archive.namelist())
            with self.assertRaises(ValueError):
                open_hwpx_safely(source, "w")

    def test_safe_opener_preserves_entry_and_size_limits(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "limited.hwpx"
            make_hwpx(source)
            from docfit_core.hwpx import HwpxLimits
            with self.assertRaises(HwpxSecurityError):
                open_hwpx_safely(source, limits=HwpxLimits(max_entries=2))
            with self.assertRaises(HwpxSecurityError):
                open_hwpx_safely(source, limits=HwpxLimits(max_file_size=8))

    def test_inspect_and_markdown_export(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "sample.hwpx"
            make_hwpx(source)
            report = inspect_hwpx(source)
            self.assertEqual(report.section_count, 1)
            self.assertEqual(report.paragraph_count, 1)
            self.assertEqual(report.table_count, 1)
            self.assertEqual(report.image_count, 1)
            target = export_markdown(source)
            markdown = target.read_text(encoding="utf-8")
            self.assertIn("## 1. 문서 제목", markdown)
            self.assertIn("| 항목 | 내용 |", markdown)

    def test_rejects_path_traversal(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "unsafe.hwpx"
            with ZipFile(source, "w") as archive:
                archive.writestr("Contents/header.xml", HEADER)
                archive.writestr("Contents/section0.xml", SECTION)
                archive.writestr("../outside.txt", b"bad")
            with self.assertRaises(HwpxSecurityError):
                validate_hwpx(source)

    def test_integrity_comparison_detects_missing_table(self):
        no_table = b'''<hs:sec xmlns:hs="urn:section" xmlns:hp="urn:para"><hp:p><hp:run><hp:t>1. \xeb\xac\xb8\xec\x84\x9c \xec\xa0\x9c\xeb\xaa\xa9</hp:t></hp:run></hp:p></hs:sec>'''
        with tempfile.TemporaryDirectory() as folder:
            before_path = Path(folder) / "before.hwpx"
            after_path = Path(folder) / "after.hwpx"
            make_hwpx(before_path)
            make_hwpx(after_path, no_table)
            result = compare_documents(inspect_hwpx(before_path), inspect_hwpx(after_path))
            # 표가 사라지면 경고가 아니라 오류다(무결성 검사 실패).
            self.assertTrue(any(issue["severity"] == "error" and "표가 사라졌습니다" in issue["message"]
                                for issue in result["issues"]))
            self.assertFalse(result["ok"])
            # 반대로 표가 늘어난 것은 경고로 둔다.
            added = compare_documents(inspect_hwpx(after_path), inspect_hwpx(before_path))
            self.assertTrue(added["ok"])
            self.assertTrue(any("표 개수" in issue["message"] for issue in added["issues"]))

    def test_integrity_comparison_detects_missing_control(self):
        # '새 쪽 번호' 같은 글자 없는 컨트롤(hp:ctrl)이 사라지면 오류다.
        with_ctrl = SECTION.replace(
            b'<hp:p><hp:run><hp:t>',
            b'<hp:p><hp:run><hp:ctrl><hp:newNum num="14" numType="PAGE"/></hp:ctrl><hp:t>', 1)
        with tempfile.TemporaryDirectory() as folder:
            before_path = Path(folder) / "before.hwpx"
            after_path = Path(folder) / "after.hwpx"
            make_hwpx(before_path, with_ctrl)
            make_hwpx(after_path)
            result = compare_documents(inspect_hwpx(before_path), inspect_hwpx(after_path))
            self.assertFalse(result["ok"])
            self.assertTrue(any("컨트롤이 사라졌습니다" in issue["message"] for issue in result["issues"]))

    def test_text_moved_into_tables_is_not_counted_as_lost(self):
        # 준말 줄·박스 그림 표를 표로 바꾸면 글이 문단에서 표 칸으로 옮겨 간다. 선 글자(─│)는 분량에서 뺀다.
        lines = ['┌──────────┬──────────┐', '│ 구  분   │ 내  용   │', '├──────────┼──────────┤',
                 '│ 추진 방향│ 기술 차단 │', '└──────────┴──────────┘', '끝.']
        text_doc = ('<hs:sec xmlns:hs="urn:section" xmlns:hp="urn:para">'
                    + ''.join(f'<hp:p><hp:run><hp:t>{line}</hp:t></hp:run></hp:p>' for line in lines)
                    + '</hs:sec>').encode()
        cells = ''.join(f'<hp:tr><hp:tc><hp:subList><hp:p><hp:run><hp:t>{a}</hp:t></hp:run></hp:p></hp:subList></hp:tc>'
                        f'<hp:tc><hp:subList><hp:p><hp:run><hp:t>{b}</hp:t></hp:run></hp:p></hp:subList></hp:tc></hp:tr>'
                        for a, b in (('구  분', '내  용'), ('추진 방향', '기술 차단')))
        table_doc = ('<hs:sec xmlns:hs="urn:section" xmlns:hp="urn:para"><hp:p><hp:run><hp:tbl>' + cells
                     + '</hp:tbl></hp:run></hp:p><hp:p><hp:run><hp:t>끝.</hp:t></hp:run></hp:p></hs:sec>').encode()
        with tempfile.TemporaryDirectory() as folder:
            before_path = Path(folder) / "before.hwpx"
            after_path = Path(folder) / "after.hwpx"
            make_hwpx(before_path, text_doc)
            make_hwpx(after_path, table_doc)
            result = compare_documents(inspect_hwpx(before_path), inspect_hwpx(after_path))
            self.assertTrue(result["ok"], result["issues"])
            self.assertFalse(any("분량" in issue["message"] for issue in result["issues"]))
            # 표 칸 글까지 실제로 지워지면 여전히 오류다.
            make_hwpx(after_path, b'<hs:sec xmlns:hs="urn:section" xmlns:hp="urn:para">'
                                  b'<hp:p><hp:run><hp:t>\xeb\x81\x9d.</hp:t></hp:run></hp:p></hs:sec>')
            lost = compare_documents(inspect_hwpx(before_path), inspect_hwpx(after_path))
            self.assertFalse(lost["ok"])

    def test_text_after_inline_space_element_is_read(self):
        # 원본: 'ㅇ' + 전각 공백 요소 + 본문(tail). 한/글 재저장본: 같은 글자를 일반 텍스트로 저장.
        inline = '<hs:sec xmlns:hs="urn:section" xmlns:hp="urn:para"><hp:p><hp:run><hp:t>ㅇ<hp:fwSpace/>본문 내용</hp:t></hp:run></hp:p></hs:sec>'.encode()
        plain = '<hs:sec xmlns:hs="urn:section" xmlns:hp="urn:para"><hp:p><hp:run><hp:t>ㅇ 본문 내용</hp:t></hp:run></hp:p></hs:sec>'.encode()
        with tempfile.TemporaryDirectory() as folder:
            before_path = Path(folder) / "before.hwpx"
            after_path = Path(folder) / "after.hwpx"
            make_hwpx(before_path, inline)
            make_hwpx(after_path, plain)
            before = inspect_hwpx(before_path)
            self.assertEqual(before.text, "ㅇ 본문 내용")
            self.assertEqual(compare_documents(before, inspect_hwpx(after_path))["text_similarity"], 1.0)

    def test_identical_body_skips_expensive_sequence_matching(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'same.hwpx'
            make_hwpx(source)
            document = inspect_hwpx(source)
            with patch('docfit_core.hwpx.SequenceMatcher', side_effect=AssertionError('동일 본문 재비교')):
                result = compare_documents(document, document)
            self.assertTrue(result['ok'])
            self.assertEqual(result['text_similarity'], 1.0)

    def test_all_body_text_can_move_into_a_table(self):
        before_xml = '<sec><p><run><t>보존할 보고서 본문입니다</t></run></p></sec>'.encode()
        after_xml = ('<sec><p><run><tbl><tr><tc><p><run><t>보존할 보고서 본문입니다</t>'
                     '</run></p></tc></tr></tbl></run></p></sec>').encode()
        with tempfile.TemporaryDirectory() as folder:
            before_path, after_path = Path(folder) / 'before.hwpx', Path(folder) / 'after.hwpx'
            make_hwpx(before_path, before_xml)
            make_hwpx(after_path, after_xml)
            result = compare_documents(inspect_hwpx(before_path), inspect_hwpx(after_path))
            self.assertTrue(result['ok'], result['issues'])

    def test_table_only_document_still_detects_content_loss(self):
        template = '<sec><tbl><tr><tc><p><run><t>{}</t></run></p></tc></tr></tbl></sec>'
        with tempfile.TemporaryDirectory() as folder:
            before_path, after_path = Path(folder) / 'before.hwpx', Path(folder) / 'after.hwpx'
            make_hwpx(before_path, template.format('표 안의 중요한 본문 내용입니다').encode())
            make_hwpx(after_path, template.format('표').encode())
            result = compare_documents(inspect_hwpx(before_path), inspect_hwpx(after_path))
            self.assertFalse(result['ok'])
            self.assertTrue(any('분량' in issue['message'] for issue in result['issues']))

    def test_summary_does_not_copy_discarded_blocks(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'summary.hwpx'
            make_hwpx(source)
            document = inspect_hwpx(source)
            class NoCopy:
                def __deepcopy__(self, memo):
                    raise AssertionError('요약에서 버릴 문서 구조를 복사함')
            document.blocks = [NoCopy()]
            summary = document.summary()
            self.assertNotIn('blocks', summary)
            self.assertNotIn('text', summary)
            summary['warnings'].append('독립된 목록')
            self.assertEqual(document.warnings, [])

    def test_combined_validation_and_inspection_checks_crc_once(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'checked.hwpx'
            make_hwpx(source)
            original = ZipFile.testzip
            with patch.object(ZipFile, 'testzip', autospec=True, side_effect=original) as crc:
                validation, document = validate_and_inspect_hwpx(source)
            crc.assert_called_once()
            self.assertEqual(validation['entry_count'], 3)
            self.assertEqual(document.table_count, 1)
            with patch.object(ZipFile, 'testzip', return_value='Contents/section0.xml'):
                with self.assertRaisesRegex(HwpxSecurityError, 'CRC'):
                    validate_and_inspect_hwpx(source)


    def test_large_hwpx_structure_mode_bypasses_bomb_limit_while_strict_mode_blocks(self):
        """그림 많은 큰 합성 HWPX: 한도를 작게 주입했을 때 strict 모드는 차단하고 structure 모드는 통과한다."""
        from docfit_core.hwpx import HwpxLimits, validate_hwpx_structure
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "large_images.hwpx"
            with ZipFile(source, "w", ZIP_DEFLATED) as archive:
                archive.writestr("Contents/header.xml", HEADER)
                archive.writestr("Contents/section0.xml", SECTION)
                # 그림 많은 대형 문서 모사 (합성 이미지 데이터)
                archive.writestr("BinData/image1.bin", b"A" * 4096)
                archive.writestr("BinData/image2.bin", b"B" * 4096)

            # 1) 압축 해제 크기 한도(예: 1024바이트)를 작게 주입해 외부 입력 검증 시뮬레이션
            small_limits = HwpxLimits(max_total_size=1024)
            with self.assertRaises(HwpxSecurityError) as ctx:
                validate_hwpx(source, limits=small_limits)
            self.assertIn("압축 해제 예상 크기", str(ctx.exception))

            # 2) 결과 검증(structure_only=True / mode='structure' / validate_hwpx_structure)은 한도 초과에도 통과
            res1 = validate_hwpx(source, limits=small_limits, structure_only=True)
            self.assertEqual(res1["section_names"], ["Contents/section0.xml"])
            self.assertGreater(res1["total_uncompressed_size"], 8000)

            res2 = validate_hwpx(source, limits=small_limits, mode="structure")
            self.assertEqual(res2["section_names"], ["Contents/section0.xml"])

            res3 = validate_hwpx_structure(source)
            self.assertEqual(res3["section_names"], ["Contents/section0.xml"])

    def test_structure_mode_validates_xml_and_required_files(self):
        """structure 모드라도 필수 파일·섹션 누락이나 깨진 XML은 거부한다."""
        with tempfile.TemporaryDirectory() as folder:
            # 1) header.xml 누락
            no_header = Path(folder) / "no_header.hwpx"
            with ZipFile(no_header, "w") as z:
                z.writestr("Contents/section0.xml", SECTION)
            with self.assertRaises(HwpxSecurityError) as ctx:
                validate_hwpx(no_header, structure_only=True)
            self.assertIn("Contents/header.xml", str(ctx.exception))

            # 2) section 누락
            no_section = Path(folder) / "no_section.hwpx"
            with ZipFile(no_section, "w") as z:
                z.writestr("Contents/header.xml", HEADER)
            with self.assertRaises(HwpxSecurityError) as ctx:
                validate_hwpx(no_section, structure_only=True)
            self.assertIn("본문 섹션", str(ctx.exception))

            # 3) XML 구문 오류 (열리지 않는 깨진 XML)
            bad_xml = Path(folder) / "bad_xml.hwpx"
            with ZipFile(bad_xml, "w") as z:
                z.writestr("Contents/header.xml", b"<unclosed>")
                z.writestr("Contents/section0.xml", SECTION)
            with self.assertRaises(HwpxSecurityError) as ctx:
                validate_hwpx(bad_xml, structure_only=True)
            self.assertIn("XML 파싱 실패", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()

class HwpxSecurityAuditTests(unittest.TestCase):
    def test_duplicate_zip_members_are_rejected(self):
        import warnings
        with tempfile.TemporaryDirectory() as folder:
            source=Path(folder)/'duplicate.hwpx'
            make_hwpx(source)
            with warnings.catch_warnings():
                warnings.simplefilter('ignore',UserWarning)
                with ZipFile(source,'a') as archive:
                    archive.writestr('Contents/section0.xml',SECTION)
            with self.assertRaises(HwpxSecurityError):
                validate_hwpx(source)

    def test_external_entity_cannot_read_local_file(self):
        with tempfile.TemporaryDirectory() as folder:
            secret=Path(folder)/'private.txt';secret.write_text('must not leak')
            source=Path(folder)/'entity.hwpx'
            xml=('<!DOCTYPE sec [<!ENTITY steal SYSTEM "'+secret.as_uri()+'">]>'
                 '<sec><p><run><t>&steal;</t></run></p></sec>').encode()
            make_hwpx(source,xml)
            with self.assertRaises(HwpxSecurityError):
                inspect_hwpx(source)
