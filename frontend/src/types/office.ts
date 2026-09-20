/** Word → PDF 批量转换相关类型 */

/** 质量等级:高/中/低 */
export type WordQualityLevel = 'high' | 'medium' | 'low'

/** 成功条目 */
export interface WordSucceededItem {
  /** 源 Word 路径 */
  input: string
  /** 输出 PDF 路径 */
  output: string
}

/** 失败条目 */
export interface WordFailedItem {
  /** 源 Word 路径 */
  input: string
  /** 失败原因 */
  reason: string
}

/** 批量转换结果报告 */
export interface WordConvertReport {
  /** 实际使用的引擎(word_com / wps_com / wps_cli / libreoffice / none) */
  engine: string
  succeeded: WordSucceededItem[]
  failed: WordFailedItem[]
  elapsed_ms: number
  output_dir: string
}

/** 引擎自检结果 */
export interface OfficeSelftestOutput {
  ok: boolean
  libreoffice: string | null
  word_com: boolean
  /** Windows 上是否注册了 WPS COM */
  wps_com: boolean
  /** Linux/macOS 的 wps 可执行文件路径 */
  wps_cli: string | null
  message: string
}
