//! Tantivy 索引生命周期与 Schema

use std::path::Path;
use std::sync::Mutex;

use tantivy::schema::{Schema, TextFieldIndexing, TextOptions, STORED, STRING};
use tantivy::tokenizer::{RemoveLongFilter, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter};

use crate::errors::Result;

/// 索引 Schema 与字段句柄
pub struct SearchSchema {
    pub schema: Schema,
    /// documents 表主键(STORED,搜索命中后回查 SQLite)
    pub doc_id: tantivy::schema::Field,
    /// 文件名(jieba 分词索引,不存储)
    pub name: tantivy::schema::Field,
    /// 标签(jieba 分词索引,不存储)
    pub tags: tantivy::schema::Field,
    /// 文件类型(raw 精确匹配)
    pub file_type: tantivy::schema::Field,
}

/// Tantivy 搜索引擎句柄(索引/读/写一体,单进程使用)
///
/// writer 用 Mutex 包裹:tantivy 0.22 的 `commit` 需要 `&mut self`,
/// 而本句柄以 `&self` 提供 API 并将放入 `Arc` 跨命令共享,故用内部可变性,
/// 同时天然序列化多个命令的并发写。
#[allow(dead_code)]
pub struct TantivySearch {
    index: Index,
    pub(crate) reader: IndexReader,
    pub(crate) writer: Mutex<IndexWriter>,
    pub sc: SearchSchema,
}

impl SearchSchema {
    fn build() -> Self {
        let mut builder = Schema::builder();
        let doc_id = builder.add_u64_field("doc_id", STORED);
        // jieba 分词 + 带词频位置索引;不存储正文,结果回查 SQLite
        let text_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("jieba")
                .set_index_option(tantivy::schema::IndexRecordOption::WithFreqsAndPositions),
        );
        let name = builder.add_text_field("name", text_options.clone());
        let tags = builder.add_text_field("tags", text_options);
        let file_type = builder.add_text_field("file_type", STRING);
        Self {
            schema: builder.build(),
            doc_id,
            name,
            tags,
            file_type,
        }
    }
}

impl TantivySearch {
    /// 打开(或首次创建)索引目录。目录损坏由调用方清空后重试。
    pub fn open(dir: &Path) -> Result<Self> {
        let sc = SearchSchema::build();
        let index = if dir.join("meta.json").exists() {
            Index::open_in_dir(dir)?
        } else {
            std::fs::create_dir_all(dir)?;
            Index::create_in_dir(dir, sc.schema.clone())?
        };
        // 注册 jieba 中文分词器(QueryParser/手动分词均按名取用)
        let jieba = TextAnalyzer::builder(tantivy_jieba::JiebaTokenizer)
            .filter(RemoveLongFilter::limit(40))
            .build();
        index.tokenizers().register("jieba", jieba);

        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        // 50MB 写入堆预算:单机低频 CRUD 足够
        let writer = Mutex::new(index.writer(50_000_000)?);
        Ok(Self {
            index,
            reader,
            writer,
            sc,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_then_reopens() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _s = TantivySearch::open(dir.path()).unwrap();
        }
        // 目录已含 meta.json,二次打开走 open_in_dir 分支
        let s2 = TantivySearch::open(dir.path()).unwrap();
        assert_eq!(s2.sc.schema.num_fields(), 4);
    }
}
