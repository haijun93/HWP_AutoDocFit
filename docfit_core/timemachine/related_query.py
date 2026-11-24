"""옵션으로 켜는 세미콜론 연관 질의의 순수 파서.

묶음 안은 한 문단의 부분 문자열 AND, 묶음 사이는 문서 교집합이다.
따옴표 안 공백과 세미콜론은 그대로 두며 &, ~N, 필터는 일반 문자다.
"""
from dataclasses import dataclass
import unicodedata


@dataclass(frozen=True)
class QueryGroup:
    query: str
    terms: tuple[str, ...]


def parse_related_query(query: str) -> tuple[QueryGroup, ...]:
    if not isinstance(query, str) or len(query) > 200:
        raise ValueError("검색어는 200자 이내로 입력해 주세요.")
    query = unicodedata.normalize("NFC", query)
    groups, terms, token, label = [], [], [], []
    quote = None
    escaped = False

    def flush_token():
        word = "".join(token)
        if word and word not in terms:
            terms.append(word)
        token.clear()

    def flush_group():
        flush_token()
        if terms:
            groups.append(QueryGroup("".join(label).strip(), tuple(terms)))
        terms.clear()
        label.clear()

    for char in query:
        if escaped:
            if char not in ("\\", quote):
                token.append("\\")
            token.append(char)
            label.append(char)
            escaped = False
        elif quote and char == "\\":
            label.append(char)
            escaped = True
        elif quote and char == quote:
            label.append(char)
            quote = None
        elif not quote and char in ('"', '“'):
            label.append(char)
            quote = '”' if char == '“' else '"'
        elif not quote and char == ";":
            flush_group()
        else:
            label.append(char)
            if not quote and char.isspace():
                flush_token()
            else:
                token.append(char)
    if quote or escaped:
        raise ValueError("검색어의 따옴표를 닫아 주세요.")
    flush_group()
    return tuple(groups)


def normalized(text):
    return unicodedata.normalize("NFC", text or "").lower()


def matches(text, group):
    folded = normalized(text)
    return all(normalized(term) in folded for term in group.terms)
