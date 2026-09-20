export type TaskState = 'pending' | 'running' | 'done' | 'failed' | 'cancelled'

export interface TaskStatus {
  id: string
  kind: string
  state: TaskState
  progress: number
  current: number
  total: number
  error: string | null
  elapsed_ms: number
}
