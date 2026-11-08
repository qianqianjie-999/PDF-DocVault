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
  /** 实际重编码的图片数(level 1..=3 恒为 0) */
  images_reencoded: number
  /** true = 图片处理失败已回退为纯结构压缩 */
  fell_back: boolean
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

/** 单页预览结果(与 Rust 端 PreviewOutput 对齐) */
export interface PreviewOutput {
  /** data:image/png;base64, 前缀的 data URI */
  data: string
  width: number
  height: number
  page: number
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
  /** 源文件本身已加密时的打开密码 */
  input_password?: string | null
}

export interface Permissions {
  allow_print: boolean
  allow_copy: boolean
  allow_modify: boolean
  allow_annotate: boolean
}

/** 加密结果:页数 / 算法修订号 / 落盘体积,以及服务端回读到的权限位 */
export interface EncryptReport {
  output: string
  pages: number
  bits: number
  /** R6 = AES-256,R4 = AES-128 */
  revision: number
  was_encrypted: boolean
  size: number
  permissions: Permissions
}

export interface DecryptReport {
  output: string
  pages: number
  was_encrypted: boolean
  size: number
}

export interface EncryptionInfo {
  encrypted: boolean
  /** 需要密码才能读(未给密码或密码错误) */
  locked: boolean
  pages: number
  permissions: Permissions
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
