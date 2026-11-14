"""kfind-core 문장 나누기 규칙의 Python 기준 구현과 대조 표 생성기(시험 전용, 규칙 판 SENTENCE_RULE_VERSION).

Rust `kfind_core::split_sentences`(src/sentence.rs)와 결과가 글자 단위로 같아야 한다. 규칙을 바꾸면 양쪽 판 번호를 함께 올린다.

규칙(문단 하나 → 문장 구간 목록):
1. 줄바꿈(LINE_BREAKS, "\\r\\n"은 하나)은 항상 경계.
2. 끝맺음 글자(TERMINATORS) 묶음 + 바로 뒤 닫는 글자(CLOSERS) 묶음 다음이 공백이거나 글 끝이면 그 뒤가 경계.
3. 예외(경계 아님) — 묶음이 마침표(PERIODS) 하나뿐일 때만:
   ① 바로 앞 글자가 아라비아 숫자(0-9): "2026. 11. 12.", "1. 개요", "3.5"(3.5는 뒤가 공백이 아니라 원래 경계 아님)
   ② 그 줄 처음부터 마침표까지가 목록 표지: 앞 공백과 여는 괄호 하나를 빼고 한글 음절 1자, 또는 영문자 1~4자,
      또는 로마 숫자(U+2160~217F) 1~4자 — "가. 사업 개요", "IV. 결론", "Ⅱ. 추진 계획"
   ③ 마침표 2개 이상 묶음(말줄임 "...")은 경계 아님(①②와 별개로 늘 그렇다).
4. 문장은 앞뒤 공백(유니코드 White_Space)을 뺀 구간. 빈 문장은 버린다. 비교형이 빈 문장(기호만)은 남긴다.
5. 구간은 문단 안 **코드포인트** 위치 (start, len). Python 문자열 색인과 같다.

사용:
  python -X utf8 kfind/core/tools/sentence_ref.py [--check]      대조 표(tests/data/sentence_cases.tsv) 쓰기/낡음 확인
  python -X utf8 kfind/core/tools/sentence_ref.py --fuzz N EXE      무작위 문단 N개를 Rust 예제(EXE, examples/sentence_dump.rs)와 대조
  python -X utf8 kfind/core/tools/sentence_ref.py --sqlite DB EXE   SQLite 문단 표(content_paragraph_fts.body)를 Rust와 대조(사본 DB로)
"""
import random
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from .fold_ref import WHITE_SPACE as _WS_CLASS

SENTENCE_RULE_VERSION = 1

DATA = HERE.parent / "tests" / "data"
CASES = DATA / "sentence_cases.tsv"


def _expand(cls):
    """fold_ref의 정규식 문자 집합 문자열("\\u2000-\\u200a" 포함)을 글자 집합으로."""
    out, i = set(), 0
    while i < len(cls):
        if i + 2 < len(cls) and cls[i + 1] == "-":
            out.update(chr(c) for c in range(ord(cls[i]), ord(cls[i + 2]) + 1))
            i += 3
        else:
            out.add(cls[i])
            i += 1
    return frozenset(out)


WHITE_SPACE = _expand(_WS_CLASS)  # Rust char::is_whitespace와 같은 25자
LINE_BREAKS = frozenset("\n\r\x0b\x0c\x85  ")
PERIODS = frozenset(".．")  # . ．
TERMINATORS = frozenset(".?!。．？！")  # . ? ! 。 ． ？ ！
CLOSERS = frozenset("\"')]’”」』〉》】）］")  # " ' ) ] ’ ” 」 』 〉 》 】 ） ］
OPENERS = frozenset("([（［")  # ( [ （ ［


def _is_marker(chars, line_start, end):
    """chars[line_start:end]가 목록 표지(마침표 앞 부분)인가."""
    i = line_start
    while i < end and chars[i] in WHITE_SPACE:
        i += 1
    if i < end and chars[i] in OPENERS:
        i += 1
    body = chars[i:end]
    n = len(body)
    if n == 0:
        return False
    if n == 1 and "가" <= body[0] <= "힣":
        return True
    if n <= 4 and all(("a" <= c <= "z") or ("A" <= c <= "Z") for c in body):
        return True
    if n <= 4 and all("Ⅰ" <= c <= "ⅿ" for c in body):
        return True
    return False


def split(text):
    """문단 → [(start, len)] (코드포인트)."""
    chars = list(text)
    n = len(chars)
    out = []

    def emit(a, b):
        while a < b and chars[a] in WHITE_SPACE:
            a += 1
        while b > a and chars[b - 1] in WHITE_SPACE:
            b -= 1
        if a < b:
            out.append((a, b - a))

    seg = line = i = 0
    while i < n:
        c = chars[i]
        if c in LINE_BREAKS:
            emit(seg, i)
            j = i + 1
            if c == "\r" and j < n and chars[j] == "\n":
                j += 1
            seg = line = i = j
            continue
        if c in TERMINATORS:
            j = i
            while j < n and chars[j] in TERMINATORS:
                j += 1
            k = j
            while k < n and chars[k] in CLOSERS:
                k += 1
            if k == n or chars[k] in WHITE_SPACE:
                run = chars[i:j]
                if all(r in PERIODS for r in run):
                    exempt = len(run) >= 2 or (i > 0 and "0" <= chars[i - 1] <= "9") or _is_marker(chars, line, i)
                else:
                    exempt = False
                if not exempt:
                    emit(seg, k)
                    seg = k
            i = k
            continue
        i += 1
    emit(seg, n)
    return out


def sentences(text):
    return [text[s:s + n] for s, n in split(text)]


def index_spans(text):
    """색인용 구간: split(text)에서 비교형이 빈 문장(기호·공백만)을 뺀 것.
    Rust `kfind_core::index_sentences`와 같다(2026-11-13 결정: 기호만 있는 문장은 색인 제외)."""
    from .fold_ref import fold
    return [(s, n) for s, n in split(text) if fold(text[s:s + n])]


# ---- 대조 표 ----

def to_hex(s):
    return " ".join(f"{ord(c):04X}" for c in s) if s else "-"


def from_hex(h):
    return "" if h == "-" else "".join(chr(int(x, 16)) for x in h.split(" "))


def spans_text(spans):
    return " ".join(f"{s}:{n}" for s, n in spans) if spans else "-"


# 손으로 고른 경우: (문단, 설명). 규칙 하나하나와 경계 사례.
HAND = [
    ("예산을 집행한다. 계획을 세운다.", "마침표 + 공백"),
    ("예산을 집행한다.계획을 세운다.", "마침표 뒤 공백 없음 → 한 문장"),
    ("정말인가? 그렇다! 좋다.", "물음표·느낌표"),
    ("정말?! 그렇다.", "끝맺음 묶음 ?!"),
    ("검토 중... 결과는 다음 주에 나온다.", "말줄임 ... 은 경계 아님"),
    ("검토 중.. 결과", "마침표 두 개도 경계 아님"),
    ("검토 중…. 다음", "… 뒤 마침표 하나 → 경계"),
    ("2026. 11. 12. 회의를 연다. 참석 바람.", "날짜 마침표는 경계 아님"),
    ("'26. 11. 회의", "줄인 연도"),
    ("비율은 3.5%이다. 다음 해는 4.0%다.", "소수점"),
    ("1. 사업 개요", "숫자 목록 표지"),
    ("1. 사업 개요. 2. 추진 계획.", "숫자 목록 표지가 문장 안에"),
    ("가. 사업 개요", "한글 목록 표지"),
    ("  나. 추진 일정을 정한다. 다음 내용", "앞 공백 + 한글 목록 표지"),
    ("(가. 예시) 내용이다.", "여는 괄호 + 목록 표지"),
    ("A. 개요. B. 결론.", "영문 목록 표지는 줄 처음만"),
    ("IV. 결론을 맺는다. 끝.", "영문 로마 숫자 표지"),
    ("Ⅱ. 추진 계획. 세부 내용", "로마 숫자 글자 표지"),
    ("ⅲ. 소항목", "작은 로마 숫자 글자 표지"),
    ("가나. 두 글자 한글은 표지 아님", "한글 2자는 표지 아님"),
    ("예. 그렇다.", "한글 1자 문장도 표지로 본다(알려진 한계)"),
    ("Plan A. 다음 단계", "줄 처음이 아니면 영문자도 경계"),
    ("ABCDE. 다섯 글자", "영문 5자는 표지 아님"),
    ("○ 예산 집행 계획 수립", "마침표 없는 개조식은 한 문장"),
    ("- 세부 추진 사항 점검", "붙임표 개조식"),
    ("※ 참고: 별첨 자료 확인", "참고표"),
    ("첫째 줄\n둘째 줄", "줄바꿈 \\n"),
    ("첫째 줄\r\n둘째 줄", "줄바꿈 \\r\\n 하나"),
    ("첫째 줄\r둘째 줄", "줄바꿈 \\r"),
    ("첫째 둘째 셋째", "줄 구분자 U+2028·U+2029"),
    ("첫째\x0b둘째\x0c셋째\x85넷째", "수직 탭·용지 넘김·NEL"),
    ("\n\n\n", "줄바꿈만 → 문장 없음"),
    ("", "빈 문단"),
    ("   ", "공백만"),
    ("　　예산　　", "전각 공백 앞뒤 제거"),
    ("가. 개요\n나. 목적", "줄마다 목록 표지"),
    ("내용이다.\n가. 다음 항목. 다음 문장.", "줄바꿈 뒤 표지"),
    ("“사업을 추진한다.”라고 밝혔다. 다음.", "닫는 따옴표 뒤가 글자면 경계 아님"),
    ("그는 “좋다.” 고 했다.", "닫는 따옴표 뒤 공백 → 경계"),
    ("「조례」를 따른다.」 다음", "닫는 낫표"),
    ("(예시이다.) 다음 문장.", "닫는 괄호까지 한 문장"),
    ("그렇다?” 다음", "물음표 + 닫는 따옴표"),
    ("끝났다.)) 다음", "닫는 괄호 둘"),
    ("終わり。次の文。", "。 뒤 공백 없음 → 한 문장"),
    ("終わり。 次の文。", "。 + 공백"),
    ("전각 마침표．다음．", "전각 마침표 뒤 공백 없음"),
    ("전각 마침표． 다음．", "전각 마침표 + 공백"),
    ("１２３．４５ 다음", "전각 숫자 뒤 전각 마침표는 숫자 예외 아님(아라비아 숫자만)"),
    ("전각 물음표？ 전각 느낌표！ 끝", "전각 ？！"),
    ("파일 a.hwp 를 연다. 다음", "파일 이름 안 마침표"),
    ("www.mapo.go.kr 참고. 끝", "주소 안 마침표"),
    ("e.g. 예시 i.e. 설명", "영문 약어(e.g.)는 ① 영문자 표지 규칙으로 줄 처음만 보호"),
    ("총 30명. 남 10명.", "숫자 뒤 단위 글자 + 마침표 → 경계"),
    ("버전 2.0. 다음", "숫자로 끝나는 문장(알려진 한계: 경계 아님)"),
    ("제3조. 목적", "조 + 마침표 → 경계"),
    ("예산 1,000,000원. 집행 500,000원.", "쉼표 숫자"),
    ("끝.", "마침표로 끝"),
    ("끝. ", "마침표 + 끝 공백"),
    (".", "마침표 하나뿐"),
    ("?", "물음표 하나뿐"),
    (". 앞에 마침표", "맨 앞 마침표"),
    ("  앞뒤 공백 문장.   다음 문장.  ", "앞뒤 공백"),
    ("탭\t구분\t표\t칸.\t다음", "탭은 공백(경계 뒤 공백으로 인정)"),
    ("예산 집행. 다음", "줄바꿈 없는 공백 U+00A0"),
    ("조합형 예산. 다음", "조합형 자모(나누기는 NFC 안 함)"),
    ("\U0001F600 이모지. 다음", "보충 평면 글자 위치(코드포인트)"),
    ("𠀀 확장 한자. 다음 문장", "보충 평면 한자"),
    ("가.나. 붙은 표지", "한글 표지 뒤 공백 없음"),
    ("(1) 첫째. (2) 둘째.", "괄호 숫자 표지"),
    ("1) 항목이다. 2) 항목이다.", "반괄호 숫자 표지"),
    ("①항목이다. ②항목이다.", "원숫자"),
    ("Q. 질문인가? A. 답이다.", "Q./A. — 줄 처음 Q만 표지"),
    ("Mr. Kim met Dr. Lee.", "영문 약어(줄 처음 Mr.만 보호, Dr. 뒤는 나뉨: 알려진 한계)"),
    ("끝!!! 다음", "느낌표 셋"),
    ("끝?.. 다음", "섞인 묶음(마침표만이 아님) → 경계"),
    ("다음.\n\n\n그다음.", "빈 줄 여러 개"),
    ("가.\n나.", "줄마다 한글 1자 + 마침표(표지 → 경계 없음, 줄바꿈으로 나뉨)"),
    ("   \n  내용.  \n  ", "공백 줄"),
    ("A.B. 약어", "영문 약어 A.B."),
    ("3. ", "숫자 표지만"),
    ("가)\t사업 개요", "반괄호 한글 표지"),
    ("ㄱ. 자음 표지", "한글 자모는 음절 아님 → 경계"),
    ("예산(안). 다음", "닫는 괄호 뒤 마침표"),
    ("‘작은따옴표.’ 다음", "작은 닫는 따옴표"),
    ("『책 이름.』 다음", "닫는 겹낫표"),
    ("《제목.》 다음", "닫는 겹화살괄호"),
    ("〈제목.〉 다음", "닫는 홑화살괄호"),
    ("【알림.】 다음", "닫는 검은 대괄호"),
    ("（전각 괄호.） 다음", "전각 닫는 괄호"),
    ("［전각 대괄호.］ 다음", "전각 닫는 대괄호"),
    ("[대괄호.] 다음", "닫는 대괄호"),
]

STEMS = ["예산을 집행한다", "계획을 수립한다", "주민 의견을 듣는다", "시설을 관리한다", "안전 점검을 실시한다",
         "사업 효과를 분석한다", "관계 부서와 협의한다", "결과를 보고한다", "민원을 처리한다", "교육을 실시한다"]
MARKERS = ["", "", "", "1. ", "2. ", "가. ", "나. ", "○ ", "- ", "□ ", "(1) ", "Ⅰ. ", "IV. ", "※ ", "  "]
ENDS = [".", ".", ".", "?", "!", "", "...", "。", ".”", ".)", ". "]
INSERTS = ["", "", "2026. 11. 13. ", "3.5% ", "총 30명. ", "“인용.”이라 ", "a.hwp ", "１２．"]
GLUE = [" ", " ", " ", "", "\n", "\r\n", "  ", "\t", "　"]


def make_paragraph(rng):
    parts = [rng.choice(MARKERS)]
    for k in range(rng.randint(1, 5)):
        if k:
            parts.append(rng.choice(GLUE))
        parts.append(rng.choice(INSERTS) + rng.choice(STEMS) + rng.choice(ENDS))
    if rng.random() < 0.2:
        parts.append(rng.choice(["  ", "\n", " "]))
    return "".join(parts)


def cases():
    rng = random.Random(20261113)
    rows = list(HAND)
    i = 0
    while len(rows) < 200:
        i += 1
        rows.append((make_paragraph(rng), f"합성 {i}"))
    return rows


def cases_tsv():
    lines = [f"# 문단 코드포인트(16진)\t기대 문장 구간(시작:길이, 코드포인트)\t설명. 규칙 판 {SENTENCE_RULE_VERSION}. 생성: tools/sentence_ref.py"]
    for text, why in cases():
        lines.append(f"{to_hex(text)}\t{spans_text(split(text))}\t{why}")
    return "\n".join(lines) + "\n"


# ---- Rust와 직접 대조(무작위·SQLite) ----

def _rust(exe, paragraphs):
    """EXE는 줄마다 16진 문단을 받아 줄마다 구간을 돌려준다(examples/sentence_dump.rs)."""
    inp = "".join(to_hex(p) + "\n" for p in paragraphs).encode("ascii")
    out = subprocess.run([str(Path(exe).resolve())], input=inp, capture_output=True, check=True).stdout.decode("ascii")
    return out.splitlines()


def compare(exe, paragraphs, label):
    got = _rust(exe, paragraphs)
    if len(got) != len(paragraphs):
        sys.exit(f"{label}: Rust 줄 수 {len(got)} ≠ 문단 수 {len(paragraphs)}")
    diff = 0
    sents = 0
    for p, g in zip(paragraphs, got):
        want = split(p)
        sents += len(want)
        if spans_text(want) != g:
            diff += 1
            if diff <= 5:
                print(f"차이: {to_hex(p)[:120]} | py={spans_text(want)} | rs={g}")
    print(f"{label}: 문단 {len(paragraphs)}, 문장 {sents}, 차이 {diff}")
    return diff


FUZZ_ALPHABET = list("가나다예산 집행.?!。．…\"')”」』）\n\r\t　 \x0b0123456789AIVaeⅡⅲ(（") + ["\U0001F600"]


def fuzz(n):
    rng = random.Random(7)
    return ["".join(rng.choice(FUZZ_ALPHABET) for _ in range(rng.randint(0, 40))) for _ in range(n)]


def sqlite_paragraphs(db):
    import sqlite3
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    try:
        return [r[0] for r in con.execute("SELECT body FROM content_paragraph_fts") if isinstance(r[0], str)]
    finally:
        con.close()


def main():
    argv = sys.argv[1:]
    if argv[:1] == ["--fuzz"]:
        sys.exit(1 if compare(argv[2], fuzz(int(argv[1])), "무작위") else 0)
    if argv[:1] == ["--sqlite"]:
        sys.exit(1 if compare(argv[2], sqlite_paragraphs(argv[1]), "SQLite 문단") else 0)
    text = cases_tsv()
    if "--check" in argv:
        if not CASES.exists() or CASES.read_text(encoding="utf-8") != text:
            sys.exit("대조 표가 낡았다: " + CASES.name)
        return
    CASES.write_text(text, encoding="utf-8", newline="\n")
    print(CASES.name, len(cases()))


if __name__ == "__main__":
    main()
