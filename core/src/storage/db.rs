//! SQLite 数据库封装
//!
//! `Database` 持有 `Mutex<Connection>`,通过 `with_conn` 闭包访问,保证线程安全。
//! 启动时开启 WAL 模式与外键约束,并执行迁移。

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

use crate::errors::{CoreError, Result};

/// 线程安全的 SQLite 数据库句柄
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// 打开数据库并执行迁移
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        super::schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 内存数据库(用于测试)
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        super::schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 在锁保护下访问连接
    pub fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let guard = self
            .conn
            .lock()
            .map_err(|_| CoreError::DbLockPoisoned)?;
        f(&guard)
    }
}
