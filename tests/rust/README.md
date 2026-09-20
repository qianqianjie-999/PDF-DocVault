# PDF-DocVault 集成测试 crate

跨模块端到端流程验证(merge → split → pages → watermark → compress → 加密/渲染)。
fixture 不提交二进制样本,测试运行时由 `common/mod.rs` 动态生成。

```bash
# 运行全部集成测试
cargo test -p pdf-docvault-tests

# 渲染测试需要 pdfium(开发环境默认探测 frontend/src-tauri/resources/pdfium/lib)
# 加密测试需要系统 qpdf,缺失时自动跳过
```
