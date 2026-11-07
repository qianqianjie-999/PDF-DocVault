# 打磨轮次规格：水印资源/内容流健壮性与压缩透明蒙版

- 日期：2026-11-06
- 范围：Rust core（`core/src/pdf/watermark.rs`、`core/src/pdf/compress.rs`），不改前端 / pycore
- 目标：修复 3 项已核实、用户可见的问题，不做功能扩张
- 说明：上一轮（`docs/specs/2026-11-06-polish/`）的 3 项前端修复已提交 b35704b，工作区干净；
  本轮只新增 core 修复，避开已修复项与「文字水印中文乱码（缺内置 CJK 字体）」等高成本项

## 背景与问题

### P1 资源子字典为间接引用时，添加水印直接失败（用户可见功能错误）

`core/src/pdf/watermark.rs::ensure_resources_entry`（L188-L227）在注册资源名时执行：

```rust
let entry = res_dict.get_mut(category)?;
entry.as_dict_mut()?.set(name, Object::Reference(object_id));
```

PDF 允许 `/Resources` 下的 `/Font`、`/XObject`、`/ExtGState` 本身是指向字典的
**间接引用**（如 `/Font 10 0 R`，`10 0 R` 才是 `<< /F1 5 0 R >>`）。此时
`entry` 是 `Object::Reference`，`as_dict_mut()` 返回类型错误，整个水印操作报错中止，
用户看到「添加水印失败」。同一文件里的 `unique_resource_name`（L128-L157）已经正确处理了
引用形式，说明这是实现遗漏而非设计取舍。

### P2 页面 `/Contents` 指向间接数组时，追加水印流会破坏页面结构（用户可见内容损坏）

`core/src/pdf/watermark.rs::append_content`（L229-L257）对 `Some(Object::Reference(id))`
一律包装成 `[id, new_id]`。但当该引用指向的是**数组对象**（`/Contents 12 0 R`，
`12 0 R` 为 `[stream1 stream2]`，PDF 规范允许）时，结果是 `/Contents [12 0 R 13 0 R]`，
即数组元素引用了另一个数组——非法的内容流结构，阅读器可能忽略正文或渲染异常。

### P3 压缩高分档会把透明蒙版（SMask）当普通图片有损重编码（用户可见画质退化）

`core/src/pdf/compress.rs` 的图片重编码循环（L76-L81）遍历所有 Image XObject，
`shrink_image`（L134-L246）只检查图片自身字典是否带 `SMask` / `Mask` / `Decode`
（L148-L155），因此**被别的图片当作 SMask 引用的灰度图**会被重编码为低质量 JPEG。
软蒙版的 DCT 压缩会在半透明边缘产生块状噪声/白边，尤其在带阴影、透明 logo 的 PDF 上可见。
蒙版本身的 8 位灰度 + JPEG 是合法结构，所以不会校验失败，属于静默画质损失。

## 决策（实现方案）

### D1 `core/src/pdf/watermark.rs::ensure_resources_entry`（P1）

重写注册逻辑，兼容「Resources 子字典为内联字典 / 间接引用」：

1. 保持现有的 `resources_ref` → `resources_id` 解析、以及 `Resources` 内联时抽为独立对象的处理。
2. 取出 `category` 对象（克隆），三种情况：
   - 缺失：在 `resources_id` 字典中 `set(category, Object::Dictionary(dictionary! {}))`，
     再按内联字典处理；
   - `Object::Dictionary(_)`：在 `resources_id` 字典上 `get_mut(category)?.as_dict_mut()?.set(name, Reference(object_id))`；
   - `Object::Reference(cat_id)`：`doc.get_object_mut(cat_id)?.as_dict_mut()?.set(name, Reference(object_id))`；
   - 其他类型：返回 `CoreError::Internal("资源子字典类型异常")`。
3. 注意借用顺序：先在短作用域内克隆 `category` 对象，再对 `doc` 可变借用，避免
   `res_dict` 与 `doc.get_object_mut` 同时存活。
4. `ensure_resources_font` / `ensure_resources_xobject` 两个薄封装保持不变；
   文字水印与图片水印因此同时受益。

### D2 `core/src/pdf/watermark.rs::append_content`（P2）

先读取 `/Contents` 的原始值，再分支处理：

1. `Some(Object::Reference(id))`：
   - 若 `doc.get_object(id)?` 是 `Object::Array`，直接把 `Reference(new_content_id)`
     push 进该数组对象，页面 `/Contents` 保持原引用不变；
   - 否则（普通内容流引用）沿用 `[id, new_id]` 包装。
2. `Some(Object::Array(arr))`：沿用 push 到页面内联数组。
3. `None` / 其他：`set("Contents", Reference(new_content_id))`。
4. 现有「空白页无 /Contents 也成功」的行为保持不变。

### D3 `core/src/pdf/compress.rs`（P3）

1. 在重编码循环前先收集「被引用为蒙版的图片对象号」：

   ```rust
   // 遍历所有 Stream 且 Subtype == Image 的对象，读取 SMask / Mask；
   // Mask 可为单个引用或引用数组，逐个收集为 HashSet<ObjectId>。
   let protected: std::collections::HashSet<ObjectId> = ...
   ```

2. 循环改为 `for (id, obj) in doc.objects.iter_mut()`，`protected.contains(id)` 时跳过。
3. `shrink_image` 自身的 SMask/Mask/Decode 检查保持不变（两层保护互不替代）。
4. 其他策略（level 分档、字节没变小回滚、页数校验回退）一律不动。

## 非目标

- 不处理文字水印 CJK 字体（缺内置中文字体，成本高）。
- 不改 `compress` 最终兜底「复制加密原件」的极端路径（只在结构压缩产出坏文件时触发，概率极低）。
- 不改前端、pycore；不新增页面或功能项。
- 不动 `id_cards_to_pdf` 标签右侧裁切等已知小影响项。

## 测试与验证

### 新增单测（`core/src/pdf/watermark.rs` 新增 `#[cfg(test)] mod tests`）

1. **P1**：构造「页面 Resources → 间接引用字典；该字典 Font → 间接引用字典」的 PDF，
   调用 `add_text_watermark`，断言：
   - 返回 `Ok`；
   - 输出 PDF 可加载，页数 1；
   - 页面 Resources 的 Font 字典中存在 `F1`（未被原资源覆盖），且其指向 Font 对象。
2. **P2**：构造页面 `/Contents` 指向间接数组 `[stream1, stream2]` 的 PDF，调用
   `add_text_watermark`，断言：
   - 返回 `Ok`；输出可加载；
   - 该数组对象长度为 3，最后一个元素是新增内容流（解码后含 `Tj` 与水印文本）；
   - 原两个内容流对象仍在。

### 新增单测（`core/src/pdf/compress.rs::tests`）

3. **P3**：在现有 `build_pdf` 夹具旁新增「带 SMask 的图片」构造器（base 为 RGB JPEG，
   SMask 为 600×600 DeviceGray FlateDecode 流），level 9 压缩后断言
   `images_reencoded == 0`（base 因带 SMask 被跳过、SMask 因受保护被跳过），输出可加载。
4. 附加断言（同夹具再加一张无蒙版的大图时 `images_reencoded == 1`），确保保护集合
   不会误伤普通图片。

### 全量验证命令

1. `cargo fmt --all -- --check`
2. `cargo test -p pdf_docvault_core`
3. `cd tests/rust && cargo test -p pdf-docvault-tests --test pdf_flow`
4. `cargo clippy -p pdf_docvault_core --all-targets -- -D warnings`
5. `cd frontend && pnpm typecheck && pnpm build`
6. `cd frontend && ./node_modules/.bin/vitest run --root . > /tmp/vitest-run.log 2>&1`（读日志确认全绿）

## 验收标准

1. 对「Resources 子字典为间接引用」的 PDF 添加文字/图片水印不再报错，水印正常出现在页面上。
2. 对 `/Contents` 为间接数组的 PDF 添加水印后，正文与水印都能正常显示，页面结构合法。
3. 压缩透明图片（带 SMask）的 PDF 后，半透明边缘不出现明显的块状噪点/白边。
4. 上述全部验证命令通过（core 测试数在原 69 条基础上增加本轮新增用例）。

## 需要确认

以上 3 项改动是否全部实施？确认后按 tasks.md 执行。
