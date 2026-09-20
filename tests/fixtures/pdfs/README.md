# PDF fixtures 目录

按约定此目录存放测试用 PDF 样本,但当前**不提交任何二进制文件**:
集成测试(`tests/rust`)在运行时通过 `tests/rust/tests/common/mod.rs` 的
`make_pdf()` 动态生成白图 PDF 到临时目录,用后即弃。

如后续确需手工构造的复杂样本(如损坏文件、加密样本),再放置于此。
