"""HS2-D2 합성 TXT 색인 조사. 저장소 루트에서 python -m scripts.hs2_d2_probe."""
from pathlib import Path
import tempfile
from unittest.mock import patch

from docfit_core.timemachine.content_search import ContentSearchIndex


CASES = {
    "consistent": "제1장 시설 관리\nⅠ. 추진 배경\n1. 추진 계획\n가. 세부 기준\n나. 추가 기준\n2. 실행 계획\nⅡ. 운영 배경\n제2장 운영 관리",
    "isolated": "제1장 시설 관리\nⅠ. 추진 배경\n1. 추진 계획\n가. 세부 기준",
    "body_list": "시설 관리에 관한 설명입니다.\n제1장 시설 관리\nⅠ. 추진 배경\n다음 사항을 확인합니다.\n1. 추진 계획\n2. 실행 계획\n가. 세부 기준\n이상입니다.",
}


def probe():
    lines = ["| 문서 | 문단 | 실제 level |", "|---|---|---|"]
    with tempfile.TemporaryDirectory(prefix="hs2-d2-", dir="E:/docfit-tmp") as tmp:
        root = Path(tmp)
        index = ContentSearchIndex(root / "probe.sqlite")
        try:
            with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
                for name, content in CASES.items():
                    path = root / (name + ".txt")
                    path.write_text(content, encoding="utf-8")
                    result = index.index_file(path)
                    assert result["indexed"], result
                    for row in index.db.execute(
                        "SELECT body,level FROM content_paragraph_fts WHERE path=? ORDER BY paragraph_index",
                        (str(path.resolve()),),
                    ):
                        lines.append(f"| {name} | {row[0]} | {row[1]} |")
        finally:
            index.close()
    return "\n".join(lines)


if __name__ == "__main__":
    print(probe())
