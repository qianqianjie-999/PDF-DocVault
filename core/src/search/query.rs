//! 关键词查询

use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{TantivyDocument, Term};

use super::index::TantivySearch;
use crate::errors::{CoreError, Result};

/// 单条搜索命中
#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub doc_id: i64,
    pub score: f32,
}

impl TantivySearch {
    /// 关键词搜索。空关键词返回空;`file_type` 有值时精确过滤;`limit` 夹取到 1..=200。
    ///
    /// 语义:分词后 token 间 AND;同一 token 在 name/tags 间 OR;BM25 相关性排序。
    /// 不用 QueryParser:对用户输入中的特殊字符(`:`/`AND` 等)不健壮,
    /// 手动分词 + TermQuery 组合行为完全确定。
    pub fn search(
        &self,
        keyword: &str,
        file_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<SearchHit>> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 200);

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let Some(kw_query) = self.keyword_query(keyword)? else {
            return Ok(Vec::new()); // 分词后无有效 token
        };
        clauses.push((Occur::Must, kw_query));
        if let Some(ft) = file_type.map(str::trim).filter(|s| !s.is_empty()) {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.sc.file_type, ft),
                    IndexRecordOption::Basic,
                )) as Box<dyn Query>,
            ));
        }
        let query = BooleanQuery::new(clauses);

        let searcher = self.reader.searcher();
        let top = searcher.search(&query, &TopDocs::with_limit(limit as usize))?;
        let mut hits = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc: TantivyDocument = searcher.doc(addr)?;
            let Some(id) = doc.get_first(self.sc.doc_id).and_then(|v| v.as_u64()) else {
                continue; // 理论不发生:doc_id 恒 STORED
            };
            hits.push(SearchHit {
                doc_id: id as i64,
                score,
            });
        }
        Ok(hits)
    }

    /// 关键词 → 查询:token 间 Must,同一 token 字段间 Should(name/tags)。
    /// 注意必须按 token 嵌套:扁平 all-Should 会被 tantivy 按"任一匹配"解释,丢失 AND 语义。
    fn keyword_query(&self, keyword: &str) -> Result<Option<Box<dyn Query>>> {
        let mut analyzer = self
            .index
            .tokenizers()
            .get("jieba")
            .ok_or_else(|| CoreError::Search("分词器未注册".into()))?;
        let mut stream = analyzer.token_stream(keyword);
        let mut token_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        while stream.advance() {
            let text = stream.token().text.trim();
            if text.is_empty() {
                continue;
            }
            // 同一 token 在 name/tags 间 OR(内层 Should,至少命中一个字段)
            let field_clauses: Vec<(Occur, Box<dyn Query>)> = vec![
                (
                    Occur::Should,
                    Box::new(TermQuery::new(
                        Term::from_field_text(self.sc.name, text),
                        IndexRecordOption::WithFreqsAndPositions,
                    )),
                ),
                (
                    Occur::Should,
                    Box::new(TermQuery::new(
                        Term::from_field_text(self.sc.tags, text),
                        IndexRecordOption::WithFreqsAndPositions,
                    )),
                ),
            ];
            token_clauses.push((Occur::Must, Box::new(BooleanQuery::new(field_clauses))));
        }
        if token_clauses.is_empty() {
            return Ok(None);
        }
        Ok(Some(Box::new(BooleanQuery::new(token_clauses))))
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
    fn empty_keyword_returns_empty() {
        let (_dir, s) = setup();
        s.index_document(1, "发票.pdf", "pdf", None).unwrap();
        assert!(s.search("", None, 10).unwrap().is_empty());
        assert!(s.search("   ", None, 10).unwrap().is_empty());
    }

    #[test]
    fn file_type_filter() {
        let (_dir, s) = setup();
        s.index_document(1, "报告.pdf", "pdf", None).unwrap();
        s.index_document(2, "报告.docx", "docx", None).unwrap();
        let hits = s.search("报告", Some("docx"), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, 2);
    }

    #[test]
    fn multiple_tokens_joined_with_and() {
        let (_dir, s) = setup();
        s.index_document(1, "2026采购发票.pdf", "pdf", None).unwrap();
        s.index_document(2, "设备安装指导.docx", "docx", None).unwrap();

        // 两个 token 分属不同文档:AND 语义下无命中
        // (若误改成扁平 all-Should 会命中两篇,此断言用于锁定嵌套 Must 结构)
        let cross = s.search("采购 指导", None, 10).unwrap();
        assert!(cross.is_empty(), "跨文档 token 不得 OR 命中");

        // 两个 token 同属 doc 1:仅命中 doc 1
        let hits = s.search("采购 发票", None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, 1);
    }

    #[test]
    fn data_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = TantivySearch::open(dir.path()).unwrap();
            s.index_document(7, "持久化测试.pdf", "pdf", None).unwrap();
        }
        let s2 = TantivySearch::open(dir.path()).unwrap();
        let hits = s2.search("持久化", None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, 7);
    }
}
