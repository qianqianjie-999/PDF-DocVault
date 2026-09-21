//! PDF-DocVault 核心处理库
//!
//! 本地优先 · 隐私至上 —— 所有文档处理在用户设备本地完成。
//!
//! 模块划分:
//! - [`pdf`]: PDF 合并/拆分/页面操作/加密/水印/压缩/转图片
//! - [`image`]: 图像增强(去阴影/二值化等,扫描增强占位)
//! - [`pybridge`]: Rust-Python 进程隔离调用基建(spawn pycore 子进程)
//! - [`ocr`]: OCR 文字识别(Rust-Python 进程隔离调用 pycore)
//! - [`search`]: Tantivy 全文搜索(文档元数据索引)
//! - [`storage`]: SQLite 存储与目录管理
//! - [`activation`]: 设备指纹与捐赠激活验证
//! - [`task`]: 任务状态机与进度上报

pub mod activation;
pub mod errors;
pub mod image;
pub mod ocr;
pub mod office;
pub mod pdf;
pub mod pybridge;
pub mod search;
pub mod storage;
pub mod task;

pub use errors::{CoreError, Result};
