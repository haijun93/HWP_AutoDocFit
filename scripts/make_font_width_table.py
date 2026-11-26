"""공문 글꼴의 글자별 폭 표(resources/font_widths.json)를 만든다(INDENTW1).

설치된 TTF/OTF/TTC 의 cmap·hmtx 에서 글자별 폭(글꼴 단위)만 뽑는다. 글꼴 파일은 저장소에 넣지 않는다.
한/글 전용 HFT 글꼴은 읽지 않고 '측정 필요'(needs_measurement)로 남긴다. 이미 측정해 넣은 HFT 항목
(status 가 measured)과 TTF 항목의 한/글 실측 폭(measured·bold)은 다시 만들 때 지우지 않는다.
실측은 ``scripts/measure_indent_hangul.py widths`` 가 넣는다.

사용: python scripts/make_font_width_table.py [--output 경로]
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from docfit_core import hangul_fonts  # noqa: E402

DEFAULT_OUTPUT = ROOT / "resources" / "font_widths.json"

# (한/글 글꼴 이름, 별칭)
TTF_FONTS = (
    ("휴먼명조", ()),
    ("함초롬바탕", ("HCR Batang",)),
    ("함초롬돋움", ("HCR Dotum",)),
    ("HY헤드라인M", ("HYHeadLine M",)),
    ("맑은 고딕", ("Malgun Gothic",)),
    ("한컴돋움", ("Haansoft Dotum",)),
    ("HY견고딕", ()),
)

# 한/글 HFTInfo.dat 의 신명 계열 이름. 휴먼명조는 TTF 와 이름이 같은 HFT 가 따로 있다.
HFT_FONTS = (
    "휴먼명조", "한양신명조",
    "신명 세명조", "신명 신명조", "신명 신신명조", "신명 중명조", "신명 태명조", "신명 견명조",
    "신명 신문명조", "신명 순명조", "신명 세고딕", "신명 중고딕", "신명 태고딕", "신명 견고딕",
    "신명 신문고딕", "신명 세나루", "신명 디나루", "신명 신그래픽", "신명 태그래픽", "신명 궁서",
    "신명조 약자", "신명조 간자", "#신명조", "#태신명조",
)

# 표에 담는 문자 구역(한자는 빼며, 표에 없는 글자는 계산 쪽에서 대체로 표시한다).
BLOCKS = (
    (0x0020, 0x007E),  # 영문·숫자·문장부호
    (0x00A0, 0x00FF),  # 라틴 보충(· 등)
    (0x0370, 0x03FF),  # 그리스
    (0x2000, 0x2BFF),  # 일반 문장부호·화살표·수학·원문자·도형·기호
    (0x3000, 0x33FF),  # CJK 기호(「」)·한글 호환 자모(ㅇㅁ)·괄호/원 한글·단위
    (0xAC00, 0xD7A3),  # 한글 음절(가~하 포함)
    (0xFF00, 0xFFEF),  # 전각·반각 형
)


def _tables(data: bytes, offset: int) -> dict[str, int]:
    count = struct.unpack_from(">H", data, offset + 4)[0]
    result = {}
    for index in range(count):
        name, _, table_offset, _ = struct.unpack_from(">4sIII", data, offset + 12 + 16 * index)
        result[name.decode("latin-1")] = table_offset
    return result


def _face_offsets(data: bytes) -> list[int]:
    if data[:4] != b"ttcf":
        return [0]
    count = struct.unpack_from(">I", data, 8)[0]
    return list(struct.unpack_from(">%dI" % count, data, 12))


def _cmap(data: bytes, base: int) -> dict[int, int]:
    """유니코드 cmap(형식 4·12)의 {문자 코드: 글리프 번호}."""
    count = struct.unpack_from(">H", data, base + 2)[0]
    subtables = {}
    for index in range(count):
        platform, encoding, offset = struct.unpack_from(">HHI", data, base + 4 + 8 * index)
        if platform == 0 or (platform == 3 and encoding in (1, 10)):
            subtables[struct.unpack_from(">H", data, base + offset)[0]] = base + offset
    mapping: dict[int, int] = {}
    if 12 in subtables:
        start = subtables[12]
        groups = struct.unpack_from(">I", data, start + 12)[0]
        for index in range(groups):
            first, last, glyph = struct.unpack_from(">III", data, start + 16 + 12 * index)
            for code in range(first, min(last, 0xFFFF) + 1):
                mapping[code] = glyph + code - first
    elif 4 in subtables:
        start = subtables[4]
        segments = struct.unpack_from(">H", data, start + 6)[0] // 2
        ends = start + 14
        starts = ends + 2 * segments + 2
        deltas = starts + 2 * segments
        ranges = deltas + 2 * segments
        for index in range(segments):
            last = struct.unpack_from(">H", data, ends + 2 * index)[0]
            first = struct.unpack_from(">H", data, starts + 2 * index)[0]
            delta = struct.unpack_from(">H", data, deltas + 2 * index)[0]
            range_offset = struct.unpack_from(">H", data, ranges + 2 * index)[0]
            for code in range(first, min(last, 0xFFFE) + 1):
                if range_offset:
                    address = ranges + 2 * index + range_offset + 2 * (code - first)
                    glyph = struct.unpack_from(">H", data, address)[0]
                    if glyph:
                        glyph = (glyph + delta) & 0xFFFF
                else:
                    glyph = (code + delta) & 0xFFFF
                mapping[code] = glyph
    else:
        raise ValueError("유니코드 cmap 이 없습니다.")
    return {code: glyph for code, glyph in mapping.items() if glyph}


def read_advances(path: Path, face_index: int = 0) -> tuple[int, dict[int, int]]:
    """(units_per_em, {문자 코드: 폭(글꼴 단위)}) — BLOCKS 안에서 글꼴에 있는 글자만."""
    data = Path(path).read_bytes()
    tables = _tables(data, _face_offsets(data)[face_index])
    units = struct.unpack_from(">H", data, tables["head"] + 18)[0]
    metrics = struct.unpack_from(">H", data, tables["hhea"] + 34)[0]
    cmap = _cmap(data, tables["cmap"])
    advances = {}
    for first, last in BLOCKS:
        for code in range(first, last + 1):
            glyph = cmap.get(code)
            if glyph is None:
                continue
            # hmtx 의 긴 항목 수를 넘는 글리프는 마지막 폭을 쓴다(OpenType 규격).
            index = min(glyph, metrics - 1)
            advances[code] = struct.unpack_from(">H", data, tables["hmtx"] + 4 * index)[0]
    return units, advances


def compress(advances: dict[int, int]) -> list[list[int]]:
    """[첫 코드, 끝 코드, 폭] 구간 목록. 글꼴에 없는 코드는 어느 구간에도 들지 않는다."""
    ranges: list[list[int]] = []
    for code in sorted(advances):
        width = advances[code]
        if ranges and ranges[-1][1] == code - 1 and ranges[-1][2] == width:
            ranges[-1][1] = code
        else:
            ranges.append([code, code, width])
    return ranges


def _find_face(path: Path, names: tuple[str, ...]) -> int:
    for index, pair in enumerate(hangul_fonts._font_names(path)):
        if any(name in pair for name in names):
            return index
    return 0


def _installed() -> dict[str, Path]:
    """글꼴 이름 → 파일. 같은 이름이 Windows 글꼴 폴더에도 있으면 그것을 쓴다.

    한/글은 설치된 Windows 글꼴이 있으면 자기 폴더의 동명 글꼴보다 그 폭을 쓴다(HY헤드라인M 실측, INDENTW2).
    """
    installed = {entry["name"]: Path(entry["path"]) for entry in hangul_fonts.catalog()}
    for folder in hangul_fonts.system_font_dirs():
        for path in sorted(folder.iterdir()):
            if path.suffix.lower() not in hangul_fonts.FONT_EXTENSIONS or not path.is_file():
                continue
            try:
                pairs = hangul_fonts._font_names(path)
            except (OSError, struct.error, ValueError):
                continue
            for korean, english in pairs:
                name = korean or english
                if name in installed and _bold_stem(path) is None:
                    installed[name] = path
    return installed


def _bold_stem(path: Path) -> str | None:
    """굵은·가는 파생 글꼴 파일(malgunbd, HANBatangB, malgunsl)이면 그 표시. 보통 글꼴이면 None."""
    for suffix in ("bd", "sl", "B"):
        base = path.stem[: -len(suffix)]
        if path.stem.endswith(suffix) and path.with_name(base + path.suffix).is_file():
            return suffix
    return None


def build(previous: dict | None = None) -> dict:
    installed = _installed()
    kept = {entry["face"]: entry for entry in (previous or {}).get("fonts", [])
            if entry["type"] == "HFT" and entry["status"] == "measured"}
    overrides = {entry["face"]: {key: entry[key] for key in ("measured", "bold") if key in entry}
                 for entry in (previous or {}).get("fonts", []) if entry["type"] == "TTF"}
    fonts = []
    for face, aliases in TTF_FONTS:
        path = next((installed[name] for name in (face, *aliases) if name in installed), None)
        if path is None:
            raise SystemExit(f"설치되지 않은 글꼴이라 표를 만들 수 없습니다: {face}")
        units, advances = read_advances(path, _find_face(path, (face, *aliases)))
        entry = {"face": face, "type": "TTF", "aliases": list(aliases), "status": "ok",
                 "source_file": path.name, "units_per_em": units, "ranges": compress(advances)}
        entry.update(overrides.get(face, {}))
        fonts.append(entry)
    for face in HFT_FONTS:
        fonts.append(kept.get(face) or {"face": face, "type": "HFT", "aliases": [],
                                         "status": "needs_measurement"})
    return {"version": 1, "fonts": fonts}


def dumps(table: dict) -> str:
    """글꼴 하나가 한 줄인 안정된 JSON(차이 보기와 크기를 위해)."""
    lines = [json.dumps(font, ensure_ascii=False, separators=(",", ":")) for font in table["fonts"]]
    return '{"version":%d,"fonts":[\n%s\n]}\n' % (table["version"], ",\n".join(lines))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args(argv)
    previous = json.loads(args.output.read_text(encoding="utf-8")) if args.output.is_file() else None
    table = build(previous)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(dumps(table), encoding="utf-8", newline="\n")
    for font in table["fonts"]:
        print(font["face"], font["type"], font["status"], len(font.get("ranges", [])))
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    raise SystemExit(main())
