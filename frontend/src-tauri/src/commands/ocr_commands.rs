//! OCR 文字识别命令
//!
//! 通过 `pdf_docvault_core::ocr` 调用 pycore Python 子进程(rapidocr/paddleocr)。
//! 批量任务经 `tokio::task::spawn_blocking` 执行,推送 `task-progress` 事件。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;
use uuid::Uuid;

use crate::commands::{emit_complete, emit_progress};
use crate::error::{AppError, CmdResult};
use pdf_docvault_core::ocr::{ocr_convert, ocr_extract, ConvertMode, ConvertStats, OcrOptions, OcrStats};

// ===== 环境自检 =====

#[derive(Serialize)]
pub struct OcrSelftestOutput {
    /// Python 环境与引擎是否就绪
    pub ok: bool,
    /// 当前使用的引擎(rapidocr / paddleocr …)
    pub engine: Option<String>,
    /// 就绪时为可用引擎列表;失败时为安装/配置提示
    pub message: String,
}

#[tauri::command]
pub async fn ocr_selftest() -> CmdResult<OcrSelftestOutput> {
    let res = tokio::task::spawn_blocking(|| {
        let pycore = pdf_docvault_core::ocr::resolve_pycore_dir()?;
        pdf_docvault_core::ocr::ocr_selftest(&pycore)
    })
    .await
    .map_err(|e| AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })?;
    Ok(match res {
        Ok((engine, available)) => OcrSelftestOutput {
            ok: true,
            engine: Some(engine),
            message: available.join(", "),
        },
        Err(e) => OcrSelftestOutput {
            ok: false,
            engine: None,
            message: e.to_string(),
        },
    })
}

// ===== 文字识别 =====

#[derive(Deserialize)]
pub struct OcrInput {
    pub inputs: Vec<PathBuf>,
    #[serde(default)]
    pub options: OcrOptions,
    pub task_id: Option<String>,
}

#[tauri::command]
pub async fn ocr_extract_images(app: AppHandle, input: OcrInput) -> CmdResult<OcrStats> {
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = input.inputs;
    let options = input.options;
    let app2 = app.clone();
    let tid = task_id.clone();
    let stats = tokio::task::spawn_blocking(move || {
        let cb = move |current: u32, total: u32| {
            emit_progress(&app2, &tid, "ocr", current, total);
        };
        let pycore = pdf_docvault_core::ocr::resolve_pycore_dir()?;
        ocr_extract(&inputs, &pycore, &options, Some(&cb))
    })
    .await
    .map_err(|e| AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;

    emit_complete(&app, &task_id, "ocr", "done");
    Ok(stats)
}

// ===== 拍图转 Word / 表格转 Excel =====

#[derive(Deserialize)]
pub struct OcrConvertInput {
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
    /// "docx" | "xlsx"
    pub mode: String,
    #[serde(default)]
    pub options: OcrOptions,
    pub task_id: Option<String>,
}

#[tauri::command]
pub async fn ocr_convert_documents(
    app: AppHandle,
    input: OcrConvertInput,
) -> CmdResult<ConvertStats> {
    let mode = match input.mode.as_str() {
        "docx" => ConvertMode::Docx,
        "xlsx" => ConvertMode::Xlsx,
        other => {
            return Err(AppError {
                code: "INVALID_ARG".into(),
                message: format!("未知转换模式: {other}"),
            })
        }
    };
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = input.inputs;
    let output = input.output;
    let options = input.options;
    let app2 = app.clone();
    let tid = task_id.clone();
    let stats = tokio::task::spawn_blocking(move || {
        let cb = move |current: u32, total: u32| {
            emit_progress(&app2, &tid, "ocr-convert", current, total);
        };
        let pycore = pdf_docvault_core::ocr::resolve_pycore_dir()?;
        ocr_convert(&inputs, &output, mode, &pycore, &options, Some(&cb))
    })
    .await
    .map_err(|e| AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;

    emit_complete(&app, &task_id, "ocr-convert", "done");
    Ok(stats)
}
