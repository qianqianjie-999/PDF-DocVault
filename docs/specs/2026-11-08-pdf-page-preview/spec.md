# PDF 单页预览渲染

## 背景与缺口

排查发现一处「底层能力已就绪、但整条链路未串起来」的真实缺口——**PDF 单页预览**：

- 核心层 `core/src/pdf/render.rs` 已实现单页渲染 `render_page_to_bytes` / `render_page_to_image`，并由 `core/src/pdf/mod.rs:21` 导出；但全仓仅 `tests/rust/tests/pdf_flow.rs:855` 引用，属于死代码。
- 命令层 42 个 `#[tauri::command]` 中**没有任何「渲染单页为图片」命令**。
- 前端 [PdfPreview.vue](/home/qianqianjie/PDF-DocVault/frontend/src/components/pdf/PdfPreview.vue) 是占位桩：注释明示"此处仅展示文件信息占位,真实预览在 PdfToolsView 中按需渲染"，实际只做 `convertFileSrc(props.path)`，无法显示 PDF 画面，且该组件当前未被任何页面引用。
- 文档管理页 [DocumentListView.vue](/home/qianqianjie/PDF-DocVault/frontend/src/views/desktop/DocumentListView.vue) 的 `Document` 类型已含 `file_path` 与 `page_count`，具备自然接入点。

## 目标

补齐「单页预览」整条链路：**命令层新增 `render_page_preview` → 前端 api/类型 → 重写 `PdfPreview.vue` 真正渲染（支持前后翻页）→ 文档管理页接入「预览」入口**。核心层无需改动。

## 改动清单（逐文件）

1. `frontend/src-tauri/Cargo.toml`
   - `[dependencies]` 增 `base64 = { workspace = true }`（版本 0.22 已在 workspace 锁定）。

2. `frontend/src-tauri/src/commands/pdf_commands.rs`
   - 文件头补 `use base64::engine::general_purpose::STANDARD as B64;` 与 `use base64::Engine;`（`image::ImageFormat` 已存在导入）。
   - 新增结构体与命令（追加到文件末尾 `pdf_page_count` 之后）：
     - `PreviewInput { input: PathBuf, page: u32 /* 1-based */, dpi: u32 /* default 120 */ }`
     - `PreviewOutput { data: String /* data:image/png;base64,... */, width: u32, height: u32, page: u32 }`
     - 命令 `render_page_preview(input: PreviewInput) -> CmdResult<PreviewOutput>`：
       - `spawn_blocking` 内调 `pdf::render::render_page_to_image(&inp, page.saturating_sub(1), dpi)`；
       - 读 `img.width()/img.height()`，`write_to(Cursor<Vec<u8>>, ImageFormat::Png)`，编码失败 `map_err` 为 `CoreError::Image`；
       - base64 拼成 `data:image/png;base64,` 前缀 data URI 返回。

3. `frontend/src/api/pdf.ts`
   - `pdfApi` 增 `previewPage(input, page, dpi = 120)` → `call<PreviewOutput>('render_page_preview', { input: { input, page, dpi } })`。

4. `frontend/src/types/pdf.ts`
   - 增 `PreviewOutput { data: string; width: number; height: number; page: number }`。

5. `frontend/src/components/pdf/PdfPreview.vue`
   - 重写：props `{ path: string; page?: number; dpi?: number; totalPages?: number }`。
   - 内部 `current` 页码（初始 `props.page ?? 1`），`watch` `path` 变化时重置到第 1 页并重新渲染。
   - `render()` 调 `pdfApi.previewPage(path, current, dpi)` 取 data URI 显示 `<img>`；加载/错误提示。
   - `totalPages > 1` 时显示上一页/下一页按钮（页码越界在两端禁用）。
   - 移除 `convertFileSrc` 依赖。

6. `frontend/src/views/desktop/DocumentListView.vue`
   - 引入 `PdfPreview`，新增 `previewDoc` / `previewVisible` 状态与 `openPreview(doc)`（仅 `file_type === 'pdf'` 可预览）。
   - 操作列新增「预览」文本按钮（PDF 行为主，非 PDF 置灰/隐藏）。
   - 页底加 `el-drawer`，内含 `<PdfPreview :path="previewDoc.file_path" :total-pages="previewDoc.page_count ?? 0" :dpi="120" />`。

## 关键决策

- **返回 data URI 而非 `Vec<u8>`**：Tauri IPC 会把 `Vec<u8>` 序列化成 JSON 数组，单页 PNG 动辄数十万字节，性能与体积均差；base64 字符串开销固定 33%，预览场景可接受。
- **宽度/高度取自 `DynamicImage`**，避免二次解码；PNG 编码复用 command 层已有的 `image` 依赖。
- **页码语义 1-based**：与 `pdf_page_count` / 转图片按页导出一致；越界由 core `PageNotFound` 兜底（前端翻页按钮同时禁用边界）。
- **不引入核心层新函数**：复用已导出的 `render_page_to_image`，改动最小化。

## 验证步骤

```bash
cargo fmt --all -- --check        # 必要时先 cargo fmt --all
cargo test -p pdf_docvault_core   # 基线 76 passed / 5 ignored
cargo clippy -p pdf_docvault_core --all-targets -- -D warnings
cd tests/rust && cargo test -p pdf-docvault-tests --test pdf_flow   # 基线 42 passed
cargo check -p pdf-docvault       # src-tauri 编译（cd 根目录）
cd frontend && pnpm typecheck
./node_modules/.bin/vitest run --root . > /tmp/vitest-run.log 2>&1   # 基线 56 passed / 9 文件
pnpm build
```

## 范围约束

仅补「单页预览渲染」这一处真实缺失；不新增其他功能、不调整既有模块结构。