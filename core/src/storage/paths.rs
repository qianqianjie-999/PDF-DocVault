//! 应用数据目录管理
//!
//! Tauri 在 `setup` 中通过 `app.path().app_data_dir()` 提供路径,core 层提供辅助函数
//! 用于在非 Tauri 上下文(测试、CLI)中定位默认数据目录。

use std::path::PathBuf;

use crate::errors::Result;

/// 默认数据目录:`~/.pdf-docvault/`(或平台等价目录)
pub fn default_data_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| crate::errors::CoreError::Internal("无法确定 HOME 目录".into()))?;
    Ok(home.join(".pdf-docvault"))
}

/// 默认缓存目录
pub fn default_cache_dir() -> Result<PathBuf> {
    Ok(default_data_dir()?.join("cache"))
}

/// 默认临时文件目录(任务处理用,完成后自动清理)
pub fn temp_dir() -> Result<PathBuf> {
    let dir = default_data_dir()?.join("tmp");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 生成唯一临时文件路径
pub fn temp_file_path(ext: &str) -> Result<PathBuf> {
    let dir = temp_dir()?;
    let name = format!(
        "{}.{}",
        uuid::Uuid::new_v4().simple().to_string(),
        ext.trim_start_matches('.')
    );
    Ok(dir.join(name))
}
