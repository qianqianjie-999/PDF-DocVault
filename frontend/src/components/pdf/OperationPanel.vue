<script setup lang="ts">
import { ref, computed } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import { ElMessage } from 'element-plus'
import { pdfApi } from '@/api/pdf'
import { useFilesStore } from '@/stores/files'
import { useTaskStore } from '@/stores/task'
import { basename, dirname, joinPath } from '@/utils/file'
import { openOutput } from '@/utils/openOutput'
import ProgressOverlay from './ProgressOverlay.vue'
import type { SplitMode } from '@/types/pdf'

const filesStore = useFilesStore()
const taskStore = useTaskStore()
const running = ref(false)
const progress = ref(0)
const current = ref(0)
const total = ref(0)
const showProgress = ref(false)

type OpType =
  | 'merge'
  | 'split'
  | 'rotate'
  | 'delete'
  | 'extract'
  | 'encrypt'
  | 'decrypt'
  | 'watermark'
  | 'compress'
  | 'toImages'

const op = ref<OpType>('merge')

const operations: { key: OpType; label: string; icon: string }[] = [
  { key: 'merge', label: '合并', icon: '🔗' },
  { key: 'split', label: '拆分', icon: '✂️' },
  { key: 'rotate', label: '旋转', icon: '🔄' },
  { key: 'delete', label: '删除页', icon: '🗑️' },
  { key: 'extract', label: '提取页', icon: '📤' },
  { key: 'encrypt', label: '加密', icon: '🔒' },
  { key: 'decrypt', label: '解密', icon: '🔓' },
  { key: 'watermark', label: '水印', icon: '💧' },
  { key: 'compress', label: '压缩', icon: '📦' },
  { key: 'toImages', label: '转图片', icon: '🖼️' },
]

// 参数
const splitMode = ref<'ranges' | 'everyN'>('everyN')
const splitN = ref(50)
const splitRanges = ref('1-10,11-20')
const rotateDegrees = ref(90)
const pageInput = ref('') // 旋转/删除/提取通用页码输入
const encryptPassword = ref('')
const encryptBits = ref(256)
const decryptPassword = ref('')
const watermarkText = ref('PDF-DocVault')
const watermarkFontSize = ref(48)
const watermarkOpacity = ref(0.3)
const watermarkRotation = ref(45)
const compressLevel = ref(6)
const imagesDpi = ref(150)
const imagesFormat = ref<'png' | 'jpg' | 'webp'>('png')

// 需要单文件的操作
const needsSingleFile = computed(() =>
  ['split', 'rotate', 'delete', 'extract', 'encrypt', 'decrypt', 'watermark', 'compress', 'toImages'].includes(
    op.value,
  ),
)

// 需要多文件的操作
const needsMultiFile = computed(() => op.value === 'merge')

const canRun = computed(() => {
  if (running.value) return false
  if (needsSingleFile.value) return filesStore.selectedFiles.length === 1
  if (needsMultiFile.value) return filesStore.selectedFiles.length >= 2
  return filesStore.selectedFiles.length > 0
})

function parsePageNums(s: string): number[] {
  return s
    .split(',')
    .map((t) => parseInt(t.trim(), 10))
    .filter((n) => !isNaN(n) && n > 0)
}

function parseRanges(s: string): { start: number; end: number }[] {
  return s
    .split(',')
    .map((part) => {
      const [a, b] = part.trim().split('-').map((x) => parseInt(x, 10))
      if (isNaN(a)) return null
      return { start: a, end: isNaN(b) ? a : b }
    })
    .filter((r): r is { start: number; end: number } => r !== null)
}

async function pickOutput(defaultName: string): Promise<string | null> {
  return await save({
    defaultPath: defaultName,
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  })
}

async function run() {
  if (!canRun.value) {
    ElMessage.warning(needsSingleFile.value ? '请选择一个文件' : '请选择至少两个文件')
    return
  }
  const files = filesStore.selectedFiles
  running.value = true
  showProgress.value = true
  progress.value = 0
  try {
    switch (op.value) {
      case 'merge': {
        const out = await pickOutput('merged.pdf')
        if (!out) break
        const res = await pdfApi.merge(files.map((f) => f.path), out)
        ElMessage.success(`合并完成: ${res.pages} 页,耗时 ${res.elapsed_ms}ms`)
        openOutput({ output: out })
        break
      }
      case 'split': {
        const first = files[0].path
        const dir = dirname(first)
        const mode: SplitMode =
          splitMode.value === 'everyN'
            ? { kind: 'every_n_pages', n: splitN.value }
            : { kind: 'by_ranges', ranges: parseRanges(splitRanges.value) }
        const res = await pdfApi.split(first, mode, dir)
        ElMessage.success(`拆分完成: ${res.outputs.length} 个文件`)
        openOutput({ dir, firstOutput: res.outputs[0] })
        break
      }
      case 'rotate': {
        const first = files[0].path
        const out = await pickOutput(`rotated-${basename(first)}`)
        if (!out) break
        await pdfApi.rotate(first, parsePageNums(pageInput.value), rotateDegrees.value, out)
        ElMessage.success('旋转完成')
        openOutput({ output: out })
        break
      }
      case 'delete': {
        const first = files[0].path
        const out = await pickOutput(`trimmed-${basename(first)}`)
        if (!out) break
        await pdfApi.deletePages(first, parsePageNums(pageInput.value), out)
        ElMessage.success('删除页面完成')
        openOutput({ output: out })
        break
      }
      case 'extract': {
        const first = files[0].path
        const out = await pickOutput(`extracted-${basename(first)}`)
        if (!out) break
        await pdfApi.extract(first, parsePageNums(pageInput.value), out)
        ElMessage.success('提取完成')
        openOutput({ output: out })
        break
      }
      case 'encrypt': {
        const first = files[0].path
        const out = await pickOutput(`encrypted-${basename(first)}`)
        if (!out) break
        await pdfApi.encrypt(first, out, {
          user_password: encryptPassword.value,
          owner_password: null,
          permissions: { allow_print: true, allow_copy: true, allow_modify: false, allow_annotate: false },
          bits: encryptBits.value,
        })
        ElMessage.success('加密完成')
        openOutput({ output: out })
        break
      }
      case 'decrypt': {
        const first = files[0].path
        const out = await pickOutput(`decrypted-${basename(first)}`)
        if (!out) break
        await pdfApi.decrypt(first, out, decryptPassword.value)
        ElMessage.success('解密完成')
        openOutput({ output: out })
        break
      }
      case 'watermark': {
        const first = files[0].path
        const out = await pickOutput(`watermarked-${basename(first)}`)
        if (!out) break
        await pdfApi.watermark(first, out, {
          text: watermarkText.value,
          options: {
            font_size: watermarkFontSize.value,
            rotation: watermarkRotation.value,
            opacity: watermarkOpacity.value,
            color: [0.5, 0.5, 0.5],
            x: 150,
            y: 300,
          },
        })
        ElMessage.success('水印添加完成')
        openOutput({ output: out })
        break
      }
      case 'compress': {
        const first = files[0].path
        const out = await pickOutput(`compressed-${basename(first)}`)
        if (!out) break
        const res = await pdfApi.compress(first, out, compressLevel.value)
        ElMessage.success(`压缩完成: 节省 ${res.ratio.toFixed(1)}%`)
        openOutput({ output: out })
        break
      }
      case 'toImages': {
        const first = files[0].path
        const dir = dirname(first)
        const taskName = `to-images-${Date.now()}`
        // 监听进度
        taskStore.init()
        const unlisten = await (await import('@tauri-apps/api/event')).listen<{
          task_id: string
          percent: number
          current: number
          total: number
        }>('task-progress', (e) => {
          if (e.payload.task_id === taskName) {
            progress.value = e.payload.percent
            current.value = e.payload.current
            total.value = e.payload.total
          }
        })
        const res = await pdfApi.toImages(first, dir, imagesDpi.value, imagesFormat.value, taskName)
        unlisten()
        progress.value = 100
        ElMessage.success(`转图片完成: ${res.count} 张`)
        openOutput({ dir, firstOutput: res.outputs[0] })
        break
      }
    }
  } catch (e: any) {
    ElMessage.error(`处理失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (showProgress.value = false), 800)
  }
}
</script>

<template>
  <div class="operation-panel app-card">
    <div class="op-tabs">
      <div
        v-for="o in operations"
        :key="o.key"
        class="op-tab"
        :class="{ active: op === o.key }"
        @click="op = o.key"
      >
        <span class="op-icon">{{ o.icon }}</span>
        <span class="op-label">{{ o.label }}</span>
      </div>
    </div>

    <div class="op-form">
      <!-- 合并:无需额外参数 -->
      <div v-if="op === 'merge'" class="form-tip">
        将选中的多个 PDF 按顺序合并为一个文件
      </div>

      <!-- 拆分 -->
      <el-form v-else-if="op === 'split'" label-width="100px" label-position="left">
        <el-form-item label="拆分方式">
          <el-radio-group v-model="splitMode">
            <el-radio value="everyN">每 N 页</el-radio>
            <el-radio value="ranges">按页码范围</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item v-if="splitMode === 'everyN'" label="每页数">
          <el-input-number v-model="splitN" :min="1" :max="999" />
        </el-form-item>
        <el-form-item v-else label="页码范围">
          <el-input v-model="splitRanges" placeholder="如 1-10,11-20,21-30" />
        </el-form-item>
      </el-form>

      <!-- 旋转 -->
      <el-form v-else-if="op === 'rotate'" label-width="100px" label-position="left">
        <el-form-item label="页码">
          <el-input v-model="pageInput" placeholder="如 1,3,5(留空表示全部)" />
        </el-form-item>
        <el-form-item label="旋转角度">
          <el-radio-group v-model="rotateDegrees">
            <el-radio-button :value="90">90°</el-radio-button>
            <el-radio-button :value="180">180°</el-radio-button>
            <el-radio-button :value="270">270°</el-radio-button>
          </el-radio-group>
        </el-form-item>
      </el-form>

      <!-- 删除/提取 -->
      <el-form v-else-if="op === 'delete' || op === 'extract'" label-width="100px" label-position="left">
        <el-form-item label="页码">
          <el-input v-model="pageInput" placeholder="如 1,3,5-8" />
        </el-form-item>
      </el-form>

      <!-- 加密 -->
      <el-form v-else-if="op === 'encrypt'" label-width="100px" label-position="left">
        <el-form-item label="密码">
          <el-input v-model="encryptPassword" type="password" show-password />
        </el-form-item>
        <el-form-item label="加密位数">
          <el-radio-group v-model="encryptBits">
            <el-radio :value="128">128-bit</el-radio>
            <el-radio :value="256">256-bit AES</el-radio>
          </el-radio-group>
        </el-form-item>
      </el-form>

      <!-- 解密 -->
      <el-form v-else-if="op === 'decrypt'" label-width="100px" label-position="left">
        <el-form-item label="密码">
          <el-input v-model="decryptPassword" type="password" show-password />
        </el-form-item>
      </el-form>

      <!-- 水印 -->
      <el-form v-else-if="op === 'watermark'" label-width="100px" label-position="left">
        <el-form-item label="水印文字">
          <el-input v-model="watermarkText" />
        </el-form-item>
        <el-form-item label="字号">
          <el-input-number v-model="watermarkFontSize" :min="8" :max="144" />
        </el-form-item>
        <el-form-item label="旋转">
          <el-slider v-model="watermarkRotation" :min="0" :max="360" />
        </el-form-item>
        <el-form-item label="不透明度">
          <el-slider v-model="watermarkOpacity" :min="0" :max="1" :step="0.05" />
        </el-form-item>
      </el-form>

      <!-- 压缩 -->
      <el-form v-else-if="op === 'compress'" label-width="100px" label-position="left">
        <el-form-item label="压缩级别">
          <el-slider v-model="compressLevel" :min="1" :max="9" />
        </el-form-item>
      </el-form>

      <!-- 转图片 -->
      <el-form v-else-if="op === 'toImages'" label-width="100px" label-position="left">
        <el-form-item label="DPI">
          <el-input-number v-model="imagesDpi" :min="72" :max="600" :step="72" />
        </el-form-item>
        <el-form-item label="格式">
          <el-radio-group v-model="imagesFormat">
            <el-radio-button value="png">PNG</el-radio-button>
            <el-radio-button value="jpg">JPG</el-radio-button>
            <el-radio-button value="webp">WebP</el-radio-button>
          </el-radio-group>
        </el-form-item>
      </el-form>

      <div class="run-bar">
        <el-button
          type="primary"
          size="large"
          :loading="running"
          :disabled="!canRun"
          @click="run"
        >
          {{ running ? '处理中...' : '开始处理' }}
        </el-button>
        <span v-if="needsSingleFile" class="hint">需要选中 1 个文件</span>
        <span v-else-if="needsMultiFile" class="hint">需要选中 ≥2 个文件</span>
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

.operation-panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.op-tabs {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(90px, 1fr));
  gap: 8px;
}
.op-tab {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 10px 4px;
  border: 1px solid $border-color;
  border-radius: $radius-base;
  cursor: pointer;
  transition: all 0.2s;
  &:hover {
    border-color: $police-blue;
    color: $police-blue;
  }
  &.active {
    border-color: $police-blue;
    background: $police-blue;
    color: #fff;
  }
}
.op-icon {
  font-size: 20px;
}
.op-label {
  font-size: 12px;
  margin-top: 4px;
}
.op-form {
  min-height: 120px;
}
.form-tip {
  color: $text-secondary;
  padding: 20px 0;
}
.run-bar {
  margin-top: 20px;
  display: flex;
  align-items: center;
  gap: 12px;
}
.hint {
  font-size: 12px;
  color: $text-secondary;
}
</style>
