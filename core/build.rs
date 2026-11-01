//! 构建期注入 license 公钥。
//!
//! 公钥来源优先级:
//!   1. 环境变量 `PDF_DOCVAULT_LICENSE_PUBKEY`(64 位 hex)
//!   2. 仓库根 `.license/pubkey.hex`(单行 hex,已在 .gitignore 中)
//!
//! 本脚本只负责"找到就注入"。**发布包的强制拦截在 `frontend/src-tauri/build.rs`** ——
//! 那里才知道自己是不是在打应用包;放在这里会连 `license-gen keygen` 自己都编不出来。
//!
//! 未找到公钥时:`license::public_key()` 在 debug 构建回落到开发种子派生,
//! 在 release 构建直接返回错误(验签必失败),不会静默通过。

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=PDF_DOCVAULT_LICENSE_PUBKEY");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../.license/pubkey.hex");

    match resolve_pubkey() {
        Some(hex) => {
            validate_hex(&hex);
            println!("cargo:rustc-env=PDF_DOCVAULT_LICENSE_PUBKEY_RESOLVED={hex}");
            println!("cargo:rustc-cfg=has_license_pubkey");
        }
        None => {
            let profile = env::var("PROFILE").unwrap_or_default();
            if profile == "release" {
                println!(
                    "cargo:warning=release 构建未找到 license 公钥,产物将无法验证正式许可证。\
                     运行 `cargo run -p license-gen -- keygen --out-dir .license` 生成密钥对。"
                );
            } else {
                println!(
                    "cargo:warning=未找到 license 公钥,debug 构建使用开发种子派生的公钥(仅本地联调)"
                );
            }
        }
    }
}

fn resolve_pubkey() -> Option<String> {
    if let Ok(v) = env::var("PDF_DOCVAULT_LICENSE_PUBKEY") {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    let path = Path::new("../.license/pubkey.hex");
    if path.exists() {
        if let Ok(s) = fs::read_to_string(path) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// 校验是 32 字节 hex,且不是全零/全同值。
/// 全零在 ed25519 里是**合法**压缩点,不校验的话占位公钥会静默编译通过却验不过任何签名。
fn validate_hex(hex: &str) {
    let clean = hex.trim();
    assert!(
        clean.len() == 64,
        "license 公钥长度应为 64 个 hex 字符(32 字节),实际 {} 个字符: {clean}",
        clean.len()
    );
    assert!(
        clean.chars().all(|c| c.is_ascii_hexdigit()),
        "license 公钥必须是纯 hex 字符: {clean}"
    );
    assert!(
        !clean.chars().all(|c| c == '0'),
        "license 公钥不能是全零:这会编译通过但验不过任何正式签名"
    );
    assert!(
        clean
            .chars()
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1,
        "license 公钥疑似占位值(所有字符相同): {clean}"
    );
}
