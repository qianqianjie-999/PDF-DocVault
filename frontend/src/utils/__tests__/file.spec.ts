import { describe, expect, it } from 'vitest'
import { matchesAccept } from '@/utils/file'

// matchesAccept:拖拽路径按 accept 列表过滤扩展名
describe('matchesAccept', () => {
  it('未声明 accept 时不限制类型', () => {
    expect(matchesAccept('/tmp/a.docx')).toBe(true)
    expect(matchesAccept('/tmp/a.docx', '')).toBe(true)
  })

  it('单扩展名过滤且大小写不敏感', () => {
    expect(matchesAccept('/tmp/a.PDF', 'pdf')).toBe(true)
    expect(matchesAccept('/tmp/a.docx', 'pdf')).toBe(false)
  })

  it('多扩展名列表逐一匹配', () => {
    expect(matchesAccept('/tmp/x.jpeg', 'png,jpg,jpeg')).toBe(true)
    expect(matchesAccept('/tmp/x.gif', 'png, jpg, jpeg')).toBe(false)
  })

  it('无扩展名的文件在有 accept 时被拒绝', () => {
    expect(matchesAccept('/tmp/README', 'pdf')).toBe(false)
  })
})
