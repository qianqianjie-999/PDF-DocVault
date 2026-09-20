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
import type {
  GridOrder,
  ImageFitMode,
  ImagesPerA4,
  ImagesToA4Options,
  PageOrientation,
  ScanOptions,
} from '@/types/scan'

interface LocalFile {
  path: string
  name: string
}

const ACCEPT_EXT = /\.(png|jpe?g|webp|bmp|tiff?|gif)$/i

const files = ref<LocalFile[]>([])
const running = ref(false)
const showProgress = ref(false)
const progress = ref(0)
const current = ref(0)
const total = ref(0)
const currentName = ref('')

// 排版选项
const perPage = ref<ImagesPerA4>('one')
const orientation = ref<PageOrientation>('portrait')
const fitMode = ref<ImageFitMode>('contain')
const gridOrder = ref<GridOrder>('row_major')
const dpi = ref(150)
const jpegQuality = ref(85)
const marginMm = ref(6)
const gapMm = ref(3)
const enhance = ref<ScanOptions>({
  detect_boundary: false,
  perspective: false,
  remove_shadow: false,
  grayscale: false,
  adaptive_binarize: false,
  contrast_stretch: 0.0,
})

// 输出路径
const outputPath = ref('')

// 结果
const resultPages = ref(0)
const resultElapsed = ref(0)

const perPageOptions: Array<{ value: ImagesPerA4; label: string; desc: string }> = [
  { value: 'one', label: '1 张/页', desc: '整页居中,适合单据/证书' },
  { value: 'two', label: '2 张/页', desc: '上下两栏,适合证件正反面' },
  { value: 'four', label: '4 张/页', desc: '2×2 网格,适合多图汇总' },
]

const canRun = computed(() => !running.value && files.value.length > 0 && !!outputPath.value)

/** 根据排版密度预估页数 */
const estimatedPages = computed(() => {
  const n = files.value.length
  if (n === 0) return 0
  const per = perPage.value === 'one' ? 1 : perPage.value === 'two' ? 2 : 4
  return Math.ceil(n / per)
})

function onSelect(items: { path: string; name: string }[]) {
  for (const it of items) {
    if (!ACCEPT_EXT.test(it.path)) continue
    if (files.value.some((f) => f.path === it.path)) continue
    files.value.push({ path: it.path, name: it.name })
  }
  // 首次添加且输出路径为空:默认源文件所在目录
  if (!outputPath.value && files.value.length) {
    const first = files.value[0].path
    outputPath.value = `${dirname(first) || '.'}/images.pdf`
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

async function pickOutputFile() {
  const out = await save({
    defaultPath: 'images.pdf',
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  })
  if (out) outputPath.value = out
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
  running.value = true
  showProgress.value = true
  progress.value = 0
  current.value = 0
  total.value = files.value.length
  currentName.value = files.value[0]?.name ?? ''
  resultPages.value = 0
  resultElapsed.value = 0

  const taskId = `images-a4-${Date.now()}`
  const options: ImagesToA4Options = {
    enhance: enhance.value,
    per_page: perPage.value,
    orientation: orientation.value,
    fit_mode: fitMode.value,
    grid_order: gridOrder.value,
    margin_mm: marginMm.value,
    gap_mm: gapMm.value,
    dpi: dpi.value,
    jpeg_quality: jpegQuality.value,
  }
  try {
    const res = await withProgress(taskId, () =>
      scanApi.imagesToA4Pdf(
        files.value.map((f) => f.path),
        outputPath.value,
        options,
        taskId,
      ),
    )
    progress.value = 100
    resultPages.value = res.pages
    resultElapsed.value = res.elapsed_ms
    ElMessage.success(`合成完成:${res.pages} 页 A4 PDF,耗时 ${res.elapsed_ms}ms`)
    openOutput({ output: outputPath.value })
  } catch (e: any) {
    ElMessage.error(`合成 PDF 失败:${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (showProgress.value = false), 800)
  }
}
</script>

<template>
  <div class="images-pdf-view">
    <PageHeader
      title="图片转 PDF"
      subtitle="多图/单图合成 A4 PDF · 可选 1/2/4 张图每页 · 本地处理 · 文件不离开本机"
    />

    <div class="app-card">
      <FileDropZone
        accept="png,jpg,jpeg,webp,bmp,tif,tiff"
        :multiple="true"
        label="拖放图片到此处,或点击选择(支持 PNG/JPG/WebP/BMP/TIFF,可多选)"
        @select="onSelect"
      />

      <!-- 待合成图片清单 -->
      <div v-if="files.length" class="file-list">
        <div class="list-head">
          <span>图片列表({{ files.length }})· 排列顺序即页面顺序</span>
          <el-button link type="primary" size="small" @click="clearFiles">清空</el-button>
        </div>
        <div v-for="(f, idx) in files" :key="f.path" class="file-row">
          <span class="idx">{{ idx + 1 }}</span>
          <span class="name" :title="f.path">{{ f.name }}</span>
          <span class="ops">
            <el-button text size="small" :disabled="idx === 0" @click="move(idx, -1)">↑</el-button>
            <el-button
              text
              size="small"
              :disabled="idx === files.length - 1"
              @click="move(idx, 1)"
            >
              ↓
            </el-button>
            <el-icon class="remove" @click="removeFile(idx)"><Delete /></el-icon>
          </span>
        </div>
      </div>
    </div>

    <div class="app-card options-card">
      <!-- 排版密度(带迷你预览) -->
      <div class="option-block">
        <div class="option-label">每页图片数</div>
        <div class="layout-cards">
          <div
            v-for="o in perPageOptions"
            :key="o.value"
            class="layout-card"
            :class="{ active: perPage === o.value }"
            @click="perPage = o.value"
          >
            <!-- 迷你 A4 预览 -->
            <div class="mini-a4" :class="`mini-${o.value}`">
              <div
                v-for="n in (o.value === 'one' ? 1 : o.value === 'two' ? 2 : 4)"
                :key="n"
                class="mini-slot"
              ></div>
            </div>
            <div class="layout-info">
              <span class="p-label">{{ o.label }}</span>
              <span class="p-desc">{{ o.desc }}</span>
            </div>
          </div>
        </div>
        <div class="est-tip">预计生成 {{ estimatedPages }} 页 A4 PDF</div>
      </div>

      <!-- 页面布局 -->
      <div class="option-block">
        <div class="option-label">页面布局</div>
        <el-form label-width="120px" label-position="left" class="render-form">
          <el-form-item label="页面方向">
            <el-radio-group v-model="orientation">
              <el-radio value="portrait">纵向 A4</el-radio>
              <el-radio value="landscape">横向 A4</el-radio>
            </el-radio-group>
          </el-form-item>
          <el-form-item label="图片填充">
            <el-radio-group v-model="fitMode">
              <el-radio value="contain">完整显示(留白)</el-radio>
              <el-radio value="cover">等比填满(裁剪)</el-radio>
              <el-radio value="stretch">拉伸填满(变形)</el-radio>
            </el-radio-group>
          </el-form-item>
          <el-form-item label="排列顺序">
            <el-radio-group v-model="gridOrder">
              <el-radio value="row_major">行优先(→↓)</el-radio>
              <el-radio value="column_major">列优先(↓→)</el-radio>
            </el-radio-group>
          </el-form-item>
        </el-form>
      </div>

      <!-- 渲染参数 -->
      <div class="option-block">
        <div class="option-label">渲染参数</div>
        <el-form label-width="120px" label-position="left" class="render-form">
          <el-form-item label="渲染 DPI">
            <el-input-number v-model="dpi" :min="72" :max="600" :step="18" />
            <span class="opt-hint">越高越清晰,文件越大(默认 150)</span>
          </el-form-item>
          <el-form-item label="JPEG 质量">
            <el-slider
              v-model="jpegQuality"
              :min="50"
              :max="100"
              :step="5"
              show-input
              :show-input-controls="false"
              style="max-width: 400px"
            />
          </el-form-item>
          <el-form-item label="页边距(mm)">
            <el-input-number v-model="marginMm" :min="0" :max="40" :step="1" />
          </el-form-item>
          <el-form-item label="图片间距(mm)">
            <el-input-number v-model="gapMm" :min="0" :max="30" :step="1" />
          </el-form-item>
        </el-form>
      </div>

      <!-- 输出文件 -->
      <div class="option-block">
        <div class="option-label">PDF 输出路径</div>
        <div class="out-path">
          <el-input v-model="outputPath" placeholder="选择 PDF 保存路径" clearable />
          <el-button @click="pickOutputFile">浏览…</el-button>
        </div>
        <div class="tip">同名文件将被覆盖,请确认路径正确</div>
      </div>

      <el-button
        type="primary"
        size="large"
        :disabled="!canRun"
        :loading="running"
        @click="runConvert"
      >
        合成 PDF({{ files.length }} 张图 → {{ estimatedPages }} 页)
      </el-button>
    </div>

    <!-- 结果 -->
    <div v-if="resultPages" class="app-card result-card">
      <div class="result-summary">
        <span class="title">合成结果</span>
        <el-tag type="success" effect="light">{{ resultPages }} 页 A4</el-tag>
        <span class="muted">耗时 {{ resultElapsed }}ms</span>
      </div>
      <div class="result-path">
        输出文件:<span class="path" :title="outputPath">{{ basename(outputPath) }}</span>
      </div>
    </div>

    <ProgressOverlay
      v-if="showProgress"
      :visible="showProgress"
      :percent="progress"
      :current="current"
      :total="total"
      :text="currentName ? `正在排版:${currentName}(${current}/${total})` : ''"
    />
  </div>
</template>

<style scoped lang="scss">
.images-pdf-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
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
      width: 24px;
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
    .ops {
      display: flex;
      align-items: center;
      gap: 4px;
      .el-button + .el-button {
        margin-left: 0;
      }
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
  .est-tip {
    font-size: 12px;
    color: $police-blue;
    margin-top: 8px;
  }
  .layout-cards {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .layout-card {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 2px solid $border-color;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.2s;
    background: #fafbfc;
    &:hover {
      border-color: $police-blue;
      background: $police-blue-50;
    }
    &.active {
      border-color: $police-blue;
      background: $police-blue-50;
      box-shadow: 0 0 0 1px $police-blue;
    }
  }
  .mini-a4 {
    width: 24px;
    height: 34px;
    border: 1px solid #999;
    border-radius: 2px;
    display: flex;
    overflow: hidden;
    flex-shrink: 0;
    background: #fff;
  }
  .mini-slot {
    background: $police-blue;
    opacity: 0.5;
  }
  .mini-one {
    flex-direction: column;
    .mini-slot {
      flex: 1;
      margin: 2px;
      border-radius: 1px;
    }
  }
  .mini-two {
    flex-direction: column;
    .mini-slot {
      flex: 1;
      margin: 2px;
      border-radius: 1px;
    }
  }
  .mini-four {
    flex-wrap: wrap;
    .mini-slot {
      width: calc(50% - 3px);
      height: calc(50% - 3px);
      margin: 1.5px;
      border-radius: 1px;
    }
  }
  .layout-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    .p-label {
      font-size: 13px;
    }
    .p-desc {
      font-size: 11px;
    }
  }
  .render-form {
    max-width: 640px;
    .opt-hint {
      margin-left: 10px;
      font-size: 12px;
      color: $text-secondary;
    }
  }
  .out-path {
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

.result-card {
  .result-summary {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 10px;
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
  .result-path {
    font-size: 13px;
    color: $text-regular;
    .path {
      color: $police-blue;
      font-weight: 600;
      margin-left: 4px;
    }
  }
}
</style>
