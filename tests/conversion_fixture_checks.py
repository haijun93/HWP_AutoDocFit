"""압축 결과 대신 fixture의 모든 원시 내용과 ZIP 메타데이터를 대조한다."""
import copy
import json
import zipfile
from pathlib import Path

from docfit_core import conversion_fixtures as fx

COMMITTED = Path(__file__).resolve().parent / "fixtures" / "conversion_fidelity"
OFFICE_FORMATS = {"docx", "xlsx", "pptx"}


def assert_zip_contents_equal(case, actual, expected):
    # compress_size/header_offset는 deflate 출력에 종속된다. 나머지는 그대로 검사한다.
    fields = (
        "filename", "date_time", "compress_type", "comment", "extra",
        "create_system", "create_version", "extract_version", "reserved",
        "flag_bits", "volume", "internal_attr", "external_attr", "CRC", "file_size",
    )
    with zipfile.ZipFile(actual) as a, zipfile.ZipFile(expected) as b:
        case.assertEqual(a.comment, b.comment)
        case.assertEqual(a.namelist(), b.namelist())  # 순서·중복·추가·누락도 검사
        for ai, bi in zip(a.infolist(), b.infolist()):
            with case.subTest(part=ai.filename):
                case.assertEqual(tuple(getattr(ai, f) for f in fields),
                                 tuple(getattr(bi, f) for f in fields))
                # XML 공백·속성 순서·관계·그림을 정규화하지 않고 바이트 그대로 대조한다.
                # read()는 각 항목의 CRC도 검사한다.
                case.assertEqual(a.read(ai), b.read(bi))


def assert_generation_matches_committed(case, generated_dir, manifest):
    generated_dir = Path(generated_dir)
    committed = json.loads((COMMITTED / "manifest.json").read_text(encoding="utf-8"))
    manifests = []
    for directory, source in ((generated_dir, manifest), (COMMITTED, committed)):
        comparable = copy.deepcopy(source)
        for entry in comparable["fixtures"]:
            path = directory / entry["file"]
            # 압축된 파일 해시·크기는 각 manifest와 해당 실물 사이에서 계속 엄격히 검증한다.
            case.assertEqual(fx.sha256(path), entry["sha256"], str(path))
            case.assertEqual(path.stat().st_size, entry["bytes"], str(path))
            if entry["format"] in OFFICE_FORMATS:
                del entry["sha256"], entry["bytes"]
        manifests.append(comparable)
    case.assertEqual(*manifests)  # expected·ID·형식·파일명 및 텍스트 해시는 동일해야 한다.
    case.assertEqual({p.name for p in generated_dir.iterdir()},
                     {p.name for p in COMMITTED.iterdir()})
    for entry in manifest["fixtures"]:
        actual, expected = generated_dir / entry["file"], COMMITTED / entry["file"]
        with case.subTest(fixture=entry["file"]):
            if entry["format"] in OFFICE_FORMATS:
                assert_zip_contents_equal(case, actual, expected)
            else:
                case.assertEqual(actual.read_bytes(), expected.read_bytes())
    # Markdown/HTML이 참조하는 별도 PNG도 manifest 밖이라고 빠뜨리지 않는다.
    case.assertEqual((generated_dir / "image1.png").read_bytes(),
                     (COMMITTED / "image1.png").read_bytes())
