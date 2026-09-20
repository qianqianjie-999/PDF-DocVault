import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// 封装 Tauri 事件监听,返回取消监听函数
export async function onEvent<T>(
  eventName: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(eventName, (e) => handler(e.payload))
}
