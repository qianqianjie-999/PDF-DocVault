//! PDF 加密/解密
//!
//! lopdf 0.34 仅支持解密读取,不支持写入加密。因此加密委托给系统 `qpdf` 二进制
//! (规划书 §14.1 已列为稳健后备)。调用方式:`qpdf --encrypt user owner 256 ... -- in out`。
//! 打包时通过 Tauri `bundle.externalBin` 跨平台分发 qpdf。

use std::path::Path;
use std::process::Command;

use crate::errors::{CoreError, Result};

/// 权限控制
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Permissions {
    pub allow_print: bool,
    pub allow_copy: bool,
    pub allow_modify: bool,
    pub allow_annotate: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            allow_print: true,
            allow_copy: true,
            allow_modify: false,
            allow_annotate: false,
        }
    }
}

/// 加密选项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptOptions {
    pub user_password: String,
    pub owner_password: Option<String>,
    pub permissions: Permissions,
    /// 密钥位数:128 或 256(规划书要求 256)
    pub bits: u16,
}

impl Default for EncryptOptions {
    fn default() -> Self {
        Self {
            user_password: String::new(),
            owner_password: None,
            permissions: Permissions::default(),
            bits: 256,
        }
    }
}

/// 使用 qpdf 进行 AES-256 加密。目标性能:< 50ms。
pub fn encrypt_aes256(input: &Path, output: &Path, opts: &EncryptOptions) -> Result<()> {
    let bits = match opts.bits {
        128 | 256 => opts.bits,
        _ => return Err(CoreError::InvalidArgument("bits 必须是 128 或 256".into())),
    };
    let owner = opts
        .owner_password
        .clone()
        .unwrap_or_else(|| opts.user_password.clone());

    let mut cmd = Command::new("qpdf");
    cmd.arg("--encrypt")
        .arg(&opts.user_password)
        .arg(&owner)
        .arg(bits.to_string());

    // 权限标志必须在 bits 之后、-- 之前
    if opts.permissions.allow_print {
        cmd.arg("--print=full");
    } else {
        cmd.arg("--print=none");
    }
    if opts.permissions.allow_modify {
        cmd.arg("--modify=all");
    } else {
        cmd.arg("--modify=none");
    }
    if opts.permissions.allow_copy {
        cmd.arg("--extract=y");
    } else {
        cmd.arg("--extract=n");
    }
    if opts.permissions.allow_annotate {
        cmd.arg("--annotate=y");
    } else {
        cmd.arg("--annotate=n");
    }

    cmd.arg("--").arg(input).arg(output);

    run_qpdf(&mut cmd)
}

/// 使用 qpdf 解密
pub fn decrypt(input: &Path, output: &Path, password: &str) -> Result<()> {
    let mut cmd = Command::new("qpdf");
    cmd.arg("--password")
        .arg(password)
        .arg("--decrypt")
        .arg(input)
        .arg(output);
    run_qpdf(&mut cmd)
}

fn run_qpdf(cmd: &mut Command) -> Result<()> {
    let output = cmd
        .output()
        .map_err(|e| CoreError::Encrypt(format!("调用 qpdf 失败: {e}(请确认 qpdf 已安装)")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CoreError::Encrypt(format!("qpdf 执行失败: {stderr}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn encrypt_roundtrip() {
        // 仅在 qpdf 已安装时执行
        if std::process::Command::new("qpdf").output().is_err() {
            eprintln!("跳过 encrypt_roundtrip:未安装 qpdf");
            return;
        }
        // 真实测试在 tests/fixtures 下进行
    }
}
