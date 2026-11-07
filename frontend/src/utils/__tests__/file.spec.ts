import { describe, expect, it } from 'vitest'
import { matchesAccept, samePath } from '@/utils/file'

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

// samePath:拦截「输出文件与源文件相同」的覆盖操作
describe('samePath', () => {
  it('同一路径的等价写法视为相同', () => {
    expect(samePath('/tmp/a.pdf', '/tmp/a.pdf')).toBe(true)
    expect(samePath('/tmp/a.pdf', '/tmp//a.pdf')).toBe(true)
    expect(samePath('/tmp/a.pdf', '/tmp/a.pdf/')).toBe(true)
    expect(samePath('/tmp/a.pdf', '/tmp/b.pdf')).toBe(false)
  })

  it('Windows 路径忽略分隔符与大小写', () => {
    expect(samePath('C:\\Docs\\A.pdf', 'c:/docs/a.pdf')).toBe(true)
    expect(samePath('C:\\Docs\\A.pdf', 'C:/Docs/B.pdf')).toBe(false)
  })

  it('空路径不判为相同', () => {
    expect(samePath('', '')).toBe(false)
    expect(samePath('/tmp/a.pdf', '')).toBe(false)
  })
})
