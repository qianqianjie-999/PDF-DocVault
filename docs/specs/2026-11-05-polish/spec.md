# 打磨轮次规格：前端交互缺陷修复

- 日期：2026-11-05
- 范围：前端（Vue 3 + Tauri），不涉及 Rust / pycore
- 目标：修复 4 项已核实、用户可见的问题，不做功能扩张

## 背景与问题

### P1 拖拽上传未按扩展名过滤（用户可见功能错误）

`FileDropZone.vue` 的文件选择对话框用 `props.accept` 生成 filters，但拖拽路径
（`webview.onDragDropEvent` → `drop`）未做任何扩展名校验。PDF 工具页
（`PdfToolsView.vue` 传入 `accept="pdf"`）拖入 `.docx` 也会进入全局
`filesStore.selectedFiles`，随后合并/拆分等操作会以「处理失败」收场；
扫描、OCR、图片转 PDF、Word 转 PDF 页面则在本地 `onSelect` 里各自再过滤，
行为不一致。

### P2 拖拽监听未注销（跨页污染 + 重复处理）

`FileDropZone.vue` 在 setup 阶段调用 `listenDrop()`，但未调用
`webview.onDragDropEvent` 返回的 unlisten，也未在 `onUnmounted` 中清理。
每个用到该组件的页面（PDF 工具、扫描、证件、OCR、Word 转 PDF、图片转 PDF）
挂载一次就新增一个监听，永不释放：
- 在 OCR 页拖入文件会同时触发此前访问过的 PDF 工具页监听，把同一路径写入全局
  `filesStore`；
- 已卸载页面仍会收到 `select` 事件并操作其本地状态（内存泄漏 + 无意义副作用）。

### P3 OCR 导出 TXT 未应用默认输出目录（设置不一致）

`OcrView.vue` 的 `exportTxt()` 使用裸文件名 `defaultName` 作为
`save({ defaultPath })`，而同页 `runConvert()` 已使用
`settings.resolveOutput(...)`。结果：用户在设置页配置的默认输出目录对
「导出 TXT」不生效。

### P4 首页功能描述与实现不符（文案错误）

`HomeView.vue`：
- 「PDF 水印」描述为「文字/图片水印」，但核心仅实现
  `core/src/pdf/watermark.rs::add_text_watermark`，前端只有文字水印参数；
- 「PDF 压缩」描述为「无损压缩体积」，但 `core/src/pdf/compress.rs` 会按档位
  重编码图片（有损），低档才是纯结构压缩。

### 附加文案缺陷

`ActivationView.vue` 换机说明正文写了 `**原来那张卡密**`，Vue 模板不会解析
Markdown，界面会直接显示星号。

## 决策（实现方案）

### D1 `frontend/src/utils/file.ts`

新增纯函数：

```ts
/** 按 accept 列表（如 "pdf" 或 "png,jpg,jpeg"）判断扩展名是否匹配 */
export function matchesAccept(path: string, accept?: string): boolean
```

- `accept` 为空 → 返回 `true`（保持「不过滤」的默认语义）；
- 截取最后一个 `.` 之后的后缀，大小写不敏感；
- `accept` 按 `,` 拆分、`trim`、转小写后比较。

### D2 `frontend/src/components/pdf/FileDropZone.vue`

1. `drop` 分支先用 `matchesAccept(p, props.accept)` 过滤路径；全部被过滤时
   不发 `addFiles`、不发 `select`，并 `ElMessage.warning('不支持的文件类型')`。
2. 保存 `unlisten`：`let unlisten: (() => void) | null = null`；
   `listenDrop()` 中 `unlisten = await webview.onDragDropEvent(...)`。
3. `let disposed = false`；`onUnmounted(() => { disposed = true; unlisten?.(); unlisten = null })`。
   `await` 返回后若 `disposed` 为真，立即调用并置空，避免卸载后才注册成功导致泄漏。
4. 保持现有错误忽略逻辑（非 Tauri 环境）。

### D3 `frontend/src/views/desktop/OcrView.vue`

`exportTxt()` 的 `defaultPath` 改为 `settings.resolveOutput(defaultName)`。

### D4 `frontend/src/views/desktop/HomeView.vue`

- 水印：`文字/图片水印` → `为每页叠加文字水印`
- 压缩：`无损压缩体积` → `结构压缩 / 图片重编码，减小体积`

### D5 `frontend/src/views/desktop/ActivationView.vue`

`**原来那张卡密**` → `「原来那张卡密」`（纯文本，无 Markdown 标记）。

### D6 测试 `frontend/src/utils/__tests__/file.spec.ts`（新建）

覆盖 `matchesAccept`：
- 空 accept 返回 true；
- `"pdf"` 匹配 `a.PDF`、不匹配 `a.docx`；
- 多扩展名 `"png,jpg,jpeg"` 命中 `x.jpeg`，不命中 `x.gif`；
- 无扩展名文件返回 false（accept 非空时）。

## 非目标

- 不改 Rust `TaskManager` 清理策略、前端 `task` store 的 `running` 语义；
- 不动 CSP、`.license/`、`.workbuddy/`；
- 不新增页面或功能项。

## 验收标准

1. 在 PDF 工具页拖入 `.docx`：文件不进入列表，出现「不支持的文件类型」提示。
2. 依次访问 PDF 工具页、OCR 页后回到 OCR 页重复拖拽，同一路径只被加入一次；
   页面卸载后拖拽不再触发其状态变更。
3. 设置默认输出目录后，OCR 结果「导出 TXT」对话框默认落在该目录。
4. 首页水印/压缩描述与实现一致；激活页换机说明不再显示 `**`。
5. 以下命令全部通过：
   - `cd frontend && pnpm typecheck`
   - `cd frontend && pnpm exec vitest run --reporter=dot --pool=threads --poolOptions.threads.singleThread`
   - `cd frontend && pnpm build`

## 需要确认

以上 5 项改动是否全部实施？确认后按 tasks.md 执行。
