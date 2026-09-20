<script setup lang="ts">
import { computed, ref } from 'vue'
import { open, save } from '@tauri-apps/plugin-dialog'
import { ElMessage } from 'element-plus'
import { Plus, Picture } from '@element-plus/icons-vue'
import PageHeader from '@/components/common/PageHeader.vue'
import { idCardApi, type IdCardOptions } from '@/api/idcard'
import { basename } from '@/utils/file'
import { openOutput } from '@/utils/openOutput'
import type { ScanOptions } from '@/types/scan'

interface Slot {
  path: string | null
  name: string
}

const front = ref<Slot>({ path: null, name: '人像面' })
const back = ref<Slot>({ path: null, name: '国徽面' })
const running = ref(false)
const progress = ref(0)

// 排版选项
const options = ref<IdCardOptions>({
  margin_mm: 10,
  gap_mm: 8,
  label_front: '人像面',
  label_back: '国徽面',
  label_size_pt: 12,
  jpeg_quality: 90,
  // 扫描增强默认开启常用项
  enhance: {
    detect_boundary: true,
    perspective: true,
    remove_shadow: true,
    grayscale: false,
    adaptive_binarize: false,
    contrast_stretch: 0.02,
  } as Partial<ScanOptions>,
})

const IMG_EXT = /\.(png|jpe?g|webp|bmp|tiff?)$/i

const canRun = computed(
  () => !running.value && front.value.path !== null && back.value.path !== null,
)

async function pickFor(slot: 'front' | 'back') {
  const picked = await open({
    multiple: false,
    filters: [{ name: '图片', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'tif', 'tiff'] }],
  })
  if (!picked || typeof picked !== 'string') return
  if (!IMG_EXT.test(picked)) {
    ElMessage.warning('请选择图片文件')
    return
  }
  const target = slot === 'front' ? front : back
  target.value = { path: picked, name: basename(picked) }
}

function clearSlot(slot: 'front' | 'back') {
  const target = slot === 'front' ? front : back
  target.value = { path: null, name: slot === 'front' ? '人像面' : '国徽面' }
}

async function listenProgress(taskId: string) {
  const { listen } = await import('@tauri-apps/api/event')
  const unlisten = await listen<{ task_id: string; percent: number }>('task-progress', (e) => {
    if (e.payload.task_id !== taskId) return
    progress.value = e.payload.percent
  })
  return unlisten
}

async function generate() {
  if (!canRun.value) {
    ElMessage.warning('请先选择正面与反面两张图片')
    return
  }
  const out = await save({
    defaultPath: '证件排版.pdf',
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  })
  if (!out) return
  running.value = true
  progress.value = 0
  try {
    const taskId = `idcard-${Date.now()}`
    const unlisten = await listenProgress(taskId)
    try {
      const res = await idCardApi.toPdf(
        front.value.path as string,
        back.value.path as string,
        out as string,
        options.value,
        taskId,
      )
      ElMessage.success(
        `证件排版完成:${res.labeled ? '含标签,' : ''}单栏 ${Math.round(res.slot_size_mm[0])}x${Math.round(res.slot_size_mm[1])}mm,耗时 ${res.elapsed_ms}ms`,
      )
      openOutput({ output: out as string })
    } finally {
      unlisten()
    }
  } catch (e: any) {
    ElMessage.error(`证件排版失败: ${e?.message ?? e}`)
  } finally {
    running.value = false
    setTimeout(() => (progress.value = 0), 600)
  }
}
</script>

<template>
  <div class="idcard-view">
    <PageHeader title="证件排版" subtitle="正反面合成单页 A4 · 自动增强 · 本地处理" />

    <div class="slots">
      <!-- 正面 -->
      <div class="slot-card app-card" :class="{ filled: front.path }">
        <div class="card-head">
          <span class="title">正面(第 1 张)</span>
          <el-button v-if="front.path" text size="small" type="danger" @click="clearSlot('front')">
            清除
          </el-button>
        </div>
        <div v-if="!front.path" class="slot-empty" @click="pickFor('front')">
          <el-icon class="icon"><Plus /></el-icon>
          <p>点击选择人像面照片</p>
        </div>
        <div v-else class="slot-file" @click="pickFor('front')">
          <el-icon class="icon ok"><Picture /></el-icon>
          <span class="name" :title="front.path">{{ front.name }}</span>
          <span class="change">点击更换</span>
        </div>
        <el-input v-model="options.label_front" placeholder="正面标签(留空不绘制)">
          <template #prepend>正面标签</template>
        </el-input>
      </div>

      <!-- 反面 -->
      <div class="slot-card app-card" :class="{ filled: back.path }">
        <div class="card-head">
          <span class="title">反面(第 2 张)</span>
          <el-button v-if="back.path" text size="small" type="danger" @click="clearSlot('back')">
            清除
          </el-button>
        </div>
        <div v-if="!back.path" class="slot-empty" @click="pickFor('back')">
          <el-icon class="icon"><Plus /></el-icon>
          <p>点击选择国徽面照片</p>
        </div>
        <div v-else class="slot-file" @click="pickFor('back')">
          <el-icon class="icon ok"><Picture /></el-icon>
          <span class="name" :title="back.path">{{ back.name }}</span>
          <span class="change">点击更换</span>
        </div>
        <el-input v-model="options.label_back" placeholder="反面标签(留空不绘制)">
          <template #prepend>反面标签</template>
        </el-input>
      </div>
    </div>

    <div class="options-card app-card">
      <div class="card-head">
        <span class="title">排版选项</span>
      </div>
      <el-form label-width="130px" label-position="left">
        <el-form-item label="自动增强">
          <el-checkbox v-model="options.enhance!.detect_boundary">边界检测</el-checkbox>
          <el-checkbox v-model="options.enhance!.perspective">透视矫正</el-checkbox>
          <el-checkbox v-model="options.enhance!.remove_shadow">去阴影</el-checkbox>
        </el-form-item>
        <el-form-item label="页边距 mm">
          <el-input-number v-model="options.margin_mm" :min="5" :max="30" :step="1" />
        </el-form-item>
        <el-form-item label="两栏间距 mm">
          <el-input-number v-model="options.gap_mm" :min="2" :max="30" :step="1" />
        </el-form-item>
        <el-form-item label="标签字号 pt">
          <el-input-number v-model="options.label_size_pt" :min="8" :max="24" :step="1" />
        </el-form-item>
        <el-form-item label="JPEG 质量">
          <el-input-number v-model="options.jpeg_quality" :min="60" :max="100" :step="5" />
        </el-form-item>
      </el-form>
      <div class="run-bar">
        <el-button
          type="primary"
          size="large"
          :loading="running"
          :disabled="!canRun"
          @click="generate"
        >
          生成单页 A4 PDF
        </el-button>
        <el-progress
          v-if="progress > 0"
          :percentage="progress"
          class="run-progress"
          :stroke-width="10"
        />
      </div>
      <p class="hint">生成前可先在「文档扫描」页把照片处理成扫描件效果;此处会再次按上述选项做增强。</p>
    </div>
  </div>
</template>

<style scoped lang="scss">
.slots {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
  margin-bottom: 20px;
}
.slot-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  border: 1px solid $border-color;
  border-radius: $radius-base;
  padding: 16px;
  &.filled {
    border-color: $police-blue;
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
}
.slot-empty,
.slot-file {
  border: 2px dashed $border-color;
  border-radius: $radius-base;
  padding: 28px 16px;
  text-align: center;
  cursor: pointer;
  transition: all 0.2s;
  background: #fafbfc;
  &:hover {
    border-color: $police-blue;
    background: $police-blue-50;
  }
  .icon {
    font-size: 32px;
    color: $text-secondary;
    &.ok {
      color: $police-blue;
    }
  }
  p {
    margin: 8px 0 0;
    font-size: 13px;
    color: $text-secondary;
  }
}
.slot-file {
  display: flex;
  align-items: center;
  gap: 10px;
  .name {
    flex: 1;
    text-align: left;
    font-size: 13px;
    color: $text-primary;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .change {
    font-size: 12px;
    color: $text-secondary;
  }
}
.options-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px;
  border: 1px solid $border-color;
  border-radius: $radius-base;
  .card-head .title {
    font-weight: 600;
    color: $text-primary;
  }
}
.run-bar {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 8px;
  .run-progress {
    flex: 1;
  }
}
.hint {
  margin: 4px 0 0;
  font-size: 12px;
  color: $text-secondary;
}
</style>
