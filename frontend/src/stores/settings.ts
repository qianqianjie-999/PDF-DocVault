import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useSettingsStore = defineStore('settings', () => {
  const defaultOutputDir = ref('')
  const defaultDpi = ref(150)
  const defaultImageFormat = ref<'png' | 'jpg' | 'webp'>('png')
  const ocrLanguage = ref('ch')
  // 功能完成后自动打开成果文件/文件夹
  const autoOpenOutput = ref(true)

  function load() {
    const saved = localStorage.getItem('pdf-docvault-settings')
    if (saved) {
      try {
        const obj = JSON.parse(saved)
        if (obj.defaultOutputDir) defaultOutputDir.value = obj.defaultOutputDir
        if (obj.defaultDpi) defaultDpi.value = obj.defaultDpi
        if (obj.defaultImageFormat) defaultImageFormat.value = obj.defaultImageFormat
        if (obj.ocrLanguage) ocrLanguage.value = obj.ocrLanguage
        // 布尔字段需显式判断,if(obj.x) 会把 false 误判为缺省
        if (typeof obj.autoOpenOutput === 'boolean') autoOpenOutput.value = obj.autoOpenOutput
      } catch {
        /* ignore */
      }
    }
  }

  function save() {
    localStorage.setItem(
      'pdf-docvault-settings',
      JSON.stringify({
        defaultOutputDir: defaultOutputDir.value,
        defaultDpi: defaultDpi.value,
        defaultImageFormat: defaultImageFormat.value,
        ocrLanguage: ocrLanguage.value,
        autoOpenOutput: autoOpenOutput.value,
      }),
    )
  }

  return {
    defaultOutputDir,
    defaultDpi,
    defaultImageFormat,
    ocrLanguage,
    autoOpenOutput,
    load,
    save,
  }
})
