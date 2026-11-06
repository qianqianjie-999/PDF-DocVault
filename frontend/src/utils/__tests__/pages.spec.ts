import { describe, expect, it } from 'vitest'
import { pageInputInvalid, parsePageNums, parseRanges } from '@/utils/pages'

describe('parsePageNums', () => {
  it('展开单页与闭区间,保持输入顺序', () => {
    expect(parsePageNums('1,3,5-8')).toEqual([1, 3, 5, 6, 7, 8])
  })

  it('倒序区间按升序展开', () => {
    expect(parsePageNums('8-5')).toEqual([5, 6, 7, 8])
  })

  it('允许空白与首尾逗号', () => {
    expect(parsePageNums(' 1 , 3 ')).toEqual([1, 3])
    expect(parsePageNums('1,3,')).toEqual([1, 3])
    expect(parsePageNums('')).toEqual([])
  })

  it('非法片段整体判无效,而不是静默截断', () => {
    // parseInt('1-10-20') 会得到 1,此前会静默只处理第 1 页
    expect(parsePageNums('1-10-20')).toEqual([])
    expect(parsePageNums('1abc')).toEqual([])
    expect(parsePageNums('1,abc')).toEqual([])
    expect(parsePageNums('第一页')).toEqual([])
  })

  it('零页与负页码判无效', () => {
    expect(parsePageNums('0')).toEqual([])
    expect(parsePageNums('0-3')).toEqual([])
  })

  it('超大区间直接判无效,不展开成海量数组', () => {
    // 此前会 push 近十亿项,足以卡死主线程
    expect(parsePageNums('1-999999999')).toEqual([])
    expect(parsePageNums('1-10001')).toEqual([])
    // 上限边界本身仍然合法
    expect(parsePageNums('1-10000')).toHaveLength(10000)
  })
})

describe('pageInputInvalid', () => {
  it('留空表示全部,不算无效', () => {
    expect(pageInputInvalid('')).toBe(false)
    expect(pageInputInvalid('   ')).toBe(false)
  })

  it('无法解析的输入必须判无效', () => {
    expect(pageInputInvalid('abc')).toBe(true)
    expect(pageInputInvalid('1-10-20')).toBe(true)
    expect(pageInputInvalid('1,abc')).toBe(true)
    // 超大区间会被 parsePageNums 判无效,界面提示页码格式有误
    expect(pageInputInvalid('1-999999999')).toBe(true)
  })

  it('合法输入不判无效', () => {
    expect(pageInputInvalid('1,3,5-8')).toBe(false)
  })
})

describe('parseRanges', () => {
  it('解析多个闭区间与单页', () => {
    expect(parseRanges('1-10,11-20,21')).toEqual([
      { start: 1, end: 10 },
      { start: 11, end: 20 },
      { start: 21, end: 21 },
    ])
  })

  it('倒序区间自动纠正', () => {
    expect(parseRanges('10-1')).toEqual([{ start: 1, end: 10 }])
  })

  it('非法片段整体判无效,不再默默只拆前一段', () => {
    expect(parseRanges('1-10-20')).toEqual([])
    expect(parseRanges('1-10,abc')).toEqual([])
    expect(parseRanges('abc')).toEqual([])
  })

  it('空输入返回空数组', () => {
    expect(parseRanges('')).toEqual([])
  })
})
