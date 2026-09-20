<script setup lang="ts">
import { useFilesStore } from '@/stores/files'
import { formatFileSize } from '@/utils/file'
import { Delete } from '@element-plus/icons-vue'

const filesStore = useFilesStore()
</script>

<template>
  <div class="file-list" v-if="filesStore.selectedFiles.length > 0">
    <div class="list-header">
      <span>已选文件 ({{ filesStore.selectedFiles.length }})</span>
      <el-button text size="small" @click="filesStore.clear()">清空</el-button>
    </div>
    <el-table :data="filesStore.selectedFiles" size="small" border>
      <el-table-column type="index" label="#" width="50" />
      <el-table-column prop="name" label="文件名" min-width="180" show-overflow-tooltip />
      <el-table-column label="大小" width="100">
        <template #default="{ row }">{{ formatFileSize(row.size) }}</template>
      </el-table-column>
      <el-table-column label="页数" width="80">
        <template #default="{ row }">{{ row.pages ?? '-' }}</template>
      </el-table-column>
      <el-table-column label="操作" width="80" fixed="right">
        <template #default="{ row }">
          <el-button
            text
            size="small"
            type="danger"
            :icon="Delete"
            @click="filesStore.removeFile(row.path)"
          />
        </template>
      </el-table-column>
    </el-table>
  </div>
</template>

<style scoped lang="scss">
.file-list {
  margin-top: 16px;
}
.list-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
  font-size: 13px;
  color: #606266;
}
</style>
