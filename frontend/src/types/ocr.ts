/** OCR 选项(对应 core::ocr::OcrOptions,后端字段均有默认值) */
export interface OcrOptions {
  /** 指定引擎(rapidocr / paddleocr),空 = 自动选择 */
  engine?: string | null
  /** 语言代码(paddleocr 有效:ch/en/jp/kor) */
  lang?: string | null
  /** PDF 输入时的渲染 DPI(默认 200) */
  dpi?: number
  /** Python 进程超时秒数(默认 300) */
  timeout_secs?: number
}

/** 单行识别结果 */
export interface OcrLine {
  text: string
  score: number
  box: [number, number][]
}

/** 单页识别结果 */
export interface OcrPage {
  index: number
  file: string
  line_count: number
  lines: OcrLine[]
  /** 按阅读顺序拼接的纯文本 */
  text: string
}

/** OCR 任务统计(对应 core::ocr::OcrStats) */
export interface OcrStats {
  engine: string
  pages: OcrPage[]
  elapsed_ms: number
  /** 输入中来自 PDF 渲染的页数 */
  from_pdf_pages: number
}

/** 环境自检结果 */
export interface OcrSelftestOutput {
  ok: boolean
  engine: string | null
  message: string
}

/** 转换模式:docx=拍图转 Word,xlsx=表格转 Excel */
export type ConvertMode = 'docx' | 'xlsx'

/** 转换统计(对应 core::ocr::ConvertStats) */
export interface ConvertStats {
  mode: ConvertMode
  engine: string
  output: string
  images: number
  /** docx:识别总行数 */
  lines?: number | null
  /** xlsx:还原总行数 */
  rows?: number | null
  /** xlsx:每张图的工作表统计 */
  sheets?: Array<{ file: string; rows: number; cols: number }> | null
  elapsed_ms: number
}
