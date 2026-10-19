<script setup lang="ts">
import { Toaster } from '@/components/ui/sonner'
import { Button } from '@/components/ui/button'
import { AlertDialog, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { currentConfirmation, resolveConfirmation } from '@/lib/feedback'
import { computed } from 'vue'
import { useDialogFocus } from '@/hooks/useDialogFocus'
defineProps<{ dark: boolean }>()
const { ownerId, onCloseAutoFocus } = useDialogFocus(() => currentConfirmation.value?.id)
const view = computed(() => {
  const request = currentConfirmation.value
  if (!request) return undefined
  return { ...request, accept: () => resolveConfirmation(request.id, true), cancel: () => resolveConfirmation(request.id, false), changed: (open: boolean) => { if (!open) resolveConfirmation(request.id, false) } }
})
</script>
<template>
  <Toaster :theme="dark ? 'dark' : 'light'" container-aria-label="通知" :toast-options="{ closeButtonAriaLabel: '关闭' }" position="top-right" rich-colors close-button />
  <AlertDialog v-if="view" :key="view.id" :open="true" @update:open="view.changed">
    <AlertDialogContent :data-dialog-focus-owner="ownerId" @close-auto-focus="onCloseAutoFocus">
      <AlertDialogHeader>
        <AlertDialogTitle>{{ view.title }}</AlertDialogTitle><AlertDialogDescription class="whitespace-pre-wrap">
          {{ view.description }}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel @click="view.cancel">
          {{ view.cancelText || '取消' }}
        </AlertDialogCancel>
        <Button type="button" :variant="view.destructive ? 'destructive' : 'default'" @click="view.accept">
          {{ view.confirmText || '确定' }}
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>
