//! Tauri 命令分组

pub mod document_commands;
pub mod ocr_commands;
pub mod office_commands;
pub mod pdf_commands;
pub mod scan_commands;
pub mod system_commands;
pub mod task_commands;
pub mod user_commands;

// ===== 任务事件辅助(task-progress / task-complete) =====

use serde_json::json;
use tauri::{AppHandle, Emitter};

pub(crate) fn emit_progress(app: &AppHandle, task_id: &str, kind: &str, current: u32, total: u32) {
    let percent = if total > 0 { (current * 100) / total } else { 0 };
    let _ = app.emit(
        "task-progress",
        json!({
            "task_id": task_id,
            "kind": kind,
            "current": current,
            "total": total,
            "percent": percent,
        }),
    );
}

pub(crate) fn emit_complete(app: &AppHandle, task_id: &str, kind: &str, status: &str) {
    let _ = app.emit(
        "task-complete",
        json!({ "task_id": task_id, "kind": kind, "status": status }),
    );
}
