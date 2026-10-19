import { createApp } from 'vue'
import App from './App.vue'
import './plugins'
import { setupStore } from './store'
import { setupRouter } from './router'
import { registerSvgIcons } from './plugins/svg-icons'

async function bootstrap() {
  const app = createApp(App)
  registerSvgIcons()


  setupStore(app)

  await setupRouter(app)
  app.mount('#app')
}

bootstrap()
