<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

const API = '/api/v1'
const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = ref(locale.value)
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
const view = ref('timeline')
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
const activeAlbum = ref(null)
const albumItems = ref([])
const trashEntries = ref([])
const uploadLibrary = ref('')
const uploading = ref(false)
const uploadMessage = ref('')
const addTagId = ref('')
const newTagName = ref('')
const addAlbumId = ref('')
const transcodeStatus = ref('')
const videoError = ref(false)
const fileInput = ref(null)
const newAlbumName = ref('')

const libraryList = ref([])
const libraryLoading = ref(false)
const libraryMessage = ref('')
const showAddLibrary = ref(false)
const addingLibrary = ref(false)
const libraryError = ref('')
const newLibrary = ref({ path: '', name: '', read_only: true })
const browser = ref(null)
const browserLoading = ref(false)

const zoomStage = ref(null)
const zoom = ref(1)
const panX = ref(0)
const panY = ref(0)
const dragging = ref(false)
const hiResLoaded = ref(false)
const metaOpen = ref(
  typeof window === 'undefined' || !window.matchMedia
    ? true
    : !window.matchMedia('(max-width: 860px)').matches,
)
const pointers = new Map()
let dragState = null
let pinchState = null
let lastTap = { time: 0, x: 0, y: 0 }
let suppressDblClickUntil = 0

let observer = null
let scanTimer = null
let transcodeTimer = null

async function api(path, options = {}) {
  const isForm = options.body instanceof FormData
  const headers = {
    Accept: 'application/json',
    'X-Requested-With': 'photoSelf',
    ...(options.headers || {}),
  }
  if (options.body && !isForm) headers['Content-Type'] = 'application/json'
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
  try {
    const params = new URLSearchParams({ limit: '100' })
    if (query.value.trim()) params.set('q', query.value.trim())
    if (favoritesOnly.value) params.set('favorite', 'true')
    if (tagFilter.value) params.set('tag', tagFilter.value)
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

async function loadTrash() {
  try {
    const data = await api(`${API}/trash`)
    trashEntries.value = data.entries || []
  } catch (err) {
    error.value = err.message
  }
}

async function loadManagedLibraries() {
  libraryLoading.value = true
  try {
    const data = await api(`${API}/libraries`)
    libraryList.value = data.libraries || []
  } catch (err) {
    error.value = err.message
  } finally {
    libraryLoading.value = false
  }
}

function openAddLibrary() {
  libraryError.value = ''
  newLibrary.value = { path: '', name: '', read_only: true }
  browser.value = null
  showAddLibrary.value = true
}

function closeAddLibrary() {
  if (addingLibrary.value) return
  showAddLibrary.value = false
  browser.value = null
}

async function browseTo(path = '') {
  browserLoading.value = true
  libraryError.value = ''
  try {
    const params = path ? `?path=${encodeURIComponent(path)}` : ''
    browser.value = await api(`${API}/fs/dirs${params}`)
  } catch (err) {
    libraryError.value = err.message
  } finally {
    browserLoading.value = false
  }
}

function chooseBrowserDir() {
  if (!browser.value?.path) return
  newLibrary.value.path = browser.value.path
  if (!newLibrary.value.name.trim()) {
    const parts = browser.value.path.split('/').filter(Boolean)
    newLibrary.value.name = parts[parts.length - 1] || ''
  }
  browser.value = null
}

async function addLibrary() {
  if (!newLibrary.value.path.trim() || addingLibrary.value) return
  addingLibrary.value = true
  libraryError.value = ''
  try {
    const result = await api(`${API}/libraries`, {
      method: 'POST',
      body: JSON.stringify({
        path: newLibrary.value.path.trim(),
        name: newLibrary.value.name.trim() || null,
        read_only: newLibrary.value.read_only,
      }),
    })
    libraryMessage.value = t('manage.added', { name: result.library?.name || '' })
    showAddLibrary.value = false
    browser.value = null
    await loadManagedLibraries()
    if (result.scan_job) {
      scanning.value = true
      pollScan(result.scan_job)
    }
  } catch (err) {
    libraryError.value = err.message
  } finally {
    addingLibrary.value = false
  }
}

async function scanLibraryEntry(library) {
  if (scanning.value) return
  libraryMessage.value = ''
  error.value = ''
  try {
    const data = await api(`${API}/libraries/${library.id}/scan`, { method: 'POST' })
    scanning.value = true
    pollScan(data.job_id)
  } catch (err) {
    error.value = err.message
  }
}

async function toggleLibrary(library) {
  libraryMessage.value = ''
  try {
    await api(`${API}/libraries/${library.id}`, {
      method: 'PATCH',
      body: JSON.stringify({ enabled: !library.enabled }),
    })
    await loadManagedLibraries()
  } catch (err) {
    error.value = err.message
  }
}

async function removeLibrary(library) {
  if (
    !confirm(
      t('manage.removeConfirm', { name: library.name, count: library.item_count }),
    )
  ) {
    return
  }
  libraryMessage.value = ''
  try {
    await api(`${API}/libraries/${library.id}`, { method: 'DELETE' })
    await loadManagedLibraries()
    await Promise.all([loadInfo(), loadItems(true)])
  } catch (err) {
    error.value = err.message
  }
}

async function importLibraries() {
  if (!confirm(t('manage.importConfirm'))) return
  libraryMessage.value = ''
  try {
    const data = await api(`${API}/libraries/import-config`, { method: 'POST' })
    libraryMessage.value = t('manage.importDone', { count: data.imported })
    await Promise.all([loadManagedLibraries(), loadInfo()])
  } catch (err) {
    error.value = err.message
  }
}

function reloadCurrent() {
  if (view.value === 'timeline') {
    nextCursor.value = null
    loadItems(true)
  } else if (view.value === 'album') {
    loadAlbumItems()
  } else if (view.value === 'trash') {
    loadTrash()
  } else if (view.value === 'manage') {
    loadManagedLibraries()
  }
}

function search() {
  view.value = 'timeline'
  activeAlbum.value = null
  reloadCurrent()
}

function switchView(next) {
  view.value = next
  selected.value = null
  activeAlbum.value = null
  if (next === 'timeline') loadItems(true)
  if (next === 'albums') loadAlbums()
  if (next === 'trash') loadTrash()
  if (next === 'manage') loadManagedLibraries()
}

function openAlbum(album) {
  activeAlbum.value = album
  view.value = 'album'
  selected.value = null
  loadAlbumItems()
}

function backToAlbums() {
  view.value = 'albums'
  activeAlbum.value = null
  selected.value = null
  loadAlbums()
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

const groups = computed(() => groupsOf(currentItems.value))

const selectedIndex = computed(() =>
  selected.value
    ? currentItems.value.findIndex((item) => item.id === selected.value.id)
    : -1,
)

function groupsOf(list) {
  const map = new Map()
  for (const item of list) {
    const day = (item.taken_at || '1970-01-01').slice(0, 10)
    if (!map.has(day)) map.set(day, [])
    map.get(day).push(item)
  }
  return [...map.entries()].map(([date, groupItems]) => ({ date, items: groupItems }))
}

function dateLabel(day) {
  if (!day || day.startsWith('1970-')) return t('date.undated')
  const date = new Date(`${day}T00:00:00`)
  return date.toLocaleDateString(locale.value, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    weekday: 'short',
  })
}

function formatDate(value) {
  return value ? value.replace('T', ' ') : t('date.undated')
}

function formatDuration(ms) {
  if (!ms) return ''
  const total = Math.round(ms / 1000)
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`
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

const isZoomed = computed(() => zoom.value > 1.001)
const zoomPercent = computed(() => `${Math.round(zoom.value * 100)}%`)
const zoomStyle = computed(() => ({
  transform: `translate(${panX.value}px, ${panY.value}px) scale(${zoom.value})`,
  transition: dragging.value ? 'none' : 'transform 150ms ease',
}))
const showHiRes = computed(() => isZoomed.value && Boolean(selected.value?.media_url))

function clampPan(x, y, scale = zoom.value) {
  const el = zoomStage.value
  if (!el) return { x: 0, y: 0 }
  const maxX = Math.max(0, (el.offsetWidth * (scale - 1)) / 2)
  const maxY = Math.max(0, (el.offsetHeight * (scale - 1)) / 2)
  return {
    x: Math.min(maxX, Math.max(-maxX, x)),
    y: Math.min(maxY, Math.max(-maxY, y)),
  }
}

function applyZoom(nextScale, clientX, clientY) {
  const scale = Math.min(5, Math.max(1, nextScale))
  if (scale === 1) {
    resetZoom()
    return
  }
  const el = zoomStage.value
  const rect = el?.getBoundingClientRect()
  const originX = clientX != null && rect ? clientX - (rect.left + rect.width / 2) : 0
  const originY = clientY != null && rect ? clientY - (rect.top + rect.height / 2) : 0
  const ratio = scale / zoom.value
  const next = clampPan(
    panX.value * ratio + originX * (1 - ratio),
    panY.value * ratio + originY * (1 - ratio),
    scale,
  )
  zoom.value = scale
  panX.value = next.x
  panY.value = next.y
}

function resetZoom() {
  zoom.value = 1
  panX.value = 0
  panY.value = 0
  hiResLoaded.value = false
}

function zoomBy(factor) {
  const rect = zoomStage.value?.getBoundingClientRect()
  applyZoom(
    zoom.value * factor,
    rect ? rect.left + rect.width / 2 : null,
    rect ? rect.top + rect.height / 2 : null,
  )
}

function onWheel(event) {
  if (selected.value?.kind === 'video') return
  applyZoom(zoom.value * Math.exp(-event.deltaY * 0.0015), event.clientX, event.clientY)
}

function onDoubleClick(event) {
  if (performance.now() < suppressDblClickUntil) return
  if (isZoomed.value) resetZoom()
  else applyZoom(2.5, event.clientX, event.clientY)
}

function onPointerDown(event) {
  if (event.pointerType === 'mouse' && event.button !== 0) return
  try {
    event.currentTarget.setPointerCapture?.(event.pointerId)
  } catch {
    // synthetic events may reference an unknown pointer id
  }
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY })
  if (pointers.size === 2) {
    const [a, b] = [...pointers.values()]
    pinchState = { distance: Math.hypot(a.x - b.x, a.y - b.y), zoom: zoom.value }
    dragState = null
    dragging.value = true
    return
  }
  if (pointers.size === 1) {
    dragState = {
      startX: event.clientX,
      startY: event.clientY,
      originX: panX.value,
      originY: panY.value,
      pointerType: event.pointerType,
      moved: false,
    }
    dragging.value = true
  }
}

function onPointerMove(event) {
  if (!pointers.has(event.pointerId)) return
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY })
  if (pointers.size >= 2 && pinchState) {
    const [a, b] = [...pointers.values()]
    const distance = Math.hypot(a.x - b.x, a.y - b.y)
    if (pinchState.distance > 0) {
      applyZoom(
        pinchState.zoom * (distance / pinchState.distance),
        (a.x + b.x) / 2,
        (a.y + b.y) / 2,
      )
    }
    return
  }
  if (!dragState) return
  const dx = event.clientX - dragState.startX
  const dy = event.clientY - dragState.startY
  if (Math.abs(dx) > 4 || Math.abs(dy) > 4) dragState.moved = true
  if (isZoomed.value) {
    const next = clampPan(dragState.originX + dx, dragState.originY + dy)
    panX.value = next.x
    panY.value = next.y
  }
}

function onPointerUp(event) {
  pointers.delete(event.pointerId)
  if (pointers.size < 2) pinchState = null
  if (pointers.size > 0) return
  if (dragState?.pointerType === 'touch' && !dragState.moved) {
    const now = performance.now()
    const dx = event.clientX - lastTap.x
    const dy = event.clientY - lastTap.y
    if (now - lastTap.time < 320 && Math.hypot(dx, dy) < 30) {
      lastTap = { time: 0, x: 0, y: 0 }
      suppressDblClickUntil = now + 500
      if (isZoomed.value) resetZoom()
      else applyZoom(2.5, event.clientX, event.clientY)
    } else {
      lastTap = { time: now, x: event.clientX, y: event.clientY }
    }
  } else if (dragState?.pointerType === 'touch') {
    lastTap = { time: 0, x: 0, y: 0 }
  }
  if (!isZoomed.value && dragState?.moved && dragState.pointerType === 'touch') {
    const dx = event.clientX - dragState.startX
    const dy = event.clientY - dragState.startY
    if (Math.abs(dx) > 60 && Math.abs(dx) > Math.abs(dy) * 1.4) {
      if (dx < 0) next()
      else prev()
    }
  }
  dragState = null
  dragging.value = false
}

function toggleMeta() {
  metaOpen.value = !metaOpen.value
}

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

function prev() {
  if (selectedIndex.value > 0) selected.value = currentItems.value[selectedIndex.value - 1]
}

function next() {
  if (selectedIndex.value >= 0 && selectedIndex.value < currentItems.value.length - 1) {
    selected.value = currentItems.value[selectedIndex.value + 1]
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
        await Promise.all([loadInfo(), loadItems(true), loadTags()])
        if (view.value === 'manage') await loadManagedLibraries()
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
    return
  }
  uploading.value = true
  error.value = ''
  let done = 0
  for (const file of files) {
    const form = new FormData()
    form.append('library', library)
    form.append('file', file, file.name)
    try {
      await api(`${API}/upload`, { method: 'POST', body: form })
      done += 1
      uploadMessage.value = t('status.uploaded', {
        done,
        total: files.length,
        name: file.name,
      })
    } catch (err) {
      error.value = t('errors.uploadFailed', { name: file.name, message: err.message })
      break
    }
  }
  uploading.value = false
  await loadInfo()
  if (view.value === 'timeline') await loadItems(true)
  if (view.value === 'album') await loadAlbumItems()
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

async function createAlbum() {
  if (!newAlbumName.value.trim()) return
  try {
    await api(`${API}/albums`, {
      method: 'POST',
      body: JSON.stringify({ name: newAlbumName.value.trim() }),
    })
    newAlbumName.value = ''
    await loadAlbums()
  } catch (err) {
    error.value = err.message
  }
}

async function deleteAlbum(album) {
  if (!confirm(t('album.deleteConfirm', { name: album.name }))) return
  try {
    await api(`${API}/albums/${album.id}`, { method: 'DELETE' })
    if (activeAlbum.value?.id === album.id) backToAlbums()
    else await loadAlbums()
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
    uploadMessage.value = t('errors.addedToAlbum')
    await loadAlbums()
    if (activeAlbum.value?.id === addAlbumId.value) await loadAlbumItems()
  } catch (err) {
    error.value = err.message
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
  } catch (err) {
    error.value = err.message
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
    await loadTrash()
  } catch (err) {
    error.value = err.message
  }
}

async function restoreEntry(entry) {
  try {
    await api(`${API}/trash/${entry.id}/restore`, { method: 'POST' })
    await Promise.all([loadTrash(), loadInfo(), loadItems(true)])
  } catch (err) {
    error.value = err.message
  }
}

async function purgeEntry(entry) {
  if (!confirm(t('trash.purgeConfirm', { name: entry.filename }))) return
  try {
    await api(`${API}/trash/${entry.id}`, { method: 'DELETE' })
    await loadTrash()
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
  if (event.key === 'Escape') close()
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

onMounted(async () => {
  document.documentElement.lang = locale.value
  await Promise.all([loadInfo(), loadTags(), loadAlbums(), loadItems(true)])

  observer = new IntersectionObserver(
    (entries) => {
      if (
        view.value === 'timeline' &&
        entries[0]?.isIntersecting &&
        nextCursor.value &&
        !loading.value
      ) {
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
  if (transcodeTimer) clearTimeout(transcodeTimer)
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

    <nav class="tabs">
      <button :class="{ active: view === 'timeline' }" @click="switchView('timeline')">
        {{ t('nav.timeline') }}
      </button>
      <button :class="{ active: view === 'albums' || view === 'album' }" @click="switchView('albums')">
        {{ t('nav.albums') }}
      </button>
      <button
        v-if="info && info.write_enabled && info.trash_enabled"
        :class="{ active: view === 'trash' }"
        @click="switchView('trash')"
      >
        {{ t('nav.trash') }}
      </button>
      <button
        v-if="info && info.manage_enabled"
        :class="{ active: view === 'manage' }"
        @click="switchView('manage')"
      >
        {{ t('manage.title') }}
      </button>
    </nav>

    <form class="search" @submit.prevent="search">
      <input v-model="query" type="search" :placeholder="t('search.placeholder')" />
      <button type="submit">{{ t('search.submit') }}</button>
    </form>

    <div class="actions">
      <select v-model="currentLocale" :title="t('language.label')" @change="changeLanguage">
        <option value="zh-CN">中文</option>
        <option value="en-US">English</option>
      </select>
      <select v-if="view === 'timeline'" v-model="tagFilter" @change="reloadCurrent">
        <option value="">{{ t('filters.allTags') }}</option>
        <option v-for="tag in tags" :key="tag.id" :value="tag.name">
          {{ tag.name }}
        </option>
      </select>
      <select
        v-if="writableLibraries.length > 1"
        v-model="uploadLibrary"
        :title="t('actions.upload')"
      >
        <option v-for="library in writableLibraries" :key="library.id" :value="library.id">
          {{ library.name || library.id }}
        </option>
      </select>
      <button :class="{ active: favoritesOnly }" @click="favoritesOnly = !favoritesOnly; reloadCurrent()">
        {{ t('filters.favorites') }}
      </button>
      <button
        v-if="info && info.write_enabled && writableLibraries.length"
        :disabled="uploading"
        @click="fileInput?.click()"
      >
        {{ uploading ? t('actions.uploading') : t('actions.upload') }}
      </button>
      <button :disabled="scanning" @click="startScan">
        {{ scanning ? t('actions.scanning') : t('actions.scan') }}
      </button>
    </div>
  </header>

  <input
    ref="fileInput"
    class="hidden-input"
    type="file"
    multiple
    accept="image/*,video/*,.heic,.heif,.mov"
    @change="onFiles"
  />

  <div class="statusbar">
    <span>{{ t('status.items', info ? info.item_count : currentItems.length) }}</span>
    <span v-if="scanning && scanProgress">
      {{ t('status.scanProgress', { done: scanProgress.progress, total: scanProgress.total }) }}
    </span>
    <span v-if="uploadMessage">{{ uploadMessage }}</span>
    <span v-if="error" class="error">{{ error }}</span>
  </div>

  <main>
    <!-- Albums -->
    <section v-if="view === 'albums'" class="albums-view">
      <div class="create-row">
        <input v-model="newAlbumName" :placeholder="t('album.createPlaceholder')" @keyup.enter="createAlbum" />
        <button @click="createAlbum">{{ t('album.create') }}</button>
      </div>
      <div v-if="!albums.length" class="empty">
        <h2>{{ t('empty.noAlbumsTitle') }}</h2>
        <p>{{ t('empty.noAlbumsHint') }}</p>
      </div>
      <div class="album-grid">
        <article v-for="album in albums" :key="album.id" class="album-card" @click="openAlbum(album)">
          <img v-if="album.cover_url" :src="album.cover_url" :alt="album.name" @error="onImgError" />
          <div v-else class="album-cover-empty">▣</div>
          <div class="album-info">
            <strong>{{ album.name }}</strong>
            <small>{{ t('album.items', album.item_count) }}</small>
          </div>
          <button class="album-delete" :title="t('album.deleteTitle')" @click.stop="deleteAlbum(album)">×</button>
        </article>
      </div>
    </section>

    <!-- Library management -->
    <section v-else-if="view === 'manage'" class="manage-view">
      <div class="manage-head">
        <div>
          <h2>{{ t('manage.title') }}</h2>
          <p>{{ t('manage.hint') }}</p>
        </div>
        <div class="row manage-actions">
          <button type="button" @click="importLibraries">{{ t('manage.importConfig') }}</button>
          <button type="button" class="primary" @click="openAddLibrary">
            {{ t('manage.add') }}
          </button>
        </div>
      </div>

      <p v-if="libraryMessage" class="muted">{{ libraryMessage }}</p>
      <p v-if="error" class="error">{{ error }}</p>
      <p v-if="libraryLoading && !libraryList.length" class="muted">
        {{ t('actions.loading') }}
      </p>

      <div v-if="!libraryLoading && !libraryList.length" class="empty">
        <h2>{{ t('manage.empty') }}</h2>
        <p>{{ t('manage.emptyHint') }}</p>
      </div>

      <div class="library-list">
        <article
          v-for="library in libraryList"
          :key="library.id"
          class="library-card"
          :class="{ off: !library.enabled }"
        >
          <div class="library-main">
            <div class="library-title">
              <strong>{{ library.name }}</strong>
              <span v-if="library.read_only" class="badge">{{ t('manage.readOnly') }}</span>
              <span v-if="!library.enabled" class="badge warn">{{ t('manage.disabled') }}</span>
              <span class="badge source">{{ t('manage.source.' + library.source) }}</span>
            </div>
            <code class="library-path">{{ library.path }}</code>
            <small>{{ t('manage.items', library.item_count) }}</small>
          </div>
          <div class="row library-actions">
            <button
              type="button"
              :disabled="scanning || !library.enabled"
              @click="scanLibraryEntry(library)"
            >
              {{ t('manage.scan') }}
            </button>
            <button type="button" @click="toggleLibrary(library)">
              {{ library.enabled ? t('manage.disable') : t('manage.enable') }}
            </button>
            <button type="button" class="danger" @click="removeLibrary(library)">
              {{ t('manage.remove') }}
            </button>
          </div>
        </article>
      </div>

      <div v-if="showAddLibrary" class="manager-modal" @click.self="closeAddLibrary">
        <div class="manager-dialog">
          <h3>{{ t('manage.add') }}</h3>

          <label class="field">
            <span>{{ t('manage.path') }}</span>
            <div class="row">
              <input
                v-model="newLibrary.path"
                :placeholder="t('manage.pathPlaceholder')"
                spellcheck="false"
              />
              <button type="button" :disabled="browserLoading" @click="browseTo()">
                {{ t('manage.browse') }}
              </button>
            </div>
            <small>{{ t('manage.pathHint') }}</small>
          </label>

          <div v-if="browser" class="dir-browser">
            <div class="dir-head">
              <button
                v-if="browser.parent"
                type="button"
                @click="browseTo(browser.parent)"
              >
                ← {{ t('manage.up') }}
              </button>
              <code>{{ browser.path || t('manage.roots') }}</code>
              <button
                v-if="browser.path"
                type="button"
                class="primary"
                @click="chooseBrowserDir"
              >
                {{ t('manage.choose') }}
              </button>
            </div>
            <p v-if="browserLoading" class="muted">{{ t('actions.loading') }}</p>
            <ul v-else>
              <li v-for="entry in browser.entries" :key="entry.path">
                <button type="button" @click="browseTo(entry.path)">
                  📁 {{ entry.name }}
                </button>
              </li>
            </ul>
            <p v-if="!browserLoading && !browser.entries?.length" class="muted">
              {{ t('manage.noSubdirs') }}
            </p>
          </div>

          <label class="field">
            <span>{{ t('manage.name') }}</span>
            <input
              v-model="newLibrary.name"
              :placeholder="t('manage.namePlaceholder')"
            />
          </label>

          <label class="check">
            <input v-model="newLibrary.read_only" type="checkbox" />
            {{ t('manage.readOnlyLabel') }}
          </label>

          <p v-if="libraryError" class="error">{{ libraryError }}</p>

          <div class="row dialog-actions">
            <button type="button" @click="closeAddLibrary">
              {{ t('actions.close') }}
            </button>
            <button
              type="button"
              class="primary"
              :disabled="addingLibrary || !newLibrary.path.trim()"
              @click="addLibrary"
            >
              {{ addingLibrary ? t('actions.loading') : t('manage.addConfirm') }}
            </button>
          </div>
        </div>
      </div>
    </section>

    <!-- Trash -->
    <section v-else-if="view === 'trash'" class="trash-view">
      <h2>{{ t('trash.title') }}</h2>
      <p v-if="!trashEntries.length" class="empty">{{ t('empty.trash') }}</p>
      <div v-for="entry in trashEntries" :key="entry.id" class="trash-row">
        <div>
          <strong>{{ entry.filename }}</strong>
          <small>{{ t('trash.deletedAt', { date: formatDate(entry.deleted_at), library: entry.library_id }) }}</small>
        </div>
        <div class="row">
          <button @click="restoreEntry(entry)">{{ t('trash.restore') }}</button>
          <button class="danger" @click="purgeEntry(entry)">{{ t('trash.purge') }}</button>
        </div>
      </div>
    </section>

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
            <span v-if="item.tags && item.tags.length" class="tag-dot">{{ item.tags.length }}</span>
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

  <div v-if="selected" class="lightbox" @click.self="close">
    <button class="close" type="button" :aria-label="t('actions.close')" @click="close">×</button>
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
          <div><dt>{{ t('lightbox.date') }}</dt><dd>{{ formatDate(selected.taken_at) }}</dd></div>
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
</template>
