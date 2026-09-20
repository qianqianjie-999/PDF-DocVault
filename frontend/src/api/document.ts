import { call } from './index'
import type { ListResult } from '@/types/document'

export const documentApi = {
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
}
