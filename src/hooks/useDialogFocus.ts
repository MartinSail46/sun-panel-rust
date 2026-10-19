import { nextTick, toValue, useId, watch } from 'vue'
import type { MaybeRefOrGetter } from 'vue'

interface OpenerHandoff { element: HTMLElement | null }
let pendingOpener: OpenerHandoff | undefined

/** A menu can hand its real opener to the dialog it is about to open. */
export function rememberDialogOpener(element: HTMLElement | null) {
  const handoff = { element }
  pendingOpener = handoff
  // Wait for the selecting event to queue its Vue update before clearing. Copy
  // and new-tab menu actions must not contaminate a later, unrelated dialog.
  queueMicrotask(() => {
    void nextTick(() => {
      if (pendingOpener === handoff)
        pendingOpener = undefined
    })
  })
}

const openDialogs = '[data-slot="dialog-content"][data-state="open"], [data-slot="alert-dialog-content"][data-state="open"]'

/** Explicit restoration for controlled dialogs that do not have DialogTrigger. */
export function useDialogFocus(open: MaybeRefOrGetter<boolean | number | null | undefined>) {
  const ownerId = useId()
  let opener: HTMLElement | null = null
  let generation = 0
  let restoreToken: object | undefined
  watch(() => toValue(open), (value) => {
    restoreToken = undefined
    if (!value || typeof document === 'undefined')
      return
    generation++
    const handoff = pendingOpener
    pendingOpener = undefined
    if (handoff) {
      opener = handoff.element
      return
    }
    const active = document.activeElement
    if (!(active instanceof HTMLElement) || active === document.body)
      return
    // Reopening during an exit animation, or advancing a confirmation queue,
    // must keep the original button rather than an input about to disappear.
    if (active.closest('[data-dialog-focus-owner]')?.getAttribute('data-dialog-focus-owner') !== ownerId)
      opener = active
  }, { immediate: true, flush: 'sync' })

  function onCloseAutoFocus(event: Event) {
    event.preventDefault()
    const target = opener
    const closedGeneration = generation
    const token = {}
    restoreToken = token
    void nextTick(() => {
      if (restoreToken !== token || generation !== closedGeneration || toValue(open) || !target?.isConnected)
        return
      if (target.matches(':disabled') || target.closest('[inert]'))
        return
      const top = [...document.querySelectorAll<HTMLElement>(openDialogs)]
        .filter(element => element.getAttribute('data-dialog-focus-owner') !== ownerId).at(-1)
      // A nested dialog may restore to its still-open parent. A newly opened
      // sibling owns focus and must never be interrupted by this old close.
      if (top && !top.contains(target))
        return
      target.focus({ preventScroll: true })
    })
  }
  return { ownerId, onCloseAutoFocus }
}
