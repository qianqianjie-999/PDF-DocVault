# PDF-DocVault 密档大师

> 桌面端隐私文档处理中心 · 本地优先 · 隐私至上

PDF-DocVault 是一款面向个人和企业的桌面端智能文档处理工具,集 PDF 操作、文档扫描、OCR 识别、文档管理于一体。所有文档处理任务**默认在用户设备本地完成**,文件数据永不离开本机。

## 核心特性

- 🛡️ **本地优先**:纯离线处理,零数据上传,文件不离开本机
- ⚡ **高性能**:Rust 核心(lopdf + pdfium),合并/拆分毫秒级完成
- 🔒 **隐私至上**:支持 AES-256 加密,本地 SQLite 存储
- 💡 **轻量安装**:基于 Tauri 2.x,安装包仅几十 MB
- 🎨 **Police Blue 主题**:深警蓝配色,专业稳重

## 技术栈

| 层级 | 技术 | 版本 |
|---|---|---|
| 前端 | Vue 3 + TypeScript + Element Plus | 3.5 / 2.9 |
| 桌面框架 | Tauri | 2.x |
| 核心处理 | Rust + lopdf + pdfium-render | stable / 0.34 / 0.9 |
| 数据库 | SQLite (rusqlite bundled) | 0.32 |
| 激活验证 | ed25519 离线签名 | ed25519-dalek 2.1 |

## 功能模块(MVP)

| 模块 | 状态 | 说明 |
|---|---|---|
| PDF 合并 | ✅ | 多文件合并 |
| PDF 拆分 | ✅ | 按范围/每 N 页 |
| 页面操作 | ✅ | 旋转/删除/插入/提取/重排 |
| PDF 加密 | ✅ | AES-256(qpdf) |
| PDF 水印 | ✅ | 文字/图片 |
| PDF 压缩 | ✅ | 无损 Flate |
| 转图片 | ✅ | PNG/JPG/WebP(pdfium) |
| 文档扫描 | ✅ | 纯 Rust:边界检测/透视矫正/去阴影/黑白扫描/多图合成 PDF |
| 文档管理 | ✅ | 列表/标签/删除 |
| 捐赠激活 | ✅ | 1 元扫码激活 |
| OCR/扫描识别 | ⏳ | 规划中(Python/PaddleOCR) |

## 快速开始

### 环境要求

- Node.js 22+ / pnpm 10+
- Rust 1.94+ (stable)
- Linux(Ubuntu):`libwebkit2gtk-4.1-dev`、`libgtk-3-dev`、`librsvg2-dev`、`libxdo-dev`、`libayatana-appindicator3-dev`、`qpdf`
- Linux(Fedora):`webkit2gtk4.1-devel`、`gtk3-devel`、`librsvg2-devel`、`libxdo-dev`、`libayatana-appindicator3-devel`
- 推荐使用 Distrobox 容器隔离环境:见 [scripts/tauri-distrobox-setup.sh](scripts/tauri-distrobox-setup.sh)

### 安装与运行

```bash
# 1. 安装前端依赖
cd frontend && pnpm install

# 2. 开发模式启动(Tauri 桌面端)
pnpm tauri dev

# 3. 构建生产包
pnpm tauri build
```

### 核心库独立测试

```bash
# Rust 单元测试
cargo test -p pdf_docvault_core

# 性能基准
cargo bench -p pdf_docvault_core --bench pdf_ops
```

## 项目结构

```
PDF-DocVault/
├── core/              Rust 核心处理库(PDF/图像/存储/激活)
├── frontend/          Vue 3 + Tauri 桌面端
│   ├── src/           前端源码
│   └── src-tauri/     Tauri Rust 后端
├── pycore/            Python AI/OCR 服务(OCR/文档转换/去手写)
├── tests/             测试与 fixtures
├── docs/              中文文档
└── .github/workflows/ CI/CD
```

详细架构见 [docs/architecture.md](docs/architecture.md)。

## 数据安全

- 纯本地处理,无网络通信
- 本地数据库 SQLite(WAL 模式)
- 敏感文件支持 AES-256 加密
- 临时文件处理完成后自动清理
- 完全离线模式,零云端依赖

## 许可证

MIT
