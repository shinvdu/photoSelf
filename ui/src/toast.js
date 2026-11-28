import { ref } from 'vue'

export const toast = ref(null)

let toastTimer = null

export function showToast(text, isError = false) {
  toast.value = { text, error: isError }
  if (toastTimer) clearTimeout(toastTimer)
  toastTimer = setTimeout(() => {
    toast.value = null
  }, isError ? 4000 : 2500)
}

export function clearToast() {
  toast.value = null
  if (toastTimer) {
    clearTimeout(toastTimer)
    toastTimer = null
  }
}