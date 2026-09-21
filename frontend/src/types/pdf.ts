// PDF 操作相关类型(与 Rust 端 serde 结构对齐)

export interface MergeOutput {
  pages: number
  elapsed_ms: number
  output: string
}

export interface SplitOutput {
  outputs: string[]
  elapsed_ms: number
}

export interface CompressOutput {
  output: string
  original_size: number
  compressed_size: number
  ratio: number
  elapsed_ms: number
}

export interface ToImagesOutput {
  outputs: string[]
  count: number
}

/** 转图片入参(与 Rust 端 ToImagesInput 对齐) */
export interface ToImagesInput {
  input: string
  out_dir: string
  dpi: number
  format?: string
  task_id: string
  /** 待导出页码(1-based,离散列表);缺省或空 = 整本导出 */
  pages?: number[]
}

export interface PageCountOutput {
  count: number
}

export interface PageRange {
  start: number
  end: number
}

export type SplitMode =
  | { kind: 'by_ranges'; ranges: PageRange[] }
  | { kind: 'every_n_pages'; n: number }

export interface EncryptOptions {
  user_password: string
  owner_password: string | null
  permissions: Permissions
  bits: number
}

export interface Permissions {
  allow_print: boolean
  allow_copy: boolean
  allow_modify: boolean
  allow_annotate: boolean
}

export interface WatermarkOptions {
  font_size: number
  rotation: number
  opacity: number
  color: [number, number, number]
  x: number
  y: number
}

export interface ProgressPayload {
  task_id: string
  kind: string
  current: number
  total: number
  percent: number
}

export interface CompletePayload {
  task_id: string
  kind: string
  status: string
}
