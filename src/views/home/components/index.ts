import { defineAsyncComponent } from 'vue'
import AppIcon from './AppIcon/index.vue'

const EditItem = defineAsyncComponent(() => import('./EditItem/index.vue'))
const AppStarter = defineAsyncComponent(() => import('./AppStarter/index.vue'))

export { EditItem, AppIcon, AppStarter }
