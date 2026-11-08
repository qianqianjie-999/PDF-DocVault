# 规格：长任务可取消（OCR / 去手写 / 转换）

- 日期：2026-11-07
- 范围：Rust core（`pybridge.rs`、`ocr.rs`、`image/handwriting.rs`、`task/manager.rs`）+
  命令层（`ocr_commands.rs`、`scan_commands.rs`）+ 前端（`ProgressOverlay.vue`、`OcrView.vue`、`ScanView.vue`）
- 目标：让用户能在任务进行中取消 OCR 识别 / 拍图转换 / 试卷去手写三个 Python 长任务；
  纯 Rust 快任务（扫描增强、合成 PDF 等）本轮不接取消。
- 现状：`TaskManager` 已具备状态机（`cancel()` / `is_cancelled()`），命令层 `cancel_task` 已暴露，
  前端 `taskApi.cancel` 已存在，但**全项目无人调用 `is_cancelled()`**、无执行点真正中断、
  前端无取消按钮。属半实现状态，为本轮补齐。

## 背景与问题

- `core/src/task/manager.rs`：`cancel(id)` 仅把状态置为 `Cancelled`；`is_cancelled(id)` 查询。
  但 OCR/handwriting 命令从不 `register` 任务，也不轮询 `is_cancelled`，子进程只靠一次性
  `recv_timeout(timeout)` 等待，超时才 kill。因此取消按钮即使点了也无效。
- `core/src/pybridge.rs::spawn_with_timeout_watched`（L191-L269）一次性 `rx.recv_timeout(timeout)`，
  无中断检查；`run_pycore_inner`（L337-L399）组装命令并调用它。
- `core/src/errors.rs` 已有 `CoreError::TaskCancelled`，code=`TASK_CANCELLED`，message=`任务已取消`。
- 命令层 `ocr_commands.rs` / `scan_commands.rs` 的 `spawn_blocking` 用 `...??` 直接把错误上抛，
  未区分取消。
- 前端 `ProgressOverlay.vue` 纯展示无取消按钮；`OcrView.vue` / `ScanView.vue` 在本地生成 taskId，
  任务期间遮罩阻塞、无法取消。

## 决策（实现方案）

### D1 核心：可中断的 pycore 执行（`core/src/pybridge.rs`）

1. 新增 `pub(crate) type AbortCheck = dyn Fn() -> bool + Send + Sync;`
2. `spawn_with_timeout_watched` 增加第 5 参 `abort: Option<&AbortCheck>`：
   - 把一次性 `rx.recv_timeout(timeout)` 改为轮询循环：每 200ms `recv_timeout(poll)`。
   - 每轮先 `abort()`：为 true → `kill()` + `wait()` + `return Err(CoreError::TaskCancelled)`。
   - 累计等待达 `timeout` → 维持现有 `kill()` + `OCR_TIMEOUT` 错误（语义不变）。
   - `Disconnected` 分支保持不变。
3. `run_pycore_inner` 增加 `abort: Option<&AbortCheck>` 尾参并透传给 spawn；
   `run_pycore` / `run_pycore_stdin` / `run_pycore_watched` 传 `None`（其余调用点不动）。
4. 新增 `run_pycore_cancellable(pycore_dir, subcommand, input_args, timeout_secs, cleanup, abort: Option<&AbortCheck>)`
   → 调用 `run_pycore_inner(..., None, None, abort)`，供 OCR / 转换 / 去手写使用。

### D2 核心：OCR / 转换 / 去手写接入取消（`core/src/ocr.rs`、`core/src/image/handwriting.rs`）

1. `core/src/ocr.rs` 新增 `pub type CancelCheck = dyn Fn() -> bool + Send + Sync;`
2. `prepare_images` 增加尾参 `cancel: Option<&CancelCheck>`：每次进入输入循环前检查
   `cancel.map(|c| c()).unwrap_or(false)` → 清理 `tmp_dir` 并 `return Err(CoreError::TaskCancelled)`。
3. `ocr_extract(inputs, pycore_dir, opts, progress, cancel: Option<&CancelCheck>)`：
   - 将 `cancel` 传给 `prepare_images`；
   - Python 阶段由 `run_pycore` 改为 `run_pycore_cancellable(..., cancel)`。
4. `ocr_convert(inputs, output, mode, pycore_dir, opts, progress, cancel)`：同上两处。
5. `core/src/image/handwriting.rs::remove_handwriting(inputs, out_dir, pycore_dir, opts, progress, cancel)`：
   - 将 `cancel` 传给 `prepare_images`；
   - Python 阶段由 `run_pycore` 改为 `run_pycore_cancellable(..., cancel)`（修正 import）。
6. **签名变更的既有调用点一并更新**：`ocr.rs` 内两个 end-to-end 测试、`prepare_images` 测试用例、
   `handwriting.rs` 的 end-to-end 测试都补一个 `None` 尾参。

### D3 任务注册与取消判定（`core/src/task/manager.rs`）

1. `TaskKind` 增加两个变体：`Ocr`、`Handwriting`（serde snake_case → `"ocr"` / `"handwriting"`）。
   全项目无对该枚举的穷举 match，仅 `manager.rs` 自身与 `pdf_commands.rs::ToImages` 使用，新增安全。

### D4 命令层：注册任务并把取消闭包传入核心（`ocr_commands.rs`、`scan_commands.rs`）

对 `ocr_extract_images`、`ocr_convert_documents`、`remove_handwriting` 三个命令：

1. 入参增加 `state: State<'_, AppState>`（import `crate::state::AppState` 与 `tauri::State`）。
2. 执行前 `state.tasks.register(&task_id, kind, 0)` + `state.tasks.start(&task_id)`，
   kind：OCR 提取与转换用 `TaskKind::Ocr`，去手写用 `TaskKind::Handwriting`。
3. 构造取消闭包 `let cancel = move || state_tasks_clone.is_cancelled(&task_id_clone);`，
   以 `Some(&cancel)` 传入核心函数。
4. 把 `spawn_blocking(...).await.map_err(...)??` 改为显式 match，区分四态：
   - `Ok(Ok(v))` → `complete` + `emit_complete("done")` + 返回 v；
   - `Ok(Err(CoreError::TaskCancelled))` → `state.tasks.cancel` + `emit_complete("cancelled")` +
     `return Err(CoreError::TaskCancelled.into())`（code=`TASK_CANCELLED`）；
   - `Ok(Err(e))` → `state.tasks.fail(&msg)` + `return Err(e.into())`；
   - `Err(join)` → `state.tasks.fail(&msg)` + `return Err(AppError { code:"JOIN", ... })`。

### D5 前端：取消按钮与提示（`ProgressOverlay.vue`、`OcrView.vue`、`ScanView.vue`）

1. `ProgressOverlay.vue`：
   - 新增 props `cancellable?: boolean`、`cancelling?: boolean`（默认 false）；
   - `defineEmits<{ cancel: [] }>()`；
   - `cancellable` 为真时在 footer 显示「取消任务」按钮（`type="danger"`，`:loading="cancelling"`，
     `@click="emit('cancel')"`）。
2. `OcrView.vue`（识别/转 Word/转 Excel 三种模式全部可取消）：
   - import `taskApi`；新增 `activeTaskId` 与 `cancelling` ref；
   - `runOcr` / `runConvert` 内把 taskId 赋给 `activeTaskId`，finally 里置空；
   - 新增 `async function cancelTask()`：置 `cancelling`，`await taskApi.cancel(activeTaskId.value)`；
   - catch 分支：`e?.code === 'TASK_CANCELLED'` 时 `ElMessage.info('任务已取消')`，否则保持原错误提示；
   - `<ProgressOverlay>` 绑定 `:cancellable="running"` `:cancelling="cancelling"` `@cancel="cancelTask"`。
3. `ScanView.vue`（仅去手写可取消，扫描增强/合成 PDF 不接）：
   - import `taskApi`；新增 `activeTaskId`、`cancelling`、`canCancel` ref；
   - `runRemoveHandwriting` 赋 taskId 且 `canCancel = true`；`runEnhance` / `runToPdf` 置 `canCancel = false`；
   - 新增 `cancelTask()` 同上；
   - catch 分支区分 `TASK_CANCELLED`；
   - `<ProgressOverlay>` 绑定 `:cancellable="canCancel"` `:cancelling="cancelling"` `@cancel="cancelTask"`。

## 非目标

- 扫描增强、合成 PDF、A4 排版、证件排版等纯 Rust 快任务，本轮不接取消。
- 不新增全局任务队列 / 并发取消 / 批量进度聚合等能力。
- 不改变 `task-progress` / `task-complete` 事件契包已有的字段。

## 测试与验证

### 新增单测（`core/src/task/manager.rs::tests`）

- 「取消状态往返」：`register` 后 `is_cancelled == false` → `cancel` 后 `is_cancelled == true`
  → `status` 为 `TaskState::Cancelled`。

### 既有调用点回归

- 确保 `ocr_extract` / `ocr_convert` / `remove_handwriting` / `prepare_images` 的新增 `None` 尾参已同步更新，
  core 与集成测试全部编译通过。

### 全量验证命令

1. `cargo fmt --all -- --check`
2. `cargo test -p pdf_docvault_core`
3. `cd tests/rust && cargo test -p pdf-docvault-tests --test pdf_flow`
4. `cargo clippy -p pdf_docvault_core --all-targets -- -D warnings`
5. `cd frontend && pnpm typecheck && pnpm build`
6. `cd frontend && ./node_modules/.bin/vitest run --root . > /tmp/vitest-run.log 2>&1`（读日志确认全绿）

## 验收标准

1. 任务进行中点击「取消任务」，OCR 识别 / 拍图转换 / 试卷去手写能被中断，遮罩关闭并提示「任务已取消」。
2. 取消通过 ``cancel_task` → `TaskManager.cancel` → 核心轮询 `is_cancelled` / Python 子进程 kill` 链路生效，
   无残留运行中的 Python 进程。
3. core 测试在原 75 passed/5 ignored 基础上新增取消状态用例；pdf_flow 42 passed、vitest 全绿不受影响。

## 需要确认

以上方案是否按此实施？确认后按 tasks.md 执行。