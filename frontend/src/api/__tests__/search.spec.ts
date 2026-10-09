import { beforeEach, describe, expect, it, vi } from 'vitest'

// mock 底层 invoke,只验证参数组装(Tauri 命令参数为 snake_case)
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => []),
}))

import { invoke } from '@tauri-apps/api/core'
import { documentApi } from '@/api/document'

const mockedInvoke = vi.mocked(invoke)

describe('documentApi.search 参数组装', () => {
  beforeEach(() => {
    mockedInvoke.mockClear()
  })

  it('search 传递 keyword/file_type/limit,空值转 null', async () => {
    await documentApi.search('发票')
    expect(mockedInvoke).toHaveBeenCalledWith('search_documents', {
      params: { keyword: '发票', file_type: null, limit: null },
    })

    await documentApi.search('报告', 'docx', 20)
    expect(mockedInvoke).toHaveBeenCalledWith('search_documents', {
      params: { keyword: '报告', file_type: 'docx', limit: 20 },
    })
  })
})
