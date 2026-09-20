"""表格转 Excel —— OCR 结果还原表格结构并生成 xlsx。

还原算法(MVP 线性表):
1. 行聚类:按单元格框中心 y 坐标聚类,容差 = 中位行高 * 0.5(表格行高较均匀)
2. 列边界:对所有单元格的 x 起点(左边界)聚类,容差 = 中位单元格宽 * 0.5;
   每个单元格归属到与其左边界最近的列
3. 行 × 列 → 二维数组,空缺补空字符串,经 openpyxl 写出

限制:合并单元格/跨行跨列的复杂表格按拆散处理,后续可引入版面分析模型增强。
"""

from __future__ import annotations

from pathlib import Path
from statistics import median

from ocr.engine import Line, OcrEngine


def _cell_geometry(line: Line) -> tuple[float, float, float]:
    """返回单元格 (中心y, 左x, 宽度)。box 为 4 点框。"""
    box = line.get("box") or []
    xs = [float(p[0]) for p in box if len(p) >= 2]
    ys = [float(p[1]) for p in box if len(p) >= 2]
    if not xs or not ys:
        return 0.0, 0.0, 0.0
    return (max(ys) + min(ys)) / 2, min(xs), max(xs) - min(xs)


def _cluster(values: list[float], tol: float) -> list[float]:
    """一维数值聚类,返回每簇代表值(均值),输入需有序。"""
    if not values:
        return []
    clusters: list[list[float]] = [[values[0]]]
    for v in values[1:]:
        if v - clusters[-1][-1] <= tol:
            clusters[-1].append(v)
        else:
            clusters.append([v])
    return [sum(c) / len(c) for c in clusters]


def image_to_grid(engine: OcrEngine, image_path: str) -> list[list[str]]:
    """识别单张图片并还原为二维表格。"""
    lines = engine.recognize(image_path)
    if not lines:
        return []
    cells = [(_cell_geometry(ln), ln["text"]) for ln in lines]
    cells = [c for c in cells if c[0][2] > 0]
    if not cells:
        return []

    # 1. 行聚类(y 中心,容差 = 中位行高 * 0.5)
    heights = sorted(g[2] for g, _ in cells)
    row_tol = (median(heights) if heights else 1.0) * 0.5
    cells.sort(key=lambda c: (c[0][0], c[0][1]))
    rows: list[list[tuple[tuple[float, float, float], str]]] = [[cells[0]]]
    for cell in cells[1:]:
        if abs(cell[0][0] - rows[-1][-1][0][0]) <= row_tol:
            rows[-1].append(cell)
        else:
            rows.append([cell])

    # 2. 列边界(单元格左 x 聚类,容差 = 中位单元格宽 * 0.5)
    widths = sorted(g[2] for g, _ in cells)
    col_tol = (median(widths) if widths else 1.0) * 0.5
    lefts = sorted(g[1] for g, _ in cells)
    col_refs = _cluster(lefts, col_tol)

    # 3. 分配单元格到列
    def col_of(left_x: float) -> int:
        return min(range(len(col_refs)), key=lambda i: abs(col_refs[i] - left_x))

    n_cols = len(col_refs)
    grid: list[list[str]] = []
    for row in rows:
        grid_row = [""] * n_cols
        for (geom, text) in row:
            idx = col_of(geom[1])
            # 同格已有内容时追加(竖排拆行等情况)
            grid_row[idx] = f"{grid_row[idx]}{text}" if grid_row[idx] else text
        grid.append(grid_row)
    return grid


def images_to_xlsx(engine: OcrEngine, images: list[str], output: str) -> dict:
    """识别多张表格图片生成 xlsx,每张图片一个工作表。"""
    from openpyxl import Workbook

    wb = Workbook()
    wb.remove(wb.active)

    used_titles: set[str] = set()
    total_rows = 0
    sheets: list[dict] = []
    for idx, img in enumerate(images):
        grid = image_to_grid(engine, img)
        title = _sheet_title(Path(img).stem, idx, used_titles)
        ws = wb.create_sheet(title=title)
        for row in grid:
            ws.append(row)
        # 简单列宽自适应
        if grid:
            for col_idx, col_cells in enumerate(zip(*grid), start=1):
                width = max((len(str(c)) for c in col_cells), default=8)
                ws.column_dimensions[ws.cell(row=1, column=col_idx).column_letter].width = min(
                    max(width + 2, 8), 60
                )
        total_rows += len(grid)
        sheets.append({"file": Path(img).name, "sheet": title, "rows": len(grid), "cols": len(grid[0]) if grid else 0})

    wb.save(output)
    return {"images": len(images), "rows": total_rows, "sheets": sheets}


# Excel 工作表名禁止字符与长度上限(31)
_SHEET_INVALID_CHARS = set(":\\/?*[]")
_SHEET_TITLE_MAX = 31


def _sheet_title(stem: str, idx: int, used: set[str]) -> str:
    """由文件名生成合法且不重复的工作表名。

    非法字符(:\\/?*[])替换为下划线;超长截断;重名追加 ~2/~3 序号。
    """
    title = "".join("_" if ch in _SHEET_INVALID_CHARS else ch for ch in stem).strip()
    title = title[:_SHEET_TITLE_MAX] or f"sheet{idx + 1}"
    base, n = title, 2
    while title in used:
        suffix = f"~{n}"
        title = base[: _SHEET_TITLE_MAX - len(suffix)] + suffix
        n += 1
    used.add(title)
    return title
