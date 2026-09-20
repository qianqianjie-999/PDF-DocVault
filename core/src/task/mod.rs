//! 任务管理模块
//!
//! 跟踪长任务(如 PDF 转图片)的状态与进度,支持取消。短任务(合并/拆分)无需注册。

pub mod manager;

pub use manager::{TaskKind, TaskManager, TaskState, TaskStatus};
