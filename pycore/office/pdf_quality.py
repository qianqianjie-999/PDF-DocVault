"""PDF 图片质量后处理(用于 WPS CLI 等不透出压缩参数的引擎)。

按质量等级对 PDF 内嵌图片降采样并 JPEG 重压缩:
    - high:不做处理(原样保留)
    - medium:有效分辨率 ≤150 DPI · JPEG 80
    - low:有效分辨率 ≤96 DPI · JPEG 60

"有效分辨率"按内容流中图片的绘制尺寸(CTM)计算,与 LibreOffice 的
MaxImageResolution 语义一致;个别图片解析失败时保持原样,不影响整体。
"""

from __future__ import annotations

import io

import pikepdf
from PIL import Image

# 等级 → (目标 DPI 上限, JPEG 质量)
_DPI_TARGET: dict[str, tuple[int, int]] = {
    "medium": (150, 80),
    "low": (96, 60),
}


def _displayed_sizes(page: pikepdf.Page) -> dict[tuple[int, int], tuple[float, float]]:
    """解析页面内容流,返回各图片对象绘制尺寸(pt)。

    键为图片 XObject 的 objgen(对象号, 生成号),值为 (宽 pt, 高 pt)。
    跟踪 `cm` 矩阵后紧跟 `Do` 的常见模式,足够估算。
    """
    sizes: dict[tuple[int, int], tuple[float, float]] = {}
    ctm = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)  # 单位矩阵
    try:
        ops = pikepdf.parse_content_stream(page)
    except Exception:
        return sizes
    for operands, operator in ops:
        try:
            op = str(operator)
            if op == "cm" and len(operands) == 6:
                ctm = tuple(float(v) for v in operands)  # type: ignore[assignment]
            elif op == "Do" and operands:
                xobjs = page.get("/Resources", {}).get("/XObject", {})
                xobj = xobjs.get(operands[0])
                if xobj is not None and str(xobj.get("/Subtype", "")) == "/Image":
                    # 绘制尺寸取 CTM 缩放分量(忽略旋转/斜切的少见场景)
                    sizes[xobj.objgen] = (abs(float(ctm[0])), abs(float(ctm[3])))
        except Exception:
            continue
    return sizes


def apply_quality(pdf_path, quality: str) -> None:
    """就地按等级处理 `pdf_path` 的内嵌图片。high 或未知等级为无操作。"""
    target = _DPI_TARGET.get(quality)
    if target is None:
        return
    max_dpi, jpeg_q = target

    with pikepdf.open(pdf_path, allow_overwriting_input=True) as pdf:
        changed = False
        for page in pdf.pages:
            sizes = _displayed_sizes(page)
            if not sizes:
                continue
            xobjs = page.get("/Resources", {}).get("/XObject", {})
            for _, xobj in xobjs.items():
                try:
                    if str(xobj.get("/Subtype", "")) != "/Image":
                        continue
                    w_pt, h_pt = sizes.get(xobj.objgen, (0.0, 0.0))
                    px_w = int(xobj.get("/Width", 0))
                    px_h = int(xobj.get("/Height", 0))
                    if w_pt <= 0 or h_pt <= 0 or px_w <= 0 or px_h <= 0:
                        continue
                    # 当前有效 DPI = 像素 / 绘制尺寸(pt) × 72;未超标则跳过
                    eff_dpi = max(px_w / w_pt, px_h / h_pt) * 72
                    if eff_dpi <= max_dpi:
                        continue
                    scale = max_dpi / eff_dpi
                    new_w = max(1, int(px_w * scale))
                    new_h = max(1, int(px_h * scale))

                    pil_img = pikepdf.PdfImage(xobj).as_pil_image()
                    pil_img = pil_img.convert("RGB").resize((new_w, new_h), Image.LANCZOS)
                    buf = io.BytesIO()
                    pil_img.save(buf, format="JPEG", quality=jpeg_q)

                    xobj.write(buf.getvalue(), filter=pikepdf.Name("/DCTDecode"))
                    xobj["/Width"] = new_w
                    xobj["/Height"] = new_h
                    xobj["/ColorSpace"] = pikepdf.Name("/DeviceRGB")
                    xobj["/BitsPerComponent"] = 8
                    if "/DecodeParms" in xobj:
                        del xobj["/DecodeParms"]
                    changed = True
                except Exception:
                    continue  # 单图失败保持原样

        if changed:
            pdf.save(pdf_path)
