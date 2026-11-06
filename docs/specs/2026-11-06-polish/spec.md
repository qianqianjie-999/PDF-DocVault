# 打磨轮次规格：解密流程与页面输入健壮性

- 日期：2026-11-06
- 范围：前端（Vue 3 + Tauri），不改 Rust / pycore
- 目标：修复 3 项已核实、用户可见的问题,不做功能扩张
- 说明：工作区已有上一轮未提交的 Rust 改动（merge/pages/render/pdf_flow），本轮只新增前端修复,
  验证时连同这些改动一起跑完整测试

## 背景与问题

### P1 仅限制权限的 PDF 无法解密（用户可见功能错误）

`pycore/pdfcrypt/crypt.py::decrypt_file` 支持空密码:对「用户密码为空、只设置权限」
的加密 PDF,`pikepdf.open(path, password="")` 可以正常打开并移除加密。
`probe_encryption` 也如实返回 `encrypted: true, locked: false`。

但 `frontend/src/components/pdf/OperationPanel.vue` 的 `decrypt` 分支（L240-L256）
在密码为空时直接 `ElMessage.warning('请输入该 PDF 的打开密码')` 并中止;
并且既有的加密探测 `watch`（L71-L83）只对 `op === 'encrypt'` 生效,解密页完全
拿不到 `locked` 信息。结果:这类 PDF 在界面上永远无法解密,除非用户误填一个密码
（必然失败）。

### P2 超大页码区间会卡死界面（用户可见卡顿/内存暴涨）

`frontend/src/utils/pages.ts::parsePageNums` 对每个区间 `for (let n = a; n <= b; n++)`
无条件展开。用户输入 `1-999999999`（或粘贴一段脏数据）会一次性向数组 push
近十亿个数字,主线程长时间阻塞甚至 OOM;随后后端才报页码越界。
`parsePageNums` 是旋转/删除/提取/转图片共用的入口,影响全部四个功能。

### P3 图片转 PDF 结果卡片与文件列表脱节 + 浏览对话框忽略当前路径

`frontend/src/views/desktop/ImagesToPdfView.vue`：

1. `clearFiles()` 会清空 `resultPages/resultElapsed`,但 `removeFile()`、`move()`、
   `onSelect()` 在列表变化后都不清空。合成成功后删掉一张图或再添加图片,页面仍显示
   上一轮的「合成结果 N 页」,容易被当成当前列表的产物。
2. `pickOutputFile()` 的 `defaultPath` 恒为 `settings.resolveOutput('images.pdf')`,
   忽略输入框里已有的 `outputPath`。用户已选好自定义路径后再次点「浏览…」,
   对话框又跳回默认目录/默认文件名。

## 决策（实现方案）

### D1 `frontend/src/components/pdf/OperationPanel.vue`

1. 将探测 `watch` 的触发条件由 `op.value !== 'encrypt'` 改为
   `!['encrypt', 'decrypt'].includes(op.value)`;仍要求恰好选中 1 个文件,
   探测失败时保持 `encryptProbe = null`（不打扰用户）。
2. `decrypt` 分支的密码校验改为:
   - 仅当 `encryptProbe?.locked === true` 时提示「请输入该 PDF 的打开密码」并中止;
   - 其余情况(未加密 / 仅权限限制 / 探测失败)允许空密码,交给 pycore 判定,
     失败时由现有 `catch` 统一提示。
3. 解密表单内新增上下文提示（`el-alert`，`closable=false`）:
   - `encryptProbe && !encryptProbe.encrypted` → info:「该文件未加密,解密只会另存一份副本」
   - `encryptProbe?.encrypted && !encryptProbe.locked` → info:「该文件仅限制权限,
     无需打开密码,可直接解密」
4. 不改变加密分支的现有行为(已加密文件仍提示先解密)。

### D2 `frontend/src/utils/pages.ts`

新增模块级常量并改造 `parsePageNums`:

```ts
/** 单次输入最多展开的页码数:超过即视为非法输入,避免超大区间拖死主线程 */
export const MAX_EXPANDED_PAGES = 10000
```

- 每 push 一个页码前检查 `nums.length >= MAX_EXPANDED_PAGES`,超限立即
  `return []`（沿用「任一非法片段 → 整体无效」的既有语义）;
- 同时限制区间展开循环,避免 `1-999999999` 在检查点之前先跑很久:
  先按 `b - a + 1` 判断 `nums.length + span > MAX_EXPANDED_PAGES` 则直接返回 `[]`;
- 单页、常规区间、`8-5` 归一化、空片段等既有行为保持不变;
- `pageInputInvalid` 与 `parseRanges` 不改（`parseRanges` 不展开,无此风险）。

### D3 `frontend/src/views/desktop/ImagesToPdfView.vue`

1. 新增 `function resetResult() { resultPages.value = 0; resultElapsed.value = 0 }`,
   `clearFiles()` 改用它（行为不变）。
2. `onSelect`:只有确实新增了文件（`added === true`）时调用 `resetResult()`;
   全是重复路径时不重置。
3. `removeFile(idx)`、`move(idx, delta)` 成功后调用 `resetResult()`。
4. `pickOutputFile()` 的 `defaultPath` 改为
   `outputPath.value || settings.resolveOutput('images.pdf')`。

## 非目标

- 不改 pycore / Rust 代码,不改 `probe_encryption` 的损坏文件异常处理;
- 不改加密面板的 `input_password`（已加密文件仍要求先解密）;
- 不新增组件测试基建,不新增页面/功能项;
- 不处理文字水印 CJK 字体、`id_cards_to_pdf` 标签裁切等已知高成本项。

## 测试与验证

### 新增/修改单测（P2）

`frontend/src/utils/__tests__/pages.spec.ts` 增加:

- `parsePageNums('1-999999999')` → `[]`（不卡死、不返回超大数组）;
- `parsePageNums('1-10000')` → 长度 10000（边界值仍合法）;
- `parsePageNums('1-10001')` → `[]`;
- `pageInputInvalid('1-999999999')` → `true`（界面会提示页码格式有误）。

### 全量验证命令

1. `cd frontend && pnpm typecheck`
2. `cd frontend && ./node_modules/.bin/vitest run --root . > /tmp/vitest-run.log 2>&1`（读日志确认全绿）
3. `cd frontend && pnpm build`
4. `cargo fmt --all -- --check`
5. `cargo test -p pdf_docvault_core`
6. `cd tests/rust && cargo test -p pdf-docvault-tests --test pdf_flow`
7. `cargo clippy -p pdf_docvault_core --all-targets -- -D warnings`

## 验收标准

1. 选一个「仅限制权限、无打开密码」的加密 PDF,切到「解密」页:出现「仅限制权限」提示,
   密码留空可直接解密成功;对真正需要密码的 PDF,留空仍提示请输入密码。
2. 在旋转/删除/提取/转图片的页码框输入 `1-999999999`:界面立即提示页码格式有误,
   不出现长时间卡顿。
3. 图片转 PDF:合成成功后删除一张图或再添加图片,结果卡片立即消失;
   已选自定义输出路径时点「浏览…」,对话框从该路径开始。
4. 上述全部验证命令通过。

## 需要确认

以上 3 项改动是否全部实施？确认后按 tasks.md 执行。
