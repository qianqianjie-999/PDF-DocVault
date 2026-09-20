import { beforeEach, describe, expect, it, vi } from 'vitest'

// mock 底层 invoke,只验证参数组装(Tauri 命令参数为 snake_case)
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => ({})),
}))

import { invoke } from '@tauri-apps/api/core'
import { pdfApi } from '@/api/pdf'
import type { EncryptOptions, Permissions, SplitMode } from '@/types/pdf'

const mockedInvoke = vi.mocked(invoke)

describe('pdfApi 参数组装', () => {
  beforeEach(() => {
    mockedInvoke.mockClear()
  })

  it('merge 传递 inputs/output/task_id', async () => {
    await pdfApi.merge(['/a.pdf', '/b.pdf'], '/out/m.pdf', 't1')
    expect(mockedInvoke).toHaveBeenCalledWith('merge_pdfs', {
      input: { inputs: ['/a.pdf', '/b.pdf'], output: '/out/m.pdf', task_id: 't1' },
    })
  })

  it('split 传递 mode/out_dir/task_id', async () => {
    const mode: SplitMode = { kind: 'every_n_pages', n: 3 }
    await pdfApi.split('/a.pdf', mode, '/out', 't2')
    expect(mockedInvoke).toHaveBeenCalledWith('split_pdf', {
      input: { input: '/a.pdf', mode, out_dir: '/out', task_id: 't2' },
    })
  })

  it('rotate/extract 传递 page_nums(snake_case)', async () => {
    await pdfApi.rotate('/a.pdf', [1, 3], 90, '/out.pdf')
    expect(mockedInvoke).toHaveBeenCalledWith('rotate_pages', {
      input: { input: '/a.pdf', page_nums: [1, 3], degrees: 90, output: '/out.pdf' },
    })

    await pdfApi.extract('/a.pdf', [2, 4], '/out.pdf')
    expect(mockedInvoke).toHaveBeenCalledWith('extract_pages', {
      input: { input: '/a.pdf', page_nums: [2, 4], output: '/out.pdf' },
    })
  })

  it('insert 传递 after_page(snake_case)', async () => {
    await pdfApi.insert('/a.pdf', '/b.pdf', 2, '/out.pdf')
    expect(mockedInvoke).toHaveBeenCalledWith('insert_pages', {
      input: { input1: '/a.pdf', input2: '/b.pdf', after_page: 2, output: '/out.pdf' },
    })
  })

  it('encrypt 透传 options,decrypt 传递 password', async () => {
    const permissions: Permissions = {
      allow_print: true,
      allow_copy: false,
      allow_modify: false,
      allow_annotate: false,
    }
    const options: EncryptOptions = {
      user_password: '123',
      owner_password: null,
      permissions,
      bits: 128,
    }
    await pdfApi.encrypt('/a.pdf', '/enc.pdf', options)
    expect(mockedInvoke).toHaveBeenCalledWith('encrypt_pdf', {
      input: { input: '/a.pdf', output: '/enc.pdf', options },
    })

    await pdfApi.decrypt('/enc.pdf', '/dec.pdf', '123')
    expect(mockedInvoke).toHaveBeenCalledWith('decrypt_pdf', {
      input: { input: '/enc.pdf', output: '/dec.pdf', password: '123' },
    })
  })

  it('toImages 传递 out_dir/dpi/format/task_id', async () => {
    await pdfApi.toImages('/a.pdf', '/out', 200, 'jpg', 't3')
    expect(mockedInvoke).toHaveBeenCalledWith('pdf_to_images', {
      input: { input: '/a.pdf', out_dir: '/out', dpi: 200, format: 'jpg', task_id: 't3' },
    })
  })

  it('compress 默认 level=6,pageCount 单参数', async () => {
    await pdfApi.compress('/a.pdf', '/c.pdf')
    expect(mockedInvoke).toHaveBeenCalledWith('compress_pdf', {
      input: { input: '/a.pdf', output: '/c.pdf', level: 6 },
    })

    await pdfApi.pageCount('/a.pdf')
    expect(mockedInvoke).toHaveBeenCalledWith('pdf_page_count', {
      input: { input: '/a.pdf' },
    })
  })
})
