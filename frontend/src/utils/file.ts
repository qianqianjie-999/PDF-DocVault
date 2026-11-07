// 文件工具函数

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}

export function formatTimestamp(ts: number): string {
  return new Date(ts * 1000).toLocaleString('zh-CN')
}

export function basename(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || path
}

/** 按 accept 列表(如 "pdf" 或 "png,jpg,jpeg")判断扩展名是否匹配 */
export function matchesAccept(path: string, accept?: string): boolean {
  if (!accept) return true
  const name = basename(path)
  const idx = name.lastIndexOf('.')
  if (idx < 0) return false
  const ext = name.slice(idx + 1).toLowerCase()
  return accept
    .split(',')
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean)
    .includes(ext)
}

export function dirname(path: string): string {
  const normalized = path.replace(/\\/g, '/')
  const idx = normalized.lastIndexOf('/')
  return idx >= 0 ? normalized.slice(0, idx) : ''
}

export function joinPath(dir: string, name: string): string {
  return `${dir.replace(/[\\/]+$/, '')}/${name}`
}

/**
 * 补全 PDF 输出路径的扩展名。
 * 保存对话框会自动加 `.pdf`,但输入框允许手动填写,缺失后缀会生成无扩展名文件,
 * 系统既无法按类型关联也无法被部分阅读器识别。
 */
export function ensurePdfExtension(path: string): string {
  const trimmed = path.trim()
  if (!trimmed) return ''
  return /\.pdf$/i.test(trimmed) ? trimmed : `${trimmed}.pdf`
}

/** 判断两个路径是否指向同一个文件(用于阻断「输出覆盖源文件」) */
export function samePath(a: string, b: string): boolean {
  if (!a || !b) return false
  const normalize = (p: string) => p.replace(/\\/g, '/').replace(/\/+/g, '/').replace(/\/$/, '')
  const na = normalize(a)
  const nb = normalize(b)
  // Windows 盘符路径与 UNC 路径大小写不敏感,其余平台按原样比较
  const windows = /^[a-z]:[\\/]/i.test(a) || /^\\\\/.test(a) || /^[a-z]:[\\/]/i.test(b) || /^\\\\/.test(b)
  return windows ? na.toLowerCase() === nb.toLowerCase() : na === nb
}
