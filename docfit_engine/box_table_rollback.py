"""박스 표 교체의 COM 편집 트랜잭션. 결과 확정 전 HWPX로 확인한다."""
from collections import Counter
from contextlib import contextmanager
from pathlib import Path
import re
import tempfile

from defusedxml import ElementTree as ET

from docfit_core.hwpx import inspect_hwpx, open_hwpx_safely


class BoxTableCancelled(RuntimeError):
    pass


def _document_signature(path):
    """원문·서식·개체를 비교한다. 재조판되는 linesegarray만 제외한다."""
    def tree(node):
        return (node.tag, tuple(sorted(node.attrib.items())), node.text or '',
                tuple((tree(child), child.tail or '') for child in node
                      if child.tag.rsplit('}', 1)[-1] != 'linesegarray'))

    result = {}
    with open_hwpx_safely(path) as archive:
        for name in archive.namelist():
            if name == 'Contents/header.xml' or re.fullmatch(r'Contents/section\d+\.xml', name):
                result[name] = tree(ET.fromstring(archive.read(name)))
            elif name.startswith('BinData/'):
                result[name] = archive.read(name)
    if 'Contents/header.xml' not in result or not any(re.fullmatch(r'Contents/section\d+\.xml', name) for name in result):
        raise RuntimeError('박스 표 복원용 HWPX 구조가 불완전합니다.')
    return result


def _tables(path):
    # inspect_hwpx의 Markdown용 | 이스케이프를 되돌려 실제 칸 내용으로 비교한다.
    return Counter(tuple(tuple(cell.replace(r'\|', '|') for cell in row) for row in block.rows)
                   for block in inspect_hwpx(path).blocks
                   if block.type == 'table')


@contextmanager
def protect_box_replacements(namespace, *, save, reopen, cleanup, cancelled, log):
    """작업 컨텍스트의 교체 함수에만 보호를 건다. 대상이 없으면 저장하지 않는다.

    상태: 보관 → 교체 → 삽입검증 → 확정. 예외/취소는 복원 → 복원검증으로
    이동한다. 복원에 성공해도 변환 실패를 문서까지 전파하여 결과를 저장하지 않는다.
    """
    replace = namespace['_박스그림표_교체']

    def guarded(start, end, rows):
        folder = Path(tempfile.mkdtemp(prefix='hwp_table_guard_box_'))
        before, after, restored = (folder / name for name in ('before.hwpx', 'after.hwpx', 'restored.hwpx'))
        try:
            # 보관에 실패하면 아직 삭제를 시작하지 않는다.
            save(before)
            signature = _document_signature(before)
            tables = _tables(before)
            try:
                if cancelled():
                    raise BoxTableCancelled('박스 표 교체 중단')
                result = replace(start, end, rows)
                if result is False:
                    raise RuntimeError('박스 표 교체 결과가 False입니다.')
                if cancelled():
                    raise BoxTableCancelled('박스 표 교체 중단')
                save(after)
                columns = max(map(len, rows))
                expected = tuple(tuple(row + [''] * (columns - len(row))) for row in rows)
                tables[expected] += 1
                if _tables(after) != tables:
                    raise RuntimeError('박스 표 삽입 확인 실패: 표 개수 또는 칸 내용 불일치')
            except BaseException as error:
                # 취소 이벤트를 확인하지 않고 복원한다. Open=True만으로 복원 성공을
                # 선언하지 않으며, 다시 저장한 내용과 서식까지 보관본과 비교한다.
                try:
                    if reopen(before) is False:
                        raise RuntimeError('보관 문서 재열기 실패')
                    save(restored)
                    if _document_signature(restored) != signature:
                        raise RuntimeError('원문·서식 비교 불일치')
                except Exception as restore_error:
                    raise RuntimeError(f'박스 표 원문·서식 복원 확인 실패 — 결과 저장 중단: {restore_error}') from error
                log('박스 표 변환 실패 후 원문·서식 복원 검증 완료 — 결과 저장 중단')
                raise
            return result
        finally:
            # SaveAs는 임시 파일을 현재 문서로 남길 수 있다. 기존 지연 정리를 사용한다.
            cleanup(folder)

    namespace['_박스그림표_교체'] = guarded
    try:
        yield
    finally:
        namespace['_박스그림표_교체'] = replace
