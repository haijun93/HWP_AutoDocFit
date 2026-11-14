"""src/fold_table.rs(고정표)와 tests/data/fold_cases.tsv(시험 표)를 Python 기준 구현에서 만든다.

사용: python -X utf8 kfind/core/tools/gen_fold_table.py [--check]
--check: 파일을 쓰지 않고 지금 파일과 같은지만 본다(다르면 종료 코드 1).
"""
import sys
import unicodedata
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from fold_ref import FOLD_RULE_VERSION, TABLE_UNICODE_VERSION, fold  # noqa: E402

# (입력, 설명). 기대값은 기준 구현이 낸다. 유니코드 16.0 이전부터 있던 글자만 쓴다.
CASES = [
    ("예산 집행, (계획).", "공백·문장부호 제거"),
    ("ABC_d-e", "영문 소문자화, 밑줄·붙임표 제거"),
    ("\u110b\u1168\u1109\u1161\u11ab", "조합형(첫가끝) 자모 → 완성형 '예산'"),
    ("\uc608\uc0b0", "완성형 '예산' 그대로"),
    ("\u1112\u1161\u11ab\u1100\u1173\u11af", "조합형 '한글'"),
    ("\u3147\u3156\u3145", "호환 자모는 합치지 않음"),
    ("\uc608\u11ab", "완성형 뒤 종성 자모는 NFC로 합쳐짐(예+ㄴ=옌)"),
    ("Caf\u0065\u0301", "결합 악센트 → é"),
    ("STRASSE Stra\u00dfe", "ß는 casefold로 ss"),
    ("\u0130stanbul", "터키어 İ → i + 결합 점(결합 점은 글자·숫자가 아니라 빠짐)"),
    ("\u03a3\u03af\u03c3\u03c5\u03c6\u03bf\u03c2", "그리스어 끝 시그마 ς → σ"),
    ("\uff21\uff22\uff23\uff11\uff12", "전각 영문은 소문자화, NFC라 반각으로 바꾸지 않음"),
    ("\u2460\u2461 \u00bd \u00b2", "원 숫자·분수·위 첨자는 숫자로 남음"),
    ("\u24b6\u24d0", "원 문자 Ⓐⓐ는 글자·숫자가 아니라 빠짐"),
    ("제1조(목적) 1.5배", "숫자 사이 마침표 제거"),
    ("2026. 11. 12.", "날짜"),
    ("○ 예산·집행 ― 계획", "개조식 기호·가운뎃점 제거"),
    ("\u4e88\u7b97\u3000\u57f7\u884c", "한자, 전각 공백 제거"),
    ("\uf9b5", "호환 한자 例(U+F9B5)는 NFC로 U+4F8B"),
    ("e\u200bx\ufeff", "폭 없는 공백·BOM 제거"),
    ("\u00c5\u212b\u0041\u030a", "Å 세 가지 표기 → å"),
    ("\ufb01\ufb00", "합자 ﬁ ﬀ는 casefold로 풀림"),
    ("\u01c5", "ǅ(제목형) → ǆ"),
    ("\u1e9e", "대문자 ẞ → ss"),
    ("\ud55c\u0301", "한글 뒤 결합 문자는 빠짐"),
    ("a\u0308\u0304", "여러 결합 문자(ǟ)"),
    ("\u0958", "합성 제외 문자(क़)는 NFC로 풀리고 결합 기호는 빠짐"),
    ("", "빈 글"),
    (" \t\n", "공백만"),
    ("ｅ－ｍａｉｌ ＠", "전각 기호 제거"),
    ("\u3131\u314f", "호환 자모 ㄱㅏ"),
    ("\U0001d400\U0001f600x", "수학 굵은 A는 글자(소문자 없음), 그림 문자는 빠짐"),
]


def ranges(pred):
    out, start = [], None
    for cp in range(0x110000):
        ok = not (0xD800 <= cp <= 0xDFFF) and pred(chr(cp))
        if ok and start is None:
            start = cp
        elif not ok and start is not None:
            out.append((start, cp - 1))
            start = None
    if start is not None:
        out.append((start, 0x10FFFF))
    return out


def table_rs():
    keep = ranges(str.isalnum)
    folds = []
    for cp in range(0x110000):
        if 0xD800 <= cp <= 0xDFFF:
            continue
        f = chr(cp).casefold()
        if f != chr(cp):
            assert 1 <= len(f) <= 3
            folds.append((cp, [ord(c) for c in f] + [0] * (3 - len(f))))
    lines = [
        "//! 생성 파일: kfind/core/tools/gen_fold_table.py가 만든다. 손으로 고치지 않는다.",
        f"//! Python {unicodedata.unidata_version} 유니코드 자료의 `str.isalnum`·`str.casefold`를 그대로 옮긴 고정표.",
        f"pub const UNICODE_VERSION: &str = \"{unicodedata.unidata_version}\";",
        f"pub const FOLD_RULE_VERSION: u32 = {FOLD_RULE_VERSION};",
        "/// 글자·숫자(남기는 글자) 구간, 양 끝 포함, 오름차순.",
        f"pub static KEEP: [(u32, u32); {len(keep)}] = [",
    ]
    for i in range(0, len(keep), 6):
        lines.append("    " + " ".join(f"(0x{a:X}, 0x{b:X})," for a, b in keep[i:i + 6]))
    lines += ["];", "/// casefold가 자기 자신이 아닌 글자 → 최대 3글자(0은 없음), 오름차순.",
              f"pub static CASEFOLD: [(u32, [u32; 3]); {len(folds)}] = ["]
    for i in range(0, len(folds), 4):
        lines.append("    " + " ".join(f"(0x{cp:X}, [0x{f[0]:X}, 0x{f[1]:X}, 0x{f[2]:X}])," for cp, f in folds[i:i + 4]))
    lines.append("];")
    return "\n".join(lines) + "\n"


def hexes(s):
    return " ".join(f"{ord(c):04X}" for c in s) or "-"


def cases_tsv():
    rows = ["# 입력 코드포인트(16진)\t기대 비교형 코드포인트(16진)\t설명. '-'는 빈 글. 생성: tools/gen_fold_table.py"]
    for text, note in CASES:
        rows.append(f"{hexes(text)}\t{hexes(fold(text))}\t{note}")
    return "\n".join(rows) + "\n"


def main():
    if unicodedata.unidata_version != TABLE_UNICODE_VERSION:
        sys.exit(f"이 Python의 유니코드 판({unicodedata.unidata_version})이 고정표 판({TABLE_UNICODE_VERSION})과 다르다")
    outputs = {HERE.parent / "src" / "fold_table.rs": table_rs(), HERE.parent / "tests" / "data" / "fold_cases.tsv": cases_tsv()}
    if "--check" in sys.argv:
        stale = [str(p) for p, text in outputs.items() if not p.exists() or p.read_text(encoding="utf-8") != text]
        if stale:
            sys.exit("생성 파일이 낡았다: " + ", ".join(stale))
        return
    for p, text in outputs.items():
        p.write_text(text, encoding="utf-8", newline="\n")
        print(p, len(text))


if __name__ == "__main__":
    main()
