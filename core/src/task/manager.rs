//! 任务状态机与进度上报

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 任务种类
#[derive(Debug, Clone, Copy, serde::Serialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Merge,
    Split,
    Encrypt,
    Watermark,
    Compress,
    ToImages,
    Rotate,
    Delete,
    Insert,
    Extract,
    Reorder,
}

/// 任务状态
#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

/// 任务状态快照
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskStatus {
    pub id: String,
    pub kind: TaskKind,
    pub state: TaskState,
    pub progress: u32, // 0..=100
    pub current: u32,
    pub total: u32,
    pub error: Option<String>,
    pub elapsed_ms: u128,
}

struct TaskEntry {
    kind: TaskKind,
    state: TaskState,
    progress: u32,
    current: u32,
    total: u32,
    error: Option<String>,
    started: Instant,
}

/// 任务管理器(线程安全)
#[derive(Clone)]
pub struct TaskManager {
    inner: Arc<Mutex<HashMap<String, TaskEntry>>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册新任务
    pub fn register(&self, id: &str, kind: TaskKind, total: u32) {
        let mut map = self.inner.lock().unwrap();
        map.insert(
            id.to_string(),
            TaskEntry {
                kind,
                state: TaskState::Pending,
                progress: 0,
                current: 0,
                total,
                error: None,
                started: Instant::now(),
            },
        );
    }

    /// 标记任务开始运行
    pub fn start(&self, id: &str) {
        if let Some(t) = self.inner.lock().unwrap().get_mut(id) {
            t.state = TaskState::Running;
        }
    }

    /// 上报进度
    pub fn report(&self, id: &str, current: u32, total: u32) {
        if let Some(t) = self.inner.lock().unwrap().get_mut(id) {
            t.current = current;
            t.total = total;
            t.progress = if total > 0 {
                (current as u32 * 100) / total
            } else {
                0
            };
        }
    }

    /// 标记完成
    pub fn complete(&self, id: &str) {
        if let Some(t) = self.inner.lock().unwrap().get_mut(id) {
            t.state = TaskState::Done;
            t.progress = 100;
        }
    }

    /// 标记失败
    pub fn fail(&self, id: &str, err: &str) {
        if let Some(t) = self.inner.lock().unwrap().get_mut(id) {
            t.state = TaskState::Failed;
            t.error = Some(err.to_string());
        }
    }

    /// 取消任务(仅标记状态,实际中断需业务侧轮询)
    pub fn cancel(&self, id: &str) {
        if let Some(t) = self.inner.lock().unwrap().get_mut(id) {
            t.state = TaskState::Cancelled;
        }
    }

    /// 查询是否已取消
    pub fn is_cancelled(&self, id: &str) -> bool {
        self.inner
            .lock()
            .unwrap()
            .get(id)
            .map(|t| t.state == TaskState::Cancelled)
            .unwrap_or(false)
    }

    /// 获取任务状态快照
    pub fn status(&self, id: &str) -> Option<TaskStatus> {
        self.inner.lock().unwrap().get(id).map(|t| TaskStatus {
            id: id.to_string(),
            kind: t.kind,
            state: t.state,
            progress: t.progress,
            current: t.current,
            total: t.total,
            error: t.error.clone(),
            elapsed_ms: t.started.elapsed().as_millis(),
        })
    }
}
