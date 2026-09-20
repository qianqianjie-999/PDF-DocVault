//! 试卷去手写 —— 颜色阈值 + OpenCV inpaint 修复。
//!
//! 原理与限制见 `pycore/scan/handwriting.py`:彩色笔迹(蓝/红/绿等)在 HSV
//! 色相上与黑白印刷可分,生成笔迹掩膜后用周围像素修复;黑色/铅笔手写与
//! 印刷同为灰色系,颜色上不可分,属原理性限制。
//!
//! 架构:Rust 通过 [`crate::pybridge`] spawn `pycore/main.py remove_handwriting`,
//! PDF 输入先由 pdfium 渲染为 PNG(复用 [`crate::ocr::prepare_images`])。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::ocr::ProgressFn;
use crate::pybridge::run_pycore;
use crate::{CoreError, Result};

/// 去手写选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandwritingOptions {
    /// 笔迹颜色(blue/red/green/purple/orange/cyan),空 = Python 侧默认 blue+red
    #[serde(default)]
    pub colors: Vec<String>,
    /// 掩膜膨胀次数,覆盖笔迹抗锯齿边缘(默认 2)
    #[serde(default = "default_dilate")]
    pub dilate: u32,
    /// PDF 输入时的渲染 DPI(默认 200)
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    /// Python 进程超时秒数(默认 300)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_dilate() -> u32 {
    2
}
fn default_dpi() -> u32 {
    200
}
fn default_timeout() -> u64 {
    300
}

impl Default for HandwritingOptions {
    fn default() -> Self {
        Self {
            colors: Vec::new(),
            dilate: default_dilate(),
            dpi: default_dpi(),
            timeout_secs: default_timeout(),
        }
    }
}

/// 去手写统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandwritingStats {
    /// 清理后的图片路径(输出目录内 `<原名>_cleaned.png`)
    pub outputs: Vec<PathBuf>,
    /// 笔迹像素占比均值(0-1),0 表示未检测到笔迹
    pub mask_ratio: f32,
    pub images: usize,
    pub elapsed_ms: u128,
    /// 输入中是否包含 PDF(内部已渲染并清理临时文件)
    pub from_pdf_pages: u32,
}

/// 对图片(或 PDF,先渲染为图片)执行试卷去手写。
///
/// `out_dir` 为清理结果输出目录(不存在会自动创建);
/// `pycore_dir` 指向 pycore 目录(含 main.py)。
pub fn remove_handwriting(
    inputs: &[PathBuf],
    out_dir: &Path,
    pycore_dir: &Path,
    opts: &HandwritingOptions,
    progress: Option<&ProgressFn>,
) -> Result<HandwritingStats> {
    if inputs.is_empty() {
        return Err(CoreError::InvalidArgument("未提供输入文件".into()));
    }
    let notify = |cur: u32, total: u32| {
        if let Some(cb) = progress {
            cb(cur, total);
        }
    };

    // 1. PDF 输入先渲染为临时 PNG(与 OCR 共用渲染流水线)
    let (tmp_dir, images, pdf_pages) = crate::ocr::prepare_images(inputs, opts.dpi, &notify)?;

    // 2. spawn Python 子进程
    let mut args: Vec<(String, String)> = vec![
        ("output-dir".into(), out_dir.to_string_lossy().into_owned()),
        ("dilate".into(), opts.dilate.to_string()),
    ];
    if !opts.colors.is_empty() {
        args.push(("colors".into(), opts.colors.join(",")));
    }
    for img in &images {
        args.push(("input".into(), img.to_string_lossy().into_owned()));
    }
    let resp = run_pycore(pycore_dir, "remove_handwriting", args, opts.timeout_secs, || {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    });

    let resp = resp?;
    if resp.ok {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    if let Some(e) = resp.error_or_none() {
        return Err(e);
    }

    let outputs = resp.outputs.unwrap_or_default();
    if outputs.is_empty() {
        return Err(CoreError::Ocr("pycore 未返回任何输出文件".into()));
    }
    let stats = HandwritingStats {
        images: resp.images.unwrap_or(outputs.len()),
        outputs,
        mask_ratio: resp.mask_ratio.unwrap_or(0.0),
        elapsed_ms: resp.elapsed_ms,
        from_pdf_pages: pdf_pages,
    };
    notify(images.len() as u32, images.len() as u32);
    Ok(stats)
}

// ===== 单元测试 =====

#[cfg(test)]
mod tests {
    use super::*;

    /// 端到端:需要 pycore/.venv(opencv)与 /tmp/hw-test.png 测试图
    /// (用 /tmp/gen_hw_test.py 生成)。运行:
    /// `cargo test -p pdf_docvault_core handwriting_ -- --ignored --nocapture`
    #[test]
    #[ignore = "需要 pycore 环境与测试图,仅本地端到端验证"]
    fn handwriting_end_to_end() {
        let input = std::path::Path::new("/tmp/hw-test.png");
        if !input.is_file() {
            eprintln!("跳过:缺少测试图 /tmp/hw-test.png");
            return;
        }
        let out_dir = std::env::temp_dir().join("pdf-docvault-hw-rs-test");
        let _ = std::fs::remove_dir_all(&out_dir);
        let pycore = crate::pybridge::resolve_pycore_dir().expect("定位 pycore");
        let opts = HandwritingOptions {
            colors: vec!["blue".into(), "red".into()],
            ..Default::default()
        };
        let stats = remove_handwriting(&[input.to_path_buf()], &out_dir, &pycore, &opts, None)
            .expect("去手写失败");
        eprintln!("stats = {stats:?}");
        assert_eq!(stats.images, 1);
        assert!(stats.outputs[0].is_file());
        assert!(stats.mask_ratio > 0.0, "应检测到笔迹");
    }
}
