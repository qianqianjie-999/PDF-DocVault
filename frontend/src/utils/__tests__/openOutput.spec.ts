import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

// mock 底层 invoke,验证 openOutput 的打开策略
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => ({})),
}))

import { invoke } from '@tauri-apps/api/core'
import { openOutput } from '@/utils/openOutput'
import { useSettingsStore } from '@/stores/settings'

const mockedInvoke = vi.mocked(invoke)

describe('openOutput 自动打开成果', () => {
  beforeEach(() => {
    mockedInvoke.mockClear()
    setActivePinia(createPinia())
  })

  it('设置关闭时不调用任何命令', async () => {
    const settings = useSettingsStore()
    settings.autoOpenOutput = false

    await openOutput({ output: '/out/merged.pdf' })
    await openOutput({ dir: '/out', firstOutput: '/out/a.pdf' })
    expect(mockedInvoke).not.toHaveBeenCalled()
  })

  it('单文件任务直接打开成品文件', async () => {
    await openOutput({ output: '/out/merged.pdf' })
    expect(mockedInvoke).toHaveBeenCalledOnce()
    expect(mockedInvoke).toHaveBeenCalledWith('open_path', { path: '/out/merged.pdf' })
  })

  it('多文件任务定位第一个输出文件', async () => {
    await openOutput({ dir: '/out', firstOutput: '/out/a-1.pdf' })
    expect(mockedInvoke).toHaveBeenCalledWith('reveal_path', { path: '/out/a-1.pdf' })
  })

  it('无首个输出时打开输出目录', async () => {
    await openOutput({ dir: '/out' })
    expect(mockedInvoke).toHaveBeenCalledWith('reveal_path', { path: '/out' })
  })

  it('未提供任何路径时不调用', async () => {
    await openOutput({})
    expect(mockedInvoke).not.toHaveBeenCalled()
  })

  it('invoke 失败不抛错(静默告警)', async () => {
    mockedInvoke.mockRejectedValueOnce(new Error('xdg-open 不存在'))
    await expect(openOutput({ output: '/out/merged.pdf' })).resolves.toBeUndefined()
  })
})
