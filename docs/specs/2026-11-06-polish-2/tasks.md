# 任务清单：水印资源/内容流健壮性与压缩透明蒙版

- [x] T1 `core/src/pdf/watermark.rs::ensure_resources_entry`
      - 兼容 Font / XObject / ExtGState 子字典为间接引用
      - 缺失时补空字典；其他类型返回 `CoreError::Internal`
      - 文字水印与图片水印共用，行为保持一致
- [x] T2 `core/src/pdf/watermark.rs::append_content`
      - `/Contents` 指向间接数组时向该数组 push，页面引用不变
      - 普通流引用仍包装为 `[原流, 新流]`；无 `/Contents` 时直接挂载
- [x] T3 `core/src/pdf/watermark.rs` 新增单测模块
      - P1：间接 Font 子字典的水印成功 + 资源注册正确
      - P2：间接 `/Contents` 数组追加后长度为 3、原流保留
- [x] T4 `core/src/pdf/compress.rs` 保护蒙版图片
      - 重编码前收集 SMask / Mask（含数组形式）引用的对象号
      - 循环跳过受保护对象；`shrink_image` 自身检查不动
- [x] T5 `core/src/pdf/compress.rs::tests` 新增用例
      - 带 SMask 的图片 level 9 → `images_reencoded == 0`
      - 同文档再加一张无蒙版大图 → `images_reencoded == 1`
- [x] T6 验证：
      `cargo fmt --all -- --check`、`cargo test -p pdf_docvault_core`、
      `cd tests/rust && cargo test -p pdf-docvault-tests --test pdf_flow`、
      `cargo clippy -p pdf_docvault_core --all-targets -- -D warnings`、
      `pnpm typecheck`、`pnpm build`、`vitest run`（日志重定向）
- [ ] T7 汇报本轮未提交改动，等待用户确认是否提交
