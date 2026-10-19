<script setup lang="ts">
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useAttrs } from 'vue'
import { cn } from '@/lib/utils'
import { useDialogFocus } from '@/hooks/useDialogFocus'
defineOptions({ inheritAttrs: false })
const props = withDefaults(defineProps<{ title?: string; show: boolean; closable?: boolean; maskClosable?: boolean; closeOnEsc?: boolean; contentStyle?: Record<string, string>; bodyClass?: string; fullScreenOnMobile?: boolean }>(), { closable: true, maskClosable: true, closeOnEsc: true })
const emit = defineEmits<{ 'update:show': [show: boolean] }>()
const attrs = useAttrs()
const { ownerId, onCloseAutoFocus } = useDialogFocus(() => props.show)
function update(open: boolean) { if (open || props.closable) emit('update:show', open) }
function outside(event: Event) { if (!props.maskClosable || !props.closable) event.preventDefault() }
function escape(event: Event) { if (!props.closeOnEsc || !props.closable) event.preventDefault() }
</script>
<template>
  <Dialog :open="show" @update:open="update">
    <DialogContent v-bind="{ ...attrs, class: undefined }" :data-dialog-focus-owner="ownerId" :show-close-button="closable" close-label="关闭" :class="cn('flex w-[calc(100vw-24px)] max-w-xl max-h-[calc(100dvh-24px)] flex-col gap-4 rounded-xl p-5', fullScreenOnMobile && 'round-card-modal-fullscreen', attrs.class as string)" @pointer-down-outside="outside" @interact-outside="outside" @escape-key-down="escape" @close-auto-focus="onCloseAutoFocus">
      <DialogHeader class="shrink-0 pr-5 text-left">
        <DialogTitle>
          <slot name="header">
            {{ title || '系统应用' }}
          </slot>
        </DialogTitle>
        <DialogDescription class="sr-only">
          {{ title || '系统应用' }}
        </DialogDescription>
      </DialogHeader>
      <div :class="cn('min-h-0 overflow-y-auto', bodyClass)" :style="contentStyle">
        <slot />
      </div>
      <DialogFooter v-if="$slots.footer" class="shrink-0">
        <slot name="footer" />
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

<style>
@media (width < 640px) {
  .round-card-modal-fullscreen {
    top: 0;
    left: 0;
    width: 100vw;
    max-width: none;
    height: 100dvh;
    max-height: 100dvh;
    translate: none;
    border: 0;
    border-radius: 0;
    padding-top: max(1rem, env(safe-area-inset-top));
    padding-right: max(1rem, env(safe-area-inset-right));
    padding-bottom: max(1rem, env(safe-area-inset-bottom));
    padding-left: max(1rem, env(safe-area-inset-left));
  }

  .round-card-modal-fullscreen [data-slot="dialog-close"] {
    top: max(1rem, env(safe-area-inset-top));
    right: max(1rem, env(safe-area-inset-right));
  }
}
</style>
