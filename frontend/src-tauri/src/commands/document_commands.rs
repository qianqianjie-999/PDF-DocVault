//! 文档管理命令:列表/删除/打标签/导入

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

/// 文档记录
#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: i64,
    pub name: String,
    pub file_path: String,
    pub file_type: String,
    pub file_size: i64,
    pub page_count: Option<i64>,
    pub status: String,
    pub tags: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub file_type: Option<String>,
    pub tag: Option<String>,
}

#[derive(Serialize)]
pub struct ListResult {
    pub items: Vec<Document>,
    pub total: u32,
}

#[tauri::command]
pub async fn list_documents(state: State<'_, AppState>, params: ListParams) -> CmdResult<ListResult> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 200);
    let offset = (page - 1) * page_size;

    let items: Vec<Document> = state.db.with_conn(|conn| {
        let mut sql = String::from(
            "SELECT id, name, file_path, file_type, file_size, page_count, status, tags, created_at, updated_at \
             FROM documents WHERE status != 'deleted'",
        );
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(t) = &params.file_type {
            sql.push_str(" AND file_type = ?");
            p.push(Box::new(t.clone()));
        }
        if let Some(tag) = &params.tag {
            sql.push_str(" AND tags LIKE ?");
            p.push(Box::new(format!("%\"{}\"%", tag)));
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        p.push(Box::new(page_size));
        p.push(Box::new(offset));

        let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(refs.as_slice(), |r| {
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
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })?;

    let total: u32 = state.db.with_conn(|conn| {
        let mut sql = String::from("SELECT COUNT(*) FROM documents WHERE status != 'deleted'");
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(t) = &params.file_type {
            sql.push_str(" AND file_type = ?");
            p.push(Box::new(t.clone()));
        }
        let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        let n: i64 = conn.query_row(&sql, refs.as_slice(), |r| r.get(0))?;
        Ok(n as u32)
    })?;

    Ok(ListResult { items, total })
}

#[derive(Deserialize)]
pub struct ImportParams {
    pub file_path: PathBuf,
    pub file_type: String,
    pub page_count: Option<i64>,
}

#[tauri::command]
pub async fn import_document(state: State<'_, AppState>, params: ImportParams) -> CmdResult<i64> {
    let path = params.file_path.clone();
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("未命名")
        .to_string();
    let size = std::fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(0);
    let now = chrono::Utc::now().timestamp();

    let id = state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO documents (name, file_path, file_type, file_size, page_count, status, created_at, updated_at) \
             VALUES (?,?,?,?,?,?,?,?)",
            params![name, path.to_string_lossy(), params.file_type, size, params.page_count, "active", now, now],
        )?;
        Ok(conn.last_insert_rowid())
    })?;

    // 实时同步搜索索引(失败仅告警,不阻断导入)
    if let Some(search) = &state.search {
        if let Err(e) = search.index_document(id, &name, &params.file_type, None) {
            tracing::warn!("搜索索引同步失败(导入): {e}");
        }
    }
    Ok(id)
}

#[derive(Deserialize)]
pub struct TagParams {
    pub id: i64,
    pub tags: String,
}

#[tauri::command]
pub async fn tag_document(state: State<'_, AppState>, params: TagParams) -> CmdResult<bool> {
    let now = chrono::Utc::now().timestamp();
    state.db.with_conn(|conn| {
        conn.execute(
            "UPDATE documents SET tags = ?, updated_at = ? WHERE id = ?",
            params![params.tags, now, params.id],
        )?;
        Ok(())
    })?;

    // 回查文件名/类型用于重索引
    let (name, file_type) = state.db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT name, file_type FROM documents WHERE id = ?",
            params![params.id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?)
    })?;

    if let Some(search) = &state.search {
        if let Err(e) = search.update_document(params.id, &name, &file_type, Some(&params.tags)) {
            tracing::warn!("搜索索引同步失败(标签): {e}");
        }
    }
    Ok(true)
}

#[derive(Deserialize)]
pub struct DeleteParams {
    pub id: i64,
    pub permanent: Option<bool>,
}

#[tauri::command]
pub async fn delete_document(state: State<'_, AppState>, params: DeleteParams) -> CmdResult<bool> {
    if params.permanent.unwrap_or(false) {
        state.db.with_conn(|conn| {
            conn.execute("DELETE FROM documents WHERE id = ?", params![params.id])?;
            Ok(())
        })?;
    } else {
        let now = chrono::Utc::now().timestamp();
        state.db.with_conn(|conn| {
            conn.execute(
                "UPDATE documents SET status = 'deleted', updated_at = ? WHERE id = ?",
                params![now, params.id],
            )?;
            Ok(())
        })?;
    }

    // 软删/硬删都从索引移除(软删后 list 不再返回,搜索也应一致)
    if let Some(search) = &state.search {
        if let Err(e) = search.remove_document(params.id) {
            tracing::warn!("搜索索引同步失败(删除): {e}");
        }
    }
    Ok(true)
}
