"""PDF 加密/解密(基于 pikepdf)。"""

from .crypt import CryptError, decrypt_file, encrypt_file, probe_encryption

__all__ = ["CryptError", "decrypt_file", "encrypt_file", "probe_encryption"]
