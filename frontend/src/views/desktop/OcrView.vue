<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'
import { ElMessage } from 'element-plus'
import PageHeader from '@/components/common/PageHeader.vue'
import FileDropZone from '@/components/pdf/FileDropZone.vue'
import ProgressOverlay from '@/components/pdf/ProgressOverlay.vue'
import { ocrApi } from '@/api/ocr'
import { basename } from '@/utils/file'
import { openOutput } from '@/utils/openOutput'
import type { ConvertMode, OcrOptions, OcrPage, OcrSelftestOutput } from '@/types/ocr'

interface LocalFile {
  path: string
  name: string
  isPdf: boolean
}

const files = ref<LocalFile[]>([])
const running = ref(false)
const showProgress = ref(false)
const progress = ref(0)
const current = ref(0)
const total = ref(0)

// 环境自检状态
const envChecking = ref(true)
const env = ref<OcrSelftestOutput | null>(null)

// 选项
const options = ref<OcrOptions>({
  engine: null,
  lang: null,
  dpi: 200,
  timeout_secs: 300,
})

// 输出模式:text=纯文本识别,docx=拍图转 Word,xlsx=表格转 Excel
const outMode = ref<'text' | ConvertMode>('text')

// 结果
const pages = ref<OcrPage[]>([])
const engineUsed = ref('')
const elapsedMs = ref(0)
const activeTab = ref('0')

const PDF_EXT = /\.pdf$/i
const ACCEPT_EXT = /\.(png|jpe?g|webp|bmp|tiff?|pdf)$/i

const canRun = computed(() => !running.value && files.value.length > 0 && env.value?.ok)

const allText = computed(() => pages.value.map((p) => p.text).join('\n\n'))

onMounted(checkEnv)

async function checkEnv() {
  envChecking.value = true
  try {
    env.value = await ocrApi.selftest()
  } catch (e: any) {
    env.value = { ok: false, engine: null, message: String(e?.message ?? e) }
  } finally {
    envChecking.value = false
  }
}

function onSelect(items: { path: string; name: string }[]) {
  for (const it of items) {
    if (!ACCEPT_EXT.test(it.path)) continue
    if (files.value.some((f) => f.path === it.path)) continue
    files.value.push({ path: it.path, name: it.name, isPdf: PDF_EXT.test(it.path) })
  }
}

function removeFile(idx: number) {
  files.value.splice(idx, 1)
}

function clearFiles() {
  files.value = []
  pages.value = []
}

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

async function runOcr() {
  if (!canRun.value) return
  // 文档转换模式:弹出保存对话框后走 convert 链路
  if (outMode.value !== 'text') {
    await runConvert(outMode.value)
    return
  }
  running.value = true
  showProgress.value = true
  progress.value = 0
  pages.value = []
  try {
    const taskId = `ocr-${Date.now()}`
    const res = await withProgress(taskId, () =>
      ocrApi.extractImages(
        files.value.map((f) => f.path),
        options.value,
        taskId,
      ),
    )
    pages.value = res.pages
    engineUsed.value = res.engine
    elapsedMs.value = res.elapsed_ms
    activeTab.value = '0'
    ElMessage.success(
      `识别完成: ${res.pages.length} 页,引擎 ${res.engine},耗时 ${res.elapsed_ms}ms`,
    )
  } catch (e: any) {
    ElMessage.error(`OCR 识别失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (showProgress.value = false), 800)
  }
}

async function runConvert(mode: ConvertMode) {
  const isXlsx = mode === 'xlsx'
  const first = files.value[0]?.name ?? 'ocr'
  const out = await save({
    defaultPath: first.replace(/\.[^.]+$/, '') + (isXlsx ? '.xlsx' : '.docx'),
    filters: isXlsx
      ? [{ name: 'Excel', extensions: ['xlsx'] }]
      : [{ name: 'Word', extensions: ['docx'] }],
  })
  if (!out) return
  running.value = true
  showProgress.value = true
  progress.value = 0
  try {
    const taskId = `ocr-convert-${Date.now()}`
    const res = await withProgress(taskId, () =>
      ocrApi.convert(
        files.value.map((f) => f.path),
        out as string,
        mode,
        options.value,
        taskId,
      ),
    )
    const detail = isXlsx
      ? `还原 ${res.rows ?? 0} 行表格`
      : `生成 ${res.lines ?? 0} 行文本`
    ElMessage.success(
      `${isXlsx ? '转 Excel' : '转 Word'}完成: ${res.images} 张图,${detail},耗时 ${res.elapsed_ms}ms`,
    )
    openOutput({ output: out as string })
  } catch (e: any) {
    ElMessage.error(`转换失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (showProgress.value = false), 800)
  }
}

async function copyAll() {
  try {
    await navigator.clipboard.writeText(allText.value)
    ElMessage.success('已复制到剪贴板')
  } catch {
    ElMessage.error('复制失败')
  }
}

async function exportTxt() {
  if (!pages.value.length) return
  const first = files.value[0]?.name ?? 'ocr'
  const defaultName = first.replace(/\.[^.]+$/, '') + '-ocr.txt'
  const out = await save({
    defaultPath: defaultName,
    filters: [{ name: '文本', extensions: ['txt'] }],
  })
  if (!out) return
  try {
    await writeFile(out, new TextEncoder().encode(allText.value))
    ElMessage.success(`已导出: ${basename(out)}`)
    openOutput({ output: out as string })
  } catch (e: any) {
    ElMessage.error(`导出失败: ${e?.message ?? e}`)
  }
}
</script>

<template>
  <div class="ocr-view">
    <PageHeader title="OCR 识别" subtitle="图片/扫描件转文字 · 本地推理 · 文件不离开本机" />

    <!-- 环境状态条 -->
    <el-alert
      v-if="env"
      :type="env.ok ? 'success' : 'warning'"
      :closable="false"
      class="env-bar"
    >
      <template #title>
        <span v-if="envChecking">正在检测 OCR 环境…</span>
        <span v-else-if="env.ok">
          OCR 环境就绪(引擎: {{ env.engine }}<template v-if="env.message">,可用: {{ env.message }}</template>)
        </span>
        <span v-else>OCR 环境未就绪:{{ env.message }}</span>
      </template>
    </el-alert>

    <div class="ocr-grid">
      <!-- 左侧:文件列表 -->
      <div class="left-col">
        <FileDropZone
          accept="png,jpg,jpeg,webp,bmp,tif,tiff,pdf"
          label="点击选择图片/PDF,或拖拽到此处"
          @select="onSelect"
        />
        <div class="file-card app-card">
          <div class="card-head">
            <span class="title">文件列表({{ files.length }})</span>
            <el-button v-if="files.length" text size="small" type="danger" @click="clearFiles">
              清空
            </el-button>
          </div>
          <div v-if="!files.length" class="empty-tip">尚未添加文件</div>
          <div v-else class="file-list">
            <div v-for="(f, i) in files" :key="f.path" class="file-item">
              <span class="badge" :class="f.isPdf ? 'pdf' : 'img'">{{ f.isPdf ? 'PDF' : '图片' }}</span>
              <span class="name" :title="f.path">{{ f.name }}</span>
              <el-button text size="small" type="danger" @click="removeFile(i)">✕</el-button>
            </div>
          </div>
        </div>
      </div>

      <!-- 右侧:选项与操作 -->
      <div class="right-col">
        <div class="options-card app-card">
          <div class="card-head">
            <span class="title">识别选项</span>
          </div>
          <el-form label-width="110px" label-position="left">
            <el-form-item label="输出模式">
              <el-radio-group v-model="outMode">
                <el-radio-button value="text">纯文本</el-radio-button>
                <el-radio-button value="docx">转 Word</el-radio-button>
                <el-radio-button value="xlsx">转 Excel</el-radio-button>
              </el-radio-group>
              <span class="opt-hint">
                {{ outMode === 'xlsx' ? '表格图片还原为工作表' : outMode === 'docx' ? '识别文字生成可编辑文档' : '分页展示并支持导出 TXT' }}
              </span>
            </el-form-item>
            <el-form-item label="PDF 渲染 DPI">
              <el-input-number v-model="options.dpi" :min="96" :max="400" :step="48" />
              <span class="opt-hint">PDF 先渲染为图片再识别,越高越准越慢</span>
            </el-form-item>
            <el-form-item label="超时(秒)">
              <el-input-number v-model="options.timeout_secs" :min="60" :max="1800" :step="60" />
            </el-form-item>
          </el-form>
          <div class="run-bar">
            <el-button
              type="primary"
              size="large"
              :loading="running"
              :disabled="!canRun"
              @click="runOcr"
            >
              {{ outMode === 'xlsx' ? '识别表格并导出' : outMode === 'docx' ? '识别并生成 Word' : '开始识别' }}
            </el-button>
          </div>
        </div>
      </div>
    </div>

    <!-- 识别结果(仅纯文本模式) -->
    <div v-if="outMode === 'text' && pages.length" class="result-card app-card">
      <div class="card-head">
        <span class="title">
          识别结果({{ pages.length }} 页 · {{ engineUsed }} · {{ elapsedMs }}ms)
        </span>
        <span class="ops">
          <el-button size="small" @click="copyAll">复制全文</el-button>
          <el-button size="small" type="primary" @click="exportTxt">导出 TXT</el-button>
        </span>
      </div>
      <el-tabs v-model="activeTab">
        <el-tab-pane
          v-for="p in pages"
          :key="p.index"
          :label="`${p.index + 1}. ${p.file}`"
          :name="String(p.index)"
        >
          <pre class="ocr-text">{{ p.text || '(本页未识别到文字)' }}</pre>
        </el-tab-pane>
      </el-tabs>
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
.env-bar {
  margin-bottom: 16px;
}
.ocr-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
  align-items: start;
  margin-bottom: 20px;
}
.left-col,
.right-col {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.file-card,
.options-card,
.result-card {
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
  max-height: 300px;
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
  .badge {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    &.pdf {
      background: $police-blue-50;
      color: $police-blue;
    }
    &.img {
      background: #f0f9eb;
      color: #67c23a;
    }
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
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
.result-card {
  margin-bottom: 24px;
}
.ocr-text {
  margin: 0;
  padding: 12px;
  background: #fafbfc;
  border: 1px solid $border-color;
  border-radius: $radius-base;
  font-size: 13px;
  line-height: 1.8;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 420px;
  overflow-y: auto;
}
</style>
