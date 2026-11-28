import { createRouter, createWebHashHistory } from 'vue-router'
import App from './App.vue'

// Hash history keeps deep links working on embedded/static deployments where
// the server has no SPA fallback.
const routes = [
  { path: '/', redirect: '/timeline' },
  { path: '/timeline', name: 'timeline', component: App },
  { path: '/albums', name: 'albums', component: App },
  { path: '/albums/:id', name: 'album', component: App, props: true },
  { path: '/trash', name: 'trash', component: App },
  { path: '/libraries', name: 'libraries', component: App },
  { path: '/:pathMatch(.*)*', redirect: '/timeline' },
]

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
})

export default router