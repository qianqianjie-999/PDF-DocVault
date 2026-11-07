import { defineStore } from 'pinia'
import { ref } from 'vue'
import { samePath } from '@/utils/file'

export interface SelectedFile {
  path: string
  name: string
  size: number
  pages?: number
}

export const useFilesStore = defineStore('files', () => {
  const selectedFiles = ref<SelectedFile[]>([])

  function addFiles(files: SelectedFile[]) {
    for (const f of files) {
      // Windows 上大小写/分隔符不同的等价路径不应重复入列
      if (!selectedFiles.value.some((s) => samePath(s.path, f.path))) {
        selectedFiles.value.push(f)
      }
    }
  }

  function removeFile(path: string) {
    selectedFiles.value = selectedFiles.value.filter((f) => !samePath(f.path, path))
  }

  function clear() {
    selectedFiles.value = []
  }

  return { selectedFiles, addFiles, removeFile, clear }
})
