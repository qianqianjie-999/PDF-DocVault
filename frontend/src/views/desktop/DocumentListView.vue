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
// 后端 search 的 limit 上限,与 search_commands 的 clamp 保持一致
const SEARCH_LIMIT = 100

// 关键词按空白拆分:后端每个 token 都要求命中(AND),前端逐 token 高亮才与结果一致
const keywordTokens = computed(() =>
  keyword.value
    .split(/\s+/)
    .filter(Boolean)
    .map((t) => t.toLowerCase()),
)

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
    searchResults.value = await documentApi.search(kw, fileType.value || undefined, SEARCH_LIMIT)
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
  // 无关键词时类型筛选走列表查询,有关键词时并入全文搜索
  page.value = 1
  if (keyword.value.trim()) void doSearch()
  else void load()
}

function clearSearch() {
  keyword.value = ''
  fileType.value = ''
  searchResults.value = null
  page.value = 1
  void load()
}

/** 文件名关键词高亮分段:按 token 标记命中字符,再合并为连续段 */
function segments(name: string, tokens: string[]): { text: string; hit: boolean }[] {
  if (!tokens.length) return [{ text: name, hit: false }]
  const lower = name.toLowerCase()
  const hitMask = Array<boolean>(name.length).fill(false)
  for (const t of tokens) {
    let i = 0
    for (;;) {
      const idx = lower.indexOf(t, i)
      if (idx < 0) break
      for (let j = idx; j < idx + t.length; j++) hitMask[j] = true
      i = idx + t.length
    }
  }
  const out: { text: string; hit: boolean }[] = []
  for (let i = 0; i < name.length; ) {
    const hit = hitMask[i]
    let j = i + 1
    while (j < name.length && hitMask[j] === hit) j++
    out.push({ text: name.slice(i, j), hit })
    i = j
  }
  return out
}

/** 标签存储为 JSON 数组字符串;解析失败时按原文展示 */
function tagList(tags: string | null): string[] {
  if (!tags) return []
  try {
    const parsed = JSON.parse(tags)
    return Array.isArray(parsed) ? parsed.filter((t) => typeof t === 'string' && t) : [tags]
  } catch {
    return [tags]
  }
}

/** 标签是否命中任一搜索 token(大小写不敏感) */
function tagHit(tag: string, tokens: string[]): boolean {
  const lower = tag.toLowerCase()
  return tokens.some((t) => lower.includes(t))
}

async function load() {
  loading.value = true
  try {
    const res = await documentApi.list({
      page: page.value,
      page_size: pageSize.value,
      file_type: fileType.value || undefined,
    })
    documents.value = res.items
    total.value = res.total
  } catch (e: any) {
    ElMessage.error(`加载文档列表失败: ${e?.message ?? e}`)
  } finally {
    loading.value = false
  }
}

async function remove(doc: Document) {
  try {
    await ElMessageBox.confirm(`确认删除文档「${doc.name}」?`, '确认', { type: 'warning' })
  } catch {
    return // 用户取消
  }
  try {
    await documentApi.delete(doc.id)
    ElMessage.success('已删除')
    // 搜索态下删除后刷新搜索结果,否则刷新列表
    if (searchResults.value) await doSearch()
    else {
      await load()
      // 删掉的是当前页最后一条时回退一页,避免停留在空页
      if (!documents.value.length && page.value > 1) {
        page.value -= 1
        await load()
      }
    }
  } catch (e: any) {
    ElMessage.error(`删除失败: ${e?.message ?? e}`)
  }
}

async function editTags(doc: Document) {
  let value: string
  try {
    const res = await ElMessageBox.prompt('标签(JSON 数组,如 ["发票","2026"])', '编辑标签', {
      inputValue: doc.tags || '[]',
    })
    value = res.value
  } catch {
    return // 用户取消
  }
  try {
    await documentApi.tag(doc.id, value || '[]')
    ElMessage.success('已更新')
    // 标签参与索引,搜索态下同步刷新
    if (searchResults.value) await doSearch()
    else await load()
  } catch (e: any) {
    ElMessage.error(`标签更新失败: ${e?.message ?? e}`)
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
        <span v-if="searchResults.length >= SEARCH_LIMIT" class="search-limit">
          (已达上限,请细化关键词)
        </span>
        <el-button text size="small" @click="clearSearch">清除搜索</el-button>
      </span>
    </div>

    <div class="app-card">
      <el-table :data="rows" v-loading="loading || searching" border>
        <el-table-column type="index" label="#" width="50" />
        <el-table-column prop="name" label="文件名" min-width="200" show-overflow-tooltip>
          <template #default="{ row }">
            <template v-for="(seg, si) in segments(row.name, keywordTokens)" :key="si">
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
        <el-table-column label="标签" min-width="160">
          <template #default="{ row }">
            <template v-if="tagList(row.tags).length">
              <el-tag
                v-for="(t, ti) in tagList(row.tags)"
                :key="ti"
                size="small"
                :type="tagHit(t, keywordTokens) ? 'warning' : 'info'"
                class="tag-chip"
              >
                {{ t }}
              </el-tag>
            </template>
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
.tag-chip {
  margin: 2px 4px 2px 0;
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
    .search-limit {
      color: #e6a23c;
    }
  }
}
.hl {
  background: #ffe58f;
  padding: 0 1px;
  border-radius: 2px;
}
</style>
