// 扫描增强相关类型

export interface ScanOptions {
  detect_boundary: boolean
  perspective: boolean
  remove_shadow: boolean
  grayscale: boolean
  adaptive_binarize: boolean
  contrast_stretch: number
}

export interface ScanBatchOutput {
  outputs: string[]
  corrected: number
  elapsed_ms: number
}

export interface ImagesToPdfOutput {
  pages: number
  elapsed_ms: number
  output: string
}

/** 试卷去手写结果 */
export interface HandwritingOutput {
  outputs: string[]
  /** 笔迹像素占比均值(0-1),0 = 未检测到笔迹 */
  mask_ratio: number
  images: number
  elapsed_ms: number
}

// ===== 多图 A4 排版 =====

/** 每页几张图 */
export type ImagesPerA4 = 'one' | 'two' | 'four'

/** 页面方向 */
export type PageOrientation = 'portrait' | 'landscape'

/** 图片填充模式 */
export type ImageFitMode = 'contain' | 'stretch' | 'cover'

/** 网格排列顺序 */
export type GridOrder = 'row_major' | 'column_major'

export interface ImagesToA4Options {
  enhance: ScanOptions
  per_page: ImagesPerA4
  /** 页面方向(默认纵向) */
  orientation: PageOrientation
  /** 图片填充模式(默认等比完整显示) */
  fit_mode: ImageFitMode
  /** 网格排列顺序(默认行优先) */
  grid_order: GridOrder
  margin_mm: number
  gap_mm: number
  /** 渲染 DPI,越高越清晰文件越大(默认 150) */
  dpi: number
  /** JPEG 质量 1-100(默认 85) */
  jpeg_quality: number
}
