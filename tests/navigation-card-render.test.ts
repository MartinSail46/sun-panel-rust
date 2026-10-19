import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import * as Vue from 'vue'
import { compileScript, parse } from 'vue/compiler-sfc'
import { renderToString } from 'vue/server-renderer'
import ts from 'typescript'

const enums = { PanelPanelConfigStyleEnum: { info: 0, icon: 1, small: 1 } }
const ItemIcon = Vue.defineComponent({ props: ['itemIcon'], setup: props => () => Vue.h('span', String(props.itemIcon?.text || '')) })

function component(source: string, dependencies: Record<string, unknown>) {
  const descriptor = parse(source).descriptor
  const compiled = compileScript(descriptor, { id: 'card-regression', inlineTemplate: true })
  const javascript = ts.transpileModule(compiled.content, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
  const module = { exports: {} as { default: Vue.Component } }
  const require = (name: string) => {
    if (name === 'vue') return Vue
    if (name in dependencies) return dependencies[name]
    throw new Error(`Unexpected card dependency: ${name}`)
  }
  // Execute the actual compiled component using small presentational stubs;
  // no browser, store or network is needed to verify visible card content.
  runInNewContext(javascript, { require, module, exports: module.exports })
  return module.exports.default
}

const Card = component(readFileSync(new URL('../src/views/home/components/AppIcon/index.vue', import.meta.url), 'utf8'), {
  '@/components/common': { ItemIcon },
  '@/enums': enums,
})
const home = readFileSync(new URL('../src/views/home/index.vue', import.meta.url), 'utf8')
const binding = home.match(/<AppIcon\b[\s\S]*?\/>/)?.[0]
assert.ok(binding, 'Home should render navigation cards')
const Parent = component(`<script setup>import AppIcon from './card';defineProps(['item'])</script><template>${binding}</template>`, {
  './card': { __esModule: true, default: Card },
  '@/enums': enums,
})
for (const mode of [0, 1]) {
  test(`home card binding renders the icon and title in display mode ${mode}`, async () => {
    const html = await renderToString(Vue.createSSRApp(Parent, {
      item: { title: 'Internal service', description: 'Private dashboard', icon: { itemType: 1, text: 'R', style: mode, textColor: '#ffffff' } },
    }))
    assert.match(html, /Internal service/)
    assert.match(html, />R</)
    assert.match(html, mode === 0 ? /app-icon-info/ : /app-icon-small/)
  })
}

test('each card keeps its own text color and visibility settings', async () => {
  const html = await renderToString(Vue.createSSRApp({
    render: () => Vue.h('section', [
      Vue.h(Card, { itemInfo: { title: '详情应用', description: '隐藏描述', icon: { itemType: 1, text: 'A', style: 0, hideDescription: true, textColor: '#22cc88' } } }),
      Vue.h(Card, { itemInfo: { title: '隐藏标题', icon: { itemType: 1, text: 'B', style: 1, hideTitle: true } } }),
    ]),
  }))
  assert.match(html, /app-icon-info/)
  assert.match(html, /app-icon-small/)
  assert.match(html, /color:#22cc88/)
  assert.match(html, /详情应用/)
  assert.doesNotMatch(html, /隐藏描述|隐藏标题/)
})
