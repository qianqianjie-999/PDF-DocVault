import { call } from './index'
import type { OfficeSelftestOutput, WordConvertReport, WordQualityLevel } from '@/types/office'

export const officeApi = {
  /** 引擎自检:LibreOffice / Word COM 是否可用 */
  selftest: () => call<OfficeSelftestOutput>('office_selftest'),

  /** 批量 Word(.doc/.docx)转 PDF,输出到统一目录 */
  convert: (
    inputs: string[],
    outputDir: string,
    quality: WordQualityLevel,
    taskId?: string,
  ) =>
    call<WordConvertReport>('word_to_pdf', {
      input: {
        inputs,
        output_dir: outputDir,
        quality,
        task_id: taskId,
      },
    }),
}
