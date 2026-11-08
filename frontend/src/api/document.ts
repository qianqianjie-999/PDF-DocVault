import { call } from './index'
import type { ListResult, SearchDocument } from '@/types/document'

export const documentApi = {
  /** 全文搜索(文件名/标签/类型);fileType/limit 可选 */
  search: (keyword: string, fileType?: string, limit?: number) =>
    call<SearchDocument[]>('search_documents', {
      params: { keyword, file_type: fileType ?? null, limit: limit ?? null },
    }),

  list: (params: {
    page?: number
    page_size?: number
    file_type?: string
    tag?: string
  }) => call<ListResult>('list_documents', { params }),

  import: (filePath: string, fileType: string, pageCount?: number) =>
    call<number>('import_document', {
      params: { file_path: filePath, file_type: fileType, page_count: pageCount ?? null },
    }),

  tag: (id: number, tags: string) => call<boolean>('tag_document', { params: { id, tags } }),

  delete: (id: number, permanent: boolean = false) =>
    call<boolean>('delete_document', { params: { id, permanent } }),

  /** 回收站:列出软删除文档 */
  listDeleted: (params: { page?: number; page_size?: number } = {}) =>
    call<ListResult>('list_deleted_documents', { params }),

  /** 回收站:恢复软删除文档 */
  restore: (id: number) => call<boolean>('restore_document', { params: { id } }),

  /** 回收站:彻底删除记录(从回收站清除) */
  purge: (id: number) => call<boolean>('delete_document', { params: { id, permanent: true } }),
}
