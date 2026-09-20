import { call } from './index'
import type {
  ScanOptions,
  ScanBatchOutput,
  ImagesToPdfOutput,
  ImagesToA4Options,
  HandwritingOutput,
} from '@/types/scan'

export const scanApi = {
  /** 批量扫描增强(边界检测/透视矫正/去阴影/二值化) */
  enhanceImages: (inputs: string[], outDir: string, options: ScanOptions, taskId?: string) =>
    call<ScanBatchOutput>('scan_enhance_images', {
      input: { inputs, out_dir: outDir, options, task_id: taskId },
    }),

  /** 多图(可选增强)合成 PDF,每图一页 */
  imagesToPdf: (
    inputs: string[],
    output: string,
    dpi: number,
    options: ScanOptions,
    taskId?: string,
  ) =>
    call<ImagesToPdfOutput>('scan_images_to_pdf', {
      input: { inputs, output, dpi, options, task_id: taskId },
    }),

  /** 多图 A4 排版合成 PDF(可选 1/2/4 张图每页) */
  imagesToA4Pdf: (
    inputs: string[],
    output: string,
    options: ImagesToA4Options,
    taskId?: string,
  ) =>
    call<ImagesToPdfOutput>('images_to_a4_pdf_cmd', {
      input: { inputs, output, options, task_id: taskId },
    }),

  /** 试卷去手写:颜色阈值 + OpenCV inpaint(仅对彩色笔迹有效) */
  removeHandwriting: (
    inputs: string[],
    outDir: string,
    colors: string[],
    dilate = 2,
    taskId?: string,
  ) =>
    call<HandwritingOutput>('remove_handwriting', {
      input: { inputs, out_dir: outDir, colors, dilate, task_id: taskId },
    }),
}
