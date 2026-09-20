//! 核心错误类型
//!
//! 统一错误枚举 [`CoreError`],实现与各依赖库错误的 `From` 转换,
//! 便于在 `?` 传播时自动转换。`code_string()` 用于前端区分错误类型。

use thiserror::Error;

/// 核心库统一错误
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("PDF 处理错误: {0}")]
    Pdf(#[from] lopdf::Error),

    #[error("PDFium 渲染错误: {0}")]
    Pdfium(String),

    #[error("图像处理错误: {0}")]
    Image(String),

    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("数据库锁中毒")]
    DbLockPoisoned,

    #[error("序列化错误: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("激活/许可证错误: {0}")]
    Activation(String),

    #[error("加密错误: {0}")]
    Encrypt(String),

    #[error("OCR 错误: {0}")]
    Ocr(String),

    #[error("页面不存在: {0}")]
    PageNotFound(u32),

    #[error("参数无效: {0}")]
    InvalidArgument(String),

    #[error("任务已取消")]
    TaskCancelled,

    #[error("内部错误: {0}")]
    Internal(String),
}

impl CoreError {
    /// 返回稳定的错误码,供前端按类型处理
    pub fn code_string(&self) -> &'static str {
        match self {
            CoreError::Io(_) => "IO",
            CoreError::Pdf(_) => "PDF",
            CoreError::Pdfium(_) => "PDFIUM",
            CoreError::Image(_) => "IMAGE",
            CoreError::Db(_) => "DB",
            CoreError::DbLockPoisoned => "DB_LOCK",
            CoreError::Serialize(_) => "SERIALIZE",
            CoreError::Activation(_) => "ACTIVATION",
            CoreError::Encrypt(_) => "ENCRYPT",
            CoreError::Ocr(_) => "OCR",
            CoreError::PageNotFound(_) => "PAGE_NOT_FOUND",
            CoreError::InvalidArgument(_) => "INVALID_ARG",
            CoreError::TaskCancelled => "TASK_CANCELLED",
            CoreError::Internal(_) => "INTERNAL",
        }
    }
}

/// PdfiumError → CoreError 自动转换(保留 ? 传播能力)
impl From<pdfium_render::prelude::PdfiumError> for CoreError {
    fn from(e: pdfium_render::prelude::PdfiumError) -> Self {
        CoreError::Pdfium(e.to_string())
    }
}

/// 核心库统一 Result 别名
pub type Result<T> = std::result::Result<T, CoreError>;
