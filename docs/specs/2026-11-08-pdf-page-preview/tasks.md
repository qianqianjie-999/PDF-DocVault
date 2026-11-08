# 任务清单 — PDF 单页预览渲染

> 目标：补齐「单页预览」整条链路（命令层 → api/类型 → 组件 → 文档管理页接入）。

## 任务

- [x] T1 命令层：`frontend/src-tauri/Cargo.toml` 增 `base64` 依赖；`pdf_commands.rs` 新增 `render_page_preview` 命令与 `PreviewInput`/`PreviewOutput`
- [x] T2 前端 API：`frontend/src/api/pdf.ts` 增 `previewPage`
- [x] T3 前端类型：`frontend/src/types/pdf.ts` 增 `PreviewOutput`
- [x] T4 组件：重写 `frontend/src/components/pdf/PdfPreview.vue` 为真实渲染 + 翻页
- [x] T5 接入：`frontend/src/views/desktop/DocumentListView.vue` 增「预览」按钮 + `el-drawer`
- [x] T6 全量验证：cargo fmt/test/clippy/check、tests/rust、frontend typecheck/vitest/build
- [x] T7 汇报未提交改动并询问是否按中文规范提交推送

## 验证结果

- `cargo fmt --all -- --check`：通过
- `cargo test -p pdf_docvault_core`：76 passed / 5 ignored ✓
- `cargo clippy -p pdf_docvault_core --all-targets -- -D warnings`：通过（0 警告）
- `cd tests/rust && cargo test -p pdf-docvault-tests --test pdf_flow`：42 passed ✓
- `cargo check -p pdf-docvault`：通过
- `cd frontend && pnpm typecheck`：通过
- `vitest run`：56 passed / 9 文件 ✓
- `pnpm build`：通过