import assert from 'node:assert/strict'
import test from 'node:test'
import { createRenderer, defineComponent, h, nextTick, ref } from 'vue'
import { rememberDialogOpener, useDialogFocus } from '../src/hooks/useDialogFocus.ts'

class Element {
  parent: Element | null = null
  owner?: string
  isConnected = true
  disabled = false
  inert = false
  focusCount = 0
  readonly name: string
  readonly document: DocumentState
  constructor(name: string, document: DocumentState) { this.name = name; this.document = document }
  getAttribute(name: string) { return name === 'data-dialog-focus-owner' ? this.owner || null : null }
  closest(selector: string): Element | null {
    if (selector === '[data-dialog-focus-owner]' && this.owner) return this
    if (selector === '[inert]' && this.inert) return this
    let element: Element | null = this.parent
    while (element) {
      if (selector === '[data-dialog-focus-owner]' && element.owner) return element
      if (selector === '[inert]' && element.inert) return element
      element = element.parent
    }
    return null
  }
  contains(target: Element) {
    let element: Element | null = target
    while (element) { if (element === this) return true; element = element.parent }
    return false
  }
  matches(selector: string) { return selector === ':disabled' && this.disabled }
  focus() { this.document.activeElement = this; this.focusCount++ }
}
interface DocumentState { body: Element; activeElement: Element; layers: Element[]; querySelectorAll: () => Element[] }
function environment() {
  const originalDocument = Object.getOwnPropertyDescriptor(globalThis, 'document')
  const originalElement = Object.getOwnPropertyDescriptor(globalThis, 'HTMLElement')
  const document = {} as DocumentState
  document.body = new Element('body', document)
  document.activeElement = document.body
  document.layers = []
  document.querySelectorAll = () => document.layers
  Object.defineProperty(globalThis, 'document', { value: document, configurable: true })
  Object.defineProperty(globalThis, 'HTMLElement', { value: Element, configurable: true })
  const element = (name: string, parent = document.body) => { const result = new Element(name, document); result.parent = parent; return result }
  return {
    document, element,
    restore() {
      if (originalDocument) Object.defineProperty(globalThis, 'document', originalDocument)
      else Reflect.deleteProperty(globalThis, 'document')
      if (originalElement) Object.defineProperty(globalThis, 'HTMLElement', originalElement)
      else Reflect.deleteProperty(globalThis, 'HTMLElement')
    },
  }
}
function mount(count = 1) {
  const opened = Array.from({ length: count }, () => ref(false))
  let hooks: ReturnType<typeof useDialogFocus>[] = []
  const renderer = createRenderer<Record<string, unknown>, Record<string, unknown>>({
    createElement: tag => ({ tag }), createText: text => ({ text }), createComment: text => ({ text }),
    insert: () => {}, remove: () => {}, setText: () => {}, setElementText: () => {},
    parentNode: () => null, nextSibling: () => null, patchProp: () => {},
  })
  const app = renderer.createApp(defineComponent({ setup() { hooks = opened.map(open => useDialogFocus(open)); return () => h('div') } }))
  app.mount({})
  return { hooks, opened, unmount: () => app.unmount() }
}
function close(hook: ReturnType<typeof useDialogFocus>) {
  let prevented = false
  hook.onCloseAutoFocus({ preventDefault() { prevented = true } } as Event)
  assert.equal(prevented, true, 'The component owns restoration instead of a missing DialogTrigger')
}

test('nested controlled dialogs restore their own original buttons in order', async () => {
  const env = environment()
  const ui = mount(2)
  try {
    const outerButton = env.element('open settings')
    env.document.activeElement = outerButton
    ui.opened[0].value = true
    const parent = env.element('settings dialog'); parent.owner = ui.hooks[0].ownerId
    env.document.layers = [parent]
    const innerButton = env.element('open group editor', parent)
    env.document.activeElement = innerButton
    ui.opened[1].value = true
    const child = env.element('group dialog'); child.owner = ui.hooks[1].ownerId
    env.document.layers.push(child)
    env.document.activeElement = env.element('group title', child)
    ui.opened[1].value = false
    env.document.layers = [parent]
    close(ui.hooks[1])
    await nextTick()
    assert.equal(env.document.activeElement, innerButton)
    ui.opened[0].value = false
    env.document.layers = []
    close(ui.hooks[0])
    await nextTick()
    assert.equal(env.document.activeElement, outerButton)
  }
  finally { ui.unmount(); env.restore() }
})

test('an old close cannot steal focus from a new dialog or a rapidly reopened one', async () => {
  const env = environment()
  const ui = mount(2)
  try {
    const firstButton = env.element('first opener')
    env.document.activeElement = firstButton
    ui.opened[0].value = true
    ui.opened[0].value = false
    close(ui.hooks[0])
    const nextButton = env.element('next opener')
    env.document.activeElement = nextButton
    ui.opened[1].value = true
    const next = env.element('new dialog'); next.owner = ui.hooks[1].ownerId
    env.document.layers = [next]
    const input = env.element('new input', next)
    env.document.activeElement = input
    await nextTick()
    assert.equal(env.document.activeElement, input)
    ui.opened[1].value = false
    close(ui.hooks[1])
    ui.opened[1].value = true
    await nextTick()
    assert.equal(env.document.activeElement, input)
    ui.opened[1].value = false
    env.document.layers = []
    close(ui.hooks[1])
    await nextTick()
    assert.equal(env.document.activeElement, nextButton)
  }
  finally { ui.unmount(); env.restore() }
})

test('menu opener handoff is consumed once and expires after the selecting Vue update', async () => {
  const env = environment()
  const ui = mount()
  try {
    const button = env.element('real menu button')
    env.document.activeElement = env.element('temporary menu item')
    rememberDialogOpener(button as unknown as HTMLElement)
    ui.opened[0].value = true
    ui.opened[0].value = false
    close(ui.hooks[0])
    await nextTick()
    assert.equal(env.document.activeElement, button)

    rememberDialogOpener(button as unknown as HTMLElement)
    await nextTick()
    await nextTick()
    const unrelated = env.element('later unrelated opener')
    env.document.activeElement = unrelated
    ui.opened[0].value = true
    ui.opened[0].value = false
    close(ui.hooks[0])
    await nextTick()
    assert.equal(env.document.activeElement, unrelated)
  }
  finally { ui.unmount(); env.restore() }
})

test('removed and disabled openers are never focused', async () => {
  const env = environment()
  const ui = mount()
  try {
    for (const disabled of [false, true]) {
      const opener = env.element('unavailable opener')
      env.document.activeElement = opener
      ui.opened[0].value = true
      if (disabled) opener.disabled = true
      else opener.isConnected = false
      env.document.activeElement = env.document.body
      ui.opened[0].value = false
      close(ui.hooks[0])
      await nextTick()
      assert.equal(opener.focusCount, 0)
    }
  }
  finally { ui.unmount(); env.restore() }
})
