"""SRCH2/3 search_bench의 순수 생성기 재사용(기준선에 없어 이 파일로 옮김)."""
import random
from pathlib import Path

WORDS = ("행정", "정책", "문서", "검색", "데이터", "민원", "안전", "환경", "지원", "절차",
         "성과", "운영", "계획", "점검", "지역", "서비스", "정보", "협력", "기준", "개선")
MARKERS = ("1.", "(1)", "가.", "□", "○", "-", "※")
PHRASES = ("행정 절차 지원", "지역 정책 운영", "안전 점검 계획", "환경 데이터 분석")


def make_paragraphs(rng: random.Random, doc_number: int) -> str:
    count = rng.randint(20, 50)
    lines = [f"제{doc_number % 12 + 1}장 {rng.choice(WORDS)} 추진 계획"]
    for paragraph in range(count - 1):
        marker = MARKERS[paragraph % len(MARKERS)]
        words = rng.sample(WORDS, k=6)
        phrase = PHRASES[(doc_number + paragraph) % len(PHRASES)]
        lines.append(f"{marker} {phrase}와 {' '.join(words)}를 바탕으로 추진 현황을 확인하고 개선 방안을 마련한다.")
    return "\n".join(lines) + "\n"


def make_corpus(folder: Path, count: int, seed: int = 20261124) -> list[Path]:
    rng = random.Random(seed)
    paths = []
    for number in range(count):
        path = folder / f"synthetic_{number:06d}.txt"
        path.write_text(make_paragraphs(rng, number), encoding="utf-8")
        paths.append(path)
    return paths
