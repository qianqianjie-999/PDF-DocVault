//! 全文搜索模块(基于 Tantivy + jieba 中文分词)
//!
//! 索引对象:文档元数据(文件名/标签/文件类型)。
//! CRUD 同步由 Tauri 命令层在 SQL 成功后调用 [`TantivySearch`] 对应方法完成。

mod index;
mod query;
mod sync;

pub use index::{SearchSchema, TantivySearch};
pub use query::SearchHit;
