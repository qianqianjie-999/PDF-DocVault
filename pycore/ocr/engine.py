"""OCR 引擎统一适配层。

引擎优先级(按部署轻量程度):
1. ``rapidocr`` (v2)     —— PaddleOCR 官方 PP-OCR 模型的 ONNX 移植,约 60MB,CPU 快,默认首选
2. ``rapidocr_onnxruntime`` (v1) —— 旧包名,接口不同,老环境兼容
3. ``paddleocr``          —— 规划书原始选型,依赖重(500MB+),可选安装

统一输出行结构::

    {"text": str, "score": float, "box": [[x, y], ...4 点]}
"""

from __future__ import annotations

import sys
from typing import Any

# 统一行类型别名
Line = dict[str, Any]


class OcrError(Exception):
    """带错误码的 OCR 异常,Rust 侧按 code 分类处理。"""

    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code
        self.message = message


def list_available_engines() -> list[str]:
    """探测当前 Python 环境可用的 OCR 引擎(按优先级排序)。"""
    engines: list[str] = []
    try:
        import rapidocr  # noqa: F401

        engines.append("rapidocr")
    except Exception:
        pass
    try:
        import rapidocr_onnxruntime  # noqa: F401

        engines.append("rapidocr_onnxruntime")
    except Exception:
        pass
    try:
        import paddleocr  # noqa: F401

        engines.append("paddleocr")
    except Exception:
        pass
    return engines


class OcrEngine:
    """统一识别接口:``recognize(image_path) -> list[Line]``。"""

    def __init__(self, name: str | None = None, lang: str | None = None):
        self.name, self._impl = self._build(name, lang)

    # ----- 构造 -----

    def _build(self, name: str | None, lang: str | None):
        available = list_available_engines()
        if not available:
            raise OcrError(
                "OCR_NO_ENGINE",
                "未找到可用 OCR 引擎,请先安装: pip install rapidocr",
            )
        if name and name not in available:
            raise OcrError(
                "OCR_ENGINE_UNAVAILABLE",
                f"引擎 {name!r} 不可用,当前可用: {available}",
            )
        chosen = name or available[0]
        if chosen == "rapidocr":
            return chosen, self._build_rapid_v2(lang)
        if chosen == "rapidocr_onnxruntime":
            return chosen, self._build_rapid_v1(lang)
        return chosen, self._build_paddle(lang)

    @staticmethod
    def _build_rapid_v2(lang: str | None):
        from rapidocr import RapidOCR  # 延迟导入,缺依赖时给出清晰错误

        kwargs: dict[str, Any] = {}
        if lang:
            kwargs["params"] = {"Rec.ocr_version": "PP-OCRv4"}
            print(f"[pycore] rapidocr lang={lang} (v2 仅支持模型级配置,忽略)", file=sys.stderr)
        return RapidOCR(**kwargs)

    @staticmethod
    def _build_rapid_v1(lang: str | None):
        from rapidocr_onnxruntime import RapidOCR

        return RapidOCR()

    @staticmethod
    def _build_paddle(lang: str | None):
        from paddleocr import PaddleOCR

        return PaddleOCR(use_angle_cls=True, lang=lang or "ch", show_log=False)

    # ----- 识别 -----

    def recognize(self, image_path: str) -> list[Line]:
        """识别单张图片,返回检测行(已按阅读顺序排序)。"""
        if self.name == "rapidocr":
            return self._recognize_rapid_v2(image_path)
        if self.name == "rapidocr_onnxruntime":
            return self._recognize_rapid_v1(image_path)
        return self._recognize_paddle(image_path)

    def _recognize_rapid_v2(self, image_path: str) -> list[Line]:
        result = self._impl(image_path)
        lines: list[Line] = []
        # 注意:字段是 numpy 数组,不能用 `or []` 简写(真值歧义)
        txts = getattr(result, "txts", None)
        boxes = getattr(result, "boxes", None)
        scores = getattr(result, "scores", None)
        txts = [] if txts is None else list(txts)
        boxes = [] if boxes is None else list(boxes)
        scores = [] if scores is None else list(scores)
        for i, text in enumerate(txts):
            box = boxes[i].tolist() if i < len(boxes) else []
            score = float(scores[i]) if i < len(scores) else 0.0
            lines.append({"text": str(text), "score": score, "box": box})
        return lines

    def _recognize_rapid_v1(self, image_path: str) -> list[Line]:
        result, _elapse = self._impl(image_path)
        lines: list[Line] = []
        for item in result if result is not None else []:
            box, text, score = item[0], item[1], item[2]
            lines.append({"text": str(text), "score": float(score), "box": box})
        return lines

    def _recognize_paddle(self, image_path: str) -> list[Line]:
        raw = self._impl.ocr(image_path, cls=True)
        lines: list[Line] = []
        for page in raw if raw is not None else []:
            for item in page if page is not None else []:
                box, (text, score) = item[0], item[1]
                lines.append({"text": str(text), "score": float(score), "box": box})
        return lines


def group_lines_to_text(lines: list[Line], y_tol_ratio: float = 0.6) -> str:
    """把检测行按阅读顺序组织为纯文本。

    依据行框中心的 y 坐标聚类(容差 = 平均行高 * y_tol_ratio),
    同一行内按 x 排序用空格连接,不同行按 y 排序换行。
    """
    if not lines:
        return ""
    enriched = []
    for ln in lines:
        box = ln.get("box") or []
        xs = [p[0] for p in box if len(p) >= 2]
        ys = [p[1] for p in box if len(p) >= 2]
        cy = sum(ys) / len(ys) if ys else 0.0
        height = (max(ys) - min(ys)) if ys else 0.0
        enriched.append((cy, min(xs) if xs else 0.0, height, ln["text"]))
    avg_h = sorted(h for _, _, h, _ in enriched)[len(enriched) // 2] or 1.0
    tol = avg_h * y_tol_ratio
    enriched.sort(key=lambda t: (t[0], t[1]))
    rows: list[list[tuple[float, str]]] = [[enriched[0]]]
    for item in enriched[1:]:
        if abs(item[0] - rows[-1][-1][0]) <= tol:
            rows[-1].append(item)
        else:
            rows.append([item])
    out_lines = []
    for row in rows:
        row.sort(key=lambda t: t[1])
        out_lines.append(" ".join(t for _, _, _, t in row))
    return "\n".join(out_lines)
