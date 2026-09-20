import { call } from './index'
import type {
  MergeOutput,
  SplitOutput,
  CompressOutput,
  ToImagesOutput,
  PageCountOutput,
  SplitMode,
  EncryptOptions,
  WatermarkOptions,
} from '@/types/pdf'

export const pdfApi = {
  merge: (inputs: string[], output: string, taskId?: string) =>
    call<MergeOutput>('merge_pdfs', {
      input: { inputs, output, task_id: taskId },
    }),

  split: (input: string, mode: SplitMode, outDir: string, taskId?: string) =>
    call<SplitOutput>('split_pdf', {
      input: { input, mode, out_dir: outDir, task_id: taskId },
    }),

  rotate: (input: string, pageNums: number[], degrees: number, output: string) =>
    call<string>('rotate_pages', {
      input: { input, page_nums: pageNums, degrees, output },
    }),

  deletePages: (input: string, pageNums: number[], output: string) =>
    call<string>('delete_pages', {
      input: { input, page_nums: pageNums, output },
    }),

  insert: (input1: string, input2: string, afterPage: number, output: string) =>
    call<string>('insert_pages', {
      input: { input1, input2, after_page: afterPage, output },
    }),

  extract: (input: string, pageNums: number[], output: string) =>
    call<string>('extract_pages', {
      input: { input, page_nums: pageNums, output },
    }),

  encrypt: (input: string, output: string, options: EncryptOptions) =>
    call<string>('encrypt_pdf', { input: { input, output, options } }),

  decrypt: (input: string, output: string, password: string) =>
    call<string>('decrypt_pdf', { input: { input, output, password } }),

  watermark: (
    input: string,
    output: string,
    opts: {
      text?: string
      image_path?: string
      options: WatermarkOptions
    },
  ) => call<string>('add_watermark', { input: { input, output, ...opts } }),

  compress: (input: string, output: string, level: number = 6) =>
    call<CompressOutput>('compress_pdf', { input: { input, output, level } }),

  toImages: (input: string, outDir: string, dpi: number, format: string, taskId: string) =>
    call<ToImagesOutput>('pdf_to_images', {
      input: { input, out_dir: outDir, dpi, format, task_id: taskId },
    }),

  pageCount: (input: string) => call<PageCountOutput>('pdf_page_count', { input: { input } }),
}
