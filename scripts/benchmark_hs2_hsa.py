"""300문서 × 40문단, 10개 질의의 검색 지연(ms). 원본 문서/COM 불필요."""
import json
import statistics
import tempfile
import time
from pathlib import Path

from docfit_core.timemachine.content_search import ContentSearchIndex


def main():
    with tempfile.TemporaryDirectory(dir="E:/docfit-tmp") as tmp:
        index = ContentSearchIndex(Path(tmp) / "index.sqlite")
        try:
            paragraphs, metas, bodies = [], [], []
            for doc in range(300):
                path = str(Path(tmp) / f"doc{doc:03}.md")
                texts = ["# 시설 사업", f"## 관광안내판 구역{doc % 10:03}"]
                texts += [f"해체공사 공정{p % 10:03} 시행 계획과 안전관리 {p}" for p in range(38)]
                paragraphs.extend((path, Path(path).name, p,
                                   "document_title" if p == 0 else "chapter" if p == 1 else "body",
                                   0 if p == 0 else 1 if p == 1 else 8, text)
                                  for p, text in enumerate(texts))
                metas.append((path, Path(path).name, "md", 1, 1., 1., 1))
                bodies.append((path, Path(path).name, "\n".join(texts)))
            index.db.executemany("INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body) VALUES(?,?,?,?,?,?)", paragraphs)
            index.db.executemany("INSERT INTO content_meta(path,name,extension,size,mtime,ctime,hierarchy_schema_version) VALUES(?,?,?,?,?,?,?)", metas)
            index.db.executemany("INSERT INTO content_fts(path,name,body) VALUES(?,?,?)", bodies)
            index.db.commit()
            queries = [f"관광안내판 해체공사 구역{i:03}" for i in range(5)]
            queries += [f"해체공사 공정{i:03}" for i in range(5)]
            for query in queries:
                index.search(query)
            samples = []
            for query in queries:
                timings = []
                for _ in range(5):
                    start = time.perf_counter()
                    rows = index.search(query)
                    timings.append((time.perf_counter() - start) * 1000)
                samples.append(statistics.median(timings))
            print(json.dumps({"documents": 300, "paragraphs": len(paragraphs),
                              "queries": queries, "query_median_ms": samples,
                              "median_ms": statistics.median(samples)}, ensure_ascii=False))
        finally:
            index.close()


if __name__ == "__main__":
    main()
