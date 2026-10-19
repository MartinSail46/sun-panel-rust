import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import * as Vue from 'vue'
import { compileScript, parse } from 'vue/compiler-sfc'
import { renderToString } from 'vue/server-renderer'
import ts from 'typescript'

const source = readFileSync(new URL('../src/components/deskModule/SystemMonitor/components/GenericProgress/index.vue', import.meta.url), 'utf8')
const compiled = compileScript(parse(source).descriptor, { id: 'monitor-progress-regression', inlineTemplate: true })
const javascript = ts.transpileModule(compiled.content, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
const module = { exports: {} as { default: Vue.Component } }
const dependencies: Record<string, unknown> = {
  'vue': Vue,
  '@/components/ui/button': { Button: Vue.defineComponent({ setup: (_, { attrs, slots }) => () => Vue.h('button', attrs, slots.default?.()) }) },
  '@/enums': { PanelPanelConfigStyleEnum: { info: 0, icon: 1 } },
}
runInNewContext(javascript, {
  module,
  exports: module.exports,
  require(name: string) {
    if (!(name in dependencies))
      throw new Error(`Unexpected monitor dependency: ${name}`)
    return dependencies[name]
  },
})

async function render(props: Record<string, unknown>) {
  const app = Vue.createSSRApp(module.exports.default, {
    textColor: '#fff', progressColor: '#fff', progressRailColor: '#888',
    cardTypeStyle: 1, infoCardLeftText: 'CPU', ...props,
  })
  return renderToString(app)
}

test('the dashboard renders real zero and full readings with an accessible SVG arc', async () => {
  const zero = await render({ percentage: 0 })
  assert.match(zero, /role="progressbar"[^>]*aria-label="CPU"[^>]*aria-valuenow="0"/)
  assert.match(zero, />0%<\/text>/)
  assert.equal((zero.match(/<circle/g) || []).length, 1, 'A zero reading should not paint a rounded foreground dot')
  const full = await render({ percentage: 100 })
  assert.match(full, /aria-valuenow="100"/)
  assert.equal((full.match(/stroke-dasharray="75 100"/g) || []).length, 2)
})

test('an initial loading or invalid reading remains unknown rather than appearing as zero', async () => {
  const loading = await render({ percentage: null, loading: true })
  assert.match(loading, /aria-busy="true"/)
  assert.match(loading, /加载中/)
  assert.match(loading, /—/)
  assert.doesNotMatch(loading, /aria-valuenow|>0%/)
  const invalid = await render({ percentage: Number.NaN })
  assert.doesNotMatch(invalid, /NaN|aria-valuenow|>0%/)
})

test('a failed refresh keeps the last value and timestamp while exposing retry and stale status', async () => {
  const html = await render({ percentage: 64.2, error: 'Metrics unavailable', updatedAt: 1792368000000, stale: true })
  assert.match(html, /aria-valuenow="64.2"/)
  assert.match(html, /Metrics unavailable/)
  assert.match(html, /最后采样/)
  assert.match(html, /数据已过期/)
  assert.match(html, /<button[^>]*>\s*重试/)
})

test('capacity cards retain used and total text with a bounded line progress bar', async () => {
  const html = await render({ cardTypeStyle: 0, percentage: 50, capacity: true, infoCardLeftText: 'RAM', infoCardRightText: '4 GB / 8 GB' })
  assert.match(html, /4 GB \/ 8 GB/)
  assert.match(html, /已用 \/ 总量/)
  assert.match(html, /aria-valuenow="50"/)
  assert.match(html, /width:50%/)
  const bounded = await render({ percentage: 101 })
  assert.match(bounded, /aria-valuenow="100"/)
})
