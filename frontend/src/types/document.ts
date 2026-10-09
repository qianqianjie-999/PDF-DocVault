export interface Document {
  id: number
  name: string
  file_path: string
  file_type: string
  file_size: number
  page_count: number | null
  status: string
  tags: string | null
  created_at: number
  updated_at: number
}

export interface ListResult {
  items: Document[]
  total: number
}

/** 搜索结果行 = 完整文档 + BM25 得分 */
export interface SearchDocument extends Document {
  score: number
}
