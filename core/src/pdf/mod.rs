//! PDF 处理模块
//!
//! 基于 `lopdf`(读写/合并/拆分/页面操作/水印)与 `pdfium-render`(转图片)实现。
//! 加密因 lopdf 0.34 不支持写入,委托给系统 `qpdf` 二进制。

pub mod compress;
pub mod encrypt;
pub mod merge;
pub mod pages;
pub mod render;
pub mod split;
pub mod watermark;

pub use merge::{merge, MergeStats};
pub use split::{split, PageRange, SplitMode, SplitStats};
pub use pages::{delete_pages, extract_pages, insert_pages, reorder_pages, rotate_pages};
pub use encrypt::{
    decrypt, encrypt_aes256, EncryptOptions, Permissions,
};
pub use watermark::{add_image_watermark, add_text_watermark, WatermarkOptions};
pub use compress::{compress, CompressStats};
pub use render::{render_all_pages, render_page_to_bytes, render_page_to_image};
