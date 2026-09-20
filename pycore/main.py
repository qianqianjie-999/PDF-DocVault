#!/usr/bin/env python3
"""pycore 命令行入口 —— Rust-Python 进程级隔离协议。

协议(单任务模式,详见 runtime/PROTOCOL.md):
    Rust spawn:  python main.py <cmd> [args...]
    stdout:      恰好一行 JSON 结果(成功/失败统一格式)
    stderr:      人类可读日志,仅用于排查

失败格式::

    {"ok": false, "error": {"code": "...", "message": "..."}}

注意:引擎初始化/推理期间把 stdout 改道到 stderr,防止第三方库
(paddle 系尤其常见)污染协议通道,识别完成后恢复再输出 JSON。
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import traceback
from pathlib import Path

# 保证能以 `python pycore/main.py` 或任意 cwd 运行
sys.path.insert(0, str(Path(__file__).resolve().parent))

from ocr.engine import OcrEngine, OcrError, group_lines_to_text, list_available_engines  # noqa: E402


class _StdioGuard:
    """识别期间将 stdout 临时改道 stderr,保护 JSON 协议通道。"""

    def __enter__(self):
        self._real = sys.stdout
        sys.stdout = sys.stderr
        return self

    def __exit__(self, *exc):
        sys.stdout = self._real
        return False


def cmd_selftest(args: argparse.Namespace) -> dict:
    """引擎可用性自检:探测 + 实际初始化(触发模型加载)。"""
    with _StdioGuard():
        available = list_available_engines()
        engine = OcrEngine(name=args.engine)
        # 加载即验证;不做真实推理(避免自检耗时过长)
        _ = engine.name
    return {"ok": True, "engine": engine.name, "available": available}


def cmd_extract(args: argparse.Namespace) -> dict:
    """识别一张或多张图片(PDF 由 Rust 侧 pdfium 先渲染成图片)。"""
    started = time.time()
    pages = []
    with _StdioGuard():
        engine = OcrEngine(name=args.engine, lang=args.lang)
        for idx, path in enumerate(args.input):
            img = Path(path)
            if not img.is_file():
                raise OcrError("IO_NOT_FOUND", f"输入文件不存在: {path}")
            lines = engine.recognize(str(img))
            pages.append(
                {
                    "index": idx,
                    "file": img.name,
                    "line_count": len(lines),
                    "lines": lines,
                    "text": group_lines_to_text(lines),
                }
            )
    return {
        "ok": True,
        "engine": engine.name,
        "pages": pages,
        "elapsed_ms": int((time.time() - started) * 1000),
    }


def cmd_to_docx(args: argparse.Namespace) -> dict:
    """识别图片生成可编辑 Word 文档(拍图转 Word)。"""
    import time

    from convert.docx_builder import image_to_docx

    started = time.time()
    for path in args.input:
        if not Path(path).is_file():
            raise OcrError("IO_NOT_FOUND", f"输入文件不存在: {path}")
    with _StdioGuard():
        engine = OcrEngine(name=args.engine, lang=args.lang)
        stats = image_to_docx(engine, args.input, args.output)
    return {
        "ok": True,
        "engine": engine.name,
        "output": args.output,
        **stats,
        "elapsed_ms": int((time.time() - started) * 1000),
    }


def cmd_to_xlsx(args: argparse.Namespace) -> dict:
    """识别表格图片生成 Excel(表格转 Excel)。"""
    import time

    from convert.xlsx_builder import images_to_xlsx

    started = time.time()
    for path in args.input:
        if not Path(path).is_file():
            raise OcrError("IO_NOT_FOUND", f"输入文件不存在: {path}")
    with _StdioGuard():
        engine = OcrEngine(name=args.engine, lang=args.lang)
        stats = images_to_xlsx(engine, args.input, args.output)
    return {
        "ok": True,
        "engine": engine.name,
        "output": args.output,
        **stats,
        "elapsed_ms": int((time.time() - started) * 1000),
    }


def cmd_remove_handwriting(args: argparse.Namespace) -> dict:
    """试卷去手写:颜色阈值 + inpaint 修复(详见 scan/handwriting.py)。"""
    import time

    from scan.handwriting import remove_handwriting_batch

    started = time.time()
    colors = args.colors.split(",") if args.colors else []
    with _StdioGuard():
        stats = remove_handwriting_batch(args.input, args.output_dir, colors, args.dilate)
    return {
        "ok": True,
        "output": args.output_dir,
        **stats,
        "elapsed_ms": int((time.time() - started) * 1000),
    }


def cmd_word_to_pdf(args: argparse.Namespace) -> dict:
    """批量 Word(.doc/.docx)转 PDF,详见 office/word_to_pdf.py。"""
    from office.word_to_pdf import convert_batch

    started = time.time()
    report = convert_batch(args.input, args.output_dir, args.quality)
    return {
        "ok": True,
        "output": args.output_dir,
        "report": report,
        "elapsed_ms": int((time.time() - started) * 1000),
    }


def cmd_office_selftest(args: argparse.Namespace) -> dict:
    """办公转换引擎自检:LibreOffice / Word COM 可用性。"""
    from office.word_to_pdf import detect_engines

    return {"ok": True, "report": detect_engines()}


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="pycore", description="PDF-DocVault Python AI 服务")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_self = sub.add_parser("selftest", help="OCR 引擎可用性自检")
    p_self.add_argument("--engine", default=None, help="指定引擎(rapidocr/paddleocr),默认自动选择")
    p_self.set_defaults(func=cmd_selftest)

    p_ext = sub.add_parser("extract", help="图片文字识别")
    p_ext.add_argument("--input", action="append", required=True, help="图片路径,可重复传多个")
    p_ext.add_argument("--engine", default=None, help="指定引擎,默认自动选择")
    p_ext.add_argument("--lang", default=None, help="语言代码(paddleocr 有效,如 ch/en)")
    p_ext.set_defaults(func=cmd_extract)

    p_docx = sub.add_parser("to_docx", help="拍图转 Word")
    p_docx.add_argument("--input", action="append", required=True, help="图片路径,可重复传多个")
    p_docx.add_argument("--output", required=True, help="输出 docx 路径")
    p_docx.add_argument("--engine", default=None, help="指定引擎,默认自动选择")
    p_docx.add_argument("--lang", default=None, help="语言代码(paddleocr 有效)")
    p_docx.set_defaults(func=cmd_to_docx)

    p_xlsx = sub.add_parser("to_xlsx", help="表格转 Excel")
    p_xlsx.add_argument("--input", action="append", required=True, help="表格图片路径,可重复")
    p_xlsx.add_argument("--output", required=True, help="输出 xlsx 路径")
    p_xlsx.add_argument("--engine", default=None, help="指定引擎,默认自动选择")
    p_xlsx.add_argument("--lang", default=None, help="语言代码(paddleocr 有效)")
    p_xlsx.set_defaults(func=cmd_to_xlsx)

    p_hw = sub.add_parser("remove_handwriting", help="试卷去手写(颜色阈值 + inpaint)")
    p_hw.add_argument("--input", action="append", required=True, help="图片路径,可重复传多个")
    p_hw.add_argument("--output-dir", required=True, help="清理结果输出目录")
    p_hw.add_argument("--colors", default=None, help="笔迹颜色,逗号分隔(blue/red/green/purple/orange/cyan)")
    p_hw.add_argument("--dilate", type=int, default=2, help="掩膜膨胀次数(默认 2,覆盖抗锯齿边缘)")
    p_hw.set_defaults(func=cmd_remove_handwriting)

    p_office = sub.add_parser("office_selftest", help="Word→PDF 引擎自检")
    p_office.set_defaults(func=cmd_office_selftest)

    p_w2p = sub.add_parser("word_to_pdf", help="批量 Word 转 PDF")
    p_w2p.add_argument("--input", action="append", required=True, help="Word 路径,可重复传多个")
    p_w2p.add_argument("--output-dir", required=True, help="PDF 输出目录")
    p_w2p.add_argument(
        "--quality",
        default="high",
        choices=["high", "medium", "low"],
        help="质量等级:high(默认)/medium/low",
    )
    p_w2p.set_defaults(func=cmd_word_to_pdf)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        result = args.func(args)
    except OcrError as e:
        result = {"ok": False, "error": {"code": e.code, "message": e.message}}
    except Exception as e:  # noqa: BLE001 —— 协议边界,任何异常都要转 JSON
        traceback.print_exc(file=sys.stderr)
        result = {"ok": False, "error": {"code": "PYTHON_CRASH", "message": f"{type(e).__name__}: {e}"}}
    sys.stdout.write(json.dumps(result, ensure_ascii=False))
    sys.stdout.write("\n")
    sys.stdout.flush()
    return 0 if result.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
