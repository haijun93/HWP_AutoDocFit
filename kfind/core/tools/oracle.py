"""kfind-core 질의 규칙의 Python 전수 기준 구현과 대조 표 생성기.

규칙(docs/KC1_KFIND_CORE_2026-11-12.md 2절)을 가장 단순한 방법(모든 문장을 하나씩 확인)으로 구현한다. 느려도 된다.
`cargo test`는 여기서 만든 tests/data/oracle_*.tsv와 Rust 엔진 결과가 완전히 같은지 본다.

사용: python -X utf8 kfind/core/tools/oracle.py [--check]
"""
import random
import sys
import unicodedata
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from fold_ref import MIN_WORD_CHARS, fold, parse_query, tokens  # noqa: E402

DATA = HERE.parent / "tests" / "data"
# 붙여 쓴 검색어 나누기의 줄기 꼬리(조사·어미) 목록 — Rust와 같은 파일을 읽는다(KB3).
STEM_SUFFIXES = [line.strip() for line in (HERE.parent / "src" / "stem_suffixes.txt").read_text(encoding="utf-8").splitlines()
                 if line.strip() and not line.strip().startswith("#")]
MAX_WORD_BYTES = 96
TOP = 20

WORDS = ["예산", "집행", "계획", "추진", "사업", "주민", "안전", "관리", "점검", "보고", "축제", "도로",
         "공사", "협의", "지원", "결과", "현황", "개선", "청소", "복지", "문화", "교육", "환경", "시설"]
TAILS = ["을", "를", "이", "가", "은", "는", "의", "에", "으로", "과", "", "", ""]
SPECIALS = ["예산집행", "예산집행계획", "주민센터", "프로그램", "주민센터프로그램", "센터", "Budget", "PLAN", "budget-plan",
            "예산·집행", "(계획)", "제1조", "1.5배", "2026.", "E-mail", "Straße", "예산집", "행계획"]
LEVELS = [1, 2, 3, 3, 8, 8, 8, 8]
DATES = [20240101, 20250315, 20250315, 20260930, 20261112]

QUERIES = [
    "예산", "예산을", "예산 집행", "예산 집행 계획", "사업 주민 안전 관리", "예산을 집행", "예산집행", "예산집행계획", "산집",
    "가나다라", "예산  집행", "집행 예산", "예산 및 집행", "및", "ㅇ", "", "예산집", "예산집행계", "주민센터", "주민센터프로그램",
    "프로그램", "센터 주민", "budget", "BUDGET plan", "budgetplan", "Budget-Plan", "e-mail", "email", "strasse", "STRASSE 예산",
    "제1조", "15배", "1.5배 예산", "2026", "예산집행 계획", "계획 예산집행", "예산집행 주민센터", "주민센터프로그램 예산집행계획",
    "행계획", "예산집 행계획", "도로공사", "도로공사협의", "복지문화교육환경", "시설을", "시설을관리", "관리 점검 보고 축제 도로",
    "예산", "예산집행", "예산,집행", "(계획)",
    "청소복지", "환경시설 개선", "교육을환경", "결과현황개선청소",
]
MODES = ["off", "dict", "bigram"]


def make_sentence(rng):
    n = rng.randint(4, 9)
    parts = [rng.choice(WORDS) + rng.choice(TAILS) for _ in range(n)]
    for _ in range(rng.choice([0, 0, 0, 1, 1, 2])):
        parts.insert(rng.randrange(len(parts) + 1), rng.choice(SPECIALS))
    text = " ".join(parts)
    kind = rng.random()
    if kind < 0.08:
        text = unicodedata.normalize("NFD", text)  # 조합형(첫가끝) 자모로 풀어 쓴 문장
    elif kind < 0.14:
        text = "○ " + text.replace(" ", "  ", 1)
    elif kind < 0.18:
        text = text.replace(" ", "　", 1) + "!"
    return text + "."


def make_doc(rng, doc_id):
    sents = []
    for para in range(rng.randint(3, 7)):
        level = rng.choice(LEVELS)
        for _ in range(rng.choice([1, 1, 2])):
            sents.append((para, level, make_sentence(rng)))
    if rng.random() < 0.05:
        sents.append((99, 8, "…"))  # 비교형이 빈 문장
    return {"doc": doc_id, "date": rng.choice(DATES), "sents": sents}


def make_states():
    """(처음 상태, 고친 뒤 상태). 고치기: 7의 배수 문서 삭제, 5의 배수 문서 새 글로 교체, 새 문서 20개 추가."""
    rng = random.Random(11)
    # 문서 ID는 u32 범위를 넘는 것도 섞는다(64비트 확인).
    first = [make_doc(rng, 1000 + i if i % 9 else (1 << 40) + i) for i in range(140)]
    second = []
    for i, d in enumerate(first):
        if i % 7 == 0:
            continue
        second.append(make_doc(rng, d["doc"]) if i % 5 == 0 else d)
    second += [make_doc(rng, 5000 + i) for i in range(20)]
    return first, second


def corpus_tsv(docs):
    rows = ["# 문서 ID\t날짜\t문단\t위계 순위\t문장(코드포인트 16진, 조합형 자모를 그대로 보존하려고 글자가 아닌 숫자로 적음)"]
    for d in docs:
        for para, level, text in d["sents"]:
            rows.append(f"{d['doc']}\t{d['date']}\t{para}\t{level}\t" + " ".join(f"{ord(c):X}" for c in text))
    return "\n".join(rows) + "\n"


def split_bigram(w):
    n = len(w)
    if n < 2 * MIN_WORD_CHARS:
        return None
    out, i = [], 0
    while i < n:
        end = n if n - i == 3 else i + 2
        out.append(w[i:end])
        i = end
    return out


def is_word_or_stem(p, vocab):
    """색인 어절이거나, 어절에서 조사·어미 꼬리를 뗀 줄기(KB3)."""
    return p in vocab or any(p + s in vocab for s in STEM_SUFFIXES)


def split_dict(w, vocab):
    n = len(w)
    if n < 2 * MIN_WORD_CHARS:
        return None
    dead = [False] * (n + 1)
    cuts = []

    def go(i):
        if i == n:
            return True
        if dead[i] or n - i < MIN_WORD_CHARS:
            return False
        end = n - MIN_WORD_CHARS if i == 0 else n
        while end >= i + MIN_WORD_CHARS:
            if (end == n or n - end >= MIN_WORD_CHARS) and is_word_or_stem(w[i:end], vocab):
                cuts.append(end)
                if go(end):
                    return True
                cuts.pop()
            end -= 1
        dead[i] = True
        return False

    if not go(0):
        return None
    out, start = [], 0
    for end in cuts:
        out.append(w[start:end])
        start = end
    return out


def split_deep(w, vocab):
    """가장 길게 나눈 뒤, 조각이 또 색인 낱말들로 나뉘면 더 나눈다(더 나뉘지 않는 낱말까지)."""
    first = split_dict(w, vocab)
    if first is None:
        return None
    out = []
    for p in first:
        out.extend(split_deep(p, vocab) or [p])
    return out


def fnv(values):
    h = 0xcbf29ce484222325
    for v in values:
        for b in v.to_bytes(8, "little"):
            h ^= b
            h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


class Oracle:
    def __init__(self, docs):
        self.sents = []
        self.vocab = set()
        for d in docs:
            for ord_, (para, level, text) in enumerate(d["sents"]):
                toks = tokens(text)
                self.sents.append((d["doc"], d["date"], ord_, para, level, "".join(toks)))
                self.vocab.update(t for t in toks if len(t) >= MIN_WORD_CHARS and len(t.encode("utf-8")) <= MAX_WORD_BYTES)

    def resolve(self, q, mode):
        words, dropped = parse_query(q)
        if mode == "off":
            pieces = [None] * len(words)
        elif mode == "bigram":
            pieces = [split_bigram(w) for w in words]
        else:
            pieces = [split_deep(w, self.vocab) for w in words]
        return list(zip(words, pieces)), dropped

    def search(self, q, mode, admits=None):
        """`admits`(문서 ID → 범위 안인지)가 있으면 범위 밖 문서의 문장은 일치에서 뺀다(KFC1). 나누기는 범위와 무관하게
        색인 전체 낱말로 한다."""
        words, dropped = self.resolve(q, mode)
        matched = []
        if words:
            for doc, date, ord_, para, level, text in self.sents:
                if admits is not None and not admits(doc):
                    continue
                strict = [w in text for w, _ in words]
                broad = [s or (p is not None and all(x in text for x in p)) for s, (_, p) in zip(strict, words)]
                if all(broad):
                    matched.append((doc, ord_, para, level, 1 if all(strict) else 0, date))
        agg = {}
        for doc, ord_, para, level, adj, date in matched:
            a = agg.setdefault(doc, [0, 255, 0, date])
            a[0] = max(a[0], adj)
            a[1] = min(a[1], level)
            a[2] += 1
        # 순위: 붙어 있는 일치 > 위계(작은 값) > 일치 문장 수 > 날짜 최신 > 문서 ID
        hits = sorted(((doc, a[0], a[1], a[2], a[3]) for doc, a in agg.items()), key=lambda h: (-h[1], h[2], -h[3], -h[4], h[0]))
        matched.sort()
        return {
            "words": " ".join(w + ("=" + "+".join(p) if p else "") for w, p in words) or "-",
            "dropped": " ".join(dropped) or "-",
            "sentences": len(matched),
            "docs": len(hits),
            "adjacent_docs": sum(h[1] for h in hits),
            "sid_hash": fnv(v for m in matched for v in m[:5]),
            "rank_hash": fnv(v for h in hits for v in h[:4]),
            "top": ";".join(f"{h[0]}:{h[1]}:{h[2]}:{h[3]}" for h in hits[:TOP]) or "-",
        }


COLUMNS = ["words", "dropped", "sentences", "docs", "adjacent_docs", "sid_hash", "rank_hash", "top"]


def expect_tsv(docs):
    oracle = Oracle(docs)
    rows = ["# 방법\t검색어(코드포인트 16진, '-'는 빈 글)\t" + "\t".join(COLUMNS)]
    for q in QUERIES:
        for mode in MODES:
            r = oracle.search(q, mode)
            rows.append(f"{mode}\t" + (" ".join(f"{ord(c):X}" for c in q) or "-") + "\t" + "\t".join(str(r[c]) for c in COLUMNS))
    return "\n".join(rows) + "\n"


# 문서 범위 거르기(KFC1) 대조: 허용·제외 집합은 규칙 이름으로 적고 Rust 시험이 같은 규칙으로 만든다.
# 규칙은 두 상태 문서 ID의 합집합과 없는 ID 하나(NO_SUCH_DOC)에 적용한다 — 지운 문서의 ID가 허용 집합에 있어도
# 결과에 다시 나오면 안 되고, 없는 ID는 아무 영향이 없어야 한다.
NO_SUCH_DOC = 999_999_999
SCOPE_RULES = {
    "-": None,
    "mod3": lambda d: d % 3 == 0,
    "mod4": lambda d: d % 4 == 1,
    "big": lambda d: d >= 1 << 40,
    "empty": lambda d: False,
    "nosuch": lambda d: d == NO_SUCH_DOC,
}
# (허용 규칙, 제외 규칙)
SCOPES = [("mod3", "-"), ("-", "mod4"), ("mod3", "mod4"), ("big", "-"), ("-", "big"), ("empty", "-"), ("nosuch", "-"), ("-", "nosuch")]
SCOPE_QUERIES = ["예산", "예산 집행", "예산을 집행", "예산집행", "예산집행계획", "주민센터프로그램", "budget", "BUDGET plan",
                 "1.5배 예산", "도로공사협의", "결과현황개선청소", "및", "", "가나다라"]


def scope_set(rule, universe):
    """규칙 이름 → 문서 ID 집합(None이면 거르지 않음)."""
    f = SCOPE_RULES[rule]
    return None if f is None else {d for d in universe | {NO_SUCH_DOC} if f(d)}


def expect_scope_tsv(docs, universe):
    oracle = Oracle(docs)
    rows = ["# 방법\t검색어(코드포인트 16진)\t허용 규칙\t제외 규칙\t구문(16진, '|' 구분)\t제외어(16진, '|' 구분)\t" + "\t".join(COLUMNS)]
    for allow_rule, deny_rule in SCOPES:
        allow, deny = scope_set(allow_rule, universe), scope_set(deny_rule, universe)

        def admits(d, allow=allow, deny=deny):
            return (allow is None or d in allow) and (deny is None or d not in deny)

        for q in SCOPE_QUERIES:
            for mode in MODES:
                r = oracle.search(q, mode, admits)
                rows.append(f"{mode}\t{hex_text(q)}\t{allow_rule}\t{deny_rule}\t-\t-\t" + "\t".join(str(r[c]) for c in COLUMNS))
    return "\n".join(rows) + "\n"


def hex_text(s):
    return " ".join(f"{ord(c):X}" for c in s) or "-"


def hex_list(items):
    return "|".join(hex_text(x) for x in items) or "-"


def outputs():
    first, second = make_states()
    universe = {d["doc"] for d in first} | {d["doc"] for d in second}
    return {
        DATA / "oracle_corpus_1.tsv": corpus_tsv(first),
        DATA / "oracle_corpus_2.tsv": corpus_tsv(second),
        DATA / "oracle_expect_1.tsv": expect_tsv(first),
        DATA / "oracle_expect_2.tsv": expect_tsv(second),
        DATA / "oracle_scope_1.tsv": expect_scope_tsv(first, universe),
        DATA / "oracle_scope_2.tsv": expect_scope_tsv(second, universe),
    }


def main():
    out = outputs()
    if "--check" in sys.argv:
        stale = [p.name for p, text in out.items() if not p.exists() or p.read_text(encoding="utf-8") != text]
        if stale:
            sys.exit("대조 표가 낡았다: " + ", ".join(stale))
        return
    for p, text in out.items():
        p.write_text(text, encoding="utf-8", newline="\n")
        print(p.name, len(text))


if __name__ == "__main__":
    main()
