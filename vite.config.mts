import process from 'node:process'
import { fileURLToPath, URL } from 'node:url'
import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
import { defineConfig, loadEnv } from 'vite'
import { VitePWA } from 'vite-plugin-pwa'
import { precompressedAssets } from './build/precompress.mts'
import packageJson from './package.json' with { type: 'json' }

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd())

  return {
    resolve: {
      alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
    },
    define: {
      'import.meta.env.VITE_APP_BUILD': JSON.stringify(env.VITE_APP_BUILD || new Date().toISOString()),
      'import.meta.env.VITE_APP_VERSION': JSON.stringify(
        env.VITE_APP_VERSION || packageJson.version,
      ),
    },
    plugins: [
      vue(),
      tailwindcss(),
      env.VITE_GLOB_APP_PWA === 'true' && VitePWA({
        injectRegister: 'auto',
        manifest: {
          name: 'Sun-Panel',
          short_name: 'Sun-Panel',
          icons: [
            { src: 'favicon.svg', sizes: 'any', type: 'image/svg+xml' },
            { src: 'logo.png', sizes: '1024x1024', type: 'image/png' },
          ],
        },
      }),
      precompressedAssets(),
    ],
    server: {
      host: '0.0.0.0',
      port: 1002,
      open: false,
      proxy: {
        '/api': { target: env.VITE_APP_API_BASE_URL, changeOrigin: true },
        '/uploads': { target: env.VITE_APP_API_BASE_URL, changeOrigin: true },
      },
    },
    build: { reportCompressedSize: true, sourcemap: false },
  }
})
