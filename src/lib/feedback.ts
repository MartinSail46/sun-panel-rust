import { computed, shallowRef } from 'vue'
export { toast } from 'vue-sonner'
export interface ConfirmationOptions {
  title: string
  description: string
  confirmText?: string
  cancelText?: string
  destructive?: boolean
}
interface Confirmation extends ConfirmationOptions { id: number; resolve: (confirmed: boolean) => void }
const queue = shallowRef<Confirmation[]>([])
let nextId = 0
export const currentConfirmation = computed(() => queue.value[0])
export function confirmDialog(options: ConfirmationOptions): Promise<boolean> {
  return new Promise(resolve => { queue.value = [...queue.value, { ...options, id: ++nextId, resolve }] })
}
export function resolveConfirmation(id: number, confirmed: boolean) {
  const current = queue.value[0]
  if (!current || current.id !== id) return
  queue.value = queue.value.slice(1)
  current.resolve(confirmed)
}
