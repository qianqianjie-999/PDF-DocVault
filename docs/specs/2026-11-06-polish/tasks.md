# 任务清单：解密流程与页面输入健壮性

- [x] T1 `frontend/src/components/pdf/OperationPanel.vue`
      - 探测 watch 同时覆盖 `encrypt` / `decrypt`
      - 解密仅在 `locked === true` 时要求密码
      - 解密表单新增「未加密 / 仅权限限制」上下文提示
- [x] T2 `frontend/src/utils/pages.ts`
      - 导出 `MAX_EXPANDED_PAGES = 10000`
      - `parsePageNums` 展开前按区间跨度、展开中按累计长度双重限流,超限返回 `[]`
- [x] T3 `frontend/src/utils/__tests__/pages.spec.ts`
      - 增加超大区间、边界值、`pageInputInvalid` 用例
- [x] T4 `frontend/src/views/desktop/ImagesToPdfView.vue`
      - 抽取 `resetResult()`;`onSelect`(仅实际新增时)、`removeFile`、`move` 调用
      - `pickOutputFile` 以当前 `outputPath` 为 `defaultPath`
- [x] T5 验证：
      `pnpm typecheck`、`vitest run`、`pnpm build`、
      `cargo fmt --all -- --check`、`cargo test -p pdf_docvault_core`、
      `cargo test -p pdf-docvault-tests --test pdf_flow`、
      `cargo clippy -p pdf_docvault_core --all-targets -- -D warnings`
- [x] T6 汇报本轮与上一轮累计未提交改动，等待用户确认是否提交
