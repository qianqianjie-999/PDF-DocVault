//! PDF-DocVault 桌面端 Tauri 后端入口
//!
//! 注册插件、初始化 AppState、挂载所有 Tauri Commands。

mod commands;
mod error;
mod state;

use commands::{document_commands, ocr_commands, office_commands, pdf_commands, scan_commands, search_commands, system_commands, task_commands, user_commands};
use state::AppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,pdf_docvault_core=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data)?;
            let db_path = app_data.join("docvault.db");
            tracing::info!("数据库路径: {}", db_path.display());
            let db = pdf_docvault_core::storage::Database::open(&db_path)?;

            // 初始化 Tantivy 全文搜索索引:失败自动清空重建;再失败则降级为无搜索
            let index_dir = app_data.join("tantivy_index");
            let search = match pdf_docvault_core::search::TantivySearch::open(&index_dir) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("打开搜索索引失败,尝试自动重建: {e}");
                    let _ = std::fs::remove_dir_all(&index_dir);
                    match pdf_docvault_core::search::TantivySearch::open(&index_dir) {
                        Ok(s) => match rebuild_search_index(&db, &s) {
                            Ok(n) => {
                                tracing::info!("搜索索引已重建,共 {n} 条文档");
                                Some(s)
                            }
                            Err(e) => {
                                tracing::warn!("搜索索引重建失败: {e}");
                                Some(s) // 索引可用但为空,后续导入会增量填充
                            }
                        },
                        Err(e2) => {
                            tracing::error!("搜索索引初始化失败,搜索功能降级: {e2}");
                            None
                        }
                    }
                }
            };
            app.manage(AppState::new(db, search.map(Arc::new)));

            // 设置 pdfium 库路径(资源目录)
            let resource_dir = app.path().resource_dir()?;
            let pdfium_dir = resource_dir.join("resources").join("pdfium");
            if pdfium_dir.exists() {
                std::env::set_var("PDFIUM_PATH", &pdfium_dir);
                tracing::info!("pdfium 路径: {}", pdfium_dir.display());
            } else {
                tracing::warn!("未找到 pdfium 资源目录: {}", pdfium_dir.display());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // PDF 操作
            pdf_commands::merge_pdfs,
            pdf_commands::split_pdf,
            pdf_commands::rotate_pages,
            pdf_commands::delete_pages,
            pdf_commands::insert_pages,
            pdf_commands::extract_pages,
            pdf_commands::encrypt_pdf,
            pdf_commands::decrypt_pdf,
            pdf_commands::add_watermark,
            pdf_commands::compress_pdf,
            pdf_commands::pdf_to_images,
            pdf_commands::pdf_page_count,
            // 文档管理
            document_commands::list_documents,
            document_commands::delete_document,
            document_commands::tag_document,
            document_commands::import_document,
            search_commands::search_documents,
            // 用户/激活
            user_commands::check_activation,
            user_commands::activate,
            user_commands::get_donation_qrcode,
            user_commands::get_device_info,
            user_commands::dev_generate_license,
            // 文档扫描
            scan_commands::scan_enhance_images,
            scan_commands::scan_images_to_pdf,
            scan_commands::images_to_a4_pdf_cmd,
            scan_commands::id_cards_to_pdf,
            scan_commands::remove_handwriting,
            // OCR 识别
            ocr_commands::ocr_selftest,
            ocr_commands::ocr_extract_images,
            ocr_commands::ocr_convert_documents,
            // Word → PDF
            office_commands::office_selftest,
            office_commands::word_to_pdf,
            // 任务
            task_commands::get_task_status,
            task_commands::cancel_task,
            // 系统交互
            system_commands::open_path,
            system_commands::reveal_path
        ])
        .run(tauri::generate_context!())
        .expect("运行 PDF-DocVault 时发生错误");
}

/// 从 SQLite 重建搜索索引(索引损坏/schema 版本升级后自动触发)
fn rebuild_search_index(
    db: &pdf_docvault_core::storage::Database,
    search: &pdf_docvault_core::search::TantivySearch,
) -> pdf_docvault_core::Result<usize> {
    let rows: Vec<(i64, String, String, Option<String>)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, file_type, tags FROM documents WHERE status != 'deleted'",
        )?;
        let it = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(it.collect::<std::result::Result<Vec<_>, _>>()?)
    })?;
    let n = rows.len();
    for (id, name, ft, tags) in &rows {
        search.index_document(*id, name, ft, tags.as_deref())?;
    }
    Ok(n)
}
