<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Search } from '@element-plus/icons-vue'
import PageHeader from '@/components/common/PageHeader.vue'
import { documentApi } from '@/api/document'
import { formatFileSize, formatTimestamp } from '@/utils/file'
import type { Document, SearchDocument } from '@/types/document'

const documents = ref<Document[]>([])
const total = ref(0)
const loading = ref(false)
const page = ref(1)
const pageSize = ref(20)

// ===== 全文搜索状态 =====
const keyword = ref('')
const fileType = ref('')
const searchResults = ref<SearchDocument[] | null>(null)
const searching = ref(false)
let searchTimer: ReturnType<typeof setTimeout> | null = null

const FILE_TYPES = ['pdf', 'doc', 'docx', 'png', 'jpg', 'jpeg']

// 展示行:搜索态显示命中结果,否则显示分页列表
const rows = computed(() => searchResults.value ?? documents.value)

async function doSearch() {
  const kw = keyword.value.trim()
  // 仅以关键词驱动搜索;类型筛选只是搜索的辅助条件,空关键词直接退出搜索态
  if (!kw) {
    searchResults.value = null
    return
  }
  searching.value = true
  try {
    searchResults.value = await documentApi.search(kw, fileType.value || undefined, 100)
  } catch (e: any) {
    ElMessage.error(`搜索失败: ${e?.message ?? e}`)
  } finally {
    searching.value = false
  }
}

/** 输入防抖 300ms */
function onKeywordInput() {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(doSearch, 300)
}

function onTypeChange() {
  if (keyword.value.trim() || fileType.value) void doSearch()
  else searchResults.value = null
}

function clearSearch() {
  keyword.value = ''
  fileType.value = ''
  searchResults.value = null
}

/** 文件名关键词高亮分段(大小写不敏感) */
function segments(name: string, kw: string): { text: string; hit: boolean }[] {
  if (!kw) return [{ text: name, hit: false }]
  const lower = name.toLowerCase()
  const k = kw.toLowerCase()
  const out: { text: string; hit: boolean }[] = []
  let i = 0
  for (;;) {
    const idx = lower.indexOf(k, i)
    if (idx < 0) {
      if (i < name.length) out.push({ text: name.slice(i), hit: false })
      break
    }
    if (idx > i) out.push({ text: name.slice(i, idx), hit: false })
    out.push({ text: name.slice(idx, idx + k.length), hit: true })
    i = idx + k.length
  }
  return out.length ? out : [{ text: name, hit: false }]
}

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
    // 搜索态下删除后刷新搜索结果,否则刷新列表
    if (searchResults.value) await doSearch()
    else await load()
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
    // 标签参与索引,搜索态下同步刷新
    if (searchResults.value) await doSearch()
    else await load()
  } catch {
    /* cancelled */
  }
}

onMounted(load)
</script>

<template>
  <div class="doc-list-view">
    <PageHeader title="文档管理" subtitle="本地文档分类管理 · 全文搜索" />

    <!-- 搜索栏 -->
    <div class="search-bar app-card">
      <el-input
        v-model="keyword"
        placeholder="搜索文件名 / 标签…"
        clearable
        :prefix-icon="Search"
        class="search-input"
        @input="onKeywordInput"
        @keyup.enter="doSearch"
        @clear="clearSearch"
      />
      <el-select
        v-model="fileType"
        placeholder="全部类型"
        clearable
        class="search-type"
        @change="onTypeChange"
      >
        <el-option v-for="t in FILE_TYPES" :key="t" :label="t.toUpperCase()" :value="t" />
      </el-select>
      <span v-if="searchResults" class="search-meta">
        找到 {{ searchResults.length }} 条
        <el-button text size="small" @click="clearSearch">清除搜索</el-button>
      </span>
    </div>

    <div class="app-card">
      <el-table :data="rows" v-loading="loading || searching" border>
        <el-table-column type="index" label="#" width="50" />
        <el-table-column prop="name" label="文件名" min-width="200" show-overflow-tooltip>
          <template #default="{ row }">
            <template v-for="(seg, si) in segments(row.name, keyword.trim())" :key="si">
              <mark v-if="seg.hit" class="hl">{{ seg.text }}</mark>
              <template v-else>{{ seg.text }}</template>
            </template>
          </template>
        </el-table-column>
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
        v-if="!searchResults && total > pageSize"
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
.search-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
  .search-input {
    flex: 1;
    max-width: 360px;
  }
  .search-type {
    width: 130px;
  }
  .search-meta {
    font-size: 13px;
    color: #606266;
  }
}
.hl {
  background: #ffe58f;
  padding: 0 1px;
  border-radius: 2px;
}
</style>
