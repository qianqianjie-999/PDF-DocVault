//! Tauri 应用构建脚本。
//!
//! 除标准 tauri-build 外,额外守护一件事:**发布包必须内置正式 license 公钥**。
//! 之前 release 分支返回全零占位公钥,导致发布版激活功能 100% 不可用且毫无提示,
//! 这里在打包环节直接把问题拦死。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    check_license_pubkey();
    tauri_build::build()
}

fn check_license_pubkey() {
    println!("cargo:rerun-if-env-changed=PDF_DOCVAULT_LICENSE_PUBKEY");
    println!("cargo:rerun-if-changed=../../.license/pubkey.hex");

    let profile = env::var("PROFILE").unwrap_or_default();
    if profile != "release" {
        return;
    }
    if env::var("PDF_DOCVAULT_ALLOW_PLACEHOLDER_KEY")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        println!(
            "cargo:warning=PDF_DOCVAULT_ALLOW_PLACEHOLDER_KEY=1:跳过公钥检查,此产物不能对外发布"
        );
        return;
    }

    let found = env::var("PDF_DOCVAULT_LICENSE_PUBKEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            let path: PathBuf = Path::new("../../.license/pubkey.hex").to_path_buf();
            fs::read_to_string(path).ok().map(|s| s.trim().to_string())
        });

    match found {
        Some(pk) if pk.len() == 64 && !pk.chars().all(|c| c == '0') => {}
        _ => panic!(
            "\n\n=== 发布包缺少正式 license 公钥,已中止打包 ===\n\n\
             没有正式公钥的发布包,所有用户都激活不了(且现场只会看到“许可证签名无效”)。\n\n\
             先在本机生成一次密钥对(只做一次,私钥务必离线备份):\n\
             \x20   cargo run -p license-gen -- keygen --out-dir .license\n\n\
             再用打包命令(公钥会被自动读取):\n\
             \x20   pnpm tauri build\n\n\
             仅构建内部测试包、明确知道激活不可用时:\n\
             \x20   PDF_DOCVAULT_ALLOW_PLACEHOLDER_KEY=1 pnpm tauri build\n\n"
        ),
    }
}
