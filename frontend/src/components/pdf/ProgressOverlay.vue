<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  visible: boolean
  percent: number
  current?: number
  total?: number
  text?: string
}>()

const status = computed(() => (props.percent >= 100 ? 'success' : undefined))
const description = computed(() => {
  if (props.text) return props.text
  if (props.total && props.total > 0) {
    return `${props.current ?? 0} / ${props.total}`
  }
  return `${props.percent}%`
})
</script>

<template>
  <el-dialog
    :model-value="visible"
    :show-close="false"
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    width="420px"
    center
  >
    <template #header>
      <span class="dialog-title">处理中</span>
    </template>
    <div class="overlay-body">
      <el-progress
        :percentage="percent"
        :status="status"
        :stroke-width="14"
        text-inside
      />
      <p class="desc">{{ description }}</p>
    </div>
  </el-dialog>
</template>

<style scoped lang="scss">
.dialog-title {
  font-weight: 600;
}
.overlay-body {
  text-align: center;
  padding: 10px 0;
}
.desc {
  margin: 14px 0 0;
  font-size: 13px;
  color: #606266;
}
</style>
