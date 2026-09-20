<script setup lang="ts">
import { computed, ref } from 'vue'
import { open, save } from '@tauri-apps/plugin-dialog'
import { ElMessage } from 'element-plus'
import { Delete } from '@element-plus/icons-vue'
import PageHeader from '@/components/common/PageHeader.vue'
import FileDropZone from '@/components/pdf/FileDropZone.vue'
import ProgressOverlay from '@/components/pdf/ProgressOverlay.vue'
import { scanApi } from '@/api/scan'
import { basename, dirname } from '@/utils/file'
import { openOutput } from '@/utils/openOutput'
import type { ScanOptions } from '@/types/scan'

interface LocalFile {
  path: string
  name: string
}

const files = ref<LocalFile[]>([])
const running = ref(false)
const showProgress = ref(false)
const progress = ref(0)
const current = ref(0)
const total = ref(0)

// 扫描选项
const options = ref<ScanOptions>({
  detect_boundary: true,
  perspective: true,
  remove_shadow: true,
  grayscale: false,
  adaptive_binarize: false,
  contrast_stretch: 0.02,
})
const dpi = ref(150)

// 试卷去手写:笔迹颜色(blue/red/green/purple/orange/cyan,与 pycore 对应)
const HW_COLORS = [
  { key: 'blue', label: '蓝笔' },
  { key: 'red', label: '红笔' },
  { key: 'green', label: '绿笔' },
  { key: 'purple', label: '紫笔' },
  { key: 'orange', label: '橙笔' },
  { key: 'cyan', label: '青笔' },
]
const hwColors = ref<string[]>(['blue', 'red'])
const hwDilate = ref(2)

const canRun = computed(() => !running.value && files.value.length > 0)

function onSelect(items: { path: string; name: string }[]) {
  const IMAGE_EXT = /\.(png|jpe?g|webp|bmp|tiff?|gif)$/i
  for (const it of items) {
    if (!IMAGE_EXT.test(it.path)) continue
    if (files.value.some((f) => f.path === it.path)) continue
    files.value.push({ path: it.path, name: it.name })
  }
}

function removeFile(idx: number) {
  files.value.splice(idx, 1)
}

function clearFiles() {
  files.value = []
}

function move(idx: number, delta: number) {
  const target = idx + delta
  if (target < 0 || target >= files.value.length) return
  const arr = files.value
  ;[arr[idx], arr[target]] = [arr[target], arr[idx]]
}

// 监听进度事件
async function withProgress<T>(taskId: string, fn: () => Promise<T>): Promise<T> {
  const { listen } = await import('@tauri-apps/api/event')
  const unlisten = await listen<{
    task_id: string
    percent: number
    current: number
    total: number
  }>('task-progress', (e) => {
    if (e.payload.task_id !== taskId) return
    progress.value = e.payload.percent
    current.value = e.payload.current
    total.value = e.payload.total
  })
  try {
    return await fn()
  } finally {
    unlisten()
  }
}

async function runEnhance() {
  if (!canRun.value) return
  const first = files.value[0].path
  let outDir = await open({
    directory: true,
    title: '选择输出目录',
    defaultPath: dirname(first),
  })
  if (!outDir) {
    // 取消则默认输出到首图所在目录
    outDir = dirname(first)
  }
  running.value = true
  showProgress.value = true
  progress.value = 0
  try {
    const taskId = `scan-${Date.now()}`
    const res = await withProgress(taskId, () =>
      scanApi.enhanceImages(
        files.value.map((f) => f.path),
        outDir as string,
        options.value,
        taskId,
      ),
    )
    ElMessage.success(
      `扫描增强完成: ${res.outputs.length} 张,其中 ${res.corrected} 张检测到文档边界并矫正,耗时 ${res.elapsed_ms}ms`,
    )
    openOutput({ dir: outDir as string, firstOutput: res.outputs[0] })
  } catch (e: any) {
    ElMessage.error(`扫描增强失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (showProgress.value = false), 800)
  }
}

async function runRemoveHandwriting() {
  if (!canRun.value) return
  const first = files.value[0].path
  let outDir = await open({
    directory: true,
    title: '选择输出目录',
    defaultPath: dirname(first),
  })
  if (!outDir) {
    // 取消则默认输出到首图所在目录
    outDir = dirname(first)
  }
  running.value = true
  showProgress.value = true
  progress.value = 0
  try {
    const taskId = `handwriting-${Date.now()}`
    const res = await withProgress(taskId, () =>
      scanApi.removeHandwriting(
        files.value.map((f) => f.path),
        outDir as string,
        hwColors.value,
        hwDilate.value,
        taskId,
      ),
    )
    ElMessage.success(
      `去手写完成: ${res.images} 张,笔迹占比 ${(res.mask_ratio * 100).toFixed(1)}%,耗时 ${res.elapsed_ms}ms`,
    )
    openOutput({ dir: outDir as string, firstOutput: res.outputs[0] })
  } catch (e: any) {
    ElMessage.error(`去手写失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (showProgress.value = false), 800)
  }
}

async function runToPdf() {
  if (!canRun.value) return
  const out = await save({
    defaultPath: 'scanned.pdf',
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  })
  if (!out) return
  running.value = true
  showProgress.value = true
  progress.value = 0
  try {
    const taskId = `images-to-pdf-${Date.now()}`
    const res = await withProgress(taskId, () =>
      scanApi.imagesToPdf(
        files.value.map((f) => f.path),
        out as string,
        dpi.value,
        options.value,
        taskId,
      ),
    )
    ElMessage.success(`合成完成: ${res.pages} 页,耗时 ${res.elapsed_ms}ms`)
    openOutput({ output: out as string })
  } catch (e: any) {
    ElMessage.error(`合成 PDF 失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (showProgress.value = false), 800)
  }
}
</script>

<template>
  <div class="scan-view">
    <PageHeader title="文档扫描" subtitle="拍照变扫描件 · 本地处理 · 文件不离开本机" />
    <div class="scan-grid">
      <!-- 左侧:选图与列表 -->
      <div class="left-col">
        <FileDropZone
          accept="png,jpg,jpeg,webp,bmp,tif,tiff"
          label="点击选择照片,或拖拽图片到此处"
          @select="onSelect"
        />
        <div class="file-card app-card">
          <div class="card-head">
            <span class="title">图片列表({{ files.length }})</span>
            <el-button
              v-if="files.length"
              text
              size="small"
              type="danger"
              @click="clearFiles"
            >
              清空
            </el-button>
          </div>
          <div v-if="!files.length" class="empty-tip">尚未添加图片</div>
          <transition-group v-else name="list" tag="div" class="file-list">
            <div v-for="(f, i) in files" :key="f.path" class="file-item">
              <span class="idx">{{ i + 1 }}.</span>
              <span class="name" :title="f.path">{{ f.name }}</span>
              <span class="ops">
                <el-button text size="small" :disabled="i === 0" @click="move(i, -1)">↑</el-button>
                <el-button
                  text
                  size="small"
                  :disabled="i === files.length - 1"
                  @click="move(i, 1)"
                >
                  ↓
                </el-button>
                <el-button text size="small" type="danger" :icon="Delete" @click="removeFile(i)" />
              </span>
            </div>
          </transition-group>
          <div class="order-tip">列表顺序即 PDF 页面顺序</div>
        </div>
      </div>

      <!-- 右侧:增强选项与操作 -->
      <div class="right-col">
        <div class="options-card app-card">
          <div class="card-head">
            <span class="title">增强选项</span>
          </div>
          <el-form label-width="110px" label-position="left">
            <el-form-item label="边界检测">
              <el-switch v-model="options.detect_boundary" />
              <span class="opt-hint">自动识别文档轮廓</span>
            </el-form-item>
            <el-form-item label="透视矫正">
              <el-switch v-model="options.perspective" :disabled="!options.detect_boundary" />
              <span class="opt-hint">倾斜文档拉正</span>
            </el-form-item>
            <el-form-item label="去阴影">
              <el-switch v-model="options.remove_shadow" />
              <span class="opt-hint">光照均匀化</span>
            </el-form-item>
            <el-form-item label="灰度输出">
              <el-switch v-model="options.grayscale" :disabled="options.adaptive_binarize" />
            </el-form-item>
            <el-form-item label="黑白扫描">
              <el-switch v-model="options.adaptive_binarize" />
              <span class="opt-hint">自适应二值化</span>
            </el-form-item>
            <el-form-item label="对比度拉伸">
              <el-slider
                v-model="options.contrast_stretch"
                :min="0"
                :max="0.1"
                :step="0.005"
                show-input
                :show-input-controls="false"
              />
            </el-form-item>
            <el-form-item label="PDF DPI">
              <el-input-number v-model="dpi" :min="72" :max="600" :step="72" />
              <span class="opt-hint">仅影响合成 PDF 的页面尺寸</span>
            </el-form-item>
          </el-form>
          <div class="run-bar">
            <el-button
              type="primary"
              size="large"
              :loading="running"
              :disabled="!canRun"
              @click="runEnhance"
            >
              批量增强导出
            </el-button>
            <el-button
              type="success"
              size="large"
              :loading="running"
              :disabled="!canRun"
              @click="runToPdf"
            >
              合成 PDF
            </el-button>
          </div>
        </div>

        <!-- 试卷去手写 -->
        <div class="options-card app-card">
          <div class="card-head">
            <span class="title">试卷去手写</span>
            <span class="opt-hint">擦除彩色笔迹,还原空白试卷</span>
          </div>
          <el-form label-width="110px" label-position="left">
            <el-form-item label="笔迹颜色">
              <el-checkbox-group v-model="hwColors">
                <el-checkbox
                  v-for="c in HW_COLORS"
                  :key="c.key"
                  :value="c.key"
                  :label="c.label"
                />
              </el-checkbox-group>
            </el-form-item>
            <el-form-item label="边缘扩展">
              <el-slider
                v-model="hwDilate"
                :min="0"
                :max="4"
                :step="1"
                show-input
                :show-input-controls="false"
              />
              <span class="opt-hint">掩膜外扩,覆盖笔迹边缘</span>
            </el-form-item>
          </el-form>
          <div class="hw-tip">
            依据颜色擦除蓝/红等彩色笔迹;黑色笔与印刷同为灰色,无法区分,故不支持
          </div>
          <div class="run-bar">
            <el-button
              type="warning"
              size="large"
              :loading="running"
              :disabled="!canRun || hwColors.length === 0"
              @click="runRemoveHandwriting"
            >
              去手写并导出
            </el-button>
          </div>
        </div>
      </div>
    </div>

    <ProgressOverlay
      :visible="showProgress"
      :percent="progress"
      :current="current"
      :total="total"
    />
  </div>
</template>

<style scoped lang="scss">
.scan-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
  align-items: start;
}
.left-col,
.right-col {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.file-card,
.options-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.card-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  .title {
    font-weight: 600;
    color: $text-primary;
  }
}
.empty-tip {
  color: $text-secondary;
  font-size: 13px;
  padding: 12px 0;
}
.file-list {
  display: flex;
  flex-direction: column;
  max-height: 320px;
  overflow-y: auto;
}
.file-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 4px;
  border-bottom: 1px solid $border-color;
  font-size: 13px;
  &:last-child {
    border-bottom: none;
  }
  .idx {
    color: $text-secondary;
    width: 24px;
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ops {
    display: flex;
    align-items: center;
    .el-button + .el-button {
      margin-left: 0;
    }
  }
}
.order-tip {
  font-size: 12px;
  color: $text-secondary;
}
.opt-hint {
  margin-left: 10px;
  font-size: 12px;
  color: $text-secondary;
}
.run-bar {
  margin-top: 8px;
  display: flex;
  gap: 12px;
}
.hw-tip {
  font-size: 12px;
  color: $text-secondary;
  line-height: 1.6;
  padding: 4px 0;
}
.list-enter-active,
.list-leave-active {
  transition: all 0.2s;
}
.list-enter-from,
.list-leave-to {
  opacity: 0;
  transform: translateX(-8px);
}
</style>
