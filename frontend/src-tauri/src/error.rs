//! Tauri 命令错误类型
//!
//! Tauri 2 要求 command 返回 `Result<T, E: Serialize>`。`AppError` 序列化为
//! `{ code, message }` JSON,前端可按 code 分类处理。

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl From<pdf_docvault_core::CoreError> for AppError {
    fn from(e: pdf_docvault_core::CoreError) -> Self {
        Self {
            code: e.code_string().to_string(),
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self {
            code: "SERIALIZE".into(),
            message: e.to_string(),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self {
            code: "IO".into(),
            message: e.to_string(),
        }
    }
}

/// 命令统一 Result 别名
pub type CmdResult<T> = std::result::Result<T, AppError>;
