import { defineStore } from 'pinia'
import { ref } from 'vue'
import { userApi, type ActivationStatus } from '@/api/user'

export const useActivationStore = defineStore('activation', () => {
  const status = ref<ActivationStatus | null>(null)
  const loading = ref(false)

  async function refresh() {
    loading.value = true
    try {
      status.value = await userApi.checkActivation()
    } finally {
      loading.value = false
    }
  }

  const activated = ref(false)
  async function check() {
    await refresh()
    activated.value = status.value?.activated ?? false
    return activated.value
  }

  return { status, loading, activated, refresh, check }
})
