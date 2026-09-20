<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { ElMessage } from 'element-plus'
import { Delete } from '@element-plus/icons-vue'
import PageHeader from '@/components/common/PageHeader.vue'
import FileDropZone from '@/components/pdf/FileDropZone.vue'
import ProgressOverlay from '@/components/pdf/ProgressOverlay.vue'
import { officeApi } from '@/api/office'
import { basename, dirname } from '@/utils/file'
import { openOutput } from '@/utils/openOutput'
import { invoke } from '@tauri-apps/api/core'
import type {
  OfficeSelftestOutput,
  WordConvertReport,
  WordQualityLevel,
} from '@/types/office'

interface LocalFile {
  path: string
  name: string
}

const ACCEPT_EXT = /\.docx?$/i
const LAST_DIR_KEY = 'pdf-docvault-word2pdf-dir'

const files = ref<LocalFile[]>([])
const running = ref(false)
const showProgress = ref(false)
const progress = ref(0)
const current = ref(0)
const total = ref(0)
const currentName = ref('')

// 引擎自检
const envChecking = ref(true)
const env = ref<OfficeSelftestOutput | null>(null)

// 选项:质量等级与输出目录
const quality = ref<WordQualityLevel>('high')
const outputDir = ref('')

// 结果报告
const report = ref<WordConvertReport | null>(null)

const qualityOptions: Array<{ value: WordQualityLevel; label: string; desc: string }> = [
  { value: 'high', label: '高', desc: '原版图片分辨率 · JPEG 90%' },
  { value: 'medium', label: '中', desc: '图片 ≤150 DPI · JPEG 80%' },
  { value: 'low', label: '低', desc: '图片 ≤96 DPI · JPEG 60%' },
]

/** 引擎代号 → 展示名 */
const ENGINE_LABELS: Record<string, string> = {
  word_com: 'Word COM',
  wps_com: 'WPS COM',
  wps_cli: 'WPS 命令行',
  libreoffice: 'LibreOffice',
}
const engineLabel = (e: string) => ENGINE_LABELS[e] ?? e

const canRun = computed(
  () => !running.value && files.value.length > 0 && outputDir.value && env.value?.ok,
)

onMounted(async () => {
  // 恢复上次使用的输出目录(存在才回填)
  const last = localStorage.getItem(LAST_DIR_KEY)
  if (last) outputDir.value = last
  await checkEnv()
})

async function checkEnv() {
  envChecking.value = true
  try {
    env.value = await officeApi.selftest()
  } catch (e: any) {
    env.value = {
      ok: false,
      libreoffice: null,
      word_com: false,
      wps_com: false,
      wps_cli: null,
      message: String(e?.message ?? e),
    }
  } finally {
    envChecking.value = false
  }
}

function onSelect(items: { path: string; name: string }[]) {
  for (const it of items) {
    if (!ACCEPT_EXT.test(it.path)) continue
    if (files.value.some((f) => f.path === it.path)) continue
    files.value.push({ path: it.path, name: it.name })
  }
  // 首次添加且输出目录为空:默认源文件所在目录
  if (!outputDir.value && files.value.length) {
    outputDir.value = dirname(files.value[0].path)
  }
}

function removeFile(idx: number) {
  files.value.splice(idx, 1)
}

function clearFiles() {
  files.value = []
}

async function pickOutputDir() {
  const dir = await open({
    directory: true,
    title: '选择 PDF 保存目录',
    defaultPath: outputDir.value || files.value[0]?.path,
  })
  if (dir) {
    outputDir.value = dir
    localStorage.setItem(LAST_DIR_KEY, dir)
  }
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
    // 进度序号对应文件列表(转换顺序与添加顺序一致)
    const f = files.value[e.payload.current - 1]
    if (f) currentName.value = f.name
  })
  try {
    return await fn()
  } finally {
    unlisten()
  }
}

async function runConvert() {
  if (!canRun.value) return
  localStorage.setItem(LAST_DIR_KEY, outputDir.value)

  running.value = true
  showProgress.value = true
  progress.value = 0
  current.value = 0
  total.value = files.value.length
  currentName.value = files.value[0]?.name ?? ''
  report.value = null

  const taskId = `word2pdf-${Date.now()}`
  try {
    const res = await withProgress(taskId, () =>
      officeApi.convert(
        files.value.map((f) => f.path),
        outputDir.value,
        quality.value,
        taskId,
      ),
    )
    progress.value = 100
    report.value = res
    if (res.failed.length === 0) {
      ElMessage.success(`转换完成: ${res.succeeded.length} 个文件,耗时 ${res.elapsed_ms}ms`)
    } else {
      ElMessage.warning(
        `完成:成功 ${res.succeeded.length} 个,失败 ${res.failed.length} 个,详见下方报告`,
      )
    }
    // 有成功文件时自动打开输出目录并定位首个 PDF
    if (res.succeeded.length) {
      openOutput({ dir: outputDir.value, firstOutput: res.succeeded[0].output })
    }
  } catch (e: any) {
    ElMessage.error(`转换失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (showProgress.value = false), 800)
  }
}

/** 点击成功条目:在文件夹中定位该 PDF */
async function revealFile(path: string) {
  try {
    await invoke('reveal_path', { path })
  } catch {
    /* ignore */
  }
}
</script>

<template>
  <div class="word2pdf-view">
    <PageHeader
      title="Word 转 PDF"
      subtitle="批量 .doc/.docx 转 PDF · 保留原版排版与图片质量 · 本地转换 · 文件不离开本机"
    />

    <!-- 引擎缺失警告 -->
    <el-alert
      v-if="!envChecking && env && !env.ok"
      type="warning"
      :closable="false"
      show-icon
      title="未检测到可用的转换引擎"
      class="engine-alert"
    >
      <template #default>
        <p>{{ env.message }}。推荐安装 LibreOffice(免费,支持 .doc 与 .docx),或 WPS Office。</p>
        <p class="muted">
          UOS / 麒麟可在应用商店搜索 LibreOffice 或 WPS;Ubuntu 执行
          <code>sudo apt install libreoffice</code>;Windows 可从 libreoffice.org 或
          wps.cn 下载。
        </p>
      </template>
    </el-alert>

    <div class="app-card">
      <FileDropZone
        accept="doc,docx"
        label="拖放 Word 文档到此处,或点击选择(支持 .doc / .docx,可多选)"
        @select="onSelect"
      />

      <!-- 待转换文件清单(选中结果可见化) -->
      <div v-if="files.length" class="file-list">
        <div class="list-head">
          <span>待转换文件({{ files.length }})</span>
          <el-button link type="primary" size="small" @click="clearFiles">清空</el-button>
        </div>
        <div v-for="(f, idx) in files" :key="f.path" class="file-row">
          <span class="idx">{{ idx + 1 }}</span>
          <span class="name" :title="f.path">{{ f.name }}</span>
          <el-icon class="remove" @click="removeFile(idx)"><Delete /></el-icon>
        </div>
      </div>
    </div>

    <div class="app-card options-card">
      <!-- 质量等级 -->
      <div class="option-block">
        <div class="option-label">PDF 质量</div>
        <el-radio-group v-model="quality">
          <el-radio
            v-for="q in qualityOptions"
            :key="q.value"
            :value="q.value"
            class="quality-radio"
          >
            <span class="q-label">{{ q.label }}</span>
            <span class="q-desc">{{ q.desc }}</span>
          </el-radio>
        </el-radio-group>
      </div>

      <!-- 输出目录 -->
      <div class="option-block">
        <div class="option-label">PDF 保存目录</div>
        <div class="out-dir">
          <el-input v-model="outputDir" placeholder="选择 PDF 保存目录" clearable />
          <el-button @click="pickOutputDir">浏览…</el-button>
        </div>
        <div class="tip">同名文件自动加 ~N 后缀,不会覆盖已有成果</div>
      </div>

      <el-button
        type="primary"
        size="large"
        :disabled="!canRun"
        :loading="running"
        @click="runConvert"
      >
        开始转换({{ files.length }} 个文件)
      </el-button>
    </div>

    <!-- 转换结果报告 -->
    <div v-if="report" class="app-card report-card">
      <div class="report-summary">
        <span class="title">转换报告</span>
        <el-tag type="success" effect="light">成功 {{ report.succeeded.length }}</el-tag>
        <el-tag v-if="report.failed.length" type="danger" effect="light">
          失败 {{ report.failed.length }}
        </el-tag>
        <span class="muted">引擎:{{ engineLabel(report.engine) }} · 耗时 {{ report.elapsed_ms }}ms</span>
      </div>

      <!-- 成功列表 -->
      <div v-if="report.succeeded.length" class="result-section">
        <div class="section-title success">✅ 成功({{ report.succeeded.length }})</div>
        <div
          v-for="item in report.succeeded"
          :key="item.input"
          class="result-row"
          @click="revealFile(item.output)"
        >
          <span class="src" :title="item.input">{{ basename(item.input) }}</span>
          <span class="arrow">→</span>
          <span class="dst" :title="item.output">{{ basename(item.output) }}</span>
        </div>
      </div>

      <!-- 失败列表 -->
      <div v-if="report.failed.length" class="result-section">
        <div class="section-title fail">❌ 失败({{ report.failed.length }})</div>
        <div v-for="item in report.failed" :key="item.input" class="result-row fail-row">
          <span class="src" :title="item.input">{{ basename(item.input) }}</span>
          <span class="reason">{{ item.reason }}</span>
        </div>
      </div>
    </div>

    <ProgressOverlay
      v-if="showProgress"
      :visible="showProgress"
      :percent="progress"
      :current="current"
      :total="total"
      :text="currentName ? `正在转换: ${currentName}(${current}/${total})` : ''"
    />
  </div>
</template>

<style scoped lang="scss">
.engine-alert {
  margin-bottom: 16px;
  p {
    margin: 4px 0;
  }
  .muted {
    color: #909399;
    font-size: 12px;
  }
}

.file-list {
  margin-top: 16px;
  border-top: 1px solid $border-color;
  padding-top: 12px;

  .list-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 13px;
    color: $text-secondary;
    margin-bottom: 8px;
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 4px;
    border-radius: 4px;
    &:hover {
      background: $police-blue-50;
      .remove {
        visibility: visible;
      }
    }
    .idx {
      width: 20px;
      text-align: center;
      font-size: 12px;
      color: $text-secondary;
    }
    .name {
      flex: 1;
      font-size: 13px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .remove {
      cursor: pointer;
      color: $text-secondary;
      visibility: hidden;
      &:hover {
        color: #f56c6c;
      }
    }
  }
}

.options-card {
  display: flex;
  flex-direction: column;
  gap: 18px;
  align-items: stretch;

  .option-label {
    font-size: 13px;
    font-weight: 600;
    color: $text-regular;
    margin-bottom: 10px;
  }
  .quality-radio {
    display: flex;
    align-items: baseline;
    margin-right: 24px;
    .q-label {
      font-weight: 600;
      margin-right: 6px;
    }
    .q-desc {
      font-size: 12px;
      color: $text-secondary;
    }
  }
  .out-dir {
    display: flex;
    gap: 10px;
    max-width: 640px;
  }
  .tip {
    font-size: 12px;
    color: $text-secondary;
    margin-top: 6px;
  }
}

.report-card {
  .report-summary {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
    .title {
      font-weight: 600;
      font-size: 15px;
    }
    .muted {
      margin-left: auto;
      font-size: 12px;
      color: $text-secondary;
    }
  }
  .section-title {
    font-size: 13px;
    font-weight: 600;
    margin: 10px 0 6px;
    &.success {
      color: #67c23a;
    }
    &.fail {
      color: #f56c6c;
    }
  }
  .result-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
    &:hover {
      background: $police-blue-50;
    }
    .src,
    .dst {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .src {
      max-width: 45%;
      color: $text-regular;
    }
    .arrow {
      color: $text-secondary;
    }
    .dst {
      color: $police-blue;
    }
  }
  .fail-row {
    cursor: default;
    &:hover {
      background: transparent;
    }
    .reason {
      color: #f56c6c;
      font-size: 12px;
    }
  }
}
</style>
