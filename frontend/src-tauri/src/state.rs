//! 应用全局状态

use pdf_docvault_core::search::TantivySearch;
use pdf_docvault_core::storage::Database;
use pdf_docvault_core::task::TaskManager;
use std::sync::Arc;

/// 注入到 Tauri 的全局状态
pub struct AppState {
    pub db: Arc<Database>,
    /// 全文搜索索引;None = 索引初始化失败,搜索功能降级
    pub search: Option<Arc<TantivySearch>>,
    pub tasks: TaskManager,
}

impl AppState {
    pub fn new(db: Database, search: Option<Arc<TantivySearch>>) -> Self {
        Self {
            db: Arc::new(db),
            search,
            tasks: TaskManager::new(),
        }
    }
}
