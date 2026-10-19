import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile, mkdir, symlink } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { brotliDecompressSync, gunzipSync } from 'node:zlib'
import { precompressDirectory, precompressedAssets } from '../build/precompress.mts'

test('precompressed assets decode to their originals; tiny, binary and symlink files are skipped', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'sun-panel-assets-'))
  try {
    await mkdir(join(directory, 'assets'))
    const path = join(directory, 'assets', 'entry-abcdefgh.js')
    const source = 'export const example = "Sun Panel";\n'.repeat(100)
    await writeFile(path, source)
    await writeFile(join(directory, 'small.js'), '// tiny')
    await writeFile(join(directory, 'image.png'), source)
    await symlink(path, join(directory, 'linked.js'))
    await precompressDirectory(directory)
    assert.equal(brotliDecompressSync(await readFile(`${path}.br`)).toString(), source)
    assert.equal(gunzipSync(await readFile(`${path}.gz`)).toString(), source)
    for (const name of ['small.js.br', 'image.png.gz', 'linked.js.br'])
      await assert.rejects(readFile(join(directory, name)), { code: 'ENOENT' })
    // Rebuilding without emptyOutDir cannot leave stale compressed content.
    await writeFile(path, '// now tiny')
    await precompressDirectory(directory)
    await assert.rejects(readFile(`${path}.br`), { code: 'ENOENT' })
    await assert.rejects(readFile(`${path}.gz`), { code: 'ENOENT' })
  }
  finally {
    await rm(directory, { recursive: true, force: true })
  }
})


test('a failed bundle reports the source error without trying to compress missing output', async () => {
  const { build } = await import('vite')
  const directory = await mkdtemp(join(tmpdir(), 'sun-panel-broken-build-'))
  try {
    const entry = join(directory, 'invalid.js')
    await writeFile(entry, 'export const broken = ;')
    await assert.rejects(build({ configFile: false, root: directory, logLevel: 'silent', plugins: [precompressedAssets()], build: { outDir: join(directory, 'missing-output'), rollupOptions: { input: entry } } }), (error: unknown) => {
      const message = error instanceof Error ? error.message : String(error)
      return message.includes('invalid.js') && !message.includes('ENOENT')
    })
  }
  finally { await rm(directory, { recursive: true, force: true }) }
})
