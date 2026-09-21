# Tantivy 全文搜索设计规格

> 日期: 2026-09-21
> 决策人: 产品负责人
> 状态: 已批准

---

## 1. 目标与范围

### 1.1 目标
在 PDF-DocVault 桌面端实现**本地全文搜索**能力，基于 Tantivy 搜索引擎。用户可在文档管理页通过关键词快速定位文档。

### 1.2 范围（已确认）

| 决策项 | 选择 | 来源 |
|---|---|---|
| 索引对象 | 仅文件名 + 标签 + 文件类型（元数据） | gap-analysis §6 #7 |
| 更新时机 | 实时同步（CRUD 时自动更新索引） | 讨论确认 |
| 搜索入口 | 文档管理页内嵌搜索框 | 讨论确认 |
| 不包含 | PDF 正文文本抽取、移动端、Web 端 | gap-analysis §6 + §8 |

### 1.3 非目标
- 不做 PDF 正文/内容的全文搜索（需要文本抽取引擎，工作量大且抽取质量不稳定）
- 不做 OCR 文字索引（同上，且搜索场景低频）
- 不做独立搜索页面（当前文档管理页内嵌足够）
- 不做搜索建议/拼写纠错/模糊匹配（Tantivy 默认精确匹配足够）

---

## 2. 架构设计

### 2.1 模块总览

```
frontend/src-tauri/src/
├── commands/
│   └── search_commands.rs       ← 新增: search_documents 命令
├── lib.rs                        ← 改动: 注册命令 + 初始化注入
└── state.rs                      ← 改动: 持有 Tantivy 实例

core/src/
├── search/                       ← 新增模块
│   ├── mod.rs                    ← 公开接口
│   ├── index.rs                  ← Tantivy Index 生命周期 + Schema
│   ├── sync.rs                   ← SearchSync trait + TantivySearch 实现
│   └── query.rs                  ← 查询解析 + 执行
├── lib.rs                        ← 改动: pub mod search
└── storage/
    ├── db.rs                     ← 改动: Database 持有 SearchSync, CRUD 回调
    └── document.rs               ← 改动: CRUD 方法触发回调
```

### 2.2 数据流

**写入路径（实时同步）：**
```
前端 → Tauri 命令(create_document/update_document/delete_document)
    → storage::document::xxx()
    → Database.with_conn() 执行 SQL
    → 调用 SearchSync 回调
    → TantivySearch 更新 IndexWriter
    → commit() 持久化
```

**读取路径：**
```
前端搜索框 → search_documents(keyword)
    → TantivySearch.search(keyword)
    → Tantivy IndexReader 查询
    → 返回 Vec<SearchHit>
    → 前端渲染 + 高亮
```

---

## 3. Tantivy Schema

### 3.1 字段定义

| 字段名 | 类型 | 选项 | 说明 |
|---|---|---|---|
| `doc_id` | STORED | 不索引 | documents 表主键，用于关联查询 |
| `name` | TEXT | tokenized(默认分词器), stored | 文件名，支持中文分词 |
| `tags` | TEXT | tokenized, stored | 标签 JSON 数组（存储为空格分隔字符串） |
| `file_type` | STRING | indexed, stored | 文件类型枚举，精确匹配 |
| `created_at` | FAST | 不索引 | 创建时间戳，用于排序 |

### 3.2 分词器

- 字段 `name` 和 `tags` 使用 **tantivy-jieba**（轻量中文分词）
- 字段 `file_type` 使用默认 STRING 类型（不分词，精确匹配）

### 3.3 索引存储

- 路径：`{db_dir}/tantivy_index/`
- `db_dir` = SQLite 数据库所在目录（`~/.local/share/com.pdfdocvault.app/`）
- 启动时检测目录是否存在：存在则打开，不存在则创建
- 关闭时自动 `commit()` 并释放锁

---

## 4. 实时同步机制

### 4.1 SearchSync Trait

```rust
// core/src/search/sync.rs
pub trait SearchSync: Send + Sync {
    fn on_document_create(
        &self,
        id: i64,
        name: &str,
        file_type: &str,
        tags: Option<&str>,
    );
    fn on_document_update(
        &self,
        id: i64,
        name: &str,
        tags: Option<&str>,
    );
    fn on_document_delete(&self, id: i64);
}
```

### 4.2 注入方式

`Database` 结构体新增 `search_sync: Option<Arc<dyn SearchSync>>` 字段：

```rust
pub struct Database {
    conn: Mutex<Connection>,
    search_sync: Option<Arc<dyn SearchSync>>,
}

impl Database {
    pub fn set_search_sync(&mut self, sync: Arc<dyn SearchSync>) {
        self.search_sync = Some(sync);
    }
}
```

### 4.3 CRUD 回调触发

在 `storage/document.rs` 的每个 CRUD 方法末尾，获取锁后调用对应回调：

```rust
pub fn create(db: &Database, name: &str, ...) -> Result<i64> {
    let id = db.with_conn(|conn| { /* SQL INSERT */ })?;
    // 触发搜索索引更新
    if let Some(sync) = &db.search_sync {
        sync.on_document_create(id, name, file_type, tags);
    }
    Ok(id)
}
```

### 4.4 批量重建（首次启动 / 版本升级）

提供 `rebuild_index(&Database)` 方法：
1. 清空 Tantivy 索引目录
2. 遍历 documents 表所有行
3. 逐行调用 `SearchSync::on_document_create`
4. commit

---

## 5. 查询接口

### 5.1 core 层

```rust
// core/src/search/query.rs
pub struct SearchHit {
    pub doc_id: i64,
    pub name: String,
    pub score: f32,
}

impl TantivySearch {
    pub fn search(
        &self,
        keyword: &str,
        filter_file_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<SearchHit>>;
}
```

- keyword 为空或空白 → 返回空结果（不做全量扫描，性能优先）
- filter_file_type 有值时在 Tantivy 查询中加 term filter
- limit 默认 50，上限 200
- score 为 Tantivy BM25 得分

### 5.2 Tauri 命令

```rust
#[tauri::command]
pub async fn search_documents(
    keyword: String,
    filter_file_type: Option<String>,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<SearchHit>>
```

返回前 50 条高分结果，前端拿到 doc_id 后再查 documents 表获取完整文档信息。

---

## 6. 前端设计

### 6.1 搜索框位置

在 `DocumentListView.vue` 的 `PageHeader` 下方、表格上方，加一行搜索区：

```
[🔍 搜索文档名/标签] [按类型 ▼] [搜索结果: 找到 3 条 / 共 42 条]
```

### 6.2 交互

- **输入触发**：输入后 300ms debounce 自动搜索（或回车）
- **清空**：点击清除图标恢复全量列表
- **类型筛选**：下拉框（全部 / PDF / DOC / 图片...）
- **高亮**：搜索结果中文件名匹配部分用 `<mark>` 黄色高亮

### 6.3 API 层

```typescript
// frontend/src/api/document.ts 新增
search: (keyword: string, fileType?: string, limit?: number) =>
  call<SearchHit[]>('search_documents', { keyword, file_type: fileType, limit }),
```

---

## 7. 依赖

```toml
# core/Cargo.toml
tantivy = "0.22"
tantivy-jieba = "0.5"  # 中文分词器
```

---

## 8. 错误处理

| 场景 | 处理方式 |
|---|---|
| Tantivy 索引目录不可写 | 搜索功能降级（返回空结果），不影响其他功能；日志告警 |
| 索引损坏（ Tantivy 版本升级后） | 自动检测 → 清空重建 → 重新索引所有 documents |
| 查询解析失败（特殊字符） | Tantivy 内部处理（忽略非法 token），返回空结果或部分结果 |
| 搜索时 Database 锁被占用 | Tantivy 独立读锁，不阻塞数据库操作 |

---

## 9. 测试策略

| 层级 | 测试 | 内容 |
|---|---|---|
| core 单测 | `cargo test -p pdf_docvault_core` | Schema 创建、中文分词效果、CRUD 同步、查询得分 |
| core 单测 | 空索引查询、损坏索引重建 | 边界条件 |
| 集成测试 | `cargo test -p pdf-docvault-tests` | create→search→delete→search 完整链路 |
| 前端 | Vitest | API 调用、debounce、高亮渲染 |

---

## 10. 工作量估算

| 模块 | 工作量 |
|---|---|
| core/search 模块（Schema + Index + SearchSync + Query） | 1.0 人日 |
| storage 模块改造（SearchSync trait + 注入） | 0.3 人日 |
| Tauri 命令层（state 初始化 + 命令注册） | 0.3 人日 |
| 前端（搜索框 + API + 高亮） | 0.5 人日 |
| 测试 + 调试 | 0.5 人日 |
| **合计** | **≈ 2.5 人日** |
