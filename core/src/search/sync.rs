//! 索引实时同步:文档增/改/删
//!
//! 每次 commit 后显式 reader.reload(),保证"写后立即可读"
//! (ReloadPolicy::OnCommitWithDelay 是后台异步重载,存在窗口期)。

use std::sync::MutexGuard;

use tantivy::doc;
use tantivy::Term;
use tantivy::IndexWriter;

use super::index::TantivySearch;
use crate::errors::{CoreError, Result};

impl TantivySearch {
    /// 加锁获取 writer(锁中毒映射为 Search 错误)
    fn lock_writer(&self) -> Result<MutexGuard<'_, IndexWriter>> {
        self.writer
            .lock()
            .map_err(|_| CoreError::Search("IndexWriter 锁中毒".into()))
    }

    /// 索引一篇文档(同 id 已存在时整体替换,天然幂等)
    pub fn index_document(
        &self,
        id: i64,
        name: &str,
        file_type: &str,
        tags: Option<&str>,
    ) -> Result<()> {
        {
            let mut w = self.lock_writer()?;
            w.delete_term(Term::from_field_u64(self.sc.doc_id, id as u64));
            let mut d = doc!(
                self.sc.doc_id => id as u64,
                self.sc.name => name,
                self.sc.file_type => file_type,
            );
            if let Some(t) = tags {
                d.add_text(self.sc.tags, t);
            }
            w.add_document(d)?;
            w.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    /// 更新 = 删除旧条目 + 重新索引
    pub fn update_document(
        &self,
        id: i64,
        name: &str,
        file_type: &str,
        tags: Option<&str>,
    ) -> Result<()> {
        self.index_document(id, name, file_type, tags)
    }

    /// 从索引移除文档(幂等,不存在时不报错)
    pub fn remove_document(&self, id: i64) -> Result<()> {
        {
            let mut w = self.lock_writer()?;
            w.delete_term(Term::from_field_u64(self.sc.doc_id, id as u64));
            w.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, TantivySearch) {
        let dir = tempfile::tempdir().unwrap();
        let s = TantivySearch::open(dir.path()).unwrap();
        (dir, s)
    }

    #[test]
    fn index_then_search_chinese_name_and_tags() {
        let (_dir, s) = setup();
        s.index_document(1, "2026采购发票.pdf", "pdf", None).unwrap();
        s.index_document(2, "安装fedora指导.docx", "docx", None).unwrap();

        let hits = s.search("发票", None, 10).unwrap();
        assert_eq!(hits.len(), 1, "应只命中发票文档");
        assert_eq!(hits[0].doc_id, 1);

        // 标签命中
        s.index_document(3, "扫描件.pdf", "pdf", Some("发票 报销")).unwrap();
        let hits2 = s.search("报销", None, 10).unwrap();
        assert_eq!(hits2.len(), 1);
        assert_eq!(hits2[0].doc_id, 3);
    }

    #[test]
    fn update_replaces_old_terms() {
        let (_dir, s) = setup();
        s.index_document(1, "旧文件名.pdf", "pdf", None).unwrap();
        s.update_document(1, "新文件名.pdf", "pdf", None).unwrap();
        assert!(s.search("旧文件", None, 10).unwrap().is_empty());
        assert_eq!(s.search("新文件", None, 10).unwrap()[0].doc_id, 1);
    }

    #[test]
    fn remove_is_idempotent() {
        let (_dir, s) = setup();
        s.index_document(1, "测试文档.pdf", "pdf", None).unwrap();
        s.remove_document(1).unwrap();
        s.remove_document(1).unwrap(); // 二次删除不应报错
        assert!(s.search("测试文档", None, 10).unwrap().is_empty());
    }
}
