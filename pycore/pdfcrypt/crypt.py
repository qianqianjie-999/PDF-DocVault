"""PDF 加密/解密 —— 基于 pikepdf(静态链接 libqpdf 12.x)。

原实现直接 spawn 系统 `qpdf` 二进制,但打包配置 `externalBin` 是空的,
用户在裸机上点「加密」必然弹「请确认 qpdf 已安装」。pikepdf 的 wheel 里
已静态编入 libqpdf,三平台同一套代码、零外部依赖,故改为走本模块。

密码一律从 stdin 传入(见 main.py 的 _read_secrets),不进命令行参数,
避免 `ps` / `/proc/<pid>/cmdline` 泄露。
"""

from __future__ import annotations

import os
from pathlib import Path

import pikepdf

# pikepdf 的序列化选项:压缩对象流,体积更小;加密与压缩不冲突
_SAVE_OPTS = {"compress_streams": True, "object_stream_mode": pikepdf.ObjectStreamMode.generate}


class CryptError(Exception):
    """协议错误:code 供 Rust 侧分类,message 直接面向用户。"""

    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code
        self.message = message


def _open(path: str, password: str = "") -> pikepdf.Pdf:
    """打开 PDF,把 pikepdf 的异常翻译成带错误码的 CryptError。"""
    p = Path(path)
    if not p.is_file():
        raise CryptError("IO_NOT_FOUND", f"文件不存在: {path}")
    try:
        return pikepdf.open(path, password=password)
    except pikepdf.PasswordError:
        raise CryptError("PDF_BAD_PASSWORD", "密码错误,无法打开该 PDF") from None
    except pikepdf.PdfError as e:
        raise CryptError("PDF_CORRUPT", f"PDF 无法解析(文件损坏或格式不支持): {e}") from None


def _build_permissions(perms: dict) -> pikepdf.Permissions:
    """把前端的 4 个布尔开关映射到 PDF 权限位。

    R=6(AES-256)规范要求 accessibility 必须为允许,否则 qpdf 会强制置真,
    这里直接给 True,避免「界面上设了却看不到效果」的错觉。
    """
    allow_print = bool(perms.get("allow_print", True))
    allow_copy = bool(perms.get("allow_copy", True))
    allow_modify = bool(perms.get("allow_modify", False))
    allow_annotate = bool(perms.get("allow_annotate", False))
    return pikepdf.Permissions(
        accessibility=True,
        extract=allow_copy,
        modify_annotation=allow_annotate,
        modify_assembly=allow_modify,
        modify_form=allow_modify,
        modify_other=allow_modify,
        print_lowres=allow_print,
        print_highres=allow_print,
    )


def _perm_report(allow: pikepdf.Allow) -> dict:
    return {
        "allow_print": bool(allow.print_highres),
        "allow_copy": bool(allow.extract),
        "allow_modify": bool(allow.modify_other),
        "allow_annotate": bool(allow.modify_annotation),
    }


def _write(pdf: pikepdf.Pdf, output: str, **kwargs) -> None:
    """保存到 output。输入输出同路径时需显式允许覆盖。"""
    out = Path(output)
    out.parent.mkdir(parents=True, exist_ok=True)
    try:
        pdf.save(output, **kwargs)
    except OSError as e:
        raise CryptError("IO_WRITE", f"写入失败({output}): {e}") from None
    except (pikepdf.PdfError, ValueError) as e:
        raise CryptError("PDF_WRITE", f"PDF 写入失败: {e}") from None


def encrypt_file(
    input_path: str,
    output_path: str,
    user_password: str,
    owner_password: str,
    bits: int,
    permissions: dict,
    input_password: str = "",
) -> dict:
    """AES 加密。bits=256 → R6(AES-256),bits=128 → R4(AES-128)。

    写完后立刻做四项校验,任一不成立即报错并删除输出:
    无密码打不开(用户密码非空时)、用户密码能打开、页数不变、权限位落盘。
    """
    if bits not in (128, 256):
        raise CryptError("INVALID_ARGUMENT", f"bits 必须是 128 或 256,收到 {bits}")
    if not user_password and not owner_password:
        raise CryptError("INVALID_ARGUMENT", "用户密码与所有者密码不能同时为空")

    owner = owner_password or user_password
    rev = 6 if bits == 256 else 4

    # R4(AES-128) 的密码只能是 PDFDocEncoding 可表示的字符:中文密码会在
    # pikepdf 内部抛 ValueError。与其崩成未知错误,不如提前给出可执行的建议。
    if rev <= 4:
        for label, pw in (("用户密码", user_password), ("所有者密码", owner)):
            if pw:
                try:
                    pw.encode("ascii")
                except UnicodeEncodeError:
                    raise CryptError(
                        "PDF_PASSWORD_ENCODING",
                        f"{label}含非 ASCII 字符,AES-128 不支持;请改用 AES-256,或把密码改成字母数字",
                    ) from None

    pdf = _open(input_path, input_password or "")
    page_count = len(pdf.pages)
    was_encrypted = bool(pdf.is_encrypted)

    enc = pikepdf.Encryption(
        user=user_password,
        owner=owner,
        R=rev,
        allow=_build_permissions(permissions),
        metadata=False,
    )
    _write(pdf, output_path, encryption=enc, **_SAVE_OPTS)
    pdf.close()

    # ---- 落盘校验:宁可报错,也不能交出一个「本机打得开、别人打不开」的坏文件 ----
    if user_password:
        try:
            opened = pikepdf.open(output_path)
        except pikepdf.PasswordError:
            opened = None  # 期望路径:无密码打不开
        except pikepdf.PikepdfError as e:
            _discard(output_path)
            raise CryptError("PDF_VERIFY_FAILED", f"加密校验失败:输出无法解析({e})") from None
        if opened is not None:
            opened.close()
            _discard(output_path)
            raise CryptError(
                "PDF_VERIFY_FAILED",
                "加密校验失败:输出文件无需密码即可打开,已丢弃",
            )

    check = _open(output_path, user_password)
    try:
        new_pages = len(check.pages)
        allow = check.allow
        if new_pages != page_count:
            _discard(output_path)
            raise CryptError(
                "PDF_VERIFY_FAILED",
                f"加密校验失败:页数由 {page_count} 变为 {new_pages},已丢弃",
            )
        # 所有者密码打开时权限位应全部放开(qpdf 语义),用户密码打开时才受限
        if user_password and owner != user_password:
            expect = _build_permissions(permissions)
            if bool(allow.extract) != bool(expect.extract):
                _discard(output_path)
                raise CryptError("PDF_VERIFY_FAILED", "加密校验失败:复制权限未按设置落盘")
            if bool(allow.print_highres) != bool(expect.print_highres):
                _discard(output_path)
                raise CryptError("PDF_VERIFY_FAILED", "加密校验失败:打印权限未按设置落盘")
        perms_out = _perm_report(allow)
    finally:
        check.close()

    return {
        "output": output_path,
        "pages": page_count,
        "bits": bits,
        "revision": rev,
        "was_encrypted": was_encrypted,
        "permissions": perms_out,
        "size": os.path.getsize(output_path),
    }


def decrypt_file(input_path: str, output_path: str, password: str) -> dict:
    """解密(去掉全部加密与权限限制)。"""
    pdf = _open(input_path, password)
    try:
        was_encrypted = bool(pdf.is_encrypted)
        page_count = len(pdf.pages)
        _write(pdf, output_path, **_SAVE_OPTS)
    finally:
        pdf.close()

    # 校验:输出必须能被无密码打开,且页数一致
    try:
        check = pikepdf.open(output_path)
    except pikepdf.PasswordError:
        _discard(output_path)
        raise CryptError("PDF_VERIFY_FAILED", "解密校验失败:输出仍需要密码,已丢弃") from None
    except pikepdf.PdfError as e:
        _discard(output_path)
        raise CryptError("PDF_VERIFY_FAILED", f"解密校验失败:输出无法解析({e})") from None
    try:
        new_pages = len(check.pages)
        still_encrypted = bool(check.is_encrypted)
    finally:
        check.close()
    if new_pages != page_count or still_encrypted:
        _discard(output_path)
        raise CryptError(
            "PDF_VERIFY_FAILED",
            f"解密校验失败(页数 {page_count}→{new_pages},仍加密={still_encrypted}),已丢弃",
        )

    return {
        "output": output_path,
        "pages": page_count,
        "was_encrypted": was_encrypted,
        "permissions": {},
        "size": os.path.getsize(output_path),
    }


def probe_encryption(input_path: str, password: str = "") -> dict:
    """探测加密状态:是否需要密码、权限位、页数。不需要密码即可读时返回明文信息。"""
    p = Path(input_path)
    if not p.is_file():
        raise CryptError("IO_NOT_FOUND", f"文件不存在: {input_path}")
    try:
        pdf = pikepdf.open(input_path, password=password)
    except pikepdf.PasswordError:
        # 需要密码但没给/给错:这是可预期的状态,不算失败
        return {
            "encrypted": True,
            "locked": True,
            "pages": 0,
            "permissions": {},
        }
    try:
        return {
            "encrypted": bool(pdf.is_encrypted),
            "locked": False,
            "pages": len(pdf.pages),
            "permissions": _perm_report(pdf.allow),
        }
    finally:
        pdf.close()


def _discard(path: str) -> None:
    """校验失败时清掉半成品,避免把坏文件留在用户目录里。"""
    try:
        os.remove(path)
    except OSError:
        pass
