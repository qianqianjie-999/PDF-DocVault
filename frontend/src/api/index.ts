import { invoke } from '@tauri-apps/api/core'

// 统一 invoke 封装,带类型
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args)
}

export { invoke }
