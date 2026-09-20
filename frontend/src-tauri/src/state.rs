//! 应用全局状态

use pdf_docvault_core::storage::Database;
use pdf_docvault_core::task::TaskManager;
use std::sync::Arc;

/// 注入到 Tauri 的全局状态
pub struct AppState {
    pub db: Arc<Database>,
    pub tasks: TaskManager,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self {
            db: Arc::new(db),
            tasks: TaskManager::new(),
        }
    }
}
