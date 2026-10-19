import { readdir, readFile, rm, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { promisify } from 'node:util'
import { brotliCompress, constants, gzip } from 'node:zlib'
import type { Plugin } from 'vite'

const compressBrotli = promisify(brotliCompress)
const compressGzip = promisify(gzip)

export async function precompressDirectory(directory: string): Promise<void> {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      await precompressDirectory(path)
      continue
    }
    if (!entry.isFile() || !/\.(?:html|js|css|json|svg|webmanifest)$/.test(entry.name))
      continue

    const source = await readFile(path)
    // Tiny files cost more in sidecars and negotiation than they save.
    const compressed = source.length >= 1024
      ? await Promise.all([
          compressBrotli(source, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }),
          compressGzip(source, { level: 9 }),
        ])
      : []
    for (const [index, extension] of ['br', 'gz'].entries()) {
      const output = compressed[index]
      if (output && output.length < source.length)
        await writeFile(`${path}.${extension}`, output)
      else
        await rm(`${path}.${extension}`, { force: true })
    }
  }
}

export function precompressedAssets(): Plugin {
  let outputDirectory: string
  let write = false
  let failed = false
  return {
    name: 'sun-panel-precompressed-assets',
    apply: 'build',
    configResolved(config) {
      outputDirectory = resolve(config.root, config.build.outDir)
      write = config.build.write
    },
    buildStart() { failed = false },
    buildEnd(error) { failed = !!error },
    closeBundle: {
      order: 'post',
      sequential: true,
      async handler() {
        if (write && !failed)
          await precompressDirectory(outputDirectory)
      },
    },
  }
}
