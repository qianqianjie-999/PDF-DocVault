"""拍图转 Word —— OCR 结果生成可编辑 docx。

策略:每张图片对应 Word 一节,OCR 行按阅读顺序组织为段落
(复用 ocr.engine 的行聚类),保留标题行(首行且字号偏大时加粗)。
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from ocr.engine import OcrEngine, group_lines_to_text  # noqa: E402


def image_to_docx(engine: OcrEngine, images: list[str], output: str) -> dict:
    """识别多张图片并生成 docx,返回统计。"""
    from docx import Document
    from docx.shared import Pt

    doc = Document()
    doc.core_properties.author = "PDF-DocVault"

    total_lines = 0
    for idx, img in enumerate(images):
        lines = engine.recognize(img)
        total_lines += len(lines)
        if idx > 0:
            doc.add_page_break()
        # 节标题 = 源文件名
        heading = doc.add_heading(Path(img).stem, level=2)
        for run in heading.runs:
            run.font.size = Pt(14)
        if not lines:
            doc.add_paragraph("(本页未识别到文字)")
            continue
        # 行聚类成段落:group_lines_to_text 产出按 y 聚类的行文本
        text = group_lines_to_text(lines)
        for para_text in text.split("\n"):
            para_text = para_text.strip()
            if para_text:
                doc.add_paragraph(para_text)

    doc.save(output)
    return {"images": len(images), "lines": total_lines}
