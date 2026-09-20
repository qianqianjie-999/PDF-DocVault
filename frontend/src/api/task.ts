import { call } from './index'
import type { TaskStatus } from '@/types/task'

export const taskApi = {
  getStatus: (taskId: string) =>
    call<TaskStatus | null>('get_task_status', { param: { task_id: taskId } }),
  cancel: (taskId: string) => call<boolean>('cancel_task', { param: { task_id: taskId } }),
}
