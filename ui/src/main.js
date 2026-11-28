import { createApp } from 'vue'
import App from './App.vue'
import './styles.css'
import i18n from './i18n'
import router from './router'

const app = createApp(App).use(i18n).use(router)

router.isReady().then(() => app.mount('#app'))
