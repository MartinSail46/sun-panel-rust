import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { runInNewContext } from 'node:vm'
import * as Vue from 'vue'
import * as Reka from 'reka-ui'
import { compileScript, parse } from 'vue/compiler-sfc'
import ts from 'typescript'
import * as feedback from '../src/lib/feedback.ts'
import { useDialogFocus } from '../src/hooks/useDialogFocus.ts'

interface HostNode { tag?: string; text?: string; props: Record<string, unknown>; children: HostNode[]; parent: HostNode | null }
function renderer() {
  const node = (extra: Partial<HostNode> = {}): HostNode => ({ props: {}, children: [], parent: null, ...extra })
  const renderer = Vue.createRenderer<HostNode, HostNode>({
    createElement: tag => node({ tag }), createText: text => node({ text }), createComment: text => node({ text }),
    insert(el, parent, anchor) {
      if (el.parent) el.parent.children.splice(el.parent.children.indexOf(el), 1)
      el.parent = parent
      const index = anchor ? parent.children.indexOf(anchor) : -1
      if (index < 0) parent.children.push(el)
      else parent.children.splice(index, 0, el)
    },
    remove(el) { if (el.parent) el.parent.children.splice(el.parent.children.indexOf(el), 1); el.parent = null },
    setText: (el, text) => { el.text = text }, setElementText: (el, text) => { el.text = text },
    parentNode: el => el.parent, nextSibling: el => el.parent?.children[el.parent.children.indexOf(el) + 1] || null,
    patchProp: (el, key, _old, value) => { el.props[key] = value },
  })
  return { renderer, root: node() }
}
function component(source: string, dependencies: Record<string, unknown>) {
  const compiled = compileScript(parse(source).descriptor, { id: 'feedback-regression', inlineTemplate: true })
  const javascript = ts.transpileModule(compiled.content, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
  const module = { exports: {} as { default: Vue.Component } }
  runInNewContext(javascript, {
    module, exports: module.exports,
    require(name: string) {
      if (name === 'vue') return Vue
      if (name in dependencies) return dependencies[name]
      throw new Error(`Unexpected confirmation dependency: ${name}`)
    },
  })
  return module.exports.default
}
const Button = component(readFileSync(new URL('../src/components/ui/button/Button.vue', import.meta.url), 'utf8'), {
  'reka-ui': Reka, 'cn': { cn: () => '' }, '.': { buttonVariants: () => '' },
})
const layout = (tag: string) => Vue.defineComponent({ setup: (_props, { slots }) => () => Vue.h(tag, slots.default?.()) })
const Host = component(readFileSync(new URL('../src/components/common/FeedbackHost.vue', import.meta.url), 'utf8'), {
  '@/components/ui/sonner': { Toaster: layout('aside') },
  '@/components/ui/button': { Button },
  '@/components/ui/alert-dialog': {
    AlertDialog: Reka.AlertDialogRoot,
    AlertDialogAction: Reka.AlertDialogAction,
    // Keep the real DialogClose click behavior used by AlertDialogCancel. The
    // portal/focus presentation is omitted from this DOM-independent test.
    AlertDialogCancel: Reka.DialogClose,
    AlertDialogContent: layout('section'), AlertDialogDescription: layout('p'),
    AlertDialogFooter: layout('footer'), AlertDialogHeader: layout('header'), AlertDialogTitle: layout('h2'),
  },
  '@/lib/feedback': feedback,
  '@/hooks/useDialogFocus': { useDialogFocus },
})
function find(root: HostNode, predicate: (node: HostNode) => boolean): HostNode | undefined {
  if (predicate(root)) return root
  for (const child of root.children) { const found = find(child, predicate); if (found) return found }
}
function text(node: HostNode): string { return (node.text || '') + node.children.map(text).join('') }
function click(node: HostNode) {
  const handlers = Array.isArray(node.props.onClick) ? node.props.onClick : [node.props.onClick]
  for (const handler of handlers) (handler as (event: object) => void)?.({})
}
function host() {
  const { renderer: runtime, root } = renderer()
  const app = runtime.createApp(Host, { dark: false })
  app.mount(root)
  return {
    app, root,
    button(label: string) {
      const button = find(root, node => node.tag === 'button' && text(node).trim() === label)
      assert.ok(button, `Visible confirmation button: ${label}`)
      return button
    },
  }
}

test('real Reka action dispatches automatic close before its supplied click handler', () => {
  const calls: string[] = []
  const { renderer: runtime, root } = renderer()
  const app = runtime.createApp(Vue.defineComponent({
    render: () => Vue.h(Reka.AlertDialogRoot, { open: true, 'onUpdate:open': () => calls.push('close') }, {
      default: () => Vue.h(Reka.AlertDialogAction, { onClick: () => calls.push('accept') }, { default: () => 'Confirm' }),
    }),
  }))
  app.mount(root)
  click(find(root, node => node.tag === 'button')!)
  assert.deepEqual(calls, ['close', 'accept'])
  app.unmount()
})

test('actual FeedbackHost confirmation resolves true and runs its caller action once', async () => {
  const ui = host()
  let deletes = 0
  const result = feedback.confirmDialog({ title: 'Delete', description: 'Delete one image?', confirmText: 'Delete now', destructive: true })
    .then((confirmed) => { if (confirmed) deletes++; return confirmed })
  await Vue.nextTick()
  click(ui.button('Delete now'))
  assert.equal(await result, true)
  assert.equal(deletes, 1)
  assert.equal(feedback.currentConfirmation.value, undefined)
  ui.app.unmount()
})

test('confirmation queue retains the next request after stale close or click callbacks', async () => {
  const ui = host()
  const first = feedback.confirmDialog({ title: 'First', description: 'First request', confirmText: 'Accept first' })
  const firstId = feedback.currentConfirmation.value!.id
  const second = feedback.confirmDialog({ title: 'Second', description: 'Second request', confirmText: 'Accept second', cancelText: 'Cancel second' })
  await Vue.nextTick()
  click(ui.button('Accept first'))
  assert.equal(await first, true)
  await Vue.nextTick()
  const secondId = feedback.currentConfirmation.value!.id
  assert.notEqual(secondId, firstId)
  feedback.resolveConfirmation(firstId, false)
  assert.equal(feedback.currentConfirmation.value!.id, secondId)
  click(ui.button('Cancel second'))
  assert.equal(await second, false)
  assert.equal(feedback.currentConfirmation.value, undefined)
  ui.app.unmount()
})
