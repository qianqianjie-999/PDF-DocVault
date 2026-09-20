"""批量 Word → PDF 转换。

智能引擎策略:
    - Windows:优先 Word COM,其次 WPS COM(两者对象模型与 Word 一致),
      均不可用时回退 LibreOffice
    - Linux/macOS:LibreOffice headless,缺失时回退 WPS 命令行
      (UOS/麒麟常预装 WPS;其 CLI 可靠性一般,失败如实写入报告)

进度协议:每开始一个文件,向 stderr 打印一行
    [PROGRESS] <current> <total> <文件名>
由 Rust 侧逐行读取并转发为 task-progress 事件。
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# ===== 引擎探测 =====


def find_libreoffice() -> str | None:
    """定位 LibreOffice 可执行文件:PATH → 各平台常见安装路径。"""
    found = shutil.which("soffice") or shutil.which("libreoffice")
    if found:
        return found
    if sys.platform.startswith("win"):
        candidates = [
            r"C:\Program Files\LibreOffice\program\soffice.exe",
            r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        ]
        for p in candidates:
            if os.path.isfile(p):
                return p
    elif sys.platform == "darwin":
        p = "/Applications/LibreOffice.app/Contents/MacOS/soffice"
        if os.path.isfile(p):
            return p
    return None


def word_com_available() -> bool:
    """Windows 上是否具备 pywin32(Word 本体在调用时再验证,避免启动开销)。"""
    if not sys.platform.startswith("win"):
        return False
    try:
        import win32com.client  # noqa: F401

        return True
    except ImportError:
        return False


def wps_com_available() -> bool:
    """Windows 上是否注册了 WPS COM(KWPS/WPS.Application),仅查注册表。"""
    if not sys.platform.startswith("win"):
        return False
    try:
        import winreg  # type: ignore
    except ImportError:
        return False
    for progid in ("KWPS.Application", "WPS.Application"):
        try:
            with winreg.OpenKey(winreg.HKEY_CLASSES_ROOT, progid):
                return True
        except OSError:
            continue
    return False


def find_wps_cli() -> str | None:
    """Linux/macOS 定位 WPS 文字(wps)可执行文件。"""
    return shutil.which("wps")


def detect_engines() -> dict:
    """自检:返回各引擎可用性,供 UI 决定提示语。"""
    return {
        "libreoffice": find_libreoffice(),
        "word_com": word_com_available(),
        "wps_com": wps_com_available(),
        "wps_cli": find_wps_cli(),
    }


# ===== 质量等级 → 引擎参数 =====

# LibreOffice writer_pdf_Export 过滤器 JSON 选项
_LO_FILTER = {
    "high": '{"ReduceImageResolution":{"type":"boolean","value":false},"Quality":{"type":"long","value":90}}',
    "medium": '{"ReduceImageResolution":{"type":"boolean","value":true},'
    '"MaxImageResolution":{"type":"long","value":150},"Quality":{"type":"long","value":80}}',
    "low": '{"ReduceImageResolution":{"type":"boolean","value":true},'
    '"MaxImageResolution":{"type":"long","value":96},"Quality":{"type":"long","value":60}}',
}

# Word COM ExportAsFixedFormat:Quality 0=Print(高),1=Minimum(低)
_COM_QUALITY = {"high": 0, "medium": 0, "low": 1}

QUALITY_DESC = {
    "high": "原版分辨率 · JPEG 90%",
    "medium": "图片 ≤150 DPI · JPEG 80%",
    "low": "图片 ≤96 DPI · JPEG 60%",
}


def _unique_pdf_path(out_dir: Path, stem: str) -> Path:
    """规划输出路径:同名加 ~N 后缀,避免覆盖已有成果。"""
    candidate = out_dir / f"{stem}.pdf"
    if not candidate.exists():
        return candidate
    n = 1
    while True:
        candidate = out_dir / f"{stem}~{n}.pdf"
        if not candidate.exists():
            return candidate
        n += 1


def _file_timeout(path: Path) -> int:
    """按文件大小放宽单文件超时:基础 60s + 每 MB 2s,上限 600s。"""
    try:
        size_mb = path.stat().st_size // (1024 * 1024)
    except OSError:
        size_mb = 0
    return min(60 + int(size_mb) * 2, 600)


# ===== LibreOffice 转换 =====


def _convert_with_libreoffice(
    soffice: str, src: Path, final_out: Path, quality: str, user_profile: Path
) -> None:
    """单文件转换:先输出到独立临时目录,成功后 rename 到规划路径。

    LibreOffice 不支持指定输出文件名,临时目录中转也顺带解决重名冲突。
    """
    with tempfile.TemporaryDirectory(prefix="lo-out-") as tmp:
        cmd = [
            soffice,
            "-env:UserInstallation=" + user_profile.as_uri(),
            "--headless",
            "--norestore",
            "--nolockcheck",
            "--convert-to",
            f"pdf:writer_pdf_Export:{_LO_FILTER[quality]}",
            "--outdir",
            tmp,
            str(src),
        ]
        subprocess.run(
            cmd,
            timeout=_file_timeout(src),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            check=False,
        )
        produced = Path(tmp) / (src.stem + ".pdf")
        if not produced.is_file():
            raise RuntimeError("LibreOffice 未生成 PDF(文件可能损坏或受密码保护)")
        shutil.move(str(produced), str(final_out))


# ===== WPS 命令行转换(Linux/macOS 回退) =====


def _convert_with_wps_cli(wps: str, src: Path, final_out: Path, quality: str) -> None:
    """WPS 命令行转换(官方语法与 LibreOffice 相同,可靠性一般,仅作回退)。

    WPS CLI 不透出图片压缩参数,质量等级由转换后的图片后处理落实
    (`pdf_quality.apply_quality`;high 无需处理)。
    """
    from office.pdf_quality import apply_quality

    with tempfile.TemporaryDirectory(prefix="wps-out-") as tmp:
        subprocess.run(
            [wps, "--convert-to", "pdf", "--outdir", tmp, str(src)],
            timeout=_file_timeout(src),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            check=False,
        )
        produced = Path(tmp) / (src.stem + ".pdf")
        # 社区反馈 WPS CLI 偶发"无输出也无报错",短轮询兜底异步落盘
        for _ in range(10):
            if produced.is_file():
                break
            time.sleep(1)
        if not produced.is_file():
            raise RuntimeError("WPS 命令行未生成 PDF(版本可能不支持,请安装 LibreOffice)")
        if quality != "high":
            apply_quality(produced, quality)
        shutil.move(str(produced), str(final_out))


# ===== Word / WPS COM 转换(Windows) =====

# 依次尝试的 COM ProgID → 引擎名(WPS 对象模型与 Word 兼容)
_COM_PROGIDS = (
    ("Word.Application", "word_com"),
    ("KWPS.Application", "wps_com"),
    ("WPS.Application", "wps_com"),
)


def _convert_with_office_com(src: Path, final_out: Path, quality: str) -> str:
    """依次尝试 Word → WPS COM 导出 PDF,返回实际使用的引擎名。

    ProgID 无法启动(未安装)则尝试下一个;成功启动后转换失败则如实抛出
    (调用方按单文件失败记录,不换引擎重试)。
    WPS COM 的 Quality 仅 Print/Minimum 两档,medium/low 的图片降采样
    由转换后的 `pdf_quality.apply_quality` 补齐(high 无需处理)。
    """
    import pywintypes  # type: ignore
    import win32com.client  # type: ignore

    from office.pdf_quality import apply_quality

    last_err: Exception | None = None
    for progid, engine_name in _COM_PROGIDS:
        try:
            app = win32com.client.DispatchEx(progid)
        except pywintypes.com_error as e:
            last_err = e
            continue
        app.Visible = False
        app.DisplayAlerts = 0
        doc = None
        try:
            doc = app.Documents.Open(str(src.resolve()), ReadOnly=True)
            # ExportFormat=17(wdExportFormatPDF);OpenAfterExport=False
            params = dict(
                OutputFileName=str(final_out),
                ExportFormat=17,
                OpenAfterExport=False,
                OptimizeFor=0,
                Range=0,
                Item=0,
                IncludeDocProps=True,
                KeepIRM=True,
                CreateBookmarks=0,
                DocStructureTags=True,
                BitmapMissingFonts=True,
                UseISO19005_1=False,
            )
            try:
                # Quality 仅部分 Word/WPS 版本支持,不支持时降级为标准签名
                doc.ExportAsFixedFormat(Quality=_COM_QUALITY[quality], **params)
            except pywintypes.com_error:
                doc.ExportAsFixedFormat(**params)
            # WPS COM 无逐级压缩参数,medium/low 转后补降采样
            if engine_name == "wps_com" and quality != "high":
                apply_quality(final_out, quality)
            return engine_name
        finally:
            if doc is not None:
                doc.Close(SaveChanges=False)
            app.Quit()
    raise RuntimeError(f"Word/WPS COM 均不可用: {last_err}")


# ===== 批量入口 =====


def convert_batch(inputs: list[str], output_dir: str, quality: str) -> dict:
    """批量转换,返回成功/失败报告。

    结构:
        {
          "engine": "word_com" | "wps_com" | "wps_cli" | "libreoffice" | "none",
          "succeeded": [{"input": ..., "output": ...}],
          "failed":    [{"input": ..., "reason": ...}],
        }
    """
    if quality not in _LO_FILTER:
        raise ValueError(f"未知质量等级: {quality}(应为 high/medium/low)")

    out_dir = Path(output_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    total = len(inputs)
    succeeded: list[dict] = []
    failed: list[dict] = []

    # 引擎链探测:Windows Word COM→WPS COM→LibreOffice;Linux/macOS LibreOffice→WPS CLI
    win = sys.platform.startswith("win")
    soffice = find_libreoffice()
    wps_cli = None if win else find_wps_cli()
    com_ready = word_com_available() or wps_com_available()
    if win:
        has_engine = com_ready or soffice is not None
    else:
        has_engine = soffice is not None or wps_cli is not None
    if not has_engine:
        # 无任何引擎:整批快速失败,UI 据 reason 给安装指引
        return {
            "engine": "none",
            "succeeded": [],
            "failed": [
                {"input": p, "reason": "未检测到 LibreOffice、Microsoft Word 或 WPS,请先安装其一"}
                for p in inputs
            ],
        }
    # 初始引擎展示名(实际以单文件转换结果为准)
    if win:
        engine = "word_com" if com_ready else "libreoffice"
    else:
        engine = "libreoffice" if soffice else "wps_cli"

    # LibreOffice 独立用户配置目录,避免与用户正在打开的 LibreOffice 冲突
    user_profile = Path(tempfile.mkdtemp(prefix="lo-profile-"))
    try:
        for current, raw in enumerate(inputs, start=1):
            src = Path(raw)
            # 进度走 stderr(不污染 stdout 的 JSON 协议),立即刷新
            print(f"[PROGRESS] {current} {total} {src.name}", file=sys.stderr, flush=True)

            if not src.is_file():
                failed.append({"input": raw, "reason": "文件不存在"})
                continue
            if src.suffix.lower() not in (".doc", ".docx"):
                failed.append({"input": raw, "reason": "仅支持 .doc / .docx 格式"})
                continue

            final_out = _unique_pdf_path(out_dir, src.stem)
            try:
                if win:
                    converted = False
                    if com_ready:
                        try:
                            engine = _convert_with_office_com(src, final_out, quality)
                            converted = True
                        except Exception:  # COM 失败:本轮回退 LibreOffice
                            if soffice is None:
                                raise
                    if not converted:
                        engine = "libreoffice"
                        _convert_with_libreoffice(
                            soffice, src, final_out, quality, user_profile  # type: ignore[arg-type]
                        )
                else:
                    try:
                        if soffice is None:
                            raise RuntimeError("LibreOffice 不可用")
                        _convert_with_libreoffice(soffice, src, final_out, quality, user_profile)
                        engine = "libreoffice"
                    except Exception:  # LO 缺失或失败:回退 WPS CLI
                        if wps_cli is None:
                            raise
                        engine = "wps_cli"
                        _convert_with_wps_cli(wps_cli, src, final_out, quality)

                if final_out.is_file():
                    succeeded.append({"input": raw, "output": str(final_out)})
                else:
                    failed.append({"input": raw, "reason": "转换结束但未找到输出文件"})
            except subprocess.TimeoutExpired:
                failed.append({"input": raw, "reason": f"转换超时(>{_file_timeout(src)}s)"})
            except Exception as e:  # 单文件失败不拖垮整批
                failed.append({"input": raw, "reason": f"{type(e).__name__}: {e}"})
    finally:
        shutil.rmtree(user_profile, ignore_errors=True)

    return {"engine": engine, "succeeded": succeeded, "failed": failed}
