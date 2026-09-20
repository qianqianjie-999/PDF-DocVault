"""试卷去手写 —— 基于颜色阈值 + OpenCV inpaint 修复。

原理:彩色笔迹(蓝/红/绿圆珠笔、荧光笔等)与黑白印刷内容在 HSV 色相上
可分,按颜色范围生成笔迹掩膜 → 膨胀覆盖抗锯齿边缘 → ``cv2.inpaint``
(Telea 算法)用周围像素填补。

原理限制:
- 黑色/铅笔手写与印刷同为灰色系,颜色上不可分,无法用本方法去除;
- 印刷内容若本身使用相近颜色(如蓝色标题、红色印章),会一并被去除,
  属预期行为,由用户在前端自行勾选笔迹颜色控制。

统一输出:清理后的 PNG 写入输出目录,文件名 ``<原名>_cleaned.png``。
"""

from __future__ import annotations

import cv2
import numpy as np

from ocr.engine import OcrError

# 支持的笔迹颜色 → HSV 范围列表(OpenCV H:0-179, S/V:0-255)。
# 红色跨越 0 度两侧,需要两段;下限放宽 S/V 以容忍拍照光照不均。
COLOR_RANGES: dict[str, list[tuple[tuple[int, int, int], tuple[int, int, int]]]] = {
    "blue": [((90, 40, 40), (140, 255, 255))],
    "cyan": [((80, 40, 40), (100, 255, 255))],
    "green": [((35, 40, 40), (88, 255, 255))],
    "red": [((0, 40, 40), (12, 255, 255)), ((168, 40, 40), (180, 255, 255))],
    "orange": [((12, 40, 40), (30, 255, 255))],
    "purple": [((140, 40, 40), (168, 255, 255))],
}

# 前端展示顺序
SUPPORTED_COLORS = ["blue", "red", "green", "purple", "orange", "cyan"]


def validate_colors(colors: list[str]) -> list[str]:
    """校验并规范化颜色列表(小写),空列表回退为默认 [blue, red]。"""
    if not colors:
        return ["blue", "red"]
    normalized: list[str] = []
    for c in colors:
        key = c.strip().lower()
        if key not in COLOR_RANGES:
            raise OcrError(
                "UNSUPPORTED_COLOR",
                f"不支持的笔迹颜色: {c}(可选: {', '.join(SUPPORTED_COLORS)})",
            )
        if key not in normalized:
            normalized.append(key)
    return normalized


def _build_mask(hsv: np.ndarray, colors: list[str]) -> np.ndarray:
    """按颜色范围逐段生成并合并笔迹掩膜(0/255)。"""
    mask = np.zeros(hsv.shape[:2], dtype=np.uint8)
    for color in colors:
        for lower, upper in COLOR_RANGES[color]:
            part = cv2.inRange(hsv, np.array(lower, np.uint8), np.array(upper, np.uint8))
            mask = cv2.bitwise_or(mask, part)
    return mask


def remove_handwriting_image(
    in_path: str,
    out_path: str,
    colors: list[str],
    dilate_iters: int = 2,
) -> float:
    """对单张图片去手写,返回笔迹像素占比(0-1)。

    raises OcrError: 图片无法读取/写入失败。
    """
    img = cv2.imread(in_path)
    if img is None:
        raise OcrError("IO_NOT_FOUND", f"图片无法读取: {in_path}")

    hsv = cv2.cvtColor(img, cv2.COLOR_BGR2HSV)
    mask = _build_mask(hsv, colors)

    # 膨胀掩膜,覆盖笔迹抗锯齿边缘,避免修复后留彩色描边
    if dilate_iters > 0:
        kernel = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (3, 3))
        mask = cv2.dilate(mask, kernel, iterations=dilate_iters)

    total = mask.size
    mask_ratio = float(np.count_nonzero(mask)) / total if total else 0.0

    # 掩膜为空说明没有检测到笔迹,直接复制原图,省去无谓的 inpaint
    if mask_ratio > 0.0:
        result = cv2.inpaint(img, mask, inpaintRadius=3, flags=cv2.INPAINT_TELEA)
    else:
        result = img

    if not cv2.imwrite(out_path, result):
        raise OcrError("IO_WRITE_FAILED", f"结果写入失败: {out_path}")
    return mask_ratio


def remove_handwriting_batch(
    inputs: list[str],
    out_dir: str,
    colors: list[str],
    dilate_iters: int = 2,
) -> dict:
    """批量去手写。

    返回::

        {"images": 处理张数, "outputs": [输出路径...], "mask_ratio": 平均占比}
    """
    from pathlib import Path

    colors = validate_colors(colors)
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)

    outputs: list[str] = []
    ratios: list[float] = []
    for path in inputs:
        src = Path(path)
        if not src.is_file():
            raise OcrError("IO_NOT_FOUND", f"输入文件不存在: {path}")
        dst = out / f"{src.stem}_cleaned.png"
        ratio = remove_handwriting_image(str(src), str(dst), colors, dilate_iters)
        outputs.append(str(dst))
        ratios.append(ratio)

    return {
        "images": len(outputs),
        "outputs": outputs,
        "mask_ratio": round(sum(ratios) / len(ratios), 4) if ratios else 0.0,
    }
