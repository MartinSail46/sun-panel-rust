import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { runInNewContext } from 'node:vm'
import * as Vue from 'vue'
import { compileScript, parse } from 'vue/compiler-sfc'
import ts from 'typescript'
import { isRequestCancelled, isUncertainWrite, RequestError } from '../src/utils/requestLifetime.ts'
import type { TrashEntry, TrashRef } from '../src/api/panel/trash.ts'

interface HostNode { tag?: string; text?: string; comment?: boolean; props: Record<string, any>; children: HostNode[]; parent: HostNode | null }
function renderer() {
  const node = (extra: Partial<HostNode> = {}): HostNode => ({ props: {}, children: [], parent: null, ...extra })
  const runtime = Vue.createRenderer<HostNode, HostNode>({
    createElement: tag => node({ tag }), createText: text => node({ text }), createComment: text => node({ text, comment: true }),
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
  return { runtime, root: node() }
}
const source = readFileSync(new URL('../src/components/apps/RecycleBin/index.vue', import.meta.url), 'utf8')
const compiled = compileScript(parse(source).descriptor, { id: 'recycle-bin-regression', inlineTemplate: true })
const javascript = ts.transpileModule(compiled.content, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
const layout = (tag: string) => Vue.defineComponent({ setup: (_props, { slots }) => () => Vue.h(tag, slots.default?.()) })
const Modal = Vue.defineComponent({
  inheritAttrs: false,
  props: ['show', 'title'],
  setup: (props, { slots }) => () => props.show ? Vue.h('section', { 'data-modal': true }, [Vue.h('h3', props.title), slots.default?.(), slots.footer?.()]) : null,
})
const Select = Vue.defineComponent({
  props: ['modelValue', 'options', 'disabled'], emits: ['update:modelValue'],
  setup: (props, { emit }) => () => Vue.h('select', { value: props.modelValue, disabled: props.disabled, onChange: (event: any) => emit('update:modelValue', Number(event.target.value)) }),
})
function all(root: HostNode, predicate: (node: HostNode) => boolean): HostNode[] {
  return [...(predicate(root) ? [root] : []), ...root.children.flatMap(child => all(child, predicate))]
}
function text(node: HostNode): string { return node.comment ? '' : (node.text || '') + node.children.map(text).join('') }
async function settle() { for (let index = 0; index < 20; index++) await Promise.resolve(); await Vue.nextTick() }
async function click(node: HostNode) {
  if (node.props.disabled) return
  const handlers = Array.isArray(node.props.onClick) ? node.props.onClick : [node.props.onClick]
  for (const handler of handlers) await handler?.({})
  await settle()
}
const application: TrashEntry = { kind: 'item', id: 7, deletionId: 'batch-7', title: 'Recover me', deletedAt: '2026-10-19T00:00:00Z', groupId: 2, groupTitle: 'Deleted group', itemCount: 1, requiresTargetGroup: true }
function host(options: { entry?: TrashEntry; loseAcknowledgement?: boolean; failRefreshAfterWrite?: boolean } = {}) {
  let data = [options.entry || application]
  let committed = false
  const calls = { restore: [] as Record<string, unknown>[], purge: [] as TrashRef[], lists: 0, refreshes: 0, guarded: 0 }
  const messages = { success: [] as string[], warning: [] as string[], error: [] as string[] }
  const navigation = Vue.reactive({
    groups: [{ id: 3, title: 'Available group', items: [] }], saving: false, loading: false, needsVerification: false,
    get busy() { return this.saving || this.loading || this.needsVerification },
    async refresh() {
      calls.refreshes++
      if (committed && options.failRefreshAfterWrite) throw new Error('Refresh unavailable')
      this.needsVerification = false
    },
    async withMutation<T>(run: () => Promise<T>) {
      calls.guarded++
      this.saving = true
      try { return await run() }
      catch (cause) { this.needsVerification = isUncertainWrite(cause); throw cause }
      finally { this.saving = false }
    },
  })
  const dependencies: Record<string, unknown> = {
    '@/api/panel/trash': {
      listTrash: async () => { calls.lists++; return { code: 0, data: { list: data, count: data.length } } },
      restoreTrash: async (payload: Record<string, unknown>) => {
        calls.restore.push(payload)
        data = []
        committed = true
        if (options.loseAcknowledgement) throw new RequestError('timeout', 'write')
        return { code: 0, revision: 2 }
      },
      purgeTrash: async (payload: TrashRef) => { calls.purge.push(payload); committed = true; data = []; return { code: 0, revision: 2 } },
    },
    '@/components/common': { RoundCardModal: Modal, SvgIcon: layout('i') },
    '@/components/common/AppSelect.vue': { __esModule: true, default: Select },
    '@/components/ui/button': { Button: layout('button') },
    '@/components/ui/label': { Label: layout('label') },
    '@/lib/feedback': { toast: Object.fromEntries(Object.keys(messages).map(kind => [kind, (value: string) => messages[kind as keyof typeof messages].push(value)])) },
    '@/store/modules/navigation': { useNavigationStore: () => navigation },
    '@/utils/requestLifetime': { isRequestCancelled, isUncertainWrite, RequestError },
  }
  const module = { exports: {} as { default: Vue.Component } }
  runInNewContext(javascript, {
    module, exports: module.exports, AbortController, Date, Error,
    require(name: string) {
      if (name === 'vue') return Vue
      if (name in dependencies) return dependencies[name]
      throw new Error(`Unexpected recycle-bin dependency: ${name}`)
    },
  })
  const { runtime, root } = renderer()
  const busy: boolean[] = []
  const app = runtime.createApp(module.exports.default, { onBusy: (value: boolean) => busy.push(value) })
  app.mount(root)
  return {
    app, root, calls, messages, busy,
    button(label: string, last = false) {
      const matches = all(root, node => node.tag === 'button' && text(node).trim() === label)
      const button = last ? matches.at(-1) : matches[0]
      assert.ok(button, `Visible button: ${label}`)
      return button
    },
    chooseGroup(id: number) {
      const select = all(root, node => node.tag === 'select')[0]
      assert.ok(select, 'Restoring an application shows the destination picker')
      select.props.onChange({ target: { value: String(id) } })
    },
  }
}

test('restoring an orphaned application requires a destination and preserves successful writes when refresh fails', async () => {
  const ui = host({ failRefreshAfterWrite: true })
  await settle()
  await click(ui.button('恢复'))
  assert.equal(ui.button('恢复', true).props.disabled, true)
  assert.equal(ui.calls.restore.length, 0)
  ui.chooseGroup(3)
  await settle()
  assert.equal(ui.button('恢复', true).props.disabled, false)
  await click(ui.button('恢复', true))
  assert.deepEqual(JSON.parse(JSON.stringify(ui.calls.restore)), [{ kind: 'item', id: 7, deletionId: 'batch-7', targetGroupId: 3 }])
  assert.equal(ui.calls.guarded, 1)
  assert.deepEqual(ui.messages.success, ['已恢复到导航'])
  assert.equal(ui.messages.error.length, 0)
  assert.match(ui.messages.warning[0]!, /已恢复，但/)
  assert.equal(ui.busy.at(-1), false)
  ui.app.unmount()
})

test('permanent group deletion requires confirmation and forwards the deletion identity', async () => {
  const ui = host({ entry: { ...application, kind: 'group', id: 2, title: 'Deleted group', itemCount: 4, deletionId: 'group-batch' } })
  await settle()
  await click(ui.button('永久删除'))
  assert.equal(ui.calls.purge.length, 0)
  assert.match(text(ui.root), /该分组及随它删除的应用将被永久删除/)
  await click(ui.button('取消'))
  assert.equal(ui.calls.purge.length, 0)
  await click(ui.button('永久删除'))
  await click(ui.button('永久删除', true))
  assert.deepEqual(JSON.parse(JSON.stringify(ui.calls.purge)), [{ kind: 'group', id: 2, deletionId: 'group-batch' }])
  assert.equal(ui.calls.guarded, 1)
  assert.deepEqual(ui.messages.success, ['已永久删除'])
  ui.app.unmount()
})

test('a lost restore acknowledgement reconciles with reads without replaying the write', async () => {
  const ui = host({ loseAcknowledgement: true })
  await settle()
  await click(ui.button('恢复'))
  ui.chooseGroup(3)
  await settle()
  await click(ui.button('恢复', true))
  assert.equal(ui.calls.restore.length, 1)
  assert.equal(ui.calls.lists, 2)
  assert.equal(ui.calls.refreshes, 2)
  assert.equal(ui.messages.success.length, 0)
  assert.match(text(ui.root), /操作结果尚未确认/)
  assert.match(text(ui.root), /回收站是空的/)
  assert.equal(all(ui.root, node => node.props['data-modal']).length, 0)
  ui.app.unmount()
})
