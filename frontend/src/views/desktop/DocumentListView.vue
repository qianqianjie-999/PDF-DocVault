<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import PageHeader from '@/components/common/PageHeader.vue'
import { documentApi } from '@/api/document'
import { formatFileSize, formatTimestamp, basename } from '@/utils/file'
import type { Document } from '@/types/document'

const documents = ref<Document[]>([])
const total = ref(0)
const loading = ref(false)
const page = ref(1)
const pageSize = ref(20)

async function load() {
  loading.value = true
  try {
    const res = await documentApi.list({ page: page.value, page_size: pageSize.value })
    documents.value = res.items
    total.value = res.total
  } finally {
    loading.value = false
  }
}

async function remove(doc: Document) {
  try {
    await ElMessageBox.confirm(`确认删除文档「${doc.name}」?`, '确认', { type: 'warning' })
    await documentApi.delete(doc.id)
    ElMessage.success('已删除')
    await load()
  } catch {
    /* cancelled */
  }
}

async function editTags(doc: Document) {
  try {
    const { value } = await ElMessageBox.prompt('标签(JSON 数组,如 ["发票","2026"])', '编辑标签', {
      inputValue: doc.tags || '[]',
    })
    await documentApi.tag(doc.id, value || '[]')
    ElMessage.success('已更新')
    await load()
  } catch {
    /* cancelled */
  }
}

onMounted(load)
</script>

<template>
  <div class="doc-list-view">
    <PageHeader title="文档管理" subtitle="本地文档分类管理 · 全文搜索(待集成 Tantivy)" />

    <div class="app-card">
      <el-table :data="documents" v-loading="loading" border>
        <el-table-column type="index" label="#" width="50" />
        <el-table-column prop="name" label="文件名" min-width="200" show-overflow-tooltip />
        <el-table-column label="类型" width="80">
          <template #default="{ row }">
            <el-tag size="small">{{ row.file_type }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="大小" width="100">
          <template #default="{ row }">{{ formatFileSize(row.file_size) }}</template>
        </el-table-column>
        <el-table-column label="页数" width="80">
          <template #default="{ row }">{{ row.page_count ?? '-' }}</template>
        </el-table-column>
        <el-table-column label="标签" width="160">
          <template #default="{ row }">
            <span v-if="row.tags">{{ row.tags }}</span>
            <span v-else class="muted">-</span>
          </template>
        </el-table-column>
        <el-table-column label="创建时间" width="160">
          <template #default="{ row }">{{ formatTimestamp(row.created_at) }}</template>
        </el-table-column>
        <el-table-column label="操作" width="160" fixed="right">
          <template #default="{ row }">
            <el-button text size="small" @click="editTags(row as Document)">标签</el-button>
            <el-button text size="small" type="danger" @click="remove(row as Document)">删除</el-button>
          </template>
        </el-table-column>
      </el-table>
      <el-pagination
        v-if="total > pageSize"
        v-model:current-page="page"
        :page-size="pageSize"
        :total="total"
        layout="prev, pager, next"
        style="margin-top: 16px"
        @current-change="load"
      />
    </div>
  </div>
</template>

<style scoped lang="scss">
.muted {
  color: #c0c4cc;
}
</style>
