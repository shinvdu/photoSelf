<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { API, api } from './api'
import { toast, showToast, clearToast } from './toast'
import { onImgError } from './placeholder'
import { formatDate, formatDuration, formatBytes, groupByDay, dateLabel } from './format'
import { useTheme } from './composables/useTheme'
import { useLightbox } from './composables/useLightbox'
import { useSelection } from './composables/useSelection'
import AppHeader from './components/AppHeader.vue'
import AlbumsView from './components/AlbumsView.vue'
import TrashView from './components/TrashView.vue'
import LibraryManager from './components/LibraryManager.vue'
import SelectionBar from './components/SelectionBar.vue'
import ContextMenu from './components/ContextMenu.vue'
import AlbumPicker from './components/AlbumPicker.vue'

const { t, locale } = useI18n({ useScope: 'global' })
const route = useRoute()
const router = useRouter()

const currentLocale = ref(locale.value)
const { theme, toggleTheme, applyThemeMeta } = useTheme()

const info = ref(null)
const items = ref([])
const nextCursor = ref(null)
const loading = ref(false)
const error = ref('')
const query = ref('')
const favoritesOnly = ref(false)
const tagFilter = ref('')
const tags = ref([])
const selected = ref(null)
const scanning = ref(false)
const scanProgress = ref(null)
const sentinel = ref(null)

const albums = ref([])
const albumItems = ref([])
const uploadLibrary = ref('')
const uploading = ref(false)
const uploadMessage = ref('')
const addTagId = ref('')
const newTagName = ref('')
const addAlbumId = ref('')
const transcodeStatus = ref('')
const videoError = ref(false)
const fileInput = ref(null)
const libraryManagerRef = ref(null)

let scanTimer = null
let transcodeTimer = null

const view = computed(() => {
  switch (route.name) {
    case 'albums':
      return 'albums'
    case 'album':
      return 'album'
    case 'trash':
      return 'trash'
    case 'libraries':
      return 'manage'
    default:
      return 'timeline'
  }
})

const activeAlbum = computed(() => {
  if (view.value !== 'album') return null
  const id = String(route.params.id)
  return albums.value.find((album) => String(album.id) === id) || null
})

async function loadInfo() {
  try {
    info.value = await api(`${API}/info`)
  } catch (err) {
    error.value = err.message
  }
}

async function loadTags() {
  try {
    const data = await api(`${API}/tags`)
    tags.value = data.tags || []
  } catch (err) {
    error.value = err.message
  }
}

async function loadItems(reset = false) {
  if (loading.value) return
  if (!reset && !nextCursor.value) return
  loading.value = true
  error.value = ''
  let succeeded = false
  try {
    const params = new URLSearchParams({ limit: '100' })
    if (query.value.trim()) params.set('q', query.value.trim())
    if (favoritesOnly.value) params.set('favorite', 'true')
    if (tagFilter.value) params.set('tag', tagFilter.value)
    if (!reset && nextCursor.value) params.set('cursor', nextCursor.value)

    const data = await api(`${API}/items?${params.toString()}`)
    items.value = reset ? data.items : [...items.value, ...data.items]
    nextCursor.value = data.next_cursor
    succeeded = true
  } catch (err) {
    error.value = err.message
  } finally {
    loading.value = false
    if (succeeded) nextTick(checkLoadMore)
  }
}

// The old IntersectionObserver kept watching the sentinel element from the
// previous render, so auto-loading stopped after switching views. A scroll
// position check works regardless of when the sentinel is re-created.
function checkLoadMore() {
  if (view.value !== 'timeline' || selected.value || loading.value || !nextCursor.value) return
  const element = sentinel.value
  if (!element) return
  const rect = element.getBoundingClientRect()
  if (rect.top <= window.innerHeight + 500) loadItems(false)
}

function onWindowScroll() {
  checkLoadMore()
}
async function loadAlbums() {
  try {
    const data = await api(`${API}/albums`)
    albums.value = data.albums || []
  } catch (err) {
    error.value = err.message
  }
}

async function loadAlbumItems() {
  if (!activeAlbum.value) return
  loading.value = true
  try {
    const data = await api(`${API}/albums/${activeAlbum.value.id}/items?limit=500`)
    albumItems.value = data.items || []
  } catch (err) {
    error.value = err.message
  } finally {
    loading.value = false
  }
}

const writableLibraries = computed(() =>
  (info.value?.libraries || []).filter((library) => !library.read_only),
)

const canDeleteSelected = computed(() => {
  if (!info.value?.write_enabled || !info.value?.trash_enabled || !selected.value) {
    return false
  }
  const library = (info.value.libraries || []).find(
    (entry) => entry.id === selected.value.library_id,
  )
  return Boolean(library && !library.read_only)
})

const currentItems = computed(() =>
  view.value === 'album' ? albumItems.value : items.value,
)

const writableLibraryIds = computed(
  () =>
    new Set(
      (info.value?.libraries || [])
        .filter((library) => !library.read_only)
        .map((library) => library.id),
    ),
)

const canDeleteSelection = computed(
  () =>
    Boolean(info.value?.write_enabled) &&
    Boolean(info.value?.trash_enabled) &&
    selectedItems.value.length > 0 &&
    selectedItems.value.every((item) =>
      writableLibraryIds.value.has(item.library_id),
    ),
)

const groups = computed(() => groupByDay(currentItems.value))

const {
  zoomStage,
  hiResLoaded,
  metaOpen,
  isZoomed,
  zoomPercent,
  zoomStyle,
  showHiRes,
  resetZoom,
  zoomBy,
  onWheel,
  onDoubleClick,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  toggleMeta,
  prev,
  next,
} = useLightbox({ selected, currentItems })

function openItem(item) {
  resetZoom()
  selected.value = item
  videoError.value = false
  transcodeStatus.value = ''
  addTagId.value = ''
  addAlbumId.value = ''
}

function close() {
  selected.value = null
}
const {
  selectionMode,
  selectedIds,
  contextMenu,
  showAlbumPicker,
  batchBusy,
  selectionActive,
  selectedItems,
  isSelected,
  clearSelection,
  toggleSelectionMode,
  handleTileClick,
  openContextMenu,
  closeContextMenu,
  onTilePointerDown,
  onTilePointerUp,
} = useSelection({ currentItems, openItem })

function routeQueryValue(value) {
  if (Array.isArray(value)) return value[0] || ''
  return value || ''
}

function timelineQuery() {
  const params = {}
  if (query.value.trim()) params.q = query.value.trim()
  if (tagFilter.value) params.tag = tagFilter.value
  if (favoritesOnly.value) params.favorite = 'true'
  return params
}

function navigate(location) {
  const resolved = router.resolve(location)
  if (resolved.fullPath === route.fullPath) return applyRoute()
  return router.push(location)
}

async function applyRoute() {
  clearSelection()
  selected.value = null
  uploadMessage.value = ''
  if (view.value === 'timeline') {
    query.value = routeQueryValue(route.query.q)
    tagFilter.value = routeQueryValue(route.query.tag)
    favoritesOnly.value = route.query.favorite === 'true'
    nextCursor.value = null
    await loadItems(true)
  } else if (view.value === 'albums') {
    await loadAlbums()
  } else if (view.value === 'album') {
    if (!albums.value.length) await loadAlbums()
    const id = String(route.params.id)
    const album = albums.value.find((entry) => String(entry.id) === id)
    if (!album) {
      await router.replace({ name: 'albums' })
      return
    }
    await loadAlbumItems()
  }
  // TrashView and LibraryManager fetch their own data when mounted.
}

function search() {
  navigate({ name: 'timeline', query: timelineQuery() })
}

function onTagChange() {
  navigate({ name: 'timeline', query: timelineQuery() })
}

function toggleFavorites() {
  favoritesOnly.value = !favoritesOnly.value
  navigate({ name: 'timeline', query: timelineQuery() })
}

function switchView(nextView) {
  if (nextView === 'timeline') {
    navigate({ name: 'timeline', query: timelineQuery() })
  } else if (nextView === 'albums') {
    navigate({ name: 'albums' })
  } else if (nextView === 'trash') {
    navigate({ name: 'trash' })
  } else if (nextView === 'manage') {
    navigate({ name: 'libraries' })
  }
}

function openAlbum(album) {
  clearSelection()
  navigate({ name: 'album', params: { id: album.id } })
}

function backToAlbums() {
  navigate({ name: 'albums' })
}

async function onAlbumCreated() {
  await loadAlbums()
}

async function onAlbumDeleted() {
  await loadAlbums()
}

async function onTrashRestored() {
  await Promise.all([loadInfo(), loadItems(true)])
}

function setError(message) {
  error.value = message
}

function reloadTimelineItems() {
  nextCursor.value = null
  loadItems(true)
}

function openAlbumPicker() {
  if (!albums.value.length) {
    showToast(t('batch.noAlbums'), true)
    return
  }
  showAlbumPicker.value = true
}

function closeAlbumPicker() {
  if (batchBusy.value) return
  showAlbumPicker.value = false
}

function downloadItem(item) {
  const link = document.createElement('a')
  link.href = item.media_url
  link.download = item.filename
  link.rel = 'noopener'
  document.body.appendChild(link)
  link.click()
  link.remove()
}

function downloadSelection() {
  const list = selectedItems.value
  if (!list.length) return
  if (list.length > 1 && !confirm(t('batch.downloadConfirm', { count: list.length }))) {
    return
  }
  list.forEach((item, index) => {
    setTimeout(() => downloadItem(item), index * 300)
  })
  showToast(t('batch.downloading', { count: list.length }))
}

function viewSelection() {
  const first = selectedItems.value[0]
  if (first) openItem(first)
}

function contextView() {
  const item = contextMenu.value?.item
  closeContextMenu()
  if (item) openItem(item)
}

function contextDownload() {
  closeContextMenu()
  downloadSelection()
}

function contextAlbum() {
  closeContextMenu()
  openAlbumPicker()
}

function contextDelete() {
  closeContextMenu()
  deleteSelection()
}

async function deleteSelection() {
  const list = selectedItems.value
  if (!list.length || batchBusy.value) return
  if (!canDeleteSelection.value) {
    showToast(t('batch.readOnly'), true)
    return
  }
  if (!confirm(t('batch.deleteConfirm', { count: list.length }))) return
  batchBusy.value = true
  let done = 0
  let failed = 0
  let lastError = ''
  for (const item of list) {
    try {
      await api(`${API}/items/${item.id}`, { method: 'DELETE' })
      done += 1
    } catch (err) {
      failed += 1
      lastError = err.message
    }
  }
  batchBusy.value = false
  clearSelection()
  await Promise.all([loadInfo(), loadItems(true)])
  if (view.value === 'album') await loadAlbumItems()
  if (failed) {
    showToast(t('batch.deleteFailed', { done, failed, message: lastError }), true)
  } else {
    showToast(t('batch.deleted', { done }))
  }
}

async function addSelectionToAlbum(album) {
  const list = selectedItems.value
  if (!list.length || batchBusy.value) return
  batchBusy.value = true
  let done = 0
  let failed = 0
  let lastError = ''
  for (const item of list) {
    try {
      await api(`${API}/albums/${album.id}/items`, {
        method: 'POST',
        body: JSON.stringify({ item_id: item.id }),
      })
      done += 1
    } catch (err) {
      failed += 1
      lastError = err.message
    }
  }
  batchBusy.value = false
  showAlbumPicker.value = false
  await loadAlbums()
  if (activeAlbum.value?.id === album.id) await loadAlbumItems()
  if (failed) {
    showToast(t('batch.addFailed', { done, failed, message: lastError }), true)
  } else {
    showToast(t('batch.addedToAlbum', { count: done, name: album.name }))
  }
}

async function toggleFavorite(item) {
  try {
    const updated = await api(`${API}/items/${item.id}`, {
      method: 'PATCH',
      body: JSON.stringify({ favorite: !item.favorite }),
    })
    replaceItem(updated)
  } catch (err) {
    error.value = err.message
  }
}

function replaceItem(updated) {
  for (const list of [items.value, albumItems.value]) {
    const index = list.findIndex((item) => item.id === updated.id)
    if (index >= 0) list.splice(index, 1, updated)
  }
  if (selected.value?.id === updated.id) selected.value = updated
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

function onLibraryScanJob(jobId) {
  scanning.value = true
  pollScan(jobId)
}

async function pollScan(jobId) {
  try {
    const job = await api(`${API}/jobs/${jobId}`)
    scanProgress.value = job
    if (job.state === 'completed' || job.state === 'failed') {
      scanning.value = false
      if (job.state === 'completed') {
        await Promise.all([loadInfo(), loadItems(true), loadTags()])
        if (view.value === 'manage') {
          await libraryManagerRef.value?.loadManagedLibraries()
        }
      } else {
        error.value = job.error || t('errors.scanFailed')
      }
      return
    }
    scanTimer = setTimeout(() => pollScan(jobId), 1000)
  } catch (err) {
    scanning.value = false
    error.value = err.message
  }
}

function onFiles(event) {
  const files = [...(event.target.files || [])]
  event.target.value = ''
  if (files.length) uploadFiles(files)
}

async function uploadFiles(files) {
  const library = uploadLibrary.value
  if (!library) {
    error.value = t('errors.noWritableLibrary')
    showToast(t('errors.noWritableLibrary'), true)
    return
  }
  uploading.value = true
  error.value = ''
  const targetAlbum = view.value === 'album' ? activeAlbum.value : null
  const uploadedIds = []
  let done = 0
  let failed = 0
  let lastError = ''
  for (const file of files) {
    const form = new FormData()
    form.append('library', library)
    form.append('file', file, file.name)
    try {
      const item = await api(`${API}/upload`, { method: 'POST', body: form })
      uploadedIds.push(item.id)
      done += 1
      uploadMessage.value = t('status.uploaded', {
        done,
        total: files.length,
        name: file.name,
      })
    } catch (err) {
      failed += 1
      lastError = t('errors.uploadFailed', {
        name: file.name,
        message: err.message,
      })
      error.value = lastError
    }
  }

  // Uploading while inside an album adds the new photos to that album.
  let albumAdded = 0
  let albumFailed = 0
  let albumError = ''
  if (targetAlbum && uploadedIds.length) {
    for (const id of uploadedIds) {
      try {
        await api(`${API}/albums/${targetAlbum.id}/items`, {
          method: 'POST',
          body: JSON.stringify({ item_id: id }),
        })
        albumAdded += 1
      } catch (err) {
        albumFailed += 1
        albumError = err.message
      }
    }
  }

  uploading.value = false
  await loadInfo()
  if (view.value === 'timeline') await loadItems(true)
  if (view.value === 'album') {
    await Promise.all([loadAlbums(), loadAlbumItems()])
  }
  if (failed) {
    showToast(t('upload.failed', { failed, message: lastError }), true)
  } else if (albumFailed) {
    showToast(
      t('upload.albumFailed', { failed: albumFailed, message: albumError }),
      true,
    )
  } else if (done && targetAlbum && albumAdded) {
    showToast(
      t('upload.doneToAlbum', { count: albumAdded, name: targetAlbum.name }),
    )
  } else if (done) {
    showToast(t('upload.done', { count: done }))
  }
  uploadMessage.value = ''
}

async function createTag() {
  if (!newTagName.value.trim()) return
  try {
    const tag = await api(`${API}/tags`, {
      method: 'POST',
      body: JSON.stringify({ name: newTagName.value.trim() }),
    })
    await loadTags()
    newTagName.value = ''
    addTagId.value = String(tag.id)
  } catch (err) {
    error.value = err.message
  }
}

async function attachTag(tagId) {
  if (!selected.value || !tagId) return
  try {
    const updated = await api(`${API}/items/${selected.value.id}/tags`, {
      method: 'POST',
      body: JSON.stringify({ tag_id: Number(tagId) }),
    })
    replaceItem(updated)
    await loadTags()
    addTagId.value = ''
  } catch (err) {
    error.value = err.message
  }
}

async function detachTag(tagId) {
  if (!selected.value) return
  try {
    const updated = await api(`${API}/items/${selected.value.id}/tags/${tagId}`, {
      method: 'DELETE',
    })
    replaceItem(updated)
    await loadTags()
  } catch (err) {
    error.value = err.message
  }
}

async function createAndAttachTag() {
  if (!newTagName.value.trim()) return
  try {
    const tag = await api(`${API}/tags`, {
      method: 'POST',
      body: JSON.stringify({ name: newTagName.value.trim() }),
    })
    newTagName.value = ''
    await loadTags()
    await attachTag(tag.id)
  } catch (err) {
    error.value = err.message
  }
}

async function addSelectedToAlbum() {
  if (!selected.value || !addAlbumId.value) return
  try {
    await api(`${API}/albums/${addAlbumId.value}/items`, {
      method: 'POST',
      body: JSON.stringify({ item_id: selected.value.id }),
    })
    const album = albums.value.find(
      (entry) => String(entry.id) === String(addAlbumId.value),
    )
    showToast(t('errors.addedToAlbumName', { name: album?.name || '' }))
    await loadAlbums()
    if (activeAlbum.value?.id === addAlbumId.value) await loadAlbumItems()
  } catch (err) {
    error.value = err.message
    showToast(err.message, true)
  }
}

async function removeSelectedFromAlbum() {
  if (!selected.value || !activeAlbum.value) return
  try {
    await api(`${API}/albums/${activeAlbum.value.id}/items/${selected.value.id}`, {
      method: 'DELETE',
    })
    await loadAlbumItems()
    await loadAlbums()
    close()
    showToast(t('lightbox.removedFromAlbum'))
  } catch (err) {
    error.value = err.message
    showToast(err.message, true)
  }
}

async function deleteSelected() {
  if (!selected.value) return
  if (!confirm(t('lightbox.deleteConfirm', { name: selected.value.filename }))) return
  const id = selected.value.id
  try {
    await api(`${API}/items/${id}`, { method: 'DELETE' })
    close()
    items.value = items.value.filter((item) => item.id !== id)
    albumItems.value = albumItems.value.filter((item) => item.id !== id)
    await loadInfo()
  } catch (err) {
    error.value = err.message
  }
}

async function transcodeSelected() {
  if (!selected.value) return
  transcodeStatus.value = t('lightbox.transcodeStarting')
  try {
    const data = await api(`${API}/items/${selected.value.id}/transcode`, {
      method: 'POST',
    })
    if (data.ready) {
      const updated = await api(`${API}/items/${selected.value.id}`)
      replaceItem(updated)
      transcodeStatus.value = t('lightbox.transcodeReady')
      return
    }
    pollTranscode(data.job_id, selected.value.id)
  } catch (err) {
    transcodeStatus.value = ''
    error.value = err.message
  }
}

async function pollTranscode(jobId, itemId) {
  try {
    const job = await api(`${API}/jobs/${jobId}`)
    transcodeStatus.value =
      job.state === 'running'
        ? t('lightbox.transcodeRunning')
        : job.message || job.state
    if (job.state === 'completed') {
      const updated = await api(`${API}/items/${itemId}`)
      replaceItem(updated)
      transcodeStatus.value = t('lightbox.transcodeReady')
      return
    }
    if (job.state === 'failed') {
      transcodeStatus.value = t('lightbox.transcodeFailed')
      return
    }
    transcodeTimer = setTimeout(() => pollTranscode(jobId, itemId), 1500)
  } catch (err) {
    transcodeStatus.value = ''
    error.value = err.message
  }
}

function changeLanguage(event) {
  const value = event.target.value
  locale.value = value
  document.documentElement.lang = value
  localStorage.setItem('photoself-lang', value)
}

watch(locale, (value) => {
  currentLocale.value = value
  document.documentElement.lang = value
})

function onKey(event) {
  if (event.key === 'Escape') {
    if (contextMenu.value) {
      closeContextMenu()
      return
    }
    if (selected.value) {
      close()
      return
    }
    if (selectionActive.value) {
      clearSelection()
      return
    }
  }
  if (!selected.value) return
  if (selected.value.kind !== 'video') {
    if (event.key === '+' || event.key === '=') {
      zoomBy(1.25)
      return
    }
    if (event.key === '-' || event.key === '_') {
      zoomBy(0.8)
      return
    }
    if (event.key === '0') {
      resetZoom()
      return
    }
  }
  if (event.key === 'ArrowLeft') prev()
  if (event.key === 'ArrowRight') next()
}

watch(selected, (value) => {
  document.body.classList.toggle('modal-open', Boolean(value))
  resetZoom()
  videoError.value = false
  transcodeStatus.value = ''
  addTagId.value = ''
  addAlbumId.value = ''
})

watch(
  info,
  (value) => {
    if (!uploadLibrary.value && value) {
      uploadLibrary.value =
        (value.libraries || []).find((library) => !library.read_only)?.id || ''
    }
  },
  { immediate: true },
)

watch(
  () => route.fullPath,
  () => {
    applyRoute()
  },
)

onMounted(async () => {
  document.documentElement.lang = locale.value
  applyThemeMeta()
  window.addEventListener('click', closeContextMenu)
  window.addEventListener('scroll', closeContextMenu, true)
  await Promise.all([loadInfo(), loadTags(), loadAlbums()])
  await applyRoute()

  window.addEventListener('scroll', onWindowScroll, { passive: true })
  window.addEventListener('resize', onWindowScroll)
  window.addEventListener('keydown', onKey)
  nextTick(checkLoadMore)
})

onUnmounted(() => {
  if (scanTimer) clearTimeout(scanTimer)
  if (transcodeTimer) clearTimeout(transcodeTimer)
  clearToast()
  window.removeEventListener('click', closeContextMenu)
  window.removeEventListener('scroll', closeContextMenu, true)
  window.removeEventListener('scroll', onWindowScroll)
  window.removeEventListener('resize', onWindowScroll)
  window.removeEventListener('keydown', onKey)
  document.body.classList.remove('modal-open')
})
</script>

<template>
  <AppHeader
    v-model:query="query"
    v-model:tag-filter="tagFilter"
    v-model:upload-library="uploadLibrary"
    v-model:current-locale="currentLocale"
    :view="view"
    :info="info"
    :tags="tags"
    :writable-libraries="writableLibraries"
    :favorites-only="favoritesOnly"
    :selection-mode="selectionMode"
    :uploading="uploading"
    :scanning="scanning"
    :theme="theme"
    @navigate="switchView"
    @search="search"
    @tag-change="onTagChange"
    @toggle-favorites="toggleFavorites"
    @toggle-selection="toggleSelectionMode"
    @upload="fileInput?.click()"
    @scan="startScan"
    @toggle-theme="toggleTheme"
    @language-change="changeLanguage"
  />

  <input
    ref="fileInput"
    class="hidden-input"
    type="file"
    multiple
    accept="image/*,video/*,.heic,.heif,.mov"
    @change="onFiles"
  />

  <div class="statusbar">
    <span v-if="view === 'timeline'">
      {{ t('status.items', info ? info.item_count : currentItems.length) }}
    </span>
    <span v-if="scanning && scanProgress">
      {{ t('status.scanProgress', { done: scanProgress.progress, total: scanProgress.total }) }}
    </span>
    <span v-if="uploadMessage">{{ uploadMessage }}</span>
    <span v-if="error" class="error">{{ error }}</span>
  </div>

  <main>
    <!-- Albums -->
    <AlbumsView
      v-if="view === 'albums'"
      :albums="albums"
      @open="openAlbum"
      @created="onAlbumCreated"
      @deleted="onAlbumDeleted"
      @error="setError"
    />

    <!-- Library management -->
    <LibraryManager
      v-else-if="view === 'manage'"
      ref="libraryManagerRef"
      :scanning="scanning"
      :error="error"
      @scan-job="onLibraryScanJob"
      @reload-info="loadInfo"
      @reload-items="reloadTimelineItems"
      @error="setError" @clear-error="setError('')"
    />

    <!-- Trash -->
    <TrashView
      v-else-if="view === 'trash'"
      @restored="onTrashRestored"
      @error="setError"
    />

    <!-- Timeline / album -->
    <template v-else>
      <div v-if="view === 'album'" class="album-header">
        <button @click="backToAlbums">{{ t('album.back') }}</button>
        <h2>{{ activeAlbum?.name }}</h2>
        <small>{{ t('album.items', albumItems.length) }}</small>
        <button v-if="selected && activeAlbum" @click="removeSelectedFromAlbum">
          {{ t('album.removeSelected') }}
        </button>
      </div>

      <div v-if="!loading && currentItems.length === 0" class="empty">
        <h2>{{ view === 'album' ? t('empty.noAlbum') : t('empty.noMediaTitle') }}</h2>
        <p v-if="view !== 'album'">{{ t('empty.noMediaHint', { config: 'config.toml', scan: t('actions.scan') }) }}</p>
      </div>

      <section v-for="group in groups" :key="group.date" class="day">
        <h2>{{ dateLabel(group.date, locale, t) }} <small>{{ group.items.length }}</small></h2>
        <div class="grid">
          <button
            v-for="item in group.items"
            :key="item.id"
            class="tile"
            :class="{ selected: isSelected(item.id) }"
            type="button"
            @click="handleTileClick(item, $event)"
            @contextmenu.prevent="openContextMenu($event, item)"
            @pointerdown="onTilePointerDown($event, item)"
            @pointerup="onTilePointerUp"
            @pointercancel="onTilePointerUp"
          >
            <img :src="item.thumb_url" :alt="item.filename" loading="lazy" @error="onImgError" />
            <span v-if="item.kind === 'video'" class="play">▶</span>
            <span v-if="item.favorite" class="fav">★</span>
            <span v-if="item.tags && item.tags.length" class="tag-dot">{{ item.tags.length }}</span>
            <span
              v-if="selectionMode || selectedIds.length"
              class="tile-check"
              :class="{ on: isSelected(item.id) }"
              aria-hidden="true"
            >
              {{ isSelected(item.id) ? '✓' : '' }}
            </span>
          </button>
        </div>
      </section>

      <div ref="sentinel" class="sentinel"></div>
      <button
        v-if="view === 'timeline' && nextCursor"
        class="load-more"
        type="button"
        :disabled="loading"
        @click="loadItems(false)"
      >
        {{ loading ? t('actions.loading') : t('actions.loadMore') }}
      </button>
    </template>
  </main>

  <footer class="app-footer">
    <span>photoSelf{{ info ? ` ${info.version}` : '' }}</span>
  </footer>

  <div v-if="selected" class="lightbox" @click.self="close">
    <button class="close" type="button" :aria-label="t('actions.close')" @click="close">×</button>
    <button
      class="info-toggle"
      type="button"
      :class="{ active: metaOpen }"
      :aria-label="metaOpen ? t('lightbox.hideInfo') : t('lightbox.showInfo')"
      :title="metaOpen ? t('lightbox.hideInfo') : t('lightbox.showInfo')"
      @click="toggleMeta"
    >
      ⓘ
    </button>
    <button class="nav prev" type="button" :aria-label="t('actions.previous')" @click.stop="prev">‹</button>

    <div class="viewer">
      <div
        v-if="selected.kind !== 'video'"
        ref="zoomStage"
        class="zoom-stage"
        :class="{ 'is-zoomed': isZoomed }"
        :style="zoomStyle"
        @wheel.prevent="onWheel"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="onPointerUp"
        @dblclick="onDoubleClick"
      >
        <img
          :src="selected.preview_url || selected.media_url"
          :alt="selected.filename"
          draggable="false"
          @error="onImgError"
        />
        <img
          v-if="showHiRes"
          class="hi-res"
          :class="{ loaded: hiResLoaded }"
          :src="selected.media_url"
          alt=""
          draggable="false"
          @load="hiResLoaded = true"
          @error="hiResLoaded = false"
        />
      </div>
      <template v-else>
        <video
          :src="selected.play_url || selected.media_url"
          controls
          autoplay
          playsinline
          @error="videoError = true"
        ></video>
        <div v-if="videoError" class="video-fallback">
          {{ t('lightbox.videoFallback') }}
          <a :href="selected.media_url" :download="selected.filename">{{ t('lightbox.downloadOriginal') }}</a>
          <span v-if="info && info.transcode_enabled">{{ t('lightbox.orGenerate') }}</span>
        </div>
      </template>

      <div v-if="selected.kind !== 'video'" class="zoom-controls">
        <button type="button" :aria-label="t('lightbox.zoomOut')" @click="zoomBy(0.8)">−</button>
        <button
          type="button"
          class="zoom-value"
          :title="t('lightbox.resetZoom')"
          @click="resetZoom"
        >
          {{ zoomPercent }}
        </button>
        <button type="button" :aria-label="t('lightbox.zoomIn')" @click="zoomBy(1.25)">+</button>
      </div>
    </div>

    <button class="nav next" type="button" :aria-label="t('actions.next')" @click.stop="next">›</button>

    <aside class="meta" :class="{ open: metaOpen }">
      <button
        type="button"
        class="meta-handle"
        :aria-expanded="metaOpen"
        :title="metaOpen ? t('lightbox.hideInfo') : t('lightbox.showInfo')"
        @click="toggleMeta"
      >
        <span class="meta-title">{{ selected.filename }}</span>
        <span class="meta-chevron" aria-hidden="true">{{ metaOpen ? '▾' : '▴' }}</span>
      </button>
      <div class="meta-body">
        <h3>{{ selected.filename }}</h3>
        <dl>
          <div><dt>{{ t('lightbox.date') }}</dt><dd>{{ formatDate(selected.taken_at, t) }}</dd></div>
          <div v-if="selected.width">
            <dt>{{ t('lightbox.dimensions') }}</dt><dd>{{ selected.width }} × {{ selected.height }}</dd>
          </div>
          <div v-if="selected.duration_ms">
            <dt>{{ t('lightbox.duration') }}</dt><dd>{{ formatDuration(selected.duration_ms) }}</dd>
          </div>
          <div v-if="selected.camera"><dt>{{ t('lightbox.camera') }}</dt><dd>{{ selected.camera }}</dd></div>
          <div><dt>{{ t('lightbox.size') }}</dt><dd>{{ formatBytes(selected.size) }}</dd></div>
        </dl>

        <div class="tags">
          <span v-for="tag in selected.tags || []" :key="tag.id" class="tag-chip">
            {{ tag.name }}
            <button type="button" @click="detachTag(tag.id)">×</button>
          </span>
        </div>
        <div class="tag-add">
          <select v-model="addTagId">
            <option value="">{{ t('lightbox.addTag') }}</option>
            <option v-for="tag in tags" :key="tag.id" :value="tag.id">{{ tag.name }}</option>
          </select>
          <button type="button" :disabled="!addTagId" @click="attachTag(addTagId)">+</button>
          <input v-model="newTagName" :placeholder="t('lightbox.newTag')" @keyup.enter="createAndAttachTag" />
          <button type="button" @click="createAndAttachTag">{{ t('lightbox.create') }}</button>
        </div>

        <div v-if="albums.length" class="tag-add">
          <select v-model="addAlbumId">
            <option value="">{{ t('lightbox.addToAlbum') }}</option>
            <option
              v-for="album in albums"
              :key="album.id"
              :value="album.id"
              :disabled="activeAlbum && activeAlbum.id === album.id"
            >
              {{ album.name }}
            </option>
          </select>
          <button type="button" :disabled="!addAlbumId" @click="addSelectedToAlbum">{{ t('lightbox.add') }}</button>
          <button v-if="activeAlbum" type="button" @click="removeSelectedFromAlbum">{{ t('lightbox.remove') }}</button>
        </div>

        <div class="row">
          <button type="button" @click="toggleFavorite(selected)">
            {{ selected.favorite ? t('lightbox.unfavorite') : t('lightbox.favorite') }}
          </button>
          <a :href="selected.media_url" :download="selected.filename">{{ t('lightbox.download') }}</a>
          <button
            v-if="selected.kind === 'video' && info && info.transcode_enabled && !selected.play_url"
            type="button"
            @click="transcodeSelected"
          >
            {{ t('lightbox.generateWeb') }}
          </button>
          <button
            v-if="canDeleteSelected"
            type="button"
            class="danger"
            @click="deleteSelected"
          >
            {{ t('lightbox.delete') }}
          </button>
        </div>
        <p v-if="transcodeStatus" class="transcode-status">{{ transcodeStatus }}</p>
      </div>
    </aside>
  </div>

  <SelectionBar
    v-if="selectionActive && !selected"
    :count="selectedItems.length"
    :can-delete="canDeleteSelection"
    :show-delete="Boolean(info && info.write_enabled && info.trash_enabled)"
    :busy="batchBusy"
    @view="viewSelection"
    @download="downloadSelection"
    @add-album="openAlbumPicker"
    @delete="deleteSelection"
    @clear="clearSelection"
  />

  <ContextMenu
    v-if="contextMenu"
    :menu="contextMenu"
    :can-delete="canDeleteSelection"
    :count="selectedItems.length"
    @view="contextView"
    @download="contextDownload"
    @album="contextAlbum"
    @delete="contextDelete"
  />

  <AlbumPicker
    v-if="showAlbumPicker"
    :albums="albums"
    :count="selectedItems.length"
    :busy="batchBusy"
    @pick="addSelectionToAlbum"
    @close="closeAlbumPicker"
  />

  <div v-if="toast" class="toast" :class="{ error: toast.error }" role="status">
    {{ toast.text }}
  </div>
</template>