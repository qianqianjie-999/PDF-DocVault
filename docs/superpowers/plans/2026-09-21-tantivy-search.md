# Tantivy 全文搜索 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在文档管理页实现基于 Tantivy 的本地全文搜索（文件名 + 标签 + 文件类型），CRUD 实时同步索引。

**架构：** core 新增 `search` 模块（TantivySearch：open/同步写/查询），Tauri 命令层持有 `Option<Arc<TantivySearch>>`（None = 降级），文档 CRUD 命令在 SQL 之后直接调用同步方法；`search_documents` 命令搜索后回查 SQLite 拼装完整行。

**技术栈：** tantivy 0.22 + tantivy-jieba（中文分词）+ rusqlite + Vue3/Element Plus。

**规格：** `docs/superpowers/specs/2026-09-21-tantivy-search-design.md`

---

## 与规格的三处实现层修正（意图不变，方案更贴现状）

1. **同步注入点**：规格 §4 假设 CRUD 在 `core/src/storage/document.rs`，实际 CRUD 直接写在 Tauri 命令层（`frontend/src-tauri/src/commands/document_commands.rs` 裸 SQL）。因此**不引入 SearchSync trait**（单实现、无第二次使用者 → YAGNI），命令层 SQL 成功后直接调用 `TantivySearch` 方法，失败仅 `tracing::warn!` 降级不阻断。
2. **Schema 精简**：去掉 `created_at` 字段（v1 无排序需求）与 name/tags/file_type 的 STORED（搜索结果由 SQLite 回查完整行，索引不存正文）。仅 `doc_id` STORED。
3. **查询构造**：不使用 `QueryParser`（对特殊字符脆弱），改为**手动分词 + TermQuery 组合**：token 间 Must（AND），同一 token 在 name/tags 间 Should（OR），行为确定可测。

## 文件结构

| 文件 | 动作 | 职责 |
|---|---|---|
| `Cargo.toml` | 修改 | workspace 增加 tantivy/tantivy-jieba 版本锁定 |
| `core/Cargo.toml` | 修改 | core 引入两依赖 |
| `core/src/errors.rs` | 修改 | 新增 `CoreError::Search` 变体 + From\<TantivyError\> |
| `core/src/search/mod.rs` | 创建 | 模块入口与再导出 |
| `core/src/search/index.rs` | 创建 | Schema + TantivySearch::open（索引/reader/writer 生命周期） |
| `core/src/search/sync.rs` | 创建 | index/update/remove document（实时同步写） |
| `core/src/search/query.rs` | 创建 | 分词查询 + SearchHit |
| `core/src/lib.rs` | 修改 | `pub mod search` |
| `frontend/src-tauri/src/state.rs` | 修改 | AppState 增加 `search: Option<Arc<TantivySearch>>` |
| `frontend/src-tauri/src/commands/search_commands.rs` | 创建 | `search_documents` 命令 |
| `frontend/src-tauri/src/commands/document_commands.rs` | 修改 | import/tag/delete 三处同步钩子 |
| `frontend/src-tauri/src/commands/mod.rs` | 修改 | `pub mod search_commands` |
| `frontend/src-tauri/src/lib.rs` | 修改 | 初始化索引（含自动重建）+ 注册命令 |
| `frontend/src/types/document.ts` | 修改 | `SearchDocument` 类型 |
| `frontend/src/api/document.ts` | 修改 | `search` 方法 |
| `frontend/src/api/__tests__/search.spec.ts` | 创建 | 参数组装单测 |
| `frontend/src/views/desktop/DocumentListView.vue` | 修改 | 搜索框/类型筛选/防抖/高亮/结果切换 |

**验证环境**：core 与前端测试在宿主机跑（`cargo test -p pdf_docvault_core`、`pnpm test:unit`）；`pdf-docvault` crate 的 check 必须在 distrobox 容器 `tauri-dev` 内。

---

### 任务 1：依赖与错误变体

**文件：**
- 修改：`Cargo.toml`（workspace.dependencies，`# Tauri` 注释前插入）
- 修改：`core/Cargo.toml`
- 修改：`core/src/errors.rs:41`（`InvalidArgument` 变体前）与 `core/src/errors.rs:57`（code_string）

- [ ] **步骤 1：workspace 与 core 依赖**

`Cargo.toml` 的 `[workspace.dependencies]` 中，`# Tauri(仅 src-tauri 用)` 一行之前加入：

```toml
# 全文搜索(Tantivy 0.22 配套的 tantivy-jieba 为 0.11;若 cargo check 报版本冲突,以 docs.rs/crate/tantivy-jieba 查兼容表后调整)
tantivy = "0.22"
tantivy-jieba = "0.11"
```

`core/Cargo.toml` 的 `[dependencies]` 末尾（`uuid` 行后）加入：

```toml
# 全文搜索
tantivy = { workspace = true }
tantivy-jieba = { workspace = true }
```

- [ ] **步骤 2：errors.rs 新增 Search 变体**

在 `CoreError` 枚举的 `#[error("参数无效: {0}")] InvalidArgument(String),` 之前插入：

```rust
    #[error("搜索错误: {0}")]
    Search(String),
```

`code_string()` 的 match 中（`CoreError::PageNotFound` 分支旁）插入：

```rust
            CoreError::Search(_) => "SEARCH",
```

文件末尾（`impl CoreError` 块之后）追加 From 转换：

```rust
impl From<tantivy::TantivyError> for CoreError {
    fn from(e: tantivy::TantivyError) -> Self {
        CoreError::Search(e.to_string())
    }
}
```

- [ ] **步骤 3：宿主机验证编译（会自动拉取并编译 jieba，首次较慢）**

运行：`cargo check -p pdf_docvault_core`
预期：`Finished`，0 error。若 tantivy-jieba 与 tantivy 0.22 版本不匹配（编译报 trait/类型错误），按 docs.rs 兼容表把 workspace 中 tantivy-jieba 改为对应版本后重跑。

- [ ] **步骤 4：Commit**

```bash
git add Cargo.toml core/Cargo.toml core/src/errors.rs Cargo.lock
git commit -m "feat(search): 引入 tantivy 依赖与 Search 错误变体"
```

---

### 任务 2：search 模块骨架 + Schema + 索引打开

**文件：**
- 创建：`core/src/search/mod.rs`
- 创建：`core/src/search/index.rs`
- 修改：`core/src/lib.rs:21`（pybridge 与 storage 之间）

- [ ] **步骤 1：先写失败测试（index.rs 内嵌测试模块）**

创建 `core/src/search/index.rs`，只含测试与空实现骨架：

```rust
//! Tantivy 索引生命周期与 Schema

use std::path::Path;

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
pub struct TantivySearch {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    pub sc: SearchSchema,
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
```

同时创建占位 `core/src/search/mod.rs`：

```rust
//! 全文搜索模块(基于 Tantivy + jieba 中文分词)
//!
//! 索引对象:文档元数据(文件名/标签/文件类型)。
//! CRUD 同步由 Tauri 命令层在 SQL 成功后调用 [`TantivySearch`] 对应方法完成。

mod index;
mod query;
mod sync;

pub use index::{SearchSchema, TantivySearch};
pub use query::SearchHit;
```

在 `core/src/lib.rs` 的 `pub mod pybridge;` 之后、`pub mod storage;` 之前插入：

```rust
pub mod search;
```

并同步更新 lib.rs 顶部模块划分注释，在 `storage` 行前加一行：

```rust
//! - [`search`]: Tantivy 全文搜索(文档元数据索引)
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cargo test -p pdf_docvault_core search:: 2>&1 | tail -5`
预期：编译失败，报 `query` 与 `sync` 模块不存在（mod.rs 已声明）。先建两个空桩文件让编译通过测试仍失败：

`core/src/search/sync.rs`：
```rust
//! 索引实时同步:文档增/改/删(任务 3 实现)
```

`core/src/search/query.rs`：
```rust
//! 关键词查询(任务 4 实现)
```

再运行：`cargo test -p pdf_docvault_core search 2>&1 | tail -5`
预期：FAIL，`SearchSchema::build`、`TantivySearch::open` 未定义。

- [ ] **步骤 3：实现 build_schema 与 open**

在 `index.rs` 的 `impl SearchSchema` 与 `impl TantivySearch`（追加到结构体定义后）：

```rust
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
        let jieba = TextAnalyzer::builder(tantivy_jieba::ChineseTokenizer)
            .filter(RemoveLongFilter::limit(40))
            .build();
        index.tokenizers().register("jieba", jieba);

        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::OnCommit)
            .try_into()?;
        // 50MB 写入堆预算:单机低频 CRUD 足够
        let writer = index.writer(50_000_000)?;
        Ok(Self {
            index,
            reader,
            writer,
            sc,
        })
    }
}
```

- [ ] **步骤 4：运行测试确认通过**

运行：`cargo test -p pdf_docvault_core search 2>&1 | tail -5`
预期：`open_creates_then_reopens ... ok`，1 passed。

- [ ] **步骤 5：Commit**

```bash
git add core/src/search core/src/lib.rs
git commit -m "feat(search): Tantivy Schema 与索引打开/创建"
```

---

### 任务 3：实时同步写入（index/update/remove）

**文件：**
- 修改：`core/src/search/sync.rs`
- 修改：`core/src/search/index.rs`（open 后追加 reader.reload 说明无需改动——reload 在 sync 内调用）

- [ ] **步骤 1：先写失败测试**

替换 `core/src/search/sync.rs` 全文为：

```rust
//! 索引实时同步:文档增/改/删
//!
//! 每次 commit 后显式 reader.reload(),保证"写后立即可读"
//! (ReloadPolicy::OnCommit 是后台异步重载,存在窗口期)。

use tantivy::Term;

use super::index::TantivySearch;
use crate::errors::Result;

impl TantivySearch {
    /// 索引一篇文档(同 id 已存在时整体替换,天然幂等)
    pub fn index_document(
        &self,
        id: i64,
        name: &str,
        file_type: &str,
        tags: Option<&str>,
    ) -> Result<()> {
        self.writer
            .delete_term(Term::from_field_u64(self.sc.doc_id, id as u64));
        let mut d = tantivy::doc!(
            self.sc.doc_id => id as u64,
            self.sc.name => name,
            self.sc.file_type => file_type,
        );
        if let Some(t) = tags {
            d.add_text(self.sc.tags, t);
        }
        self.writer.add_document(d)?;
        self.writer.commit()?;
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
        self.writer
            .delete_term(Term::from_field_u64(self.sc.doc_id, id as u64));
        self.writer.commit()?;
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
```

注意：`update_replaces_old_terms` 中查询词与文档名用同一分词器，无论 jieba 把"旧文件"切成"旧/文件"还是"旧文件"，query 与 doc 切分一致，断言稳定。

- [ ] **步骤 2：运行测试确认失败**

运行：`cargo test -p pdf_docvault_core search 2>&1 | tail -8`
预期：编译失败，`TantivySearch` 没有 `index_document` 方法（且 `search` 未定义）。

- [ ] **步骤 3：实现 query 骨架使本任务可测**

`search` 属于任务 4，但本任务测试依赖它。在 `core/src/search/query.rs` 先写最小可编译实现（完整版任务 4 再扩展）：

```rust
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
```

- [ ] **步骤 4：运行测试确认通过**

运行：`cargo test -p pdf_docvault_core search 2>&1 | tail -8`
预期：`index_then_search_chinese_name_and_tags` **仍 FAIL**（search 返回空）——这是预期中的"红"。`remove_is_idempotent` 中 `is_empty()` 断言恰好通过。此步骤确认测试基线：编译通过、2 个测试红/绿混合。

- [ ] **步骤 5：Commit（WIP 提交，任务 4 转绿）**

```bash
git add core/src/search
git commit -m "feat(search): 索引实时同步写入(index/update/remove)"
```

---

### 任务 4：分词查询实现

**文件：**
- 修改：`core/src/search/query.rs`（替换 search 最小实现 + 新增 keyword_query + 测试）

- [ ] **步骤 1：先写失败测试**

在 `query.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::TantivySearch;

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
```

- [ ] **步骤 2：运行确认失败**

运行：`cargo test -p pdf_docvault_core search 2>&1 | tail -8`
预期：新增 3 个测试中 `file_type_filter`、`data_survives_reopen` FAIL（search 恒空）。`empty_keyword` 恒过。

- [ ] **步骤 3：实现完整查询**

用以下实现**整体替换** `query.rs` 中 `impl TantivySearch` 块（保留 SearchHit 与测试模块）：

```rust
use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::Term;

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
            let doc = searcher.doc(addr)?;
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

    /// 关键词 → 查询:token 间 Must,同一 token 字段间 Should(name/tags)
    fn keyword_query(&self, keyword: &str) -> Result<Option<Box<dyn Query>>> {
        let analyzer = self
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
            token_clauses.push((
                Occur::Should,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.sc.name, text),
                    IndexRecordOption::WithFreqsAndPositions,
                )),
            ));
            token_clauses.push((
                Occur::Should,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.sc.tags, text),
                    IndexRecordOption::WithFreqsAndPositions,
                )),
            ));
        }
        if token_clauses.is_empty() {
            return Ok(None);
        }
        Ok(Some(Box::new(BooleanQuery::new(token_clauses))))
    }
}
```

- [ ] **步骤 4：运行全部 search 测试确认转绿**

运行：`cargo test -p pdf_docvault_core search 2>&1 | tail -10`
预期：7 个测试全过（index 1 + sync 3 + query 3）。

- [ ] **步骤 5：Commit**

```bash
git add core/src/search
git commit -m "feat(search): jieba 分词查询与类型过滤,写后立即可读"
```

---

### 任务 5：Tauri 状态与命令接入

**文件：**
- 修改：`frontend/src-tauri/src/state.rs`
- 创建：`frontend/src-tauri/src/commands/search_commands.rs`
- 修改：`frontend/src-tauri/src/commands/mod.rs:3`（模块声明区）
- 修改：`frontend/src-tauri/src/lib.rs`（setup 初始化 + 命令注册）
- 修改：`frontend/src-tauri/src/commands/document_commands.rs`（三处同步钩子）

- [ ] **步骤 1：AppState 增加搜索句柄**

替换 `state.rs` 全文：

```rust
//! 应用全局状态

use pdf_docvault_core::search::TantivySearch;
use pdf_docvault_core::storage::Database;
use pdf_docvault_core::task::TaskManager;
use std::sync::Arc;

/// 注入到 Tauri 的全局状态
pub struct AppState {
    pub db: Arc<Database>,
    /// 全文搜索索引;None = 索引初始化失败,搜索功能降级
    pub search: Option<Arc<TantivySearch>>,
    pub tasks: TaskManager,
}

impl AppState {
    pub fn new(db: Database, search: Option<Arc<TantivySearch>>) -> Self {
        Self {
            db: Arc::new(db),
            search,
            tasks: TaskManager::new(),
        }
    }
}
```

- [ ] **步骤 2：search_commands.rs**

创建 `frontend/src-tauri/src/commands/search_commands.rs`：

```rust
//! 文档搜索命令(Tantivy 全文检索)

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::document_commands::Document;
use crate::error::{AppError, CmdResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchParams {
    pub keyword: String,
    pub file_type: Option<String>,
    pub limit: Option<u32>,
}

/// 搜索结果 = 完整文档行 + BM25 得分(serde flatten 展平给前端)
#[derive(Serialize)]
pub struct SearchDocument {
    #[serde(flatten)]
    pub doc: Document,
    pub score: f32,
}

#[tauri::command]
pub async fn search_documents(
    state: State<'_, AppState>,
    params: SearchParams,
) -> CmdResult<Vec<SearchDocument>> {
    // 索引降级模式:返回空结果,不阻塞其他功能
    let Some(search) = state.search.clone() else {
        return Ok(Vec::new());
    };
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let keyword = params.keyword;
    let ft = params.file_type;

    let hits = tokio::task::spawn_blocking(move || search.search(&keyword, ft.as_deref(), limit))
        .await
        .map_err(|e| AppError {
            code: "JOIN".into(),
            message: e.to_string(),
        })?
        .map_err(|e| AppError {
            code: "SEARCH".into(),
            message: e.to_string(),
        })?;

    if hits.is_empty() {
        return Ok(Vec::new());
    }

    // 按 doc_id 回查 SQLite 完整行,再按 Tantivy 相关性顺序输出
    let ids = hits.iter().map(|h| h.doc_id).collect::<Vec<_>>();
    let placeholders = vec!["?"; ids.len()].join(",");
    let rows: Vec<Document> = state
        .db
        .with_conn(|conn| {
            let sql = format!(
                "SELECT id, name, file_path, file_type, file_size, page_count, status, tags, created_at, updated_at \
                 FROM documents WHERE id IN ({placeholders}) AND status != 'deleted'"
            );
            let mut stmt = conn.prepare(&sql)?;
            let it = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
                Ok(Document {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    file_path: r.get(2)?,
                    file_type: r.get(3)?,
                    file_size: r.get(4)?,
                    page_count: r.get(5)?,
                    status: r.get(6)?,
                    tags: r.get(7)?,
                    created_at: r.get(8)?,
                    updated_at: r.get(9)?,
                })
            })?;
            Ok(it.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .map_err(|e| AppError {
            code: "DB".into(),
            message: e.to_string(),
        })?;

    let mut by_id: std::collections::HashMap<i64, Document> =
        rows.into_iter().map(|d| (d.id, d)).collect();
    Ok(hits
        .iter()
        .filter_map(|h| {
            by_id.remove(&h.doc_id).map(|doc| SearchDocument {
                doc,
                score: h.score,
            })
        })
        .collect())
}
```

- [ ] **步骤 3：document_commands 三处同步钩子**

`import_document` 中，`let id = state.db.with_conn(...)?;` 之后、`Ok(id)` 之前插入：

```rust
    // 实时同步搜索索引(失败仅告警,不阻断导入)
    if let Some(search) = &state.search {
        if let Err(e) = search.index_document(id, &name, &params.file_type, None) {
            tracing::warn!("搜索索引同步失败(导入): {e}");
        }
    }
```

`tag_document` 中，`let now = ...` 之前插入回查，SQL 更新之后、`Ok(true)` 之前插入同步：

```rust
    // 回查文件名/类型用于重索引
    let (name, file_type) = state.db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT name, file_type FROM documents WHERE id = ?",
            params![params.id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?)
    })?;
```

```rust
    if let Some(search) = &state.search {
        if let Err(e) = search.update_document(params.id, &name, &file_type, Some(&params.tags)) {
            tracing::warn!("搜索索引同步失败(标签): {e}");
        }
    }
```

`delete_document` 中，`Ok(true)` 之前（if/else 之后）插入：

```rust
    // 软删/硬删都从索引移除(软删后 list 不再返回,搜索也应一致)
    if let Some(search) = &state.search {
        if let Err(e) = search.remove_document(params.id) {
            tracing::warn!("搜索索引同步失败(删除): {e}");
        }
    }
```

- [ ] **步骤 4：mod.rs 与 lib.rs 注册**

`commands/mod.rs` 模块声明区（`pub mod scan_commands;` 之后）插入：

```rust
pub mod search_commands;
```

`lib.rs`：
1. 顶部 `use commands::{...}` 列表中 `scan_commands,` 后加 `search_commands,`
2. `use std::sync::Arc;` 加入 use 区
3. `run()` 函数之后添加辅助函数：

```rust
/// 从 SQLite 重建搜索索引(索引损坏/schema 版本升级后自动触发)
fn rebuild_search_index(
    db: &pdf_docvault_core::storage::Database,
    search: &pdf_docvault_core::search::TantivySearch,
) -> pdf_docvault_core::Result<usize> {
    let rows: Vec<(i64, String, String, Option<String>)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, file_type, tags FROM documents WHERE status != 'deleted'",
        )?;
        let it = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(it.collect::<std::result::Result<Vec<_>, _>>()?)
    })?;
    let n = rows.len();
    for (id, name, ft, tags) in &rows {
        search.index_document(*id, name, ft, tags.as_deref())?;
    }
    Ok(n)
}
```

4. setup 闭包中，`let db = ...;` 之后、`app.manage(AppState::new(db));` 原行**替换**为：

```rust
            // 初始化 Tantivy 全文搜索索引:失败自动清空重建;再失败则降级为无搜索
            let index_dir = app_data.join("tantivy_index");
            let search = match pdf_docvault_core::search::TantivySearch::open(&index_dir) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("打开搜索索引失败,尝试自动重建: {e}");
                    let _ = std::fs::remove_dir_all(&index_dir);
                    match pdf_docvault_core::search::TantivySearch::open(&index_dir) {
                        Ok(s) => match rebuild_search_index(&db, &s) {
                            Ok(n) => {
                                tracing::info!("搜索索引已重建,共 {n} 条文档");
                                Some(s)
                            }
                            Err(e) => {
                                tracing::warn!("搜索索引重建失败: {e}");
                                Some(s) // 索引可用但为空,后续导入会增量填充
                            }
                        },
                        Err(e2) => {
                            tracing::error!("搜索索引初始化失败,搜索功能降级: {e2}");
                            None
                        }
                    }
                }
            };
            app.manage(AppState::new(db, search.map(Arc::new)));
```

5. `invoke_handler` 的 `document_commands::import_document,` 之后插入：

```rust
            search_commands::search_documents,
```

- [ ] **步骤 5：容器内编译验证**

运行：`distrobox enter tauri-dev -- bash -lc "cd /home/qianqianjie/PDF-DocVault && cargo check -p pdf-docvault 2>&1 | tail -5"`
预期：`Finished`，0 warning 0 error。

- [ ] **步骤 6：Commit**

```bash
git add frontend/src-tauri/src
git commit -m "feat(search): Tauri search_documents 命令与 CRUD 实时索引同步"
```

---

### 任务 6：前端类型 + API + 单测

**文件：**
- 修改：`frontend/src/types/document.ts`
- 修改：`frontend/src/api/document.ts`
- 创建：`frontend/src/api/__tests__/search.spec.ts`

- [ ] **步骤 1：类型**

`frontend/src/types/document.ts` 末尾追加：

```ts
/** 搜索结果行 = 完整文档 + BM25 得分 */
export interface SearchDocument extends Document {
  score: number
}
```

- [ ] **步骤 2：先写失败测试**

创建 `frontend/src/api/__tests__/search.spec.ts`：

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest'

// mock 底层 invoke,只验证参数组装(Tauri 命令参数为 snake_case)
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => []),
}))

import { invoke } from '@tauri-apps/api/core'
import { documentApi } from '@/api/document'

const mockedInvoke = vi.mocked(invoke)

describe('documentApi.search 参数组装', () => {
  beforeEach(() => {
    mockedInvoke.mockClear()
  })

  it('search 传递 keyword/file_type/limit,空值转 null', async () => {
    await documentApi.search('发票')
    expect(mockedInvoke).toHaveBeenCalledWith('search_documents', {
      params: { keyword: '发票', file_type: null, limit: null },
    })

    await documentApi.search('报告', 'docx', 20)
    expect(mockedInvoke).toHaveBeenCalledWith('search_documents', {
      params: { keyword: '报告', file_type: 'docx', limit: 20 },
    })
  })
})
```

- [ ] **步骤 3：运行确认失败**

运行：`cd frontend && pnpm test:unit -- search.spec 2>&1 | tail -5`
预期：FAIL，`documentApi.search is not a function`。

- [ ] **步骤 4：实现 API**

`frontend/src/api/document.ts`：首行 import 改为：

```ts
import { call } from './index'
import type { ListResult, SearchDocument } from '@/types/document'
```

`documentApi` 对象内（`list` 之前）加入：

```ts
  /** 全文搜索(文件名/标签/类型);fileType/limit 可选 */
  search: (keyword: string, fileType?: string, limit?: number) =>
    call<SearchDocument[]>('search_documents', {
      params: { keyword, file_type: fileType ?? null, limit: limit ?? null },
    }),
```

- [ ] **步骤 5：运行确认通过 + Commit**

运行：`cd frontend && pnpm test:unit -- search.spec 2>&1 | tail -5`
预期：1 passed。

```bash
git add frontend/src/types/document.ts frontend/src/api/document.ts frontend/src/api/__tests__/search.spec.ts
git commit -m "feat(search): 前端搜索 API 与类型"
```

---

### 任务 7：文档管理页搜索 UI

**文件：**
- 修改：`frontend/src/views/desktop/DocumentListView.vue`（全文替换）

- [ ] **步骤 1：替换 DocumentListView.vue 全文**

```vue
<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Search } from '@element-plus/icons-vue'
import PageHeader from '@/components/common/PageHeader.vue'
import { documentApi } from '@/api/document'
import { formatFileSize, formatTimestamp } from '@/utils/file'
import type { Document, SearchDocument } from '@/types/document'

const documents = ref<Document[]>([])
const total = ref(0)
const loading = ref(false)
const page = ref(1)
const pageSize = ref(20)

// ===== 全文搜索状态 =====
const keyword = ref('')
const fileType = ref('')
const searchResults = ref<SearchDocument[] | null>(null)
const searching = ref(false)
let searchTimer: ReturnType<typeof setTimeout> | null = null

const FILE_TYPES = ['pdf', 'doc', 'docx', 'png', 'jpg', 'jpeg']

// 展示行:搜索态显示命中结果,否则显示分页列表
const rows = computed(() => searchResults.value ?? documents.value)

async function doSearch() {
  const kw = keyword.value.trim()
  if (!kw && !fileType.value) {
    searchResults.value = null
    return
  }
  searching.value = true
  try {
    searchResults.value = await documentApi.search(kw, fileType.value || undefined, 100)
  } finally {
    searching.value = false
  }
}

/** 输入防抖 300ms */
function onKeywordInput() {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(doSearch, 300)
}

function onTypeChange() {
  if (keyword.value.trim() || fileType.value) void doSearch()
  else searchResults.value = null
}

function clearSearch() {
  keyword.value = ''
  fileType.value = ''
  searchResults.value = null
}

/** 文件名关键词高亮分段(大小写不敏感) */
function segments(name: string, kw: string): { text: string; hit: boolean }[] {
  if (!kw) return [{ text: name, hit: false }]
  const lower = name.toLowerCase()
  const k = kw.toLowerCase()
  const out: { text: string; hit: boolean }[] = []
  let i = 0
  for (;;) {
    const idx = lower.indexOf(k, i)
    if (idx < 0) {
      if (i < name.length) out.push({ text: name.slice(i), hit: false })
      break
    }
    if (idx > i) out.push({ text: name.slice(i, idx), hit: false })
    out.push({ text: name.slice(idx, idx + k.length), hit: true })
    i = idx + k.length
  }
  return out.length ? out : [{ text: name, hit: false }]
}

async function load() {
  loading.value = true
  try {
    const res = await documentApi.list({ page: page.value, page_size: pageSize.value })
    documents.value = res.items
    total.value = res.total
  } finally {
    loading.value = false
  }
}

async function remove(doc: Document) {
  try {
    await ElMessageBox.confirm(`确认删除文档「${doc.name}」?`, '确认', { type: 'warning' })
    await documentApi.delete(doc.id)
    ElMessage.success('已删除')
    // 搜索态下删除后刷新搜索结果,否则刷新列表
    if (searchResults.value) await doSearch()
    else await load()
  } catch {
    /* cancelled */
  }
}

async function editTags(doc: Document) {
  try {
    const { value } = await ElMessageBox.prompt('标签(JSON 数组,如 ["发票","2026"])', '编辑标签', {
      inputValue: doc.tags || '[]',
    })
    await documentApi.tag(doc.id, value || '[]')
    ElMessage.success('已更新')
    // 标签参与索引,搜索态下同步刷新
    if (searchResults.value) await doSearch()
    else await load()
  } catch {
    /* cancelled */
  }
}

onMounted(load)
</script>

<template>
  <div class="doc-list-view">
    <PageHeader title="文档管理" subtitle="本地文档分类管理 · 全文搜索" />

    <!-- 搜索栏 -->
    <div class="search-bar app-card">
      <el-input
        v-model="keyword"
        placeholder="搜索文件名 / 标签…"
        clearable
        :prefix-icon="Search"
        class="search-input"
        @input="onKeywordInput"
        @keyup.enter="doSearch"
        @clear="clearSearch"
      />
      <el-select
        v-model="fileType"
        placeholder="全部类型"
        clearable
        class="search-type"
        @change="onTypeChange"
      >
        <el-option v-for="t in FILE_TYPES" :key="t" :label="t.toUpperCase()" :value="t" />
      </el-select>
      <span v-if="searchResults" class="search-meta">
        找到 {{ searchResults.length }} 条
        <el-button text size="small" @click="clearSearch">清除搜索</el-button>
      </span>
    </div>

    <div class="app-card">
      <el-table :data="rows" v-loading="loading || searching" border>
        <el-table-column type="index" label="#" width="50" />
        <el-table-column prop="name" label="文件名" min-width="200" show-overflow-tooltip>
          <template #default="{ row }">
            <template v-for="(seg, si) in segments(row.name, keyword.trim())" :key="si">
              <mark v-if="seg.hit" class="hl">{{ seg.text }}</mark>
              <template v-else>{{ seg.text }}</template>
            </template>
          </template>
        </el-table-column>
        <el-table-column label="类型" width="80">
          <template #default="{ row }">
            <el-tag size="small">{{ row.file_type }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="大小" width="100">
          <template #default="{ row }">{{ formatFileSize(row.file_size) }}</template>
        </el-table-column>
        <el-table-column label="页数" width="80">
          <template #default="{ row }">{{ row.page_count ?? '-' }}</template>
        </el-table-column>
        <el-table-column label="标签" width="160">
          <template #default="{ row }">
            <span v-if="row.tags">{{ row.tags }}</span>
            <span v-else class="muted">-</span>
          </template>
        </el-table-column>
        <el-table-column label="创建时间" width="160">
          <template #default="{ row }">{{ formatTimestamp(row.created_at) }}</template>
        </el-table-column>
        <el-table-column label="操作" width="160" fixed="right">
          <template #default="{ row }">
            <el-button text size="small" @click="editTags(row as Document)">标签</el-button>
            <el-button text size="small" type="danger" @click="remove(row as Document)">删除</el-button>
          </template>
        </el-table-column>
      </el-table>
      <el-pagination
        v-if="!searchResults && total > pageSize"
        v-model:current-page="page"
        :page-size="pageSize"
        :total="total"
        layout="prev, pager, next"
        style="margin-top: 16px"
        @current-change="load"
      />
    </div>
  </div>
</template>

<style scoped lang="scss">
.muted {
  color: #c0c4cc;
}
.search-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
  .search-input {
    flex: 1;
    max-width: 360px;
  }
  .search-type {
    width: 130px;
  }
  .search-meta {
    font-size: 13px;
    color: #606266;
  }
}
.hl {
  background: #ffe58f;
  padding: 0 1px;
  border-radius: 2px;
}
</style>
```

- [ ] **步骤 2：前端验证**

运行：`cd frontend && pnpm typecheck 2>&1 | tail -3 && pnpm test:unit 2>&1 | tail -4 && pnpm build 2>&1 | tail -3`
预期：typecheck 0 错误；Vitest 全过（29+1）；build 成功。

- [ ] **步骤 3：Commit**

```bash
git add frontend/src/views/desktop/DocumentListView.vue
git commit -m "feat(search): 文档管理页全文搜索 UI(防抖/类型筛选/高亮)"
```

---

### 任务 8：全量回归 + 应用冒烟

- [ ] **步骤 1：宿主机全量测试**

运行：`cargo test -p pdf_docvault_core 2>&1 | tail -4`
预期：全部通过（原 26 + search 7 = 33+）。

- [ ] **步骤 2：容器全量 check**

运行：`distrobox enter tauri-dev -- bash -lc "cd /home/qianqianjie/PDF-DocVault && cargo check --workspace 2>&1 | tail -3"`
预期：0 warning 0 error。

- [ ] **步骤 3：重启应用冒烟**

```bash
pkill -f "pdf-docvaul[t]"; sleep 1
distrobox enter tauri-dev -- bash -lc "cd /home/qianqianjie/PDF-DocVault/frontend/src-tauri && WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 cargo run 2>&1"
```
后台运行后 `pgrep -af "pdf-docvaul[t]"` 确认启动。

- [ ] **步骤 4：手工验证清单（窗口内）**

1. 文档管理 → 导入一个 PDF（或用已有文档）
2. 搜索框输入文件名片段 → 300ms 后表格切换为命中结果，关键词黄色高亮
3. 类型下拉选 `PDF` → 立即按类型过滤
4. 给文档改标签 → 按标签词搜索应命中
5. 删除文档 → 搜索该文档关键词应不再出现
6. 清空搜索框 → 恢复分页列表

- [ ] **步骤 5：推送**

```bash
git push origin main
```

---

## 自检记录

- **规格覆盖度**：索引对象 §1.2（任务 2 Schema）、实时同步 §4（任务 3/5，注入点修正见文首）、查询 §5（任务 4）、命令 §5.2（任务 5，服务端一次拼装完整行代替前端二次查询）、前端 §6（任务 7）、降级 §8（任务 5 Option + 命令 None 分支）、索引损坏自动重建 §8（任务 5 setup 重建）、测试 §9（任务 2/3/4/6 core+前端；集成链路即 core 生命周期测试，SQLite+Tantivy 链在任务 8 手工冒烟覆盖）。
- **占位符扫描**：无 TODO/待定；任务 3 的 query 最小实现是显式声明的中间桩，任务 4 完整替换。
- **类型一致性**：`SearchHit{doc_id,score}` 定义于任务 3 桩、任务 4 使用一致；`TantivySearch::open(&Path)` 各任务一致；`SearchDocument{#[serde(flatten)] doc, score}` 与前端 `SearchDocument extends Document` 对齐（flatten 后 JSON 顶层即 Document 字段 + score）。
- **风险点**：tantivy-jieba 版本兼容（任务 1 步骤 3 有明确处置）；`TextAnalyzer::builder`/`Value::as_u64` 为 tantivy 0.22 API，若编译报 API 差异按编译器提示改为 `TextAnalyzer::from(...)`/`v.as_u64()` 变体即可，语义不变。
