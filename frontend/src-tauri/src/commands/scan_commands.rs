//! 文档扫描增强命令
//!
//! 纯 Rust 扫描流水线(边界检测/透视矫正/去阴影/二值化)的 IPC 层。
//! 批量任务通过 `tokio::task::spawn_blocking` 执行,逐张推送 `task-progress` 事件。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;
use uuid::Uuid;

use crate::commands::{emit_complete, emit_progress};
use crate::error::CmdResult;
use pdf_docvault_core::image::{
    id_cards_to_pdf as core_id_cards_to_pdf, images_to_a4_pdf, images_to_pdf,
    remove_handwriting as core_remove_handwriting, scan_enhance_batch, HandwritingOptions,
    IdCardOptions, ImagesToA4Options, ScanOptions,
};

// ===== 批量扫描增强 =====

#[derive(Deserialize)]
pub struct ScanInput {
    pub inputs: Vec<PathBuf>,
    pub out_dir: PathBuf,
    pub options: ScanOptions,
    pub task_id: Option<String>,
}

#[derive(Serialize)]
pub struct ScanOutput {
    pub outputs: Vec<PathBuf>,
    /// 检测到文档边界并完成矫正的图片数
    pub corrected: usize,
    pub elapsed_ms: u128,
}

#[tauri::command]
pub async fn scan_enhance_images(app: AppHandle, input: ScanInput) -> CmdResult<ScanOutput> {
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = input.inputs;
    let out_dir = input.out_dir;
    let options = input.options;
    let app2 = app.clone();
    let tid = task_id.clone();
    let stats = tokio::task::spawn_blocking(move || {
        let cb = move |current: u32, total: u32| {
            emit_progress(&app2, &tid, "scan", current, total);
        };
        scan_enhance_batch(&inputs, &out_dir, &options, Some(&cb))
    })
    .await
    .map_err(|e| crate::error::AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;

    emit_complete(&app, &task_id, "scan", "done");
    Ok(ScanOutput {
        outputs: stats.outputs,
        corrected: stats.corrected,
        elapsed_ms: stats.elapsed_ms,
    })
}

// ===== 多图合成 PDF =====

#[derive(Deserialize)]
pub struct ImagesToPdfInput {
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
    #[serde(default = "default_dpi")]
    pub dpi: f32,
    pub options: ScanOptions,
    pub task_id: Option<String>,
}

fn default_dpi() -> f32 {
    150.0
}

#[derive(Serialize)]
pub struct ImagesToPdfOutput {
    pub pages: u32,
    pub elapsed_ms: u128,
    pub output: PathBuf,
}

#[tauri::command]
pub async fn scan_images_to_pdf(
    app: AppHandle,
    input: ImagesToPdfInput,
) -> CmdResult<ImagesToPdfOutput> {
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = input.inputs;
    let output = input.output.clone();
    let dpi = input.dpi;
    let options = input.options;
    let app2 = app.clone();
    let tid = task_id.clone();
    let stats = tokio::task::spawn_blocking(move || {
        let cb = move |current: u32, total: u32| {
            emit_progress(&app2, &tid, "images_to_pdf", current, total);
        };
        images_to_pdf(&inputs, &output, dpi, &options, Some(&cb))
    })
    .await
    .map_err(|e| crate::error::AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;

    emit_complete(&app, &task_id, "images_to_pdf", "done");
    Ok(ImagesToPdfOutput {
        pages: stats.pages,
        elapsed_ms: stats.elapsed_ms,
        output: input.output,
    })
}

// ===== 多图 A4 排版合成 PDF =====

#[derive(Deserialize)]
pub struct ImagesToA4Input {
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
    #[serde(default)]
    pub options: ImagesToA4Options,
    pub task_id: Option<String>,
}

#[tauri::command]
pub async fn images_to_a4_pdf_cmd(
    app: AppHandle,
    input: ImagesToA4Input,
) -> CmdResult<ImagesToPdfOutput> {
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = input.inputs;
    let output = input.output.clone();
    let options = input.options;
    let app2 = app.clone();
    let tid = task_id.clone();
    let stats = tokio::task::spawn_blocking(move || {
        let cb = move |current: u32, total: u32| {
            emit_progress(&app2, &tid, "images_to_a4_pdf", current, total);
        };
        images_to_a4_pdf(&inputs, &output, &options, Some(&cb))
    })
    .await
    .map_err(|e| crate::error::AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;

    emit_complete(&app, &task_id, "images_to_a4_pdf", "done");
    Ok(ImagesToPdfOutput {
        pages: stats.pages,
        elapsed_ms: stats.elapsed_ms,
        output: input.output,
    })
}

// ===== 证件正反面排版 =====

#[derive(Deserialize)]
pub struct IdCardInput {
    pub front: PathBuf,
    pub back: PathBuf,
    pub output: PathBuf,
    #[serde(default)]
    pub options: IdCardOptions,
    pub task_id: Option<String>,
}

#[derive(Serialize)]
pub struct IdCardOutput {
    pub output: PathBuf,
    /// 标签是否绘制成功(false = 未找到 CJK 字体)
    pub labeled: bool,
    /// 单栏有效图片区域(mm)
    pub slot_size_mm: [f32; 2],
    pub elapsed_ms: u128,
}

#[tauri::command]
pub async fn id_cards_to_pdf(app: AppHandle, input: IdCardInput) -> CmdResult<IdCardOutput> {
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = vec![input.front, input.back];
    let output = input.output.clone();
    let options = input.options;
    let app2 = app.clone();
    let tid = task_id.clone();
    let stats = tokio::task::spawn_blocking(move || {
        let cb = move |current: u32, total: u32| {
            emit_progress(&app2, &tid, "idcard", current, total);
        };
        core_id_cards_to_pdf(&inputs, &output, &options, Some(&cb))
    })
    .await
    .map_err(|e| crate::error::AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;

    emit_complete(&app, &task_id, "idcard", "done");
    Ok(IdCardOutput {
        output: input.output,
        labeled: stats.labeled,
        slot_size_mm: stats.slot_size_mm,
        elapsed_ms: stats.elapsed_ms,
    })
}

// ===== 试卷去手写 =====

#[derive(Deserialize)]
pub struct HandwritingInput {
    pub inputs: Vec<PathBuf>,
    /// 清理结果输出目录(不存在自动创建)
    pub out_dir: PathBuf,
    /// 笔迹颜色(blue/red/green/purple/orange/cyan),空 = 默认蓝+红
    #[serde(default)]
    pub colors: Vec<String>,
    /// 掩膜膨胀次数,默认 2
    #[serde(default = "default_hw_dilate")]
    pub dilate: u32,
    pub task_id: Option<String>,
}

fn default_hw_dilate() -> u32 {
    2
}

#[derive(Serialize)]
pub struct HandwritingOutput {
    pub outputs: Vec<PathBuf>,
    /// 笔迹像素占比均值(0-1),0 = 未检测到笔迹
    pub mask_ratio: f32,
    pub images: usize,
    pub elapsed_ms: u128,
}

#[tauri::command]
pub async fn remove_handwriting(
    app: AppHandle,
    input: HandwritingInput,
) -> CmdResult<HandwritingOutput> {
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = input.inputs;
    let out_dir = input.out_dir;
    let options = HandwritingOptions {
        colors: input.colors,
        dilate: input.dilate,
        ..Default::default()
    };
    let app2 = app.clone();
    let tid = task_id.clone();
    let stats = tokio::task::spawn_blocking(move || {
        let cb = move |current: u32, total: u32| {
            emit_progress(&app2, &tid, "handwriting", current, total);
        };
        let pycore = pdf_docvault_core::ocr::resolve_pycore_dir()?;
        core_remove_handwriting(&inputs, &out_dir, &pycore, &options, Some(&cb))
    })
    .await
    .map_err(|e| crate::error::AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;

    emit_complete(&app, &task_id, "handwriting", "done");
    Ok(HandwritingOutput {
        outputs: stats.outputs,
        mask_ratio: stats.mask_ratio,
        images: stats.images,
        elapsed_ms: stats.elapsed_ms,
    })
}
