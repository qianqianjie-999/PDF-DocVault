//! PDF 压缩
//!
//! 利用 lopdf 的 `compress` 压缩未压缩的 content stream,`prune_objects` 清理冗余对象。
//! 注意:这是无损结构压缩,不会降低图片采样率(有损压缩需后续基于 pdfium 实现)。

use lopdf::Document;
use std::path::Path;

use crate::errors::Result;

/// 压缩统计
#[derive(Debug, Clone, serde::Serialize)]
pub struct CompressStats {
    /// 原始字节数
    pub original_size: u64,
    /// 压缩后字节数
    pub compressed_size: u64,
    /// 压缩比 %
    pub ratio: f32,
    /// 耗时(毫秒)
    pub elapsed_ms: u128,
}

/// 压缩 PDF。`level` 当前仅作占位,lopdf compress 为无损 FlateDecode。
pub fn compress(input: &Path, output: &Path, _level: u8) -> Result<CompressStats> {
    let start = std::time::Instant::now();
    let original_size = std::fs::metadata(input)?.len();
    let mut doc = Document::load(input)?;
    doc.prune_objects();
    doc.renumber_objects();
    doc.compress();
    doc.save(output)?;
    let compressed_size = std::fs::metadata(output)?.len();
    let ratio = if original_size > 0 {
        (1.0 - compressed_size as f32 / original_size as f32) * 100.0
    } else {
        0.0
    };
    Ok(CompressStats {
        original_size,
        compressed_size,
        ratio,
        elapsed_ms: start.elapsed().as_millis(),
    })
}
