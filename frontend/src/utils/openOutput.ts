import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'

export interface OpenOutputOptions {
  /** 单文件任务的成品路径,存在时直接用系统默认程序打开 */
  output?: string
  /** 多文件任务的输出目录 */
  dir?: string
  /** 多文件任务中第一个输出文件,用于在文件夹中定位高亮 */
  firstOutput?: string
}

/**
 * 功能完成后打开成果:单文件直接打开,多文件打开所在文件夹并定位第一个输出。
 * 设置页开关关闭时不做任何事;失败仅告警,不打断正常流程。
 */
export async function openOutput(opts: OpenOutputOptions): Promise<void> {
  try {
    if (!useSettingsStore().autoOpenOutput) return
    if (opts.output) {
      await invoke('open_path', { path: opts.output })
    } else if (opts.firstOutput || opts.dir) {
      await invoke('reveal_path', { path: opts.firstOutput ?? opts.dir })
    }
  } catch (e) {
    console.warn('自动打开成果失败:', e)
  }
}
