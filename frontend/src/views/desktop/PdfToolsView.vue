<script setup lang="ts">
import PageHeader from '@/components/common/PageHeader.vue'
import FileDropZone from '@/components/pdf/FileDropZone.vue'
import FileList from '@/components/pdf/FileList.vue'
import OperationPanel from '@/components/pdf/OperationPanel.vue'
</script>

<template>
  <div class="pdf-tools-view">
    <PageHeader title="PDF 工具" subtitle="本地处理 · 文件不离开本机" />
    <div class="tools-grid">
      <div class="left-col">
        <FileDropZone accept="pdf" />
        <FileList />
      </div>
      <div class="right-col">
        <OperationPanel />
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.tools-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
  align-items: start;
}
.left-col,
.right-col {
  display: flex;
  flex-direction: column;
}
</style>
