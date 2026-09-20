import { defineStore } from 'pinia'
import { ref } from 'vue'

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
      if (!selectedFiles.value.some((s) => s.path === f.path)) {
        selectedFiles.value.push(f)
      }
    }
  }

  function removeFile(path: string) {
    selectedFiles.value = selectedFiles.value.filter((f) => f.path !== path)
  }

  function clear() {
    selectedFiles.value = []
  }

  return { selectedFiles, addFiles, removeFile, clear }
})
