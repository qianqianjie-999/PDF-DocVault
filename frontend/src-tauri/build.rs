//! Tauri 应用构建脚本。
//!
//! 除标准 tauri-build 外,还守护两件「发布后才会暴露、且现场无法自救」的事:
//!
//! 1. **发布包必须内置正式 license 公钥**。之前 release 分支返回全零占位公钥,
//!    导致发布版激活功能 100% 不可用且毫无提示。
//! 2. **发布包必须内置 pycore 运行时**。加密/OCR/Office 三条链路都经 pycore 调
//!    Python;`bundle.resources` 里漏掉它,用户机上点这些按钮只会报「未找到 pycore」。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    check_license_pubkey();
    check_pycore_runtime();
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

/// 发布包必须带 pycore 运行时,否则加密/OCR/Office 在用户机上是死按钮。
///
/// 只做结构检查(入口 + 内置解释器 + 可执行位),不跑 Python —— 构建脚本里
/// 启动子进程容易在 CI 上卡住,真正的功能自检在 `scripts/pack-pycore-runtime.sh`
/// 打包时已经跑过一遍。
fn check_pycore_runtime() {
    println!("cargo:rerun-if-changed=resources/pycore/main.py");
    println!("cargo:rerun-if-changed=resources/pycore/python");

    if env::var("PROFILE").unwrap_or_default() != "release" {
        return;
    }
    if env::var("PDF_DOCVAULT_SKIP_PYCORE_CHECK")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        println!(
            "cargo:warning=PDF_DOCVAULT_SKIP_PYCORE_CHECK=1:跳过 pycore 检查,\
             此产物的加密/OCR/Office 不可用"
        );
        return;
    }

    let base = Path::new("resources/pycore");
    let missing: Vec<&str> = if !base.join("main.py").is_file() {
        vec!["resources/pycore/main.py"]
    } else {
        let candidates = [
            "python/bin/python3",
            "python/python.exe",
            "python/bin/python",
        ];
        if candidates.iter().any(|rel| base.join(rel).is_file()) {
            vec![]
        } else {
            vec!["resources/pycore/python/bin/python3"]
        }
    };

    if missing.is_empty() {
        return;
    }

    panic!(
        "\n\n=== 发布包缺少 pycore 运行时,已中止打包 ===\n\n\
         缺少: {}\n\n\
         加密 / OCR / 图片转 Word·Excel / Word→PDF 全部经 pycore 调 Python,\n\
         少了它这些功能在用户机上只会报「未找到 Python 解释器」。\n\n\
         先生成运行时(首次需联网下载解释器,之后走缓存):\n\
         \x20   scripts/pack-pycore-runtime.sh\n\n\
         只要核心 PDF 功能、不需要 OCR(体积小约 300MB):\n\
         \x20   scripts/pack-pycore-runtime.sh --core-only\n\n\
         仅构建内部测试包、明确知道这些功能不可用时:\n\
         \x20   PDF_DOCVAULT_SKIP_PYCORE_CHECK=1 pnpm tauri build\n\n",
        missing.join(", ")
    );
}
