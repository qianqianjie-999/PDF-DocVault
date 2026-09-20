import { call } from './index'
import type { ConvertMode, ConvertStats, OcrOptions, OcrSelftestOutput, OcrStats } from '@/types/ocr'

export const ocrApi = {
  /** 环境自检:Python 与 OCR 引擎是否就绪 */
  selftest: () => call<OcrSelftestOutput>('ocr_selftest'),

  /** 图片/PDF 文字识别(PDF 由后端 pdfium 渲染为图片) */
  extractImages: (inputs: string[], options: OcrOptions, taskId?: string) =>
    call<OcrStats>('ocr_extract_images', {
      input: { inputs, options, task_id: taskId },
    }),

  /** 拍图转 Word / 表格转 Excel(mode: docx | xlsx) */
  convert: (
    inputs: string[],
    output: string,
    mode: ConvertMode,
    options: OcrOptions,
    taskId?: string,
  ) =>
    call<ConvertStats>('ocr_convert_documents', {
      input: { inputs, output, mode, options, task_id: taskId },
    }),
}
