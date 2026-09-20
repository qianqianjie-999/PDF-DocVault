<script setup lang="ts">
import { ref, watch } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { pdfApi } from '@/api/pdf'

const props = defineProps<{
  path: string
  page?: number
  dpi?: number
}>()

const imgSrc = ref('')
const loading = ref(false)

async function render() {
  if (!props.path) return
  loading.value = true
  try {
    // 简化预览:用 pdfium 转图片(临时目录),再 convertFileSrc 展示
    // 此处仅展示文件信息占位,真实预览在 PdfToolsView 中按需渲染
    imgSrc.value = convertFileSrc(props.path)
  } finally {
    loading.value = false
  }
}

watch(() => props.path, render, { immediate: true })
</script>

<template>
  <div class="pdf-preview">
    <div v-if="loading" class="hint">加载中...</div>
    <iframe v-else-if="imgSrc" :src="imgSrc" class="preview-frame" />
    <div v-else class="hint">暂无预览</div>
  </div>
</template>

<style scoped lang="scss">
.pdf-preview {
  width: 100%;
  height: 100%;
  min-height: 300px;
  border: 1px solid #dcdfe6;
  border-radius: 8px;
  overflow: hidden;
  background: #f5f7fa;
  display: flex;
  align-items: center;
  justify-content: center;
}
.preview-frame {
  width: 100%;
  height: 100%;
  border: none;
}
.hint {
  color: #909399;
  font-size: 13px;
}
</style>
