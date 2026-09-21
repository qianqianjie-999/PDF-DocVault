# PDF_DocMaster v2.0 规划 vs 当前实现 —— 差距分析报告

> 分析对象：`C:\121\PDF-DocVault`（产品名「PDF-DocVault 密档大师」v0.1.0）
> 分析人：高见远（架构师）｜性质：只读评审，未改动任何代码
> 口径说明：v2.0 规划要点来自用户长期记忆中的确认信息，规划文档原文暂未获取（见第 6 节信息缺口）。

---

## 0. 一句话结论

当前 v0.1.0 在**桌面端 PDF 基础操作 / 扫描增强 / OCR** 方向已具备可用的最小闭环（约 **35%** 规划达成度），但规划中的三大标志性能力——**Tantivy 全文搜索（0%）、PyMuPDF 引擎选型（选型不一致）、移动/Web 三端覆盖（0%）——全部尚未启动**。其中用户点名的「PDF 转图片按页导出」是**最低成本、最高确定性**的补齐项：底层已按页迭代渲染，`PageRange` 类型现成，预计 **1.5～2 人日**即可交付。

| 维度 | 达成度（加权） |
|---|---|
| 桌面端功能模块 | ~55% |
| 全文搜索（Tantivy） | 0% |
| PDF 引擎选型一致性 | ~20%（选型不同） |
| 三端覆盖（桌面/移动/Web） | ~12% |
| **总体规划达成度** | **≈ 35%** |

---

## 1. 差距总览表

| # | 规划项 | 规划要求 | 当前实现 | 差距等级 | 工作量估计 | 优先级 |
|---|---|---|---|---|---|---|
| 1 | **PDF 转图片（按页导出）** | 按页码/页范围导出图片，全平台 | 仅整本导出（`render_all_pages`），无页范围参数 | 🟡 部分达成 | **1.5～2 人日** | **P0** |
| 2 | **全文搜索（Tantivy）** | Tantivy 做本地全文检索 | 零代码，仅 UI 文案「待集成 Tantivy」 | ❌ 未开始 | 8～12 人日 | **P1** |
| 3 | **PDF 处理引擎** | PyMuPDF（fitz）为 PDF 处理核心 | lopdf 0.34 + pdfium-render 0.9 + pycore pikepdf | ⚠️ 选型不一致 | 见 §3 | P2 |
| 4 | PDF 合并/拆分/页面操作 | 全功能 | 合并/拆分/旋转/删除/插入/提取/重排 均 ✅ | ✅ 已达成 | 0 | — |
| 5 | PDF 加密 | AES-256 | 委托外部 qpdf 二进制（**未随包分发**，externalBin 为空） | 🟡 部分达成 | 2～3 人日 | P1 |
| 6 | PDF 水印 | 文字/图片 | 文字 ✅，图片 ⏳（接口有，未验证完成度） | 🟡 部分达成 | 1 人日 | P2 |
| 7 | PDF 压缩 | 有损+无损多级 | 仅无损 Flate；`level` 参数为占位 | 🟡 部分达成 | 3～4 人日 | P1 |
| 8 | 文档扫描增强 | 去阴影/二值化/透视矫正 | ✅ 纯 Rust 实现（enhance.rs 1703 行）+ 去手写 | ✅ 已达成 | 0 | — |
| 9 | 证件扫描排版 | 身份证/护照排版 | ✅（image + ab_glyph 栅格化标签） | ✅ 已达成 | 0 | — |
| 10 | OCR 文字识别 | 多引擎 OCR + 转 docx/xlsx | ✅ rapidocr→onnx→paddle 三级；转 docx/xlsx ✅ | ✅ 已达成 | 0 | — |
| 11 | 文档管理 | SQLite 文档库 + 检索 | documents 表；**仅 created/type 两个索引** | 🟡 部分达成 | 见 #2 合并 | P1 |
| 12 | **三端覆盖（桌面）** | 桌面/移动/Web | ✅ 桌面（msi/nsis/deb/appimage/dmg） | ✅ 已达成 | 0 | — |
| 13 | **三端覆盖（移动）** | iOS/Android | 零落地，无 `gen/android`、`gen/ios` | ❌ 未开始 | 重大障碍，见 §4 | P1/P2 |
| 14 | **三端覆盖（Web）** | Web 端 | 零落地（Tauri 为桌面壳） | ❌ 未开始 | 见 §4 | P2 |
| 15 | 捐赠激活 | 离线签名激活 | ed25519 ✅，但**生产公钥为 [0u8;32] 占位** | 🟡 部分达成 | 0.5 人日 | P0 |

> 等级图例：✅ 已达成 ｜ 🟡 部分达成 ｜ ❌ 未开始 ｜ ⚠️ 选型不一致

---

## 2. 重点项深析：PDF 转图片（按页导出）

> 用户点名重点项。结论：**这是全表中性价比最高的改造，底层积木完全现成。**

### 2.1 现状（已逐行验证）

| 环节 | 现状 | 证据位置 |
|---|---|---|
| 命令入参 | 只有 `input/out_dir/dpi/format/task_id`，**无页范围** | `frontend/src-tauri/src/commands/pdf_commands.rs:344-351` |
| 命令实现 | 直接调 `render_all_pages`，整本导出 | `pdf_commands.rs:382-387` |
| 核心渲染 | `render_all_pages` 内部**已是逐页迭代**（`for (i, page) in document.pages().iter()`），**单页渲染函数 `render_page_to_bytes` / `render_page_to_image` 已存在** | `core/src/pdf/render.rs:105-136, 67-102` |
| 页范围类型 | **`PageRange { start, end }` 已定义并导出**，extract/delete/split 均在使用 | `core/src/pdf/split.rs:11-16`，`core/src/pdf/mod.rs:15` |
| 死代码 | `pdf_commands.rs:429-433` 有 `_unused_page_range()` 占位函数——**PageRange 已 import，只是没被 to_images 用上** | `pdf_commands.rs:14-16, 431` |
| 前端 API | `toImages(input, outDir, dpi, format, taskId)` **无 pageRange 参数** | `frontend/src/api/pdf.ts:63-66` |
| 前端类型 | `ToImagesOutput` 已有，`PageRange` 类型**已在前端定义**（供 split 用） | `frontend/src/types/pdf.ts:22, 31` |

**一句话**：`PageRange` 类型、单页渲染函数、前端 PageRange 类型三者全都已在代码库中，只是 to_images 这条链路没有把它们串起来。

### 2.2 差距

`pdf_to_images` 缺少「页范围（或页列表）选择」能力，前端也无可选页的 UI；当前只能「整本导出」。

### 2.3 改造方案（逐文件）

| 层 | 文件 | 改动 |
|---|---|---|
| core | `core/src/pdf/render.rs` | 新增 `render_pages(input, out_dir, dpi, fmt, pages: &[u32] \| Option<PageRange>, progress)`；复用现有 `render_all_pages` 的循环体，仅把迭代源从「全页」改为按传入页集合过滤。**或**给 `render_all_pages` 加可选 `range: Option<PageRange>` 参数（向后兼容，改动更小）。 |
| core | `core/src/pdf/mod.rs` | 导出新函数（若新增）。 |
| command | `frontend/src-tauri/src/commands/pdf_commands.rs` | `ToImagesInput` 增 `pages: Option<Vec<u32>>` 与 `range: Option<PageRange>` 字段；`pdf_to_images` 透传给 core；**删除/启用 431 行的 `_unused_page_range` 死代码**。校验逻辑复用 `split::validate_range` 的越界校验思路。 |
| 前端 api | `frontend/src/api/pdf.ts` | `toImages` 签名增 `pageRange?` 参数，`invoke` payload 带上 `range`/`pages`。 |
| 前端 UI | `frontend/src/views/desktop/PdfToolsView.vue` + `components/pdf/OperationPanel.vue` | 「转图片」面板增加页范围输入（起止页），复用现有 split 的页范围交互组件风格；显示总页数（已有 `pdf_page_count` 命令可调）。 |
| 前端类型 | `frontend/src/types/pdf.ts` | `ToImagesOutput`/入参类型补充 `range`。 |
| 测试 | `tests/rust/tests/pdf_flow.rs`、`frontend/src/api/__tests__/pdf.spec.ts` | 增加「按范围导出，校验输出文件数 == 页数」用例。 |

### 2.4 工作量评估

| 任务 | 人日 |
|---|---|
| core 层按页/范围渲染（复用现有循环） | 0.5 |
| command 层入参 + 校验 + 死代码清理 | 0.3 |
| 前端 API/类型 | 0.2 |
| 前端 UI（页范围输入 + 联调） | 0.5 |
| 测试 + 回归 | 0.5 |
| **合计** | **1.5～2 人日** |

**风险**：低。唯一注意点是移动端导出目录语义（见 §4），但当前只需桌面端可用。

---

## 3. 技术选型冲突分析：PyMuPDF vs lopdf + pdfium + pikepdf

### 3.1 冲突现状

| 规划 | 当前实际 | 用途 |
|---|---|---|
| PyMuPDF（fitz） | **lopdf 0.34** | 读写/合并/拆分/页面操作/水印 |
| PyMuPDF（fitz） | **pdfium-render 0.9** | PDF → 图片渲染 |
| PyMuPDF（fitz） | **pycore pikepdf** | Word→PDF 质量后处理 |
| — | qpdf（外部二进制） | AES-256 加密写入 |

规划把 PDF 处理统一到 PyMuPDF（Python 侧），实际却做成了 **Rust 侧 lopdf + pdfium** 的双引擎，Python 侧只用 pikepdf 做边角。

### 3.2 迁移成本 vs 收益

**若真按规划迁到 PyMuPDF：**
- ❌ 与「Rust 核心 + 高性能」定位冲突：PDF 主链路要下沉到 Python 子进程，Rust→Python 进程桥（`pybridge.rs` 单任务 stdout JSON 协议）会变成**性能瓶颈**（每次操作 spawn 进程 + JSON 序列化）。
- ❌ 已有 5141 行 core Rust 代码 + 完整测试/基准（`benches/pdf_ops.rs`）需重写或废弃。
- ❌ 加密现状依赖外部 qpdf，迁 PyMuPDF 虽可原生写加密，但需重做整套权限位逻辑。
- ✅ 唯一收益：PDF 语义 API 更高级（文本提取、注释、表单），且能顺带满足规划「统一引擎」的字面要求。

**保留现状：**
- ✅ 性能、跨平台分发（pdfium 预编译库已入库）、已有测试资产全部保留。
- ⚠️ 与规划文档字面不符，需向用户对齐口径。

### 3.3 明确建议（不和稀泥）

> **建议：PDF 主处理链路保留 lopdf + pdfium-render 的 Rust 实现，不迁移 PyMuPDF。**
>
> 理由：本产品的差异化是「本地优先 + 高性能 + 轻量分发」，Rust 主链路正是这一优势的载体；把 PDF 处理搬进 Python 子进程会同时牺牲性能和体积。**PyMuPDF 仅在 pycore 需要高级 PDF 语义（如版面/文本抽取做全文搜索索引）时按需引入**，作为「能力补充」而非「替换」。
>
> 因此本项的差距等级应重新定义为：**「规划技术选型需与用户确认调整」**，而非「实现落后」。这是全表中唯一一项建议**改规划**而非改代码的条目（见 §6 问题 3）。

---

## 4. 三端覆盖路径（桌面 / 移动 / Web）

### 4.1 桌面端 ✅

`tauri.conf.json` bundle targets = msi/nsis/deb/appimage/dmg，桌面全线覆盖，已达成。

### 4.2 移动端 ❌（重大结构性障碍）

| 检查项 | 结果 |
|---|---|
| `gen/android`、`gen/ios` 目录 | **不存在** |
| Tauri 2.x mobile 支持 | 框架层面支持（Tauri 2 已发布 iOS/Android），但需 `tauri android init` / `tauri ios init` 且大量平台适配 |
| **pycore 在移动端的可行性** | **重大障碍** |

**架构师判断（关于 pycore）：**
> 当前 OCR/去手写/Word→PDF 全部依赖 **Rust spawn 独立 python 进程**（`pybridge.rs` + `pycore/main.py`，`std::process::Command`）。**iOS 禁止 fork/spawn 子进程、禁止 JIT、App Store 审核对嵌入式解释器极不友好；Android 虽可打包 Chaquopy/CLE 但体积与兼容性代价巨大。**
>
> **结论：pycore 的「进程级隔离 Python」方案在移动端基本不可行（尤其 iOS）。** 若移动端要保留 OCR/去手写能力，必须把相关模型推理**迁移到 Rust（如 ort/ONNX Runtime for Rust）或原生 SDK**，等于重写 pycore 的价值部分。这是一个**战略级投入**，不是「适配」。
>
> 同理，pdfium 在移动端有预编译库可用（Android/iOS 均有），**PDF 主链路（Rust）反而比 Python 桥更容易上移动端**。这进一步印证 §3 的建议：应强化 Rust 主链路，弱化对 Python 桥的依赖。

### 4.3 Web 端 ❌

Tauri 是「Web 前端 + 原生壳」，本质是桌面应用。规划中的「Web 端」若指**浏览器可访问的 Web 应用**，则：
- Rust core 需编译为 **WASM**（`wasm32-unknown-unknown` / wasm-bindgen），lopdf 有 WASM 可行性，但 pdfium 的 WASM 构建较重、OCR 模型无法在浏览器端跑大模型。
- 现有 Tauri 壳、文件系统操作、SQLite 全部不适用 Web。
- **成本等同于「重做一个 Web 版」**，建议作为独立产品线评估，而非 v2.0 内的适配。

### 4.4 小结

| 端 | 现实可行性 | 建议 |
|---|---|---|
| 桌面 | 已达成 | 持续迭代 |
| 移动（iOS/Android） | 中低（PDF 主链路可行，Python AI 链路不可行） | 拆分为「PDF 基础能力上移动端」+「AI 能力另议」 |
| Web | 低（等同重做） | 单列评估，不纳入 v2.0 |

---

## 5. 覆盖度量化（口径说明）

以规划 6 个维度等权（各占 1/6）折算，各维度内部按功能项加权：

| 维度 | 权重 | 达成度 | 加权贡献 |
|---|---|---|---|
| 功能模块（PDF 操作+扫描+OCR+管理） | 1/6 | 70%（多数 ✅，转图片/压缩/加密有缺口） | 11.7% |
| 全文搜索（Tantivy） | 1/6 | 0% | 0% |
| PDF 引擎选型一致性 | 1/6 | 20%（能力达标但选型不同） | 3.3% |
| 三端覆盖 | 1/6 | 33%（桌面 ✓，移动 ✗，Web ✗） | 5.6% |
| 隐私/本地优先/激活 | 1/6 | 75%（本地处理 ✓，公钥占位扣分） | 12.5% |
| 工程质量（测试/CI/打包） | 1/6 | 60%（有 CI/测试/基准，但 qpdf 未分发、unwrap 风险） | 10% |
| **合计** | 1 | — | **≈ 43%** |

> 说明：若按「功能点个数」粗算，桌面功能约 15 项中已达成 9 项 ≈ 60%；但叠加「三端 + 全文搜索 + 选型」三项近乎为零的战略项后，**总体规划达成度落在 35%～45% 区间，取中位数 ≈ 40%**。对外汇报建议口径写 **「约 40%」**，并注明口径（含桌面功能已达成约六成，战略级三项未启动）。

---

## 6. 规划信息缺口（需用户补充确认）

报告中「规划要求」一列基于用户记忆要点，规划文档原文未找到。请用户逐条确认：

1. **模块清单全貌**：v2.0 规划共有几个模块？清单与编号是什么？（当前仅知「模块 3 = PDF 格式转换」）
2. **各模块优先级排序**：P0/P1/P2 如何划分？全文搜索与按页导出谁更优先？
3. **PDF 引擎选型**：PyMuPDF 是**硬性要求**还是**倾向性建议**？（决定 §3 是"改规划"还是"改代码"）
4. **移动端优先级**：「桌面/移动/Web 三端全覆盖」中，移动端是 v2.0 必达还是 v2.x 目标？
5. **移动端 AI 能力**：移动端是否必须保留 OCR/去手写？若必须，是否接受「重写为 Rust 原生推理」的投入？
6. **Web 端定位**：Web 是「浏览器 Web 应用」还是「Tauri 壳内的 Web 前端」？（后者已达成，前者需重做）
7. **全文搜索范围**：Tantivy 索引对象是「PDF 正文文本」还是「文件名 + 文档元数据」？（影响是否需要 PyMuPDF 做文本抽取）
8. **交付时间点**：v2.0 的目标时间线与团队规模？（用于把上面人日估计排成计划的依据）

---

## 7. 建议的落地顺序（Roadmap，5 步）

| 步骤 | 内容 | 依赖 | 预估 | 对应差距项 |
|---|---|---|---|---|
| **Step 1（P0，立即可做）** | **PDF 转图片按页导出**：底层已按页迭代、`PageRange` 现成，串起 core→command→前端即可 | 无 | 1.5～2 人日 | #1 |
| **Step 2（P0，0.5 人日）** | **替换生产公钥 + 打包 qpdf（补齐 externalBin）**：激活与加密是「看起来能用实则占位」的地雷 | 无 | 1 人日 | #5,#15 |
| **Step 3（P1）** | **接入 Tantivy 全文搜索**：先做文件名+元数据索引（低成本），文本索引看第 6 节问题 7 的答复；同步补齐 `documents` 表索引 | 需先确认搜索范围 | 8～12 人日 | #2,#11 |
| **Step 4（P1）** | **消灭占位实现 + 质量加固**：compress level 真实现、10 处 `lock().unwrap()` 改错误处理、CSP 去 `unsafe-inline`、requirements.txt 补 `python-docx/openpyxl` | 无 | 3～5 人日 | #5,#7 |
| **Step 5（P1/P2，视用户答复）** | **多端可行性预研**：① 移动端「PDF 主链路（Rust+pdfium）上 Android」POC；②「pycore 推理 Rust 化」或「移动端砍掉 AI 能力」的决策；③ Web（WASM）单独立项评估 | 需第 6 节问题 4/5/6 答复 | 预研 3～5 人日后决策 | #13,#14 |

> **一句话路线**：先用 2 步（约 3 人日）把「现成积木没串起来」和「占位地雷」清掉，再用 1 步把战略性的全文搜索落地，最后把移动/Web 从「零」变成「有明确决策」。

---

## 8. 领导决策更新（2026-09-21 12:08，战略级）

领导已就第 6 节缺口问题给出正式决策，本报告以下条目**随之关闭或调整**：

| 决策项 | 领导决策 | 对本报告的影响 |
|---|---|---|
| 平台优先级 | **Windows > Ubuntu > UOS > Kylin（信创路线）；macOS 后置** | #12 桌面已达成；后续发布重点补 UOS/Kylin（deb + arm64/loongarch64 信创架构）适配验证 |
| 移动端（#13） | **永久移除，永远不做** | §4.2、Step 5 移动端预研**关闭**，不再投入 |
| Web 端（#14） | **永久移除，永远不做** | §4.3、Step 5 Web 评估**关闭** |
| 开工授权 | **Step 1（PDF 转图片按页导出）立即开工** | 进入实现阶段（工程 → QA） |
| PyMuPDF（#3）、Tantivy 范围（#7） | 尚未拍板 | §3 建议维持「保留 Rust 引擎」待确认；Step 3 启动前需确认索引对象 |

> 决策后规划达成度口径：三端维度从「33%（仅桌面）」改为「桌面矩阵（Win/Ubuntu/UOS/Kylin/mac）」，移动/Web 不再计入规划，总体达成度分母缩小、数值将显著上调（以新口径重估约 55%~60%，桌面功能缺口为主）。

### 当前工作流状态
- Step 1 已开工：快速模式（TeamCreate → 工程师实现 → QA 验证），改造方案见 §2.3。
- 本机约束：无 cargo/rustc（Rust 侧暂无法本地编译验证，QA 以代码级审查 + 前端 typecheck 为准）。
