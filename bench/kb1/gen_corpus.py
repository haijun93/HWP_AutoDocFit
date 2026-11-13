"""KB1 말뭉치 생성: IS-S1·KC1과 같은 생성기(kfind/engine-bench/bench.py generate, seed 7, 문서당 문단 40)를 그대로 쓴다.

사용: python -X utf8 bench/kb1/gen_corpus.py <출력 폴더> [문서 수=3000]
출력: <출력 폴더>/corpus/corpus.tsv (`문서번호\t위계순위\t문장`)
"""
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "kfind" / "engine-bench"))
from bench import generate  # noqa: E402

if __name__ == "__main__":
    out = Path(sys.argv[1])
    docs = int(sys.argv[2]) if len(sys.argv) > 2 else 3000
    doc_of, _, folds, paragraphs = generate(out / "corpus", docs, 40)
    print(f"docs={docs} paragraphs={paragraphs} sentences={len(folds)}")
