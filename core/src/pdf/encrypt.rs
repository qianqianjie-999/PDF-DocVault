//! PDF 加密/解密
//!
//! lopdf 0.34 只支持解密读取,不支持写入加密;而原先直接 spawn 系统 `qpdf`
//! 又没随包分发(`tauri.conf.json` 的 `externalBin` 为空),用户在裸机上点
//! 「加密」必然失败。现改为经 pycore 调用 pikepdf —— 其 wheel 内静态链接了
//! libqpdf,三平台同一套代码、零外部依赖。
//!
//! 密码不进命令行参数,经 stdin 以 JSON 传给 pycore 子进程,避免被
//! `/proc/<pid>/cmdline` 旁路读取。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pybridge;
use crate::{CoreError, Result};

/// 加密默认超时:大文件重写整个 PDF,留足余量
const ENCRYPT_TIMEOUT_SECS: u64 = 300;

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
    /// 密钥位数:128(AES-128/R4) 或 256(AES-256/R6,规划书要求)
    pub bits: u16,
    /// 源文件本身已加密时的打开密码
    #[serde(default)]
    pub input_password: Option<String>,
}

impl Default for EncryptOptions {
    fn default() -> Self {
        Self {
            user_password: String::new(),
            owner_password: None,
            permissions: Permissions::default(),
            bits: 256,
            input_password: None,
        }
    }
}

/// 加密结果(页数/算法/落盘体积,供前端提示)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptReport {
    pub output: PathBuf,
    pub pages: usize,
    pub bits: u16,
    /// PDF 加密修订号:R6 = AES-256,R4 = AES-128
    pub revision: u16,
    /// 源文件在加密前是否已加密
    pub was_encrypted: bool,
    pub size: u64,
    #[serde(default)]
    pub permissions: PermissionReport,
}

/// 解密结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptReport {
    pub output: PathBuf,
    pub pages: usize,
    /// 源文件是否真的带加密(否则只是原样重写)
    pub was_encrypted: bool,
    pub size: u64,
}

/// 加密状态探测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionInfo {
    pub encrypted: bool,
    /// 需要密码才能读到内容(未给密码或密码错误)
    pub locked: bool,
    pub pages: usize,
    #[serde(default)]
    pub permissions: PermissionReport,
}

/// 服务端回读的权限位(用于确认设置真的落盘)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionReport {
    #[serde(default)]
    pub allow_print: bool,
    #[serde(default)]
    pub allow_copy: bool,
    #[serde(default)]
    pub allow_modify: bool,
    #[serde(default)]
    pub allow_annotate: bool,
}

/// 构造传给 pycore stdin 的密钥载荷。
///
/// 抽成独立函数便于单测:字段名改了但 Python 侧没同步,单测能立刻发现。
fn encrypt_payload(opts: &EncryptOptions) -> String {
    serde_json::json!({
        "user_password": opts.user_password,
        "owner_password": opts.owner_password.clone().unwrap_or_default(),
        "input_password": opts.input_password.clone().unwrap_or_default(),
        "permissions": {
            "allow_print": opts.permissions.allow_print,
            "allow_copy": opts.permissions.allow_copy,
            "allow_modify": opts.permissions.allow_modify,
            "allow_annotate": opts.permissions.allow_annotate,
        },
    })
    .to_string()
}

/// 把 pycore 的错误码映射成 `CoreError::Encrypt`,保留原文给用户看。
fn map_py_error(resp: &pybridge::PyResponse, action: &str) -> CoreError {
    match resp.error.as_ref() {
        Some(e) => CoreError::Encrypt(format!("{action}失败: {}", e.message)),
        None => CoreError::Encrypt(format!("{action}失败:pycore 未返回错误详情")),
    }
}

/// 从响应里取 report 并反序列化
fn take_report<T: serde::de::DeserializeOwned>(
    resp: &pybridge::PyResponse,
    action: &str,
) -> Result<T> {
    let value = resp
        .report
        .clone()
        .ok_or_else(|| CoreError::Encrypt(format!("{action}失败:pycore 未返回结果报告")))?;
    serde_json::from_value(value)
        .map_err(|e| CoreError::Encrypt(format!("{action}结果解析失败: {e}")))
}

/// 定位 pycore 运行时,失败时把上下文补全。
///
/// 打包版若没把 pycore 随包分发,这里的报错就是用户唯一能看到的线索,
/// 所以要说清"缺什么、去哪配",不能只丢一句「未找到目录」。
fn pycore_dir() -> Result<PathBuf> {
    pybridge::resolve_pycore_dir().map_err(|e| {
        CoreError::Encrypt(format!(
            "PDF 加密依赖 pycore 运行时(需 .venv 且已安装 pikepdf):{e}"
        ))
    })
}

/// 加密。默认 AES-256(R6);bits=128 走 AES-128(R4)。
///
/// Python 侧落盘后会做四项自检(无密码打不开 / 用户密码能打开 / 页数不变 /
/// 权限位落盘),任一失败即删掉输出并报错,不会把坏文件留给用户。
pub fn encrypt_aes256(input: &Path, output: &Path, opts: &EncryptOptions) -> Result<EncryptReport> {
    if opts.bits != 128 && opts.bits != 256 {
        return Err(CoreError::InvalidArgument("bits 必须是 128 或 256".into()));
    }
    if opts.user_password.is_empty() {
        return Err(CoreError::InvalidArgument("用户密码不能为空".into()));
    }
    let pycore_dir = pycore_dir()?;
    let args = [
        ("input".to_string(), path_str(input)),
        ("output".to_string(), path_str(output)),
        ("bits".to_string(), opts.bits.to_string()),
    ];

    let resp = pybridge::run_pycore_stdin(
        &pycore_dir,
        "encrypt_pdf",
        args,
        encrypt_payload(opts),
        ENCRYPT_TIMEOUT_SECS,
        || {},
    )?;
    if !resp.ok {
        return Err(map_py_error(&resp, "加密"));
    }
    take_report(&resp, "加密")
}

/// 解密:去掉全部加密与权限限制。
pub fn decrypt(input: &Path, output: &Path, password: &str) -> Result<DecryptReport> {
    let pycore_dir = pybridge::resolve_pycore_dir()?;
    let args = [
        ("input".to_string(), path_str(input)),
        ("output".to_string(), path_str(output)),
    ];
    let payload = serde_json::json!({ "password": password }).to_string();

    let resp = pybridge::run_pycore_stdin(
        &pycore_dir,
        "decrypt_pdf",
        args,
        payload,
        ENCRYPT_TIMEOUT_SECS,
        || {},
    )?;
    if !resp.ok {
        return Err(map_py_error(&resp, "解密"));
    }
    take_report(&resp, "解密")
}

/// 探测是否加密;需要密码但未提供时返回 `locked: true` 而非报错。
pub fn probe_encryption(input: &Path, password: &str) -> Result<EncryptionInfo> {
    let pycore_dir = pybridge::resolve_pycore_dir()?;
    let args = [("input".to_string(), path_str(input))];
    let payload = serde_json::json!({ "password": password }).to_string();

    let resp =
        pybridge::run_pycore_stdin(&pycore_dir, "pdf_encryption_info", args, payload, 60, || {})?;
    if !resp.ok {
        return Err(map_py_error(&resp, "读取加密状态"));
    }
    take_report(&resp, "读取加密状态")
}

/// 路径转字符串:非法 UTF-8 时退回有损转换,至少不让调用方 panic
fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> EncryptOptions {
        EncryptOptions {
            user_password: "u-pw".into(),
            owner_password: Some("o-pw".into()),
            permissions: Permissions {
                allow_print: true,
                allow_copy: false,
                allow_modify: false,
                allow_annotate: true,
            },
            bits: 256,
            input_password: None,
        }
    }

    #[test]
    fn payload_carries_all_fields() {
        let v: serde_json::Value = serde_json::from_str(&encrypt_payload(&opts())).unwrap();
        assert_eq!(v["user_password"], "u-pw");
        assert_eq!(v["owner_password"], "o-pw");
        assert_eq!(v["permissions"]["allow_copy"], false);
        assert_eq!(v["permissions"]["allow_annotate"], true);
        assert_eq!(v["input_password"], "");
    }

    #[test]
    fn payload_defaults_missing_owner_and_input_password_to_empty() {
        let o = EncryptOptions {
            user_password: "only-user".into(),
            ..Default::default()
        };
        let v: serde_json::Value = serde_json::from_str(&encrypt_payload(&o)).unwrap();
        assert_eq!(v["owner_password"], "");
        assert_eq!(v["input_password"], "");
    }

    #[test]
    fn rejects_unsupported_bits() {
        let o = EncryptOptions {
            bits: 512,
            ..opts()
        };
        let err = encrypt_aes256(Path::new("in.pdf"), Path::new("out.pdf"), &o).unwrap_err();
        assert!(matches!(err, CoreError::InvalidArgument(_)));
    }

    #[test]
    fn rejects_empty_user_password() {
        let o = EncryptOptions {
            user_password: String::new(),
            ..opts()
        };
        let err = encrypt_aes256(Path::new("in.pdf"), Path::new("out.pdf"), &o).unwrap_err();
        assert!(matches!(err, CoreError::InvalidArgument(_)));
    }

    #[test]
    fn report_parses_permissions_payload() {
        // 固化 Python 侧返回的字段名:改名而没同步时这里会红
        let json = serde_json::json!({
            "output": "/tmp/x.pdf",
            "pages": 3,
            "bits": 256,
            "revision": 6,
            "was_encrypted": false,
            "size": 12345,
            "permissions": {
                "allow_print": true,
                "allow_copy": false,
                "allow_modify": false,
                "allow_annotate": true
            }
        });
        let r: EncryptReport = serde_json::from_value(json).unwrap();
        assert_eq!(r.pages, 3);
        assert_eq!(r.revision, 6);
        assert!(!r.permissions.allow_copy);
    }

    #[test]
    fn missing_permissions_block_still_parses() {
        // 解密报告不带 permissions,缺省必须能过
        let json = serde_json::json!({
            "output": "/tmp/x.pdf",
            "pages": 1,
            "was_encrypted": true,
            "size": 100
        });
        let r: DecryptReport = serde_json::from_value(json).unwrap();
        assert!(r.was_encrypted);
    }
}
