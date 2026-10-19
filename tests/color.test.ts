import assert from 'node:assert/strict'
import test from 'node:test'
import { parseColor, serializeColor } from '../src/lib/color.ts'

test('short and full hexadecimal formats retain alpha bytes', () => {
  assert.deepEqual(parseColor('#abc'), { r: 170, g: 187, b: 204, a: 255 })
  assert.deepEqual(parseColor('#AbC8'), { r: 170, g: 187, b: 204, a: 136 })
  assert.equal(serializeColor(parseColor('#2a2a2a6b')!), '#2a2a2a6b')
  assert.equal(serializeColor(parseColor('#00000000')!), '#00000000')
  assert.equal(serializeColor(parseColor('#ffffffff')!), '#ffffff')
})

test('RGB and RGBA values save and reopen with their opacity', () => {
  for (const [input, saved] of [
    ['rgb(208, 48, 80)', '#d03050'],
    ['rgba(208, 48, 80, 1)', '#d03050'],
    ['rgba(42, 42, 42, 0.42)', '#2a2a2a6b'],
    ['rgb(100% 0% 50% / 25%)', '#ff008040'],
    ['rgba(0, 0, 0, 0)', '#00000000'],
    ['transparent', '#00000000'],
  ]) {
    const parsed = parseColor(input)
    assert.ok(parsed, input)
    assert.equal(serializeColor(parsed), saved)
    assert.deepEqual(parseColor(saved), parsed)
  }
})

test('all alpha bytes round trip exactly and RGB preview does not mutate them', () => {
  for (let a = 0; a <= 255; a++) {
    const color = { r: 42, g: 84, b: 168, a }
    const saved = serializeColor(color)
    assert.deepEqual(parseColor(saved), color)
    assert.equal(serializeColor(color, false), '#2a54a8')
    assert.equal(color.a, a)
  }
})

test('invalid or partial text cannot silently replace a valid color', () => {
  for (const input of [undefined, null, '', '#', '#12', '#12345', '#ffgg00', 'red', 'rgb(1,2)', 'rgb(256,0,0)', 'rgb(-1,0,0)', 'rgba(1,2,3)', 'rgba(1,2,3,2)', 'rgba(1,2,3,)', 'rgb(0 0 0 /)', 'rgb(0,0,0/50%)', 'rgb(1 2 3 / 50% / 20%)'])
    assert.equal(parseColor(input), null, String(input))
  assert.throws(() => serializeColor({ r: 256, g: 0, b: 0, a: 255 }), TypeError)
  assert.throws(() => serializeColor({ r: 0, g: 0, b: 0, a: 1.5 }), TypeError)
})
