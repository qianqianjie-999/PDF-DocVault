import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

// 捕获 onEvent 注册的处理器,便于在测试中模拟后端事件
const registered: Array<(payload: unknown) => void> = []
vi.mock('@/utils/event', () => ({
  onEvent: vi.fn(async (_name: string, handler: (p: unknown) => void) => {
    registered.push(handler)
    return () => {}
  }),
}))

import { useTaskStore } from '@/stores/task'

// 任务 store:监听 task-progress/complete/error 并维护进度表
describe('useTaskStore', () => {
  beforeEach(() => {
    registered.length = 0
    setActivePinia(createPinia())
  })

  // 每次返回全新对象,避免 complete 处理器原地改写(percent=100)污染后续用例
  const makePayload = () => ({
    task_id: 't1',
    kind: 'merge',
    current: 2,
    total: 5,
    percent: 40,
  })

  it('init 只注册一次,progress 写入任务表', async () => {
    const store = useTaskStore()
    await store.init()
    await store.init() // 第二次调用应被忽略
    expect(registered).toHaveLength(3)

    registered[0](makePayload())
    expect(store.getProgress('t1')?.percent).toBe(40)
    expect(store.running).toBe(true)
  })

  it('task-complete 将进度置为 100 并结束 running', async () => {
    const store = useTaskStore()
    await store.init()

    registered[0](makePayload())
    registered[1]({ task_id: 't1' })
    expect(store.getProgress('t1')?.percent).toBe(100)
    expect(store.getProgress('t1')?.current).toBe(5)
    expect(store.running).toBe(false)
  })

  it('task-error 结束 running,未知任务 complete 不影响状态', async () => {
    const store = useTaskStore()
    await store.init()

    registered[0](makePayload())
    registered[2]({ task_id: 't1' })
    expect(store.running).toBe(false)
    // 进度记录保留
    expect(store.getProgress('t1')?.percent).toBe(40)

    // 未知任务 id 的 complete 不应改动任何记录
    registered[1]({ task_id: 'unknown' })
    expect(store.getProgress('unknown')).toBeUndefined()
  })
})
