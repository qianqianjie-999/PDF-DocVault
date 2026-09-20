import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { ProgressPayload } from '@/types/pdf'
import { onEvent } from '@/utils/event'

export const useTaskStore = defineStore('task', () => {
  const tasks = ref<Map<string, ProgressPayload>>(new Map())
  const running = ref(false)
  let inited = false

  async function init() {
    if (inited) return
    inited = true
    await onEvent<ProgressPayload>('task-progress', (p) => {
      tasks.value.set(p.task_id, p)
      running.value = true
    })
    await onEvent<{ task_id: string }>('task-complete', (p) => {
      const cur = tasks.value.get(p.task_id)
      if (cur) {
        cur.percent = 100
        cur.current = cur.total
      }
      running.value = false
    })
    await onEvent<{ task_id: string; message?: string }>('task-error', () => {
      running.value = false
    })
  }

  function getProgress(taskId: string): ProgressPayload | undefined {
    return tasks.value.get(taskId)
  }

  return { tasks, running, init, getProgress }
})
