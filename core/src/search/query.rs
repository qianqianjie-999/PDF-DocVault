//! 关键词查询

use serde::Serialize;

use super::index::TantivySearch;
use crate::errors::Result;

/// 单条搜索命中
#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub doc_id: i64,
    pub score: f32,
}

impl TantivySearch {
    /// 关键词搜索(任务 4 完整实现;当前返回空,供同步测试编译)
    pub fn search(&self, _keyword: &str, _file_type: Option<&str>, _limit: u32) -> Result<Vec<SearchHit>> {
        Ok(Vec::new())
    }
}
