//! 集成测试公共工具:fixture 动态生成与外部依赖探测。
//!
//! 规划书约定 fixtures/pdfs/ 存放样本,但二进制样本难以审查与维护,
//! 这里改为运行时合成(多页空白 PDF),语义等价且零仓库负担。
//!
//! 每个测试二进制都会单独编译本模块,部分工具只被其中之一使用,
//! 统一关闭 dead_code 提示。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static SEQ: AtomicU32 = AtomicU32::new(0);

/// 生成包含 `pages` 页空白页面的 PDF fixture,返回 PDF 路径。
///
/// 基于 core 的多图合成 PDF 流水线(lopdf 写入),页面为纯白 A4 比例图。
pub fn make_pdf(pages: u32) -> PathBuf {
    use image::{Rgb, RgbImage};
    use pdf_docvault_core::image::{images_to_pdf, ScanOptions};

    assert!(pages >= 1, "至少 1 页");
    let dir = std::env::temp_dir().join(format!(
        "pdf-docvault-tests-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("创建 fixture 临时目录");

    let mut inputs = Vec::with_capacity(pages as usize);
    for i in 0..pages {
        let img: RgbImage = RgbImage::from_pixel(595, 842, Rgb([255u8, 255, 255])); // A4 pt 比例
        let p = dir.join(format!("src-{i}.png"));
        img.save(&p).expect("保存 fixture 图片");
        inputs.push(p);
    }
    let out = dir.join(format!("fixture-{pages}p.pdf"));
    images_to_pdf(&inputs, &out, 96.0, &ScanOptions::default(), None)
        .expect("合成 fixture PDF 应成功");
    out
}

/// 读取 PDF 页数(lopdf 重新加载验证)。
pub fn page_count(path: &Path) -> u32 {
    let doc = lopdf::Document::load(path).expect("重新加载 PDF 应成功");
    doc.get_pages().len() as u32
}

/// 探测并配置 pdfium:已设置 PDFIUM_PATH 则信任调用方;
/// 否则尝试开发环境默认位置(frontend/src-tauri/resources/pdfium/lib)。
/// 返回 false 表示本环境无 pdfium,调用方应跳过渲染类测试。
pub fn ensure_pdfium() -> bool {
    if let Ok(p) = std::env::var("PDFIUM_PATH") {
        return Path::new(&p).exists();
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../frontend/src-tauri/resources/pdfium/lib");
    if dev.join("libpdfium.so").is_file() {
        // SAFETY:测试单线程初始化阶段调用
        unsafe { std::env::set_var("PDFIUM_PATH", &dev) };
        return true;
    }
    false
}

/// 探测系统 qpdf(加密/解密测试用),缺失时调用方跳过。
pub fn qpdf_available() -> bool {
    std::process::Command::new("qpdf")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
