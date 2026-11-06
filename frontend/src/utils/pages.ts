// 页码范围输入解析

/** 闭区间页码(1-based) */
export interface PageRange {
  start: number
  end: number
}

const RANGE_RE = /^(\d+)\s*-\s*(\d+)$/
const SINGLE_RE = /^\d+$/

/**
 * 解析页码输入为 1-based 页码列表,支持逗号分隔的单页与闭区间范围,
 * 如 "1,3,5-8" → [1,3,5,6,7,8];"8-5" 视为 "5-8",输出保持输入顺序。
 *
 * 任何一个非空片段不合法(如 "1-10-20"、"1abc"、"abc")时整体视为无效并返回空数组:
 * 此前会退化为 parseInt 只取前一段,用户以为在操作 "1-10-20",实际只处理了第 1 页。
 */
export function parsePageNums(input: string): number[] {
  const nums: number[] = []
  if (!input.trim()) return nums
  for (const part of input.split(',')) {
    const t = part.trim()
    // 允许首尾/连续逗号这类空片段,不影响其余部分的解析
    if (!t) continue
    const range = t.match(RANGE_RE)
    if (range) {
      let a = parseInt(range[1], 10)
      let b = parseInt(range[2], 10)
      if (a < 1 || b < 1) return []
      if (a > b) [a, b] = [b, a]
      for (let n = a; n <= b; n++) nums.push(n)
      continue
    }
    if (!SINGLE_RE.test(t)) return []
    const n = parseInt(t, 10)
    if (n < 1) return []
    nums.push(n)
  }
  return nums
}

/**
 * 输入既非空白、又解析不出任何页码(如「第一页」「abc」「1-10-20」)时返回 true。
 * 这类输入不能被当作「留空 = 全部」,否则旋转/转图片会静默处理整本文档。
 */
export function pageInputInvalid(raw: string): boolean {
  return raw.trim().length > 0 && parsePageNums(raw).length === 0
}

/**
 * 解析拆分用的页码范围,如 "1-10,11-20" → [{start:1,end:10},{start:11,end:20}]。
 * 同样是严格解析:出现 "1-10-20" 这类非法片段时返回空数组,而不是只拆前一段。
 */
export function parseRanges(input: string): PageRange[] {
  const ranges: PageRange[] = []
  if (!input.trim()) return ranges
  for (const part of input.split(',')) {
    const t = part.trim()
    if (!t) continue
    const range = t.match(RANGE_RE)
    if (range) {
      const a = parseInt(range[1], 10)
      const b = parseInt(range[2], 10)
      if (a < 1 || b < 1) return []
      ranges.push({ start: Math.min(a, b), end: Math.max(a, b) })
      continue
    }
    if (!SINGLE_RE.test(t)) return []
    const n = parseInt(t, 10)
    if (n < 1) return []
    ranges.push({ start: n, end: n })
  }
  return ranges
}
