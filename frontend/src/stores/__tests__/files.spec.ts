import { beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useFilesStore } from '@/stores/files'

// 文件列表 store:去重/移除/清空
describe('useFilesStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  const f = (path: string, name = 'a.pdf') => ({ path, name, size: 1024 })

  it('addFiles 按路径去重', () => {
    const store = useFilesStore()
    store.addFiles([f('/tmp/a.pdf')])
    // 同一路径重复添加应被忽略
    store.addFiles([f('/tmp/a.pdf'), f('/tmp/b.pdf', 'b.pdf')])
    expect(store.selectedFiles).toHaveLength(2)
    expect(store.selectedFiles.map((s) => s.name)).toEqual(['a.pdf', 'b.pdf'])
  })

  it('removeFile 只删除匹配路径', () => {
    const store = useFilesStore()
    store.addFiles([f('/tmp/a.pdf'), f('/tmp/b.pdf', 'b.pdf')])
    store.removeFile('/tmp/a.pdf')
    expect(store.selectedFiles).toHaveLength(1)
    expect(store.selectedFiles[0].path).toBe('/tmp/b.pdf')
  })

  it('clear 清空全部', () => {
    const store = useFilesStore()
    store.addFiles([f('/tmp/a.pdf'), f('/tmp/b.pdf', 'b.pdf')])
    store.clear()
    expect(store.selectedFiles).toHaveLength(0)
  })
})
