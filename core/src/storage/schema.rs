//! 数据库 schema 与迁移
//!
//! 采用简单线性迁移:`MIGRATIONS` 数组每项是一段 SQL,通过 `schema_version` 表跟踪当前版本。

use rusqlite::Connection;

use crate::errors::Result;

/// 迁移语句(按版本顺序)
pub const MIGRATIONS: &[&str] = &[
    // v1: 初始 schema —— 文档/任务/激活/设置
    r#"
    CREATE TABLE IF NOT EXISTS documents (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        name            TEXT NOT NULL,
        file_path       TEXT NOT NULL,
        file_type       TEXT NOT NULL,
        file_size       INTEGER NOT NULL,
        page_count      INTEGER,
        status          TEXT NOT NULL DEFAULT 'active',
        tags            TEXT,
        created_at      INTEGER NOT NULL,
        updated_at      INTEGER NOT NULL,
        sha256          TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_documents_created ON documents(created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_documents_type ON documents(file_type);

    CREATE TABLE IF NOT EXISTS tasks (
        id              TEXT PRIMARY KEY,
        kind            TEXT NOT NULL,
        input_paths     TEXT NOT NULL,
        output_path     TEXT,
        options         TEXT,
        status          TEXT NOT NULL DEFAULT 'pending',
        progress        INTEGER NOT NULL DEFAULT 0,
        error_message   TEXT,
        created_at      INTEGER NOT NULL,
        started_at      INTEGER,
        finished_at     INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);

    CREATE TABLE IF NOT EXISTS activations (
        id              INTEGER PRIMARY KEY,
        device_id       TEXT NOT NULL UNIQUE,
        license_blob    TEXT NOT NULL,
        activated_at    INTEGER NOT NULL,
        signature       TEXT NOT NULL,
        app_version     TEXT
    );

    CREATE TABLE IF NOT EXISTS app_settings (
        key             TEXT PRIMARY KEY,
        value           TEXT NOT NULL
    );
    "#,
];

/// 执行迁移,将数据库升级到最新版本
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (v INTEGER);")?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(v), 0) FROM schema_version",
        [],
        |r| r.get(0),
    )?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let v = (i + 1) as i64;
        if v > current {
            conn.execute_batch(sql)?;
            conn.execute("INSERT INTO schema_version (v) VALUES (?)", [v])?;
            tracing::info!("数据库迁移到版本 {}", v);
        }
    }
    Ok(())
}
