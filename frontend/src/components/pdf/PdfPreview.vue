<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { pdfApi } from '@/api/pdf'

const props = withDefaults(
  defineProps<{
    path: string
    page?: number
    dpi?: number
    /** 总页数;>1 时显示翻页按钮 */
    totalPages?: number
  }>(),
  { page: 1, dpi: 120, totalPages: 0 },
)

const current = ref(props.page)
const imgSrc = ref('')
const loading = ref(false)
const error = ref('')

const canPrev = computed(() => current.value > 1)
const canNext = computed(() => props.totalPages > 1 && current.value < props.totalPages)

async function render() {
  if (!props.path) return
  loading.value = true
  error.value = ''
  imgSrc.value = ''
  try {
    const out = await pdfApi.previewPage(props.path, current.value, props.dpi)
    imgSrc.value = out.data
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}

function goPrev() {
  if (!canPrev.value) return
  current.value -= 1
  void render()
}

function goNext() {
  if (!canNext.value) return
  current.value += 1
  void render()
}

watch(
  () => props.path,
  () => {
    current.value = props.page
    void render()
  },
  { immediate: true },
)
</script>

<template>
  <div class="pdf-preview">
    <div v-if="loading" class="hint">加载中...</div>
    <img v-else-if="imgSrc" :src="imgSrc" class="preview-img" alt="PDF 预览" />
    <div v-else-if="error" class="hint error">{{ error }}</div>
    <div v-else class="hint">暂无预览</div>

    <div v-if="props.totalPages > 1" class="toolbar">
      <button class="nav-btn" :disabled="!canPrev" @click="goPrev">上一页</button>
      <span class="page-info">{{ current }} / {{ props.totalPages }}</span>
      <button class="nav-btn" :disabled="!canNext" @click="goNext">下一页</button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.pdf-preview {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}
.preview-img {
  max-width: 100%;
  max-height: 70vh;
  border: 1px solid #dcdfe6;
  border-radius: 6px;
  background: #fff;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 16px;
}
.nav-btn {
  padding: 6px 16px;
  font-size: 13px;
  border: 1px solid $border-color;
  border-radius: 4px;
  background: #fff;
  color: $text-regular;
  cursor: pointer;
  &:hover:not(:disabled) {
    color: $police-blue;
    border-color: $police-blue;
  }
  &:disabled {
    color: #c0c4cc;
    cursor: not-allowed;
  }
}
.page-info {
  font-size: 13px;
  color: $text-secondary;
}
.hint {
  color: #909399;
  font-size: 13px;
  &.error {
    color: #f56c6c;
  }
}
</style>