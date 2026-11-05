//! PDF 处理模块
//!
//! 基于 `lopdf`(读写/合并/拆分/页面操作/水印)与 `pdfium-render`(转图片)实现。
//! 加密因 lopdf 不支持写入,经 pycore 委托 pikepdf(静态链接 libqpdf),不依赖系统 qpdf。

pub mod compress;
pub mod encrypt;
pub mod merge;
pub mod pages;
pub mod render;
pub mod split;
pub mod watermark;

pub use compress::{compress, CompressStats};
pub use encrypt::{
    decrypt, encrypt_aes256, probe_encryption, DecryptReport, EncryptOptions, EncryptReport,
    EncryptionInfo, PermissionReport, Permissions,
};
pub use merge::{merge, MergeStats};
pub use pages::{delete_pages, extract_pages, insert_pages, reorder_pages, rotate_pages};
pub use render::{render_all_pages, render_page_to_bytes, render_page_to_image};
pub use split::{split, PageRange, SplitMode, SplitStats};
pub use watermark::{add_image_watermark, add_text_watermark, WatermarkOptions};
