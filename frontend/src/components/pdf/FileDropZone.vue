<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useFilesStore, type SelectedFile } from '@/stores/files'
import { basename } from '@/utils/file'
import { ElMessage } from 'element-plus'

const props = defineProps<{
  accept?: string
  multiple?: boolean
  label?: string
}>()

const emit = defineEmits<{ (e: 'select', files: SelectedFile[]): void }>()

const filesStore = useFilesStore()

async function pickFiles() {
  const extensions = props.accept ? props.accept.split(',').map((s) => s.trim()) : ['pdf']
  const selected = await open({
    multiple: props.multiple ?? true,
    filters: [{ name: '文档', extensions }],
  })
  if (!selected) return
  const arr = Array.isArray(selected) ? selected : [selected]
  const items: SelectedFile[] = await Promise.all(
    arr.map(async (p) => {
      let size = 0
      try {
        const fs = await import('@tauri-apps/plugin-fs')
        const stat = await fs.stat(p)
        size = Number(stat.size) || 0
      } catch {
        /* ignore */
      }
      return { path: p, name: basename(p), size }
    }),
  )
  filesStore.addFiles(items)
  emit('select', items)
}

// 监听 Tauri 窗口拖拽事件
async function listenDrop() {
  try {
    const webview = getCurrentWebview()
    webview.onDragDropEvent(async (event) => {
      if (event.payload.type === 'drop') {
        const paths = event.payload.paths
        const items: SelectedFile[] = paths.map((p) => ({
          path: p,
          name: basename(p),
          size: 0,
        }))
        filesStore.addFiles(items)
        emit('select', items)
      }
    })
  } catch (e) {
    // 非 Tauri 环境(浏览器)忽略
  }
}
listenDrop()
</script>

<template>
  <div class="drop-zone" @click="pickFiles">
    <el-icon class="upload-icon"><UploadFilled /></el-icon>
    <p class="drop-text">{{ label || '点击选择文件,或拖拽文件到此处' }}</p>
    <p class="drop-hint">支持 {{ accept || 'PDF' }} 格式</p>
  </div>
</template>

<style scoped lang="scss">

.drop-zone {
  border: 2px dashed $border-color;
  border-radius: $radius-base;
  padding: 32px 20px;
  text-align: center;
  cursor: pointer;
  transition: all 0.2s;
  background: #fafbfc;
  &:hover {
    border-color: $police-blue;
    background: $police-blue-50;
  }
}
.upload-icon {
  font-size: 40px;
  color: $police-blue;
}
.drop-text {
  margin: 12px 0 4px;
  font-size: 14px;
  color: $text-primary;
}
.drop-hint {
  margin: 0;
  font-size: 12px;
  color: $text-secondary;
}
</style>
