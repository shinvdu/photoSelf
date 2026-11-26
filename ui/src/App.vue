<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'

const API = '/api/v1'
const PLACEHOLDER =
  'data:image/svg+xml;utf8,' +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 320">' +
      '<rect width="320" height="320" fill="#1b2028"/>' +
      '<path d="M80 240l55-70 40 48 30-36 45 58H80z" fill="#2d3642"/>' +
      '<circle cx="120" cy="115" r="22" fill="#2d3642"/>' +
      '</svg>',
  )

const info = ref(null)
const items = ref([])
const nextCursor = ref(null)
const loading = ref(false)
const error = ref('')
const query = ref('')
const favoritesOnly = ref(false)
const selected = ref(null)
const scanning = ref(false)
const scanProgress = ref(null)
const sentinel = ref(null)

let observer = null
let scanTimer = null

async function api(path, options = {}) {
  const headers = { Accept: 'application/json', ...(options.headers || {}) }
  if (options.body) headers['Content-Type'] = 'application/json'
  const response = await fetch(path, { ...options, headers })
  if (!response.ok) {
    let message = response.statusText
    try {
      const problem = await response.json()
      message = problem.detail || problem.title || message
    } catch {
      // ignore non-JSON errors
    }
    throw new Error(message || `HTTP ${response.status}`)
  }
  if (response.status === 204) return null
  return response.json()
}

async function loadInfo() {
  try {
    info.value = await api(`${API}/info`)
  } catch (err) {
    error.value = err.message
  }
}

async function loadItems(reset = false) {
  if (loading.value) return
  if (!reset && !nextCursor.value) return
  loading.value = true
  error.value = ''
  try {
    const params = new URLSearchParams({ limit: '100' })
    if (query.value.trim()) params.set('q', query.value.trim())
    if (favoritesOnly.value) params.set('favorite', 'true')
    if (!reset && nextCursor.value) params.set('cursor', nextCursor.value)

    const data = await api(`${API}/items?${params.toString()}`)
    items.value = reset ? data.items : [...items.value, ...data.items]
    nextCursor.value = data.next_cursor
  } catch (err) {
    error.value = err.message
  } finally {
    loading.value = false
  }
}

function reload() {
  nextCursor.value = null
  loadItems(true)
}

function toggleFavorites() {
  favoritesOnly.value = !favoritesOnly.value
  reload()
}

const groups = computed(() => {
  const map = new Map()
  for (const item of items.value) {
    const day = (item.taken_at || '1970-01-01').slice(0, 10)
    if (!map.has(day)) map.set(day, [])
    map.get(day).push(item)
  }
  return [...map.entries()].map(([date, groupItems]) => ({ date, items: groupItems }))
})

const selectedIndex = computed(() =>
  selected.value ? items.value.findIndex((item) => item.id === selected.value.id) : -1,
)

function dateLabel(day) {
  if (!day || day.startsWith('1970-')) return 'Undated'
  const date = new Date(`${day}T00:00:00`)
  return date.toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    weekday: 'short',
  })
}

function formatDate(value) {
  if (!value) return 'Unknown'
  return value.replace('T', ' ')
}

function formatDuration(ms) {
  if (!ms) return ''
  const total = Math.round(ms / 1000)
  const minutes = Math.floor(total / 60)
  const seconds = total % 60
  return `${minutes}:${String(seconds).padStart(2, '0')}`
}

function formatBytes(bytes) {
  if (!bytes) return ''
  const units = ['B', 'KB', 'MB', 'GB']
  let index = 0
  let value = bytes
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index += 1
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[index]}`
}

function openItem(item) {
  selected.value = item
}

function close() {
  selected.value = null
}

function prev() {
  if (selectedIndex.value > 0) selected.value = items.value[selectedIndex.value - 1]
}

function next() {
  if (selectedIndex.value >= 0 && selectedIndex.value < items.value.length - 1) {
    selected.value = items.value[selectedIndex.value + 1]
  }
}

async function toggleFavorite(item) {
  try {
    const updated = await api(`${API}/items/${item.id}`, {
      method: 'PATCH',
      body: JSON.stringify({ favorite: !item.favorite }),
    })
    item.favorite = updated.favorite
    if (selected.value?.id === item.id) selected.value.favorite = updated.favorite
  } catch (err) {
    error.value = err.message
  }
}

function onImgError(event) {
  if (event.target.src !== PLACEHOLDER) event.target.src = PLACEHOLDER
}

async function startScan() {
  if (scanning.value) return
  scanning.value = true
  error.value = ''
  try {
    const { job_id: jobId } = await api(`${API}/scan`, { method: 'POST' })
    pollScan(jobId)
  } catch (err) {
    error.value = err.message
    scanning.value = false
  }
}

async function pollScan(jobId) {
  try {
    const job = await api(`${API}/jobs/${jobId}`)
    scanProgress.value = job
    if (job.state === 'completed' || job.state === 'failed') {
      scanning.value = false
      if (job.state === 'completed') {
        await Promise.all([loadInfo(), loadItems(true)])
      } else {
        error.value = job.error || 'scan failed'
      }
      return
    }
    scanTimer = setTimeout(() => pollScan(jobId), 1000)
  } catch (err) {
    scanning.value = false
    error.value = err.message
  }
}

function onKey(event) {
  if (event.key === 'Escape') close()
  if (!selected.value) return
  if (event.key === 'ArrowLeft') prev()
  if (event.key === 'ArrowRight') next()
}

watch(selected, (value) => {
  document.body.classList.toggle('modal-open', Boolean(value))
})

onMounted(async () => {
  await Promise.all([loadInfo(), loadItems(true)])

  observer = new IntersectionObserver(
    (entries) => {
      if (entries[0]?.isIntersecting && nextCursor.value && !loading.value) {
        loadItems(false)
      }
    },
    { rootMargin: '600px' },
  )
  if (sentinel.value) observer.observe(sentinel.value)

  window.addEventListener('keydown', onKey)
})

onUnmounted(() => {
  observer?.disconnect()
  if (scanTimer) clearTimeout(scanTimer)
  window.removeEventListener('keydown', onKey)
  document.body.classList.remove('modal-open')
})
</script>

<template>
  <header class="topbar">
    <div class="brand">
      <span class="logo">▣</span>
      <span>photoSelf</span>
      <small v-if="info">{{ info.version }}</small>
    </div>

    <form class="search" @submit.prevent="reload">
      <input v-model="query" type="search" placeholder="Search filename…" />
      <button type="submit">Search</button>
    </form>

    <div class="actions">
      <button :class="{ active: favoritesOnly }" @click="toggleFavorites">★ Favorites</button>
      <button :disabled="scanning" @click="startScan">
        {{ scanning ? 'Scanning…' : 'Scan' }}
      </button>
    </div>
  </header>

  <div class="statusbar">
    <span>{{ info ? info.item_count : items.length }} items</span>
    <span v-if="scanning && scanProgress">
      scan {{ scanProgress.progress }} / {{ scanProgress.total }}
    </span>
    <span v-if="error" class="error">{{ error }}</span>
  </div>

  <main>
    <div v-if="!loading && items.length === 0" class="empty">
      <h2>No media indexed yet</h2>
      <p>Check <code>config.toml</code>, then press <strong>Scan</strong>.</p>
    </div>

    <section v-for="group in groups" :key="group.date" class="day">
      <h2>{{ dateLabel(group.date) }} <small>{{ group.items.length }}</small></h2>
      <div class="grid">
        <button
          v-for="item in group.items"
          :key="item.id"
          class="tile"
          type="button"
          @click="openItem(item)"
        >
          <img :src="item.thumb_url" :alt="item.filename" loading="lazy" @error="onImgError" />
          <span v-if="item.kind === 'video'" class="play">▶</span>
          <span v-if="item.favorite" class="fav">★</span>
        </button>
      </div>
    </section>

    <div ref="sentinel" class="sentinel"></div>
    <button
      v-if="nextCursor"
      class="load-more"
      type="button"
      :disabled="loading"
      @click="loadItems(false)"
    >
      {{ loading ? 'Loading…' : 'Load more' }}
    </button>
  </main>

  <div v-if="selected" class="lightbox" @click.self="close">
    <button class="close" type="button" @click="close">×</button>
    <button class="nav prev" type="button" @click.stop="prev">‹</button>

    <div class="viewer">
      <img
        v-if="selected.kind !== 'video'"
        :src="selected.preview_url || selected.media_url"
        :alt="selected.filename"
      />
      <video
        v-else
        :src="selected.media_url"
        controls
        autoplay
        playsinline
      ></video>
    </div>

    <button class="nav next" type="button" @click.stop="next">›</button>

    <aside class="meta">
      <h3>{{ selected.filename }}</h3>
      <dl>
        <div><dt>Date</dt><dd>{{ formatDate(selected.taken_at) }}</dd></div>
        <div v-if="selected.width">
          <dt>Dimensions</dt><dd>{{ selected.width }} × {{ selected.height }}</dd>
        </div>
        <div v-if="selected.duration_ms">
          <dt>Duration</dt><dd>{{ formatDuration(selected.duration_ms) }}</dd>
        </div>
        <div v-if="selected.camera"><dt>Camera</dt><dd>{{ selected.camera }}</dd></div>
        <div><dt>Size</dt><dd>{{ formatBytes(selected.size) }}</dd></div>
      </dl>
      <div class="row">
        <button type="button" @click="toggleFavorite(selected)">
          {{ selected.favorite ? '★ Unfavorite' : '☆ Favorite' }}
        </button>
        <a :href="selected.media_url" :download="selected.filename">Download</a>
      </div>
    </aside>
  </div>
</template>
