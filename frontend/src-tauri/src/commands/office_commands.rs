//! Word → PDF 办公转换命令
//!
//! 通过 `pdf_docvault_core::office` 调用 pycore(Word COM / LibreOffice)。
//! 批量任务在 `spawn_blocking` 中执行,逐文件推送 `task-progress` 事件。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;
use uuid::Uuid;

use crate::commands::{emit_complete, emit_progress};
use crate::error::{AppError, CmdResult};
use pdf_docvault_core::office::{WordConvertReport, WordQuality};

// ===== 引擎自检 =====

#[derive(Serialize)]
pub struct OfficeSelftestOutput {
    /// 是否至少存在一个可用引擎
    pub ok: bool,
    pub libreoffice: Option<String>,
    pub word_com: bool,
    pub wps_com: bool,
    pub wps_cli: Option<String>,
    /// 无可用引擎时为安装指引
    pub message: String,
}

#[tauri::command]
pub async fn office_selftest() -> CmdResult<OfficeSelftestOutput> {
    let res = tokio::task::spawn_blocking(|| {
        let pycore = pdf_docvault_core::office::resolve_pycore_dir()?;
        pdf_docvault_core::office::office_selftest(&pycore)
    })
    .await
    .map_err(|e| AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })?;

    match res {
        Ok(s) => {
            let ok = s.libreoffice.is_some() || s.word_com || s.wps_com || s.wps_cli.is_some();
            let message = if ok {
                match (
                    s.libreoffice.as_deref(),
                    s.word_com,
                    s.wps_com,
                    s.wps_cli.as_deref(),
                ) {
                    (Some(path), _, _, _) => format!("LibreOffice: {path}"),
                    (None, true, _, _) => "Microsoft Word(本机 Office)".into(),
                    (None, false, true, _) => "WPS Office(本机 COM)".into(),
                    (None, false, false, Some(path)) => format!("WPS Office(命令行): {path}"),
                    // ok=true 时不可能出现,语义兜底
                    _ => "无可用引擎".into(),
                }
            } else {
                "未检测到 LibreOffice、Microsoft Word 或 WPS,请先安装其一".into()
            };
            Ok(OfficeSelftestOutput {
                ok,
                libreoffice: s.libreoffice,
                word_com: s.word_com,
                wps_com: s.wps_com,
                wps_cli: s.wps_cli,
                message,
            })
        }
        Err(e) => Ok(OfficeSelftestOutput {
            ok: false,
            libreoffice: None,
            word_com: false,
            wps_com: false,
            wps_cli: None,
            message: e.to_string(),
        }),
    }
}

// ===== 批量转换 =====

#[derive(Deserialize)]
pub struct WordToPdfInput {
    pub inputs: Vec<PathBuf>,
    pub output_dir: PathBuf,
    /// "high" | "medium" | "low"
    pub quality: String,
    pub task_id: Option<String>,
}

/// 成功条目(源路径, PDF 路径)
#[derive(Serialize)]
pub struct SucceededItem {
    pub input: String,
    pub output: String,
}

/// 失败条目(源路径, 失败原因)
#[derive(Serialize)]
pub struct FailedItem {
    pub input: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct WordConvertReportOutput {
    pub engine: String,
    pub succeeded: Vec<SucceededItem>,
    pub failed: Vec<FailedItem>,
    pub elapsed_ms: u128,
    pub output_dir: PathBuf,
}

impl WordConvertReportOutput {
    fn from_report(r: WordConvertReport) -> Self {
        WordConvertReportOutput {
            engine: r.engine,
            succeeded: r
                .succeeded
                .into_iter()
                .map(|(input, output)| SucceededItem { input, output })
                .collect(),
            failed: r
                .failed
                .into_iter()
                .map(|(input, reason)| FailedItem { input, reason })
                .collect(),
            elapsed_ms: r.elapsed_ms,
            output_dir: r.output_dir,
        }
    }
}

#[tauri::command]
pub async fn word_to_pdf(
    app: AppHandle,
    input: WordToPdfInput,
) -> CmdResult<WordConvertReportOutput> {
    let quality = WordQuality::parse(&input.quality).map_err(|e| AppError {
        code: "INVALID_ARG".into(),
        message: e.to_string(),
    })?;
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = input.inputs;
    let output_dir = input.output_dir;

    let app2 = app.clone();
    let tid = task_id.clone();
    let report = tokio::task::spawn_blocking(move || {
        // 文件级进度:每个文件完成时转发为 task-progress
        let cb = move |current: u32, total: u32| {
            emit_progress(&app2, &tid, "word-to-pdf", current, total);
        };
        pdf_docvault_core::office::word_to_pdf(
            &inputs,
            &output_dir,
            quality,
            1800,
            Some(cb),
        )
    })
    .await
    .map_err(|e| AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;

    emit_complete(&app, &task_id, "word-to-pdf", "done");
    Ok(WordConvertReportOutput::from_report(report))
}
