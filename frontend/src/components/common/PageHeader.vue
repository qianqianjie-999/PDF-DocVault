<script setup lang="ts">
defineProps<{
  title: string
  subtitle?: string
}>()
</script>

<template>
  <div class="page-header">
    <h2 class="page-title">{{ title }}</h2>
    <p v-if="subtitle" class="page-subtitle">{{ subtitle }}</p>
  </div>
</template>

<style scoped lang="scss">
.page-header {
  margin-bottom: 20px;
}
.page-title {
  font-size: 22px;
  font-weight: 600;
  margin: 0 0 6px;
  color: #303133;
}
.page-subtitle {
  font-size: 13px;
  color: #909399;
  margin: 0;
}
</style>
