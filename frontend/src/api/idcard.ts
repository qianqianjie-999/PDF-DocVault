import { call } from './index'
import type { ScanOptions } from '@/types/scan'

/** 证件排版选项(对应 core::image::IdCardOptions,后端字段均有默认值) */
export interface IdCardOptions {
  /** 证件扫描增强(边界检测/透视矫正/去阴影等) */
  enhance?: Partial<ScanOptions>
  /** 页边距 mm(默认 10) */
  margin_mm?: number
  /** 两栏间距 mm(默认 8) */
  gap_mm?: number
  /** 正面标签,空字符串不绘制 */
  label_front?: string
  /** 反面标签 */
  label_back?: string
  /** 标签字号 pt(默认 12) */
  label_size_pt?: number
  /** 嵌入 JPEG 质量 1-100(默认 90) */
  jpeg_quality?: number
}

/** 排版结果 */
export interface IdCardOutput {
  output: string
  labeled: boolean
  slot_size_mm: [number, number]
  elapsed_ms: number
}

export const idCardApi = {
  /** 两张证件照(正面/反面)排版为单页 A4 PDF */
  toPdf: (front: string, back: string, output: string, options: IdCardOptions, taskId?: string) =>
    call<IdCardOutput>('id_cards_to_pdf', {
      input: { front, back, output, options, task_id: taskId },
    }),
}
