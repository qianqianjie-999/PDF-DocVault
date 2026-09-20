# PDF-DocVault 架构设计

## 1. 整体架构

```
┌─────────────────────────────────────────┐
│        前端展示层 (Vue 3 + Element Plus)        │
│   views / components / stores / api          │
└──────────────────┬──────────────────────┘
                   │ Tauri IPC (invoke / event)
┌──────────────────┴──────────────────────┐
│      Tauri Commands 层 (Rust, src-tauri)      │
│   pdf_commands / document_commands / ...     │
└──────────────────┬──────────────────────┘
                   │ 进程内调用(零开销)
┌──────────────────┴──────────────────────┐
│        核心处理层 (core crate, Rust)          │
│  pdf / image / storage / activation / task   │
└─────────────────────────────────────────┘
```

## 2. 分层说明

### 前端展示层
- Vue 3 + Pinia + Element Plus
- 通过 `@tauri-apps/api/core` 的 `invoke` 调用后端命令
- 通过 `@tauri-apps/api/event` 的 `listen` 接收进度事件
- Police Blue 主题(#1E5AA8)

### Tauri Commands 层
- `#[tauri::command] async fn`,同步操作用 `tokio::task::spawn_blocking` 包装
- 返回 `Result<T, AppError>`,AppError 序列化为 `{code, message}`
- 长任务通过 `app.emit("task-progress", ...)` 推送进度

### 核心处理层(core crate)
- `pdf`:lopdf(读写/合并/拆分/页面/水印)+ pdfium-render(转图片)+ qpdf(加密)
- `storage`:rusqlite(bundled SQLite)+ 线性迁移
- `activation`:machine-uid 设备指纹 + ed25519 离线验签
- `task`:任务状态机与进度跟踪

## 3. 关键设计决策

### 为什么 PDF 加密用 qpdf 而非 lopdf?
lopdf 0.34 仅支持解密读取,不支持写入加密。qpdf 是成熟的 C++ 库,支持 AES-256 R5/R6,规划书 §14.1 已列为后备。通过 `std::process::Command` 调用系统 qpdf,打包时随二进制分发。

### 为什么 PDF 转图片用 pdfium-render?
lopdf 只能解析结构,无法渲染位图。pdfium 是 Chrome 同款引擎,渲染质量最高,通过 `bblanchon/pdfium-binaries` 提供预编译库跨平台分发。

### 为什么激活用 ed25519 离线签名?
1 元捐赠后服务端用私钥签名 license,客户端用内置公钥验签。完全离线友好,私钥不进客户端,换机需重新激活。详见 [activation-design.md](activation-design.md)。

### 为什么用 Tauri 2.x 而非 Electron?
安装包 3-15MB(Electron 动辄 100MB+),Rust 原生后端零网络开销,内存占用低。

## 4. Cargo Workspace

```
workspace
├── core/                    # lib crate,被 src-tauri 依赖
└── frontend/src-tauri/      # bin crate,依赖 core
```

依赖版本在根 `Cargo.toml` 的 `[workspace.dependencies]` 统一锁定,各 crate 用 `{ workspace = true }` 引用。

## 5. 数据流示例:PDF 合并

```
用户拖入 3 个 PDF → FileDropZone → filesStore
  → OperationPanel 点击"开始处理"
  → pdfApi.merge(paths, output) → invoke('merge_pdfs')
  → Tauri: spawn_blocking(|| core::pdf::merge::merge(...))
  → lopdf: 并行加载 + 重新编号 + 合并对象 + compress + save
  → 返回 MergeStats → 前端 ElMessage.success
```
