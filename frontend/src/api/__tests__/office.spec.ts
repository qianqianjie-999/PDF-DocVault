import { beforeEach, describe, expect, it, vi } from 'vitest'

// mock 底层 invoke,验证 officeApi 的参数组装
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => ({})),
}))

import { invoke } from '@tauri-apps/api/core'
import { officeApi } from '@/api/office'

const mockedInvoke = vi.mocked(invoke)

describe('officeApi 参数组装', () => {
  beforeEach(() => {
    mockedInvoke.mockClear()
  })

  it('selftest 无参数调用 office_selftest', async () => {
    await officeApi.selftest()
    expect(mockedInvoke).toHaveBeenCalledWith('office_selftest', undefined)
  })

  it('convert 组装 inputs/output_dir/quality/task_id(snake_case)', async () => {
    await officeApi.convert(['/a.docx', '/b.doc'], '/out', 'medium', 't1')
    expect(mockedInvoke).toHaveBeenCalledWith('word_to_pdf', {
      input: {
        inputs: ['/a.docx', '/b.doc'],
        output_dir: '/out',
        quality: 'medium',
        task_id: 't1',
      },
    })
  })

  it('convert 无 taskId 时传 undefined', async () => {
    await officeApi.convert(['/a.docx'], '/out', 'low')
    expect(mockedInvoke).toHaveBeenCalledWith('word_to_pdf', {
      input: {
        inputs: ['/a.docx'],
        output_dir: '/out',
        quality: 'low',
        task_id: undefined,
      },
    })
  })
})
