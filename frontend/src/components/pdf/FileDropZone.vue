<script setup lang="ts">
import { onUnmounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import type { SelectedFile } from '@/stores/files'
import { basename, matchesAccept } from '@/utils/file'
import { pdfApi } from '@/api/pdf'
import { ElMessage } from 'element-plus'

const props = defineProps<{
  accept?: string
  multiple?: boolean
  label?: string
}>()

const emit = defineEmits<{ (e: 'select', files: SelectedFile[]): void }>()

/** 统一生成选中项:读取真实体积,失败时退回 0 */
async function toSelectedFile(p: string): Promise<SelectedFile> {
  let size = 0
  try {
    // 走后端而不是前端 fs 插件:HOME 之外的路径不受 scope 限制
    const { invoke } = await import('@tauri-apps/api/core')
    size = Number(await invoke<number>('file_size', { path: p })) || 0
  } catch {
    /* 读取失败时体积显示为 0,不影响后续处理 */
  }
  let pages: number | undefined
  // PDF 顺带取页数用于文件列表;加密或损坏时取不到则保持留空
  if (p.toLowerCase().endsWith('.pdf')) {
    try {
      pages = (await pdfApi.pageCount(p)).count
    } catch {
      /* 页数仅用于展示,失败不影响添加文件 */
    }
  }
  return { path: p, name: basename(p), size, pages }
}

async function pickFiles() {
  const extensions = props.accept ? props.accept.split(',').map((s) => s.trim()) : ['pdf']
  const selected = await open({
    multiple: props.multiple ?? true,
    filters: [{ name: '文档', extensions }],
  })
  if (!selected) return
  const arr = Array.isArray(selected) ? selected : [selected]
  // 对话框 filters 并非所有平台都强制生效(如 Linux 可切换「所有文件」),
  // 回到前端后再按 accept 过滤一次,与拖拽路径保持一致
  const accepted = arr.filter((p) => matchesAccept(p, props.accept))
  if (!accepted.length) {
    ElMessage.warning('不支持的文件类型')
    return
  }
  const items = await Promise.all(accepted.map(toSelectedFile))
  emit('select', items)
}

// 监听 Tauri 窗口拖拽事件;组件卸载时注销,避免跨页面重复监听
let unlisten: (() => void) | null = null
let disposed = false

async function listenDrop() {
  try {
    const webview = getCurrentWebview()
    const stop = await webview.onDragDropEvent(async (event) => {
      if (event.payload.type !== 'drop') return
      // 拖拽路径与选择框保持一致的扩展名过滤
      const paths = event.payload.paths.filter((p) => matchesAccept(p, props.accept))
      if (!paths.length) {
        if (event.payload.paths.length) ElMessage.warning('不支持的文件类型')
        return
      }
      const items = await Promise.all(paths.map(toSelectedFile))
      emit('select', items)
    })
    // 卸载动作可能先于注册完成,此时直接注销
    if (disposed) stop()
    else unlisten = stop
  } catch (e) {
    // 非 Tauri 环境(浏览器)忽略
  }
}
listenDrop()

onUnmounted(() => {
  disposed = true
  unlisten?.()
  unlisten = null
})
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
