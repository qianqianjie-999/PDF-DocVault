//! 图像处理模块
//!
//! 基于 `image` 实现文档扫描增强(边界检测/透视矫正/去阴影/自适应二值化)、
//! 多图合成 PDF 与证件排版;
//! 试卷去手写(颜色阈值 + inpaint)经 pybridge 调 pycore/OpenCV 完成。

pub mod enhance;
pub mod handwriting;
pub mod idcard;

pub use enhance::{
    adaptive_binarize, detect_document_quad, images_to_a4_pdf, images_to_pdf, remove_shadow,
    scan_enhance, scan_enhance_batch, warp_perspective, GridOrder, ImageFitMode, ImagesPerA4,
    ImagesToA4Options, ImagesToPdfStats, PageOrientation, Quad, ScanBatchStats, ScanOptions,
    ScanStats, A4_H_PT, A4_W_PT,
};
pub use handwriting::{remove_handwriting, HandwritingOptions, HandwritingStats};
pub use idcard::{id_cards_to_pdf, IdCardOptions, IdCardStats};
