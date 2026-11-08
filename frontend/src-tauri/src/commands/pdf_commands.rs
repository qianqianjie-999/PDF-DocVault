//! PDF 操作命令
//!
//! 同步的 lopdf 操作通过 `tokio::task::spawn_blocking` 在独立线程执行,
//! 避免阻塞 Tauri 的异步运行时。长任务(转图片)通过 Tauri Events 推送进度。

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::error::CmdResult;
use crate::state::AppState;
use pdf_docvault_core::pdf::{self, EncryptOptions, SplitMode, WatermarkOptions};

/// 进度事件 payload
#[derive(Debug, Clone, Serialize)]
pub struct ProgressPayload {
    pub task_id: String,
    pub kind: String,
    pub current: u32,
    pub total: u32,
    pub percent: u32,
}

/// 完成事件 payload
#[derive(Debug, Clone, Serialize)]
pub struct CompletePayload {
    pub task_id: String,
    pub kind: String,
    pub status: String,
}

/// 失败事件 payload
#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    pub task_id: String,
    pub kind: String,
    pub message: String,
}

fn emit_progress(app: &AppHandle, task_id: &str, kind: &str, current: u32, total: u32) {
    let percent = if total > 0 {
        (current * 100) / total
    } else {
        0
    };
    let _ = app.emit(
        "task-progress",
        ProgressPayload {
            task_id: task_id.to_string(),
            kind: kind.to_string(),
            current,
            total,
            percent,
        },
    );
}

fn emit_complete(app: &AppHandle, task_id: &str, kind: &str, status: &str) {
    let _ = app.emit(
        "task-complete",
        CompletePayload {
            task_id: task_id.to_string(),
            kind: kind.to_string(),
            status: status.to_string(),
        },
    );
}

fn emit_error(app: &AppHandle, task_id: &str, kind: &str, message: &str) {
    let _ = app.emit(
        "task-error",
        ErrorPayload {
            task_id: task_id.to_string(),
            kind: kind.to_string(),
            message: message.to_string(),
        },
    );
}

// ===== 合并 =====

#[derive(Deserialize)]
pub struct MergeInput {
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
    pub task_id: Option<String>,
}

#[derive(Serialize)]
pub struct MergeOutput {
    pub pages: u32,
    pub elapsed_ms: u128,
    pub output: PathBuf,
}

#[tauri::command]
pub async fn merge_pdfs(app: AppHandle, input: MergeInput) -> CmdResult<MergeOutput> {
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inputs = input.inputs.clone();
    let output = input.output.clone();
    let stats = tokio::task::spawn_blocking(move || pdf::merge::merge(&inputs, &output))
        .await
        .map_err(|e| crate::error::AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })??;
    emit_complete(&app, &task_id, "merge", "done");
    Ok(MergeOutput {
        pages: stats.pages,
        elapsed_ms: stats.elapsed_ms,
        output: input.output,
    })
}

// ===== 拆分 =====

#[derive(Deserialize)]
pub struct SplitInput {
    pub input: PathBuf,
    pub mode: SplitMode,
    pub out_dir: PathBuf,
    pub task_id: Option<String>,
}

#[derive(Serialize)]
pub struct SplitOutput {
    pub outputs: Vec<PathBuf>,
    pub elapsed_ms: u128,
}

#[tauri::command]
pub async fn split_pdf(app: AppHandle, input: SplitInput) -> CmdResult<SplitOutput> {
    let task_id = input.task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let inp = input.input.clone();
    let mode = input.mode.clone();
    let out_dir = input.out_dir.clone();
    let stats = tokio::task::spawn_blocking(move || pdf::split::split(&inp, &mode, &out_dir))
        .await
        .map_err(|e| crate::error::AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })??;
    emit_complete(&app, &task_id, "split", "done");
    Ok(SplitOutput {
        outputs: stats.paths,
        elapsed_ms: stats.elapsed_ms,
    })
}

// ===== 旋转 =====

#[derive(Deserialize)]
pub struct RotateInput {
    pub input: PathBuf,
    pub page_nums: Vec<u32>,
    pub degrees: u32,
    pub output: PathBuf,
}

#[tauri::command]
pub async fn rotate_pages(input: RotateInput) -> CmdResult<PathBuf> {
    let inp = input.input.clone();
    let nums = input.page_nums.clone();
    let deg = input.degrees;
    let out = input.output.clone();
    tokio::task::spawn_blocking(move || pdf::pages::rotate_pages(&inp, &nums, deg, &out))
        .await
        .map_err(|e| crate::error::AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })??;
    Ok(input.output)
}

// ===== 删除 =====

#[derive(Deserialize)]
pub struct DeletePagesInput {
    pub input: PathBuf,
    pub page_nums: Vec<u32>,
    pub output: PathBuf,
}

#[tauri::command]
pub async fn delete_pages(input: DeletePagesInput) -> CmdResult<PathBuf> {
    let inp = input.input.clone();
    let nums = input.page_nums.clone();
    let out = input.output.clone();
    tokio::task::spawn_blocking(move || pdf::pages::delete_pages(&inp, &nums, &out))
        .await
        .map_err(|e| crate::error::AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })??;
    Ok(input.output)
}

// ===== 插入 =====

#[derive(Deserialize)]
pub struct InsertInput {
    pub input1: PathBuf,
    pub input2: PathBuf,
    pub after_page: u32,
    pub output: PathBuf,
}

#[tauri::command]
pub async fn insert_pages(input: InsertInput) -> CmdResult<PathBuf> {
    let i1 = input.input1.clone();
    let i2 = input.input2.clone();
    let after = input.after_page;
    let out = input.output.clone();
    tokio::task::spawn_blocking(move || pdf::pages::insert_pages(&i1, &i2, after, &out))
        .await
        .map_err(|e| crate::error::AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })??;
    Ok(input.output)
}

// ===== 重排 =====

#[derive(Deserialize)]
pub struct ReorderInput {
    pub input: PathBuf,
    /// 1-based 新页码顺序(必须覆盖全部页面且不重复)
    pub new_order: Vec<u32>,
    pub output: PathBuf,
}

#[tauri::command]
pub async fn reorder_pages(input: ReorderInput) -> CmdResult<PathBuf> {
    let inp = input.input.clone();
    let order = input.new_order.clone();
    let out = input.output.clone();
    tokio::task::spawn_blocking(move || pdf::pages::reorder_pages(&inp, &order, &out))
        .await
        .map_err(|e| crate::error::AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })??;
    Ok(input.output)
}

// ===== 提取 =====

#[derive(Deserialize)]
pub struct ExtractInput {
    pub input: PathBuf,
    pub page_nums: Vec<u32>,
    pub output: PathBuf,
}

#[tauri::command]
pub async fn extract_pages(input: ExtractInput) -> CmdResult<PathBuf> {
    let inp = input.input.clone();
    let nums = input.page_nums.clone();
    let out = input.output.clone();
    tokio::task::spawn_blocking(move || pdf::pages::extract_pages(&inp, &nums, &out))
        .await
        .map_err(|e| crate::error::AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })??;
    Ok(input.output)
}

// ===== 加密/解密 =====

#[derive(Deserialize)]
pub struct EncryptInput {
    pub input: PathBuf,
    pub output: PathBuf,
    pub options: EncryptOptions,
}

#[derive(Deserialize)]
pub struct DecryptInput {
    pub input: PathBuf,
    pub output: PathBuf,
    pub password: String,
}

#[derive(Deserialize)]
pub struct EncryptionInfoInput {
    pub input: PathBuf,
    #[serde(default)]
    pub password: String,
}

#[tauri::command]
pub async fn encrypt_pdf(input: EncryptInput) -> CmdResult<pdf::EncryptReport> {
    let inp = input.input.clone();
    let out = input.output.clone();
    let opts = input.options.clone();
    Ok(
        tokio::task::spawn_blocking(move || pdf::encrypt::encrypt_aes256(&inp, &out, &opts))
            .await
            .map_err(|e| crate::error::AppError {
                code: "JOIN".into(),
                message: e.to_string(),
            })??,
    )
}

#[tauri::command]
pub async fn decrypt_pdf(input: DecryptInput) -> CmdResult<pdf::DecryptReport> {
    let inp = input.input.clone();
    let out = input.output.clone();
    let pw = input.password.clone();
    Ok(
        tokio::task::spawn_blocking(move || pdf::encrypt::decrypt(&inp, &out, &pw))
            .await
            .map_err(|e| crate::error::AppError {
                code: "JOIN".into(),
                message: e.to_string(),
            })??,
    )
}

/// 探测 PDF 是否加密 / 是否需要密码,供界面决定显示「加密」还是「解密」
#[tauri::command]
pub async fn probe_pdf_encryption(input: EncryptionInfoInput) -> CmdResult<pdf::EncryptionInfo> {
    let inp = input.input.clone();
    let pw = input.password.clone();
    Ok(
        tokio::task::spawn_blocking(move || pdf::encrypt::probe_encryption(&inp, &pw))
            .await
            .map_err(|e| crate::error::AppError {
                code: "JOIN".into(),
                message: e.to_string(),
            })??,
    )
}

// ===== 水印 =====

#[derive(Deserialize)]
pub struct WatermarkInput {
    pub input: PathBuf,
    pub output: PathBuf,
    pub text: Option<String>,
    pub image_path: Option<PathBuf>,
    pub options: WatermarkOptions,
}

#[tauri::command]
pub async fn add_watermark(input: WatermarkInput) -> CmdResult<PathBuf> {
    let inp = input.input.clone();
    let out = input.output.clone();
    let opts = input.options.clone();
    let text = input.text.clone();
    let img = input.image_path.clone();
    tokio::task::spawn_blocking(move || -> pdf_docvault_core::Result<()> {
        if let Some(img_path) = img {
            pdf::watermark::add_image_watermark(&inp, &img_path, &opts, &out)
        } else {
            pdf::watermark::add_text_watermark(
                &inp,
                text.as_deref().unwrap_or("PDF-DocVault"),
                &opts,
                &out,
            )
        }
    })
    .await
    .map_err(|e| crate::error::AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;
    Ok(input.output)
}

// ===== 压缩 =====

#[derive(Deserialize)]
pub struct CompressInput {
    pub input: PathBuf,
    pub output: PathBuf,
    pub level: u8,
}

#[derive(Serialize)]
pub struct CompressOutput {
    pub output: PathBuf,
    pub original_size: u64,
    pub compressed_size: u64,
    pub ratio: f32,
    pub elapsed_ms: u128,
    /// 实际重编码的图片数(level 1..=3 恒为 0)
    pub images_reencoded: usize,
    /// true = 图片处理失败已回退为纯结构压缩
    pub fell_back: bool,
}

#[tauri::command]
pub async fn compress_pdf(input: CompressInput) -> CmdResult<CompressOutput> {
    let inp = input.input.clone();
    let out = input.output.clone();
    let level = input.level;
    let stats = tokio::task::spawn_blocking(move || pdf::compress::compress(&inp, &out, level))
        .await
        .map_err(|e| crate::error::AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })??;
    Ok(CompressOutput {
        output: input.output,
        original_size: stats.original_size,
        compressed_size: stats.compressed_size,
        ratio: stats.ratio,
        elapsed_ms: stats.elapsed_ms,
        images_reencoded: stats.images_reencoded,
        fell_back: stats.fell_back,
    })
}

// ===== 转图片 =====

#[derive(Deserialize)]
pub struct ToImagesInput {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub dpi: u32,
    pub format: Option<String>,
    /// 待导出页码(1-based,离散列表);None 或空 = 整本导出。
    /// 去重/排序/越界校验由 core 层 `render_all_pages` 统一处理。
    pub pages: Option<Vec<u32>>,
    pub task_id: String,
}

#[derive(Serialize)]
pub struct ToImagesOutput {
    pub outputs: Vec<PathBuf>,
    pub count: usize,
}

#[tauri::command]
pub async fn pdf_to_images(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ToImagesInput,
) -> CmdResult<ToImagesOutput> {
    let task_id = input.task_id.clone();
    let kind = pdf_docvault_core::task::TaskKind::ToImages;
    state.tasks.register(&task_id, kind, 0);
    state.tasks.start(&task_id);

    let fmt = match input.format.as_deref().unwrap_or("png") {
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        _ => ImageFormat::Png,
    };
    let inp = input.input.clone();
    let out_dir = input.out_dir.clone();
    let dpi = input.dpi;
    let pages = input.pages;
    let app_handle = app.clone();
    let tid = task_id.clone();
    let state_tasks = state.tasks.clone();

    let joined = tokio::task::spawn_blocking(move || {
        pdf::render::render_all_pages(
            &inp,
            &out_dir,
            dpi,
            fmt,
            pages.as_deref(),
            |current, total| {
                state_tasks.report(&tid, current, total);
                emit_progress(&app_handle, &tid, "to_images", current, total);
            },
        )
    })
    .await;

    // 失败时同步任务状态并广播事件,避免 TaskManager 永久停在 Running
    let results = match joined {
        Ok(Ok(paths)) => paths,
        Ok(Err(e)) => {
            let msg = e.to_string();
            state.tasks.fail(&task_id, &msg);
            emit_error(&app, &task_id, "to_images", &msg);
            return Err(e.into());
        }
        Err(e) => {
            let msg = e.to_string();
            state.tasks.fail(&task_id, &msg);
            emit_error(&app, &task_id, "to_images", &msg);
            return Err(crate::error::AppError {
                code: "JOIN".into(),
                message: msg,
            });
        }
    };

    state.tasks.complete(&task_id);
    emit_complete(&app, &task_id, "to_images", "done");
    Ok(ToImagesOutput {
        count: results.len(),
        outputs: results,
    })
}

// ===== 页数查询 =====

#[derive(Deserialize)]
pub struct PageCountInput {
    pub input: PathBuf,
}

#[derive(Serialize)]
pub struct PageCountOutput {
    pub count: u32,
}

#[tauri::command]
pub async fn pdf_page_count(input: PageCountInput) -> CmdResult<PageCountOutput> {
    let inp = input.input.clone();
    let count = tokio::task::spawn_blocking(move || -> pdf_docvault_core::Result<u32> {
        let doc = lopdf::Document::load(&inp)?;
        Ok(doc.get_pages().len() as u32)
    })
    .await
    .map_err(|e| crate::error::AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;
    Ok(PageCountOutput { count })
}

// ===== 单页预览 =====

#[derive(Deserialize)]
pub struct PreviewInput {
    pub input: PathBuf,
    /// 待预览页码(1-based)
    pub page: u32,
    #[serde(default = "default_preview_dpi")]
    pub dpi: u32,
}

fn default_preview_dpi() -> u32 {
    120
}

#[derive(Serialize)]
pub struct PreviewOutput {
    /// `data:image/png;base64,` 前缀的 data URI
    pub data: String,
    pub width: u32,
    pub height: u32,
    pub page: u32,
}

#[tauri::command]
pub async fn render_page_preview(input: PreviewInput) -> CmdResult<PreviewOutput> {
    let inp = input.input.clone();
    let page = input.page;
    let dpi = input.dpi;
    let out = tokio::task::spawn_blocking(move || -> pdf_docvault_core::Result<PreviewOutput> {
        let img = pdf::render::render_page_to_image(&inp, page.saturating_sub(1), dpi)?;
        let (width, height) = (img.width(), img.height());
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
            .map_err(|e| pdf_docvault_core::CoreError::Image(format!("编码预览图失败: {e}")))?;
        let data = format!("data:image/png;base64,{}", B64.encode(&buf));
        Ok(PreviewOutput {
            data,
            width,
            height,
            page,
        })
    })
    .await
    .map_err(|e| crate::error::AppError {
        code: "JOIN".into(),
        message: e.to_string(),
    })??;
    Ok(out)
}
