//! 任务查询/取消命令

use serde::Deserialize;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;
use pdf_docvault_core::task::TaskStatus;

#[derive(Deserialize)]
pub struct TaskIdParam {
    pub task_id: String,
}

#[tauri::command]
pub async fn get_task_status(
    state: State<'_, AppState>,
    param: TaskIdParam,
) -> CmdResult<Option<TaskStatus>> {
    Ok(state.tasks.status(&param.task_id))
}

#[tauri::command]
pub async fn cancel_task(state: State<'_, AppState>, param: TaskIdParam) -> CmdResult<bool> {
    state.tasks.cancel(&param.task_id);
    Ok(true)
}
