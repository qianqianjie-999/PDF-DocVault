# 任务清单：前端交互缺陷修复

- [x] T1 `frontend/src/utils/file.ts`：新增 `matchesAccept(path, accept)` 纯函数
- [x] T2 `frontend/src/utils/__tests__/file.spec.ts`：新增 4 组 `matchesAccept` 用例
- [x] T3 `frontend/src/components/pdf/FileDropZone.vue`：
      - 拖拽按 `accept` 过滤，全部不匹配时提示「不支持的文件类型」
      - 保存 `unlisten`，`onUnmounted` 注销；处理「卸载后才注册完成」的竞态
- [x] T4 `frontend/src/views/desktop/OcrView.vue`：`exportTxt` 使用 `settings.resolveOutput(defaultName)`
- [x] T5 `frontend/src/views/desktop/HomeView.vue`：修正水印/压缩描述
- [x] T6 `frontend/src/views/desktop/ActivationView.vue`：去掉 `**` 标记，改为「」
- [x] T7 验证：`pnpm typecheck`、`pnpm exec vitest run ...`、`pnpm build`
- [x] T8 汇报未提交改动并等待用户确认是否提交
