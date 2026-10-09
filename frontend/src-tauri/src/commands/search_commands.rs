//! 文档搜索命令(Tantivy 全文检索)

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::document_commands::Document;
use crate::error::{AppError, CmdResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchParams {
    pub keyword: String,
    pub file_type: Option<String>,
    pub limit: Option<u32>,
}

/// 搜索结果 = 完整文档行 + BM25 得分(serde flatten 展平给前端)
#[derive(Serialize)]
pub struct SearchDocument {
    #[serde(flatten)]
    pub doc: Document,
    pub score: f32,
}

#[tauri::command]
pub async fn search_documents(
    state: State<'_, AppState>,
    params: SearchParams,
) -> CmdResult<Vec<SearchDocument>> {
    // 索引降级模式:返回空结果,不阻塞其他功能
    let Some(search) = state.search.clone() else {
        return Ok(Vec::new());
    };
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let keyword = params.keyword;
    let ft = params.file_type;

    let hits = tokio::task::spawn_blocking(move || search.search(&keyword, ft.as_deref(), limit))
        .await
        .map_err(|e| AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })?
        .map_err(|e| AppError {
            code: "SEARCH".into(),
            message: e.to_string(),
        })?;

    if hits.is_empty() {
        return Ok(Vec::new());
    }

    // 按 doc_id 回查 SQLite 完整行,再按 Tantivy 相关性顺序输出
    let ids = hits.iter().map(|h| h.doc_id).collect::<Vec<_>>();
    let placeholders = vec!["?"; ids.len()].join(",");
    let rows: Vec<Document> = state
        .db
        .with_conn(|conn| {
            let sql = format!(
                "SELECT id, name, file_path, file_type, file_size, page_count, status, tags, created_at, updated_at \
                 FROM documents WHERE id IN ({placeholders}) AND status != 'deleted'"
            );
            let mut stmt = conn.prepare(&sql)?;
            let it = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
                Ok(Document {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    file_path: r.get(2)?,
                    file_type: r.get(3)?,
                    file_size: r.get(4)?,
                    page_count: r.get(5)?,
                    status: r.get(6)?,
                    tags: r.get(7)?,
                    created_at: r.get(8)?,
                    updated_at: r.get(9)?,
                })
            })?;
            Ok(it.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .map_err(|e| AppError {
            code: "DB".into(),
            message: e.to_string(),
        })?;

    let mut by_id: std::collections::HashMap<i64, Document> =
        rows.into_iter().map(|d| (d.id, d)).collect();
    Ok(hits
        .iter()
        .filter_map(|h| {
            by_id.remove(&h.doc_id).map(|doc| SearchDocument {
                doc,
                score: h.score,
            })
        })
        .collect())
}
