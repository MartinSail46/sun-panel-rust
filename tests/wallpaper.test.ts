import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { DEFAULT_WALLPAPER, normalizeWallpaperSource } from '../src/utils/wallpaper.ts'

test('the default wallpaper is a shipped WebP at a stable public URL', () => {
  assert.equal(DEFAULT_WALLPAPER, '/default-wallpaper.webp')
  const image = readFileSync(new URL(`../public${DEFAULT_WALLPAPER}`, import.meta.url))
  assert.equal(image.subarray(0, 4).toString('ascii'), 'RIFF')
  assert.equal(image.subarray(8, 12).toString('ascii'), 'WEBP')
  assert.ok(image.length > 1000)
})

test('missing and cleared wallpapers use the default while custom images remain selected', () => {
  assert.equal(normalizeWallpaperSource(), DEFAULT_WALLPAPER)
  assert.equal(normalizeWallpaperSource(''), DEFAULT_WALLPAPER)
  assert.equal(normalizeWallpaperSource('   '), DEFAULT_WALLPAPER)
  assert.equal(normalizeWallpaperSource('/uploads/wallpaper.webp'), '/uploads/wallpaper.webp')
  assert.equal(normalizeWallpaperSource('https://example.com/picture.png'), 'https://example.com/picture.png')
})
